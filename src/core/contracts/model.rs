//! The decision-maker. The agent talks to the `Llm` trait only; the chat format,
//! tool-call syntax and KV cache are adapter details hidden behind it.

use super::tools::{ToolCall, ToolSpec};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self { role, content: content.into() }
    }
}

/// The stable part of the prompt (system text + tools). Identical across turns,
/// so the engine caches its KV state under `cache_key`.
#[derive(Debug, Clone)]
pub struct Prefix {
    pub cache_key: String,
    pub system: String,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub prefix: Prefix,
    pub messages: Vec<Message>,
    pub temperature: f32,
}

#[derive(Debug)]
pub enum LlmEvent {
    /// User-visible text (tool-call and reasoning markup already removed).
    Text(String),
    Done(Completion),
    Error(String),
}

#[derive(Debug, Default)]
pub struct Completion {
    /// Exact model output, used as the assistant message for follow-up rounds.
    pub raw: String,
    /// Visible text only.
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    /// Tool-call blocks that could not be parsed.
    pub malformed: Vec<String>,
    pub stats: GenStats,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct GenStats {
    pub cached_tokens: usize,
    pub prompt_tokens: usize,
    pub prompt_ms: f64,
    pub gen_tokens: usize,
    pub gen_ms: f64,
}

pub trait Llm: Send + Sync {
    /// Stream a reply. Dropping the receiver cancels generation.
    fn chat(&self, req: ChatRequest) -> UnboundedReceiver<LlmEvent>;
    /// Pre-compute the KV cache for a prefix (startup, persona switch).
    fn warm(&self, prefix: Prefix);
    /// A tool call written in the model's own syntax, for worked examples in the prompt.
    fn format_tool_call(&self, call: &ToolCall) -> String;
}
