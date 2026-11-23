use serde::Serialize;

mod openrouter;

pub use openrouter::OpenRouterClient;

#[derive(Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug)]
pub enum StreamEvent {
    Delta(String),
    Finished(anyhow::Result<()>),
}
