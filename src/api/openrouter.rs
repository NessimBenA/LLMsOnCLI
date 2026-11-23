use anyhow::{bail, Context, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use super::{ChatMessage, StreamEvent};

const CHAT_COMPLETIONS_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const APP_URL: &str = "https://github.com/NessimBenA/LLMsOnCLI";

#[derive(Clone)]
pub struct OpenRouterClient {
    client: Client,
    api_key: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: ChunkDelta,
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct ChunkDelta {
    content: Option<String>,
}

#[derive(Debug, PartialEq)]
enum StreamItem {
    Text(String),
    Done,
}

#[derive(Default)]
struct SseParser {
    pending: Vec<u8>,
}

impl OpenRouterClient {
    pub fn new(api_key: String) -> Self {
        let client = Client::new();
        Self { client, api_key }
    }

    pub async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        events: &UnboundedSender<StreamEvent>,
    ) -> Result<()> {
        let mut response = self
            .client
            .post(CHAT_COMPLETIONS_URL)
            .bearer_auth(&self.api_key)
            .header("HTTP-Referer", APP_URL)
            .json(&ChatRequest {
                model,
                messages,
                stream: true,
            })
            .send()
            .await
            .context("Could not reach OpenRouter")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .context("Could not read the error response from OpenRouter")?;
            bail!(describe_error(status, &body));
        }

        let mut parser = SseParser::default();
        while let Some(bytes) = response
            .chunk()
            .await
            .context("The stream from OpenRouter broke off")?
        {
            parser.push(&bytes);
            while let Some(item) = parser.next_item()? {
                match item {
                    StreamItem::Text(text) => events.send(StreamEvent::Delta(text))?,
                    StreamItem::Done => return Ok(()),
                }
            }
        }
        bail!("OpenRouter closed the stream before sending [DONE]")
    }
}

impl SseParser {
    fn push(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
    }

    fn next_item(&mut self) -> Result<Option<StreamItem>> {
        while let Some(end) = self.pending.iter().position(|&byte| byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = std::str::from_utf8(&line)
                .context("OpenRouter sent a stream line that is not valid UTF-8")?;
            let Some(data) = line.trim_end_matches(['\n', '\r']).strip_prefix("data:") else {
                continue;
            };
            if let Some(item) = parse_data(data.strip_prefix(' ').unwrap_or(data))? {
                return Ok(Some(item));
            }
        }
        Ok(None)
    }
}

fn parse_data(data: &str) -> Result<Option<StreamItem>> {
    if data == "[DONE]" {
        return Ok(Some(StreamItem::Done));
    }
    let chunk: StreamChunk = serde_json::from_str(data)
        .with_context(|| format!("OpenRouter sent a stream chunk that is not valid JSON: {data}"))?;
    if let Some(error) = chunk.error {
        bail!("OpenRouter stopped the reply: {}", error.message);
    }
    let Some(choice) = chunk.choices.into_iter().next() else {
        return Ok(None);
    };
    if choice.finish_reason.as_deref() == Some("error") {
        bail!("OpenRouter stopped the reply with finish_reason \"error\"");
    }
    Ok(choice
        .delta
        .content
        .filter(|content| !content.is_empty())
        .map(StreamItem::Text))
}

