//! Shared by every skill group: the context skills are built with, the settings
//! every skill section accepts, and the guard that applies them.

mod cache;
mod guard;
pub mod http;
pub mod speech;

pub use guard::{Guarded, ToolOutcome};

use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

/// Everything a skill may need at construction time.
#[derive(Clone)]
pub struct SkillContext {
    /// Shared HTTP client (connection pooling, User-Agent). Per-call time limits come from the guard.
    pub http: reqwest::Client,
    /// `profile.toml` as loaded at startup.
    pub profile: toml::Table,
    /// Directory of `config.toml`; relative data paths resolve against it.
    pub config_dir: PathBuf,
    /// Writable directory for skill state (e.g. learned stock symbols).
    pub state_dir: PathBuf,
}

impl SkillContext {
    pub fn new(profile: toml::Table, config_dir: PathBuf, state_dir: PathBuf) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("jarvis/", env!("CARGO_PKG_VERSION"), " (personal voice assistant)"))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { http, profile, config_dir, state_dir })
    }

    /// A top-level string from the profile, e.g. `location` or `country`.
    pub fn profile_str(&self, key: &str) -> Option<&str> {
        self.profile.get(key).and_then(toml::Value::as_str).map(str::trim).filter(|s| !s.is_empty())
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        let dir = std::env::temp_dir().join("jarvis-skill-tests");
        Self::new(toml::Table::new(), ".".into(), dir).unwrap()
    }
}

/// How a skill is guarded when its config section doesn't say otherwise.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub timeout: Duration,
    /// Zero disables caching.
    pub cache_ttl: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self { timeout: Duration::from_secs(8), cache_ttl: Duration::ZERO }
    }
}

/// Keys every `[skills.<name>]` section accepts, on top of its own settings.
#[derive(Debug, Default, Deserialize)]
pub struct CommonSettings {
    pub enabled: Option<bool>,
    pub timeout_secs: Option<f64>,
    pub cache_ttl_secs: Option<u64>,
    /// Fall back to `web_search` when the skill fails or times out (default true).
    pub fallback: Option<bool>,
    /// Replace the built-in tool description (tune wording without recompiling).
    pub description: Option<String>,
}

impl CommonSettings {
    pub fn enabled(&self) -> bool {
        self.enabled != Some(false)
    }

    pub fn policy(&self, default: Policy) -> Policy {
        Policy {
            timeout: self.timeout_secs.map_or(default.timeout, Duration::from_secs_f64),
            cache_ttl: self.cache_ttl_secs.map_or(default.cache_ttl, Duration::from_secs),
        }
    }
}
