//! Tools the model can call. A `Skill` is one tool (MCP's tool shape); `Tools` is the
//! set the agent sees, with execution policy (time limits, caching, fallback) applied.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// MCP-shaped tool description: name, description, JSON schema.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Result of a tool call as the agent and telemetry see it.
#[derive(Debug, Clone, Serialize)]
pub struct ToolOutcome {
    pub text: String,
    pub ok: bool,
    /// Served from cache.
    pub cached: bool,
    /// The skill failed and web search answered instead.
    pub fallback: bool,
}

impl ToolOutcome {
    pub fn ok(text: String) -> Self {
        Self { text, ok: true, cached: false, fallback: false }
    }

    pub fn error(text: String) -> Self {
        Self { text, ok: false, cached: false, fallback: false }
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

#[async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// JSON schema of the arguments.
    fn parameters(&self) -> Value;
    async fn call(&self, args: Value) -> Result<String>;
    /// Time limit and cache lifetime when config doesn't set them.
    fn policy(&self) -> Policy {
        Policy::default()
    }
    /// Web search query to try if this skill fails or times out (`None`: no fallback).
    fn fallback_query(&self, _args: &Value) -> Option<String> {
        None
    }
}

/// The tools available to the agent. `allowed` is the persona's tool list.
#[async_trait]
pub trait Tools: Send + Sync {
    /// Whether a tool is enabled.
    fn contains(&self, name: &str) -> bool;
    /// Tool definitions for the allowed tools, in `allowed` order.
    fn specs(&self, allowed: &[String]) -> Vec<ToolSpec>;
    /// Run a tool call. Errors are returned as text so the model can recover.
    async fn call(&self, allowed: &[String], name: &str, args: Value) -> ToolOutcome;
}
