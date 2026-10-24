//! What the agent remembers: the conversation so far, facts about the user, and a transcript.

use super::model::Message;
use anyhow::Result;
use serde_json::Value;

/// Recent conversation fed back to the model each turn.
pub trait History: Send {
    fn messages(&self) -> &[Message];
    /// Store a completed exchange.
    fn record(&mut self, user: &str, reply: &str);
}

/// Long-term memory.
pub trait Memory: Send + Sync {
    /// Facts about the user as `key: value` lines, added to the system prompt.
    fn profile(&self) -> Result<String>;
    /// Append one record to the conversation transcript. Best effort: never fails a turn.
    fn append_transcript(&self, record: &Value);
}
