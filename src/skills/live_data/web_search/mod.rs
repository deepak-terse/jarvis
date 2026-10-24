//! `web_search` skill: the general fallback. The search backend is a provider chosen by config.

mod tavily;

use crate::skills::Skill;
use crate::skills::common::{Policy, SkillContext};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

pub struct SearchRequest<'a> {
    pub query: &'a str,
    /// `day`, `week`, `month` or `year`.
    pub time_range: Option<&'a str>,
    /// `general`, `news` or `finance`; `None` uses the configured default.
    pub topic: Option<&'a str>,
}

#[async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(&self, req: &SearchRequest<'_>) -> Result<String>;
}

pub fn create(settings: &toml::Table, ctx: &SkillContext) -> Result<Arc<dyn Skill>> {
    let provider: Box<dyn SearchProvider> = match settings.get("provider").and_then(|v| v.as_str()) {
        Some("tavily") | None => Box::new(tavily::Tavily::new(settings, ctx)?),
        Some(other) => bail!("unknown web_search provider '{other}'"),
    };
    Ok(Arc::new(WebSearch { provider }))
}

const TIME_RANGES: [&str; 4] = ["day", "week", "month", "year"];
const TOPICS: [&str; 3] = ["general", "news", "finance"];

struct WebSearch {
    provider: Box<dyn SearchProvider>,
}

#[async_trait]
impl Skill for WebSearch {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for specific news stories, protests, developing events, unfamiliar names or acronyms, \
         sports results, or anything you are unsure about. Use this for details beyond general news headlines. \
         Preserve the user's event name and include the relevant place; use topic news for current events."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "A complete, specific search query" },
                "time_range": { "type": "string", "enum": TIME_RANGES, "description": "Only results from the last day, week, month or year" },
                "topic": { "type": "string", "enum": TOPICS, "description": "news for current events, finance for markets, otherwise general" }
            },
            "required": ["query"]
        })
    }

    fn policy(&self) -> Policy {
        Policy { timeout: Duration::from_secs(10), cache_ttl: Duration::from_secs(300) }
    }

    async fn call(&self, args: Value) -> Result<String> {
        let query = args["query"].as_str().filter(|q| !q.trim().is_empty()).context("missing 'query' argument")?;
        // Small models sometimes invent values; ignore anything outside the enums rather than fail.
        let pick = |key: &str, allowed: &[&str]| args[key].as_str().filter(|v| allowed.contains(v));
        let req = SearchRequest { query, time_range: pick("time_range", &TIME_RANGES), topic: pick("topic", &TOPICS) };
        self.provider.search(&req).await
    }
}
