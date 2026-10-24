//! Working memory: a sliding window of recent user/assistant messages.

use crate::core::contracts::memory::History;
use crate::core::contracts::model::{Message, Role};

pub struct Window {
    history: Vec<Message>,
    max_messages: usize,
}

impl Window {
    pub fn new(max_messages: usize) -> Self {
        Self { history: Vec::new(), max_messages }
    }
}

impl History for Window {
    fn messages(&self) -> &[Message] {
        &self.history
    }

    /// Tool traffic stays turn-local to keep the context small.
    fn record(&mut self, user: &str, reply: &str) {
        self.history.push(Message::new(Role::User, user));
        self.history.push(Message::new(Role::Assistant, reply));
        let excess = self.history.len().saturating_sub(self.max_messages);
        self.history.drain(..excess + excess % 2); // drop whole exchanges
    }
}
