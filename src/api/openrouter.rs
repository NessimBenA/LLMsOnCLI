use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;

use super::{ChatMessage, ChatResponse, LLMClient};

const API_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub struct OpenRouterClient {
    client: Client,
    api_key: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

impl OpenRouterClient {
    pub fn new(api_key: String) -> Self {
        let client = Client::new();
        Self { client, api_key }
    }
}

#[async_trait]
impl LLMClient for OpenRouterClient {
    async fn chat(&self, messages: Vec<ChatMessage>, model: &str) -> Result<ChatResponse> {
        let request = ChatRequest {
            model: model.to_string(),
            messages,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", API_BASE_URL))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://github.com/llm-chat")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenRouter API")?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .context("Failed to read error response")?;
            anyhow::bail!("API request failed: {}", error_text);
        }

        response
            .json::<ChatResponse>()
            .await
            .context("Failed to parse API response")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = OpenRouterClient::new("test-key".to_string());
        assert_eq!(client.api_key, "test-key");
    }
}
