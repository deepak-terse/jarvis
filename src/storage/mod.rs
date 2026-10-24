//! The data folder: profile facts, conversation logs, metrics.
//!
//! ```text
//! storage/
//! ├── profile.toml        user facts, added to every system prompt
//! ├── conversations/      one JSONL file per session
//! ├── skills/             state skills keep between runs (e.g. learned stock symbols)
//! └── logs/               metrics.jsonl (per-turn timings), jarvis.jsonl (tracing)
//! ```

use anyhow::{Context, Result};
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const PROFILE_TEMPLATE: &str = "# Facts about you, added to every system prompt. Any keys work.\n\
# name = \"Tony\"\n# location = \"Mumbai, India\"\n# country = \"India\"\n# units = \"metric\"\n";

pub struct Storage {
    dir: PathBuf,
    conversation: PathBuf,
}

impl Storage {
    pub fn open(dir: &Path) -> Result<Self> {
        for sub in ["conversations", "logs", "skills"] {
            std::fs::create_dir_all(dir.join(sub)).with_context(|| format!("creating {}", dir.display()))?;
        }
        let profile = dir.join("profile.toml");
        if !profile.exists() {
            std::fs::write(&profile, PROFILE_TEMPLATE)?;
        }
        let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        Ok(Self { dir: dir.to_path_buf(), conversation: dir.join(format!("conversations/{stamp}.jsonl")) })
    }

    /// Profile facts flattened to `key: value` lines (nested tables become `a.b`).
    pub fn profile(&self) -> Result<String> {
        let mut lines = Vec::new();
        flatten("", &toml::Value::Table(self.profile_table()?), &mut lines);
        Ok(lines.join("\n"))
    }

    pub fn profile_table(&self) -> Result<toml::Table> {
        let path = self.dir.join("profile.toml");
        toml::from_str(&std::fs::read_to_string(&path)?).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.dir.join("skills")
    }

    pub fn log_message(&self, record: &impl Serialize) {
        append(&self.conversation, record);
    }

    pub fn log_metrics(&self, record: &impl Serialize) {
        append(&self.dir.join("logs/metrics.jsonl"), record);
    }

    pub fn trace_log(&self) -> Result<File> {
        Ok(OpenOptions::new().create(true).append(true).open(self.dir.join("logs/jarvis.jsonl"))?)
    }
}

fn flatten(key: &str, value: &toml::Value, out: &mut Vec<String>) {
    match value {
        toml::Value::Table(t) => {
            for (k, v) in t {
                flatten(&if key.is_empty() { k.clone() } else { format!("{key}.{k}") }, v, out);
            }
        }
        toml::Value::String(s) => out.push(format!("{key}: {s}")),
        toml::Value::Array(a) => {
            let items: Vec<String> =
                a.iter().map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).collect();
            out.push(format!("{key}: {}", items.join(", ")));
        }
        other => out.push(format!("{key}: {other}")),
    }
}

/// Best-effort JSONL append; logging must never break a conversation.
fn append(path: &Path, record: &impl Serialize) {
    let result = (|| -> Result<()> {
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{}", serde_json::to_string(record)?)?;
        Ok(())
    })();
    if let Err(e) = result {
        tracing::warn!("failed to append to {}: {e}", path.display());
    }
}