fn describe_error(status: StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<ErrorResponse>(body)
        .map(|response| response.error.message)
        .unwrap_or_else(|_| body.trim().to_string());
    format!("OpenRouter returned {status}: {detail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEEP_ALIVE: &str = ": OPENROUTER PROCESSING";
    const CONTENT_CHUNK: &str = r#"data: {"choices":[{"delta":{"content":"Hello","role":"assistant"},"finish_reason":null,"index":0}],"created":1677652288,"id":"chatcmpl-123","model":"openai/gpt-4","object":"chat.completion.chunk"}"#;
    const FINISH_CHUNK: &str = r#"data: {"id":"gen-abc123","created":1677652288,"model":"openai/gpt-4","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"","role":"assistant"},"finish_reason":"stop","native_finish_reason":"stop"}]}"#;
    const USAGE_CHUNK: &str = r#"data: {"id":"gen-abc123","created":1677652288,"model":"openai/gpt-4","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"","role":"assistant"},"finish_reason":"stop","native_finish_reason":"stop"}],"usage":{"completion_tokens":15,"completion_tokens_details":{"reasoning_tokens":5},"cost":0.0012,"cost_details":{"upstream_inference_completions_cost":0.0004,"upstream_inference_cost":null,"upstream_inference_prompt_cost":0.0008},"is_byok":false,"prompt_tokens":10,"prompt_tokens_details":{"cached_tokens":2},"server_tool_use_details":{"tool_calls_executed":2,"tool_calls_requested":2},"total_tokens":25}}"#;
    const ERROR_CHUNK: &str = r#"data: {"id":"cmpl-abc123","object":"chat.completion.chunk","created":1234567890,"model":"openai/gpt-4o","provider":"openai","error":{"code":502,"message":"Provider disconnected unexpectedly"},"choices":[{"index":0,"delta":{"content":""},"finish_reason":"error"}]}"#;
    const DONE: &str = "data: [DONE]";
    const PRE_STREAM_ERROR: &str = r#"{
  "error": {
    "code": 400,
    "message": "Invalid model specified"
  }
}"#;

    fn content_chunk(content: &str) -> String {
        CONTENT_CHUNK.replace(
            r#""Hello""#,
            &serde_json::Value::from(content).to_string(),
        )
    }

    fn events(lines: &[&str]) -> String {
        lines.iter().map(|line| format!("{line}\n\n")).collect()
    }

    fn text(content: &str) -> StreamItem {
        StreamItem::Text(content.to_string())
    }

    fn parse_all(pieces: &[&[u8]]) -> Result<Vec<StreamItem>> {
        let mut parser = SseParser::default();
        let mut items = Vec::new();
        for piece in pieces {
            parser.push(piece);
            while let Some(item) = parser.next_item()? {
                items.push(item);
            }
        }
        Ok(items)
    }

    #[test]
    fn test_client_creation() {
        let client = OpenRouterClient::new("test-key".to_string());
        assert_eq!(client.api_key, "test-key");
    }

    #[test]
    fn documented_stream_yields_its_text_then_done() -> Result<()> {
        let stream = events(&[
            KEEP_ALIVE,
            CONTENT_CHUNK,
            &content_chunk(" there"),
            &content_chunk("!"),
            FINISH_CHUNK,
            USAGE_CHUNK,
            DONE,
        ]);
        assert_eq!(
            parse_all(&[stream.as_bytes()])?,
            vec![text("Hello"), text(" there"), text("!"), StreamItem::Done]
        );
        Ok(())
    }

    #[test]
    fn final_lines_printed_without_blank_lines_still_parse() -> Result<()> {
        let stream = format!("{FINISH_CHUNK}\n{USAGE_CHUNK}\n{DONE}\n");
        assert_eq!(parse_all(&[stream.as_bytes()])?, vec![StreamItem::Done]);
        Ok(())
    }

    #[test]
    fn byte_by_byte_delivery_and_crlf_endings_give_the_same_items() -> Result<()> {
        let stream = events(&[KEEP_ALIVE, CONTENT_CHUNK, &content_chunk(" café"), DONE])
            .replace('\n', "\r\n");
        let pieces: Vec<&[u8]> = stream.as_bytes().chunks(1).collect();
        assert_eq!(
            parse_all(&pieces)?,
            vec![text("Hello"), text(" café"), StreamItem::Done]
        );
        Ok(())
    }

    #[test]
    fn mid_stream_error_chunk_fails_the_reply() -> Result<()> {
        let mut parser = SseParser::default();
        parser.push(events(&[CONTENT_CHUNK, ERROR_CHUNK]).as_bytes());
        assert_eq!(parser.next_item()?, Some(text("Hello")));
        let error = parser
            .next_item()
            .expect_err("an error chunk must fail the reply");
        assert!(error
            .to_string()
            .contains("Provider disconnected unexpectedly"));
        Ok(())
    }

    #[test]
    fn error_chunk_as_the_only_event_is_a_failure() {
        let mut parser = SseParser::default();
        parser.push(events(&[ERROR_CHUNK]).as_bytes());
        assert!(parser.next_item().is_err());
    }

    #[test]
    fn pre_stream_error_body_is_reported_with_its_message() {
        assert_eq!(
            describe_error(StatusCode::BAD_REQUEST, PRE_STREAM_ERROR),
            "OpenRouter returned 400 Bad Request: Invalid model specified"
        );
    }
}
