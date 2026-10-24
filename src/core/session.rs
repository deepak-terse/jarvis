//! Conversation state: active persona and a sliding window of history.

use crate::llm::{Message, Role};

pub struct Session {
    pub persona: String,
    history: Vec<Message>,
    max_messages: usize,
}

impl Session {
    pub fn new(persona: String, max_messages: usize) -> Self {
        Self { persona, history: Vec::new(), max_messages }
    }

    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Store a completed exchange. Tool traffic stays turn-local to keep the context small.
    pub fn record(&mut self, user: &str, reply: &str) {
        self.history.push(Message::new(Role::User, user));
        self.history.push(Message::new(Role::Assistant, reply));
        let excess = self.history.len().saturating_sub(self.max_messages);
        self.history.drain(..excess + excess % 2); // drop whole exchanges
    }
}
