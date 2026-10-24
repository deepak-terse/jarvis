//! The data folder. All AI context lives here, so it is easy to back up, inspect or delete.
//!
//! ```text
//! storage/
//! ├── profile.toml        user facts, added to every system prompt
//! ├── conversations/      one JSONL file per session
//! ├── skills/             state skills keep between runs (e.g. learned stock symbols)
//! └── logs/               metrics.jsonl (per-turn timings), jarvis.jsonl (tracing)
//! ```

use super::{profile, transcript};
use crate::core::contracts::memory::Memory;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Store {
    dir: PathBuf,
    conversation: PathBuf,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        for sub in ["conversations", "logs", "skills"] {
            std::fs::create_dir_all(dir.join(sub)).with_context(|| format!("creating {}", dir.display()))?;
        }
        profile::create_if_missing(&dir.join("profile.toml"))?;
        Ok(Self { dir: dir.to_path_buf(), conversation: transcript::new_path(&dir.join("conversations")) })
    }

    pub fn profile_table(&self) -> Result<toml::Table> {
        profile::load(&self.dir.join("profile.toml"))
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.dir.join("skills")
    }

    pub fn metrics_log(&self) -> PathBuf {
        self.dir.join("logs/metrics.jsonl")
    }

    pub fn trace_log(&self) -> Result<File> {
        Ok(OpenOptions::new().create(true).append(true).open(self.dir.join("logs/jarvis.jsonl"))?)
    }
}

impl Memory for Store {
    fn profile(&self) -> Result<String> {
        Ok(profile::flatten(&self.profile_table()?))
    }

    fn append_transcript(&self, record: &Value) {
        append_jsonl(&self.conversation, record);
    }
}

/// Best-effort JSONL append; logging must never break a conversation.
pub fn append_jsonl(path: &Path, record: &impl Serialize) {
    let result = (|| -> Result<()> {
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{}", serde_json::to_string(record)?)?;
        Ok(())
    })();
    if let Err(e) = result {
        tracing::warn!("failed to append to {}: {e}", path.display());
    }
}
