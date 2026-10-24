//! Conversation transcripts: one JSONL file per session, stored locally only.

use std::path::{Path, PathBuf};

/// A fresh transcript file in `dir`, named after the session start time.
pub fn new_path(dir: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    dir.join(format!("{stamp}.jsonl"))
}
