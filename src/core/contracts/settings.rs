//! Config plumbing shared by every pluggable part: each one receives its own TOML
//! table and parses it into a typed settings struct.

use anyhow::Result;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

/// A capability slot (`stt`, `tts`, …): which provider fills it, plus provider-specific settings.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub provider: String,
    #[serde(default = "yes")]
    pub keep_warm: bool,
    #[serde(flatten)]
    pub settings: toml::Table,
}

fn yes() -> bool {
    true
}

/// Deserialize a provider/skill settings table into its typed settings struct.
pub fn settings<T: DeserializeOwned>(table: &toml::Table) -> Result<T> {
    Ok(toml::Value::Table(table.clone()).try_into()?)
}

/// Expand a leading `~` to the home directory.
pub fn expand(path: &Path) -> PathBuf {
    match (path.strip_prefix("~"), std::env::var_os("HOME")) {
        (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => path.to_path_buf(),
    }
}
