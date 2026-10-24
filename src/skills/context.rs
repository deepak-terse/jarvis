//! What a skill is built with: shared HTTP client, user profile, and paths.

use std::path::PathBuf;
use std::time::Duration;

/// Everything a skill may need at construction time.
#[derive(Clone)]
pub struct SkillContext {
    /// Shared HTTP client (connection pooling, User-Agent). Per-call time limits come from the guard.
    pub http: reqwest::Client,
    /// `profile.toml` as loaded at startup.
    pub profile: toml::Table,
    /// Writable directory for skill state (e.g. learned stock symbols).
    pub state_dir: PathBuf,
}

impl SkillContext {
    pub fn new(profile: toml::Table, state_dir: PathBuf) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("jarvis/", env!("CARGO_PKG_VERSION"), " (personal voice assistant)"))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { http, profile, state_dir })
    }

    /// A top-level string from the profile, e.g. `location` or `country`.
    pub fn profile_str(&self, key: &str) -> Option<&str> {
        self.profile.get(key).and_then(toml::Value::as_str).map(str::trim).filter(|s| !s.is_empty())
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        let dir = std::env::temp_dir().join("jarvis-skill-tests");
        Self::new(toml::Table::new(), dir).unwrap()
    }
}
