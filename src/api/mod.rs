use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

mod openrouter;

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: ChatMessage,
}

#[async_trait]
pub trait LLMClient {
    async fn chat(&self, messages: Vec<ChatMessage>, model: &str) -> Result<ChatResponse>;
}

pub use openrouter::OpenRouterClient;
