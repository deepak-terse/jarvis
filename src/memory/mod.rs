//! What Jarvis remembers. Implements the `core::contracts::memory` traits:
//!
//! - `session`: the sliding window of recent conversation (`History`)
//! - `profile`: facts about the user from `profile.toml`
//! - `transcript`: every turn, one JSONL file per session
//! - `store`: the data folder holding all of it (`Memory`)

mod profile;
mod session;
mod store;
mod transcript;

pub use session::Window;
pub use store::{Store, append_jsonl};

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "d_history")]
    pub history_messages: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self { history_messages: d_history() }
    }
}

fn d_history() -> usize {
    20
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub dir: PathBuf,
}
