//! Conversation state: the active persona and the recent history (any `History`).

use crate::core::contracts::memory::History;
use crate::core::contracts::model::Message;

pub struct Session {
    pub persona: String,
    history: Box<dyn History>,
}

impl Session {
    pub fn new(persona: String, history: Box<dyn History>) -> Self {
        Self { persona, history }
    }

    pub fn history(&self) -> &[Message] {
        self.history.messages()
    }

    pub fn record(&mut self, user: &str, reply: &str) {
        self.history.record(user, reply);
    }
}
