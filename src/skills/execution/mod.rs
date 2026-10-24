//! Wraps every skill with the behaviour config asks for: a time limit, a result
//! cache, a web-search fallback, and an honest failure message when all else fails.

mod cache;

use super::{Policy, Skill, ToolOutcome};
use cache::TtlCache;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

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

pub struct Guarded {
    skill: Arc<dyn Skill>,
    description: String,
    timeout: Duration,
    cache: Option<TtlCache>,
    fallback: Option<Arc<Guarded>>,
}

impl Guarded {
    pub fn new(
        skill: Arc<dyn Skill>,
        description: Option<String>,
        policy: Policy,
        fallback: Option<Arc<Guarded>>,
    ) -> Self {
        let description = description.unwrap_or_else(|| skill.description().to_string());
        let cache = (!policy.cache_ttl.is_zero()).then(|| TtlCache::new(policy.cache_ttl));
        Self { skill, description, timeout: policy.timeout, cache, fallback }
    }

    pub fn name(&self) -> &str {
        self.skill.name()
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn parameters(&self) -> Value {
        self.skill.parameters()
    }

    pub async fn run(&self, args: Value) -> ToolOutcome {
        // serde_json maps are sorted, so equal arguments serialize identically.
        let key = args.to_string();
        if let Some(text) = self.cache.as_ref().and_then(|c| c.get(&key)) {
            return ToolOutcome { cached: true, ..ToolOutcome::ok(text) };
        }
        let result = match tokio::time::timeout(self.timeout, self.skill.call(args.clone())).await {
            Ok(r) => r,
            Err(_) => Err(anyhow::anyhow!("timed out after {:.1}s", self.timeout.as_secs_f64())),
        };
        let err = match result {
            Ok(text) => {
                if let Some(c) = &self.cache {
                    c.put(key, text.clone());
                }
                return ToolOutcome::ok(text);
            }
            Err(e) => e,
        };
        tracing::warn!(skill = self.name(), "skill failed: {err:#}");

        if let (Some(web), Some(query)) = (&self.fallback, self.skill.fallback_query(&args)) {
            let out = Box::pin(web.run(json!({ "query": query }))).await;
            if out.ok {
                return ToolOutcome {
                    text: format!("Live source unavailable; web results:\n{}", out.text),
                    fallback: true,
                    ..ToolOutcome::ok(String::new())
                };
            }
        }
        ToolOutcome::error(format!(
            "Error: couldn't get that information ({err}). Tell the user you couldn't get it right now; do not guess."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, bail};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        name: &'static str,
        delay: Duration,
        fail: bool,
        calls: AtomicUsize,
    }

    fn fake(name: &'static str, delay_ms: u64, fail: bool) -> Arc<Fake> {
        Arc::new(Fake { name, delay: Duration::from_millis(delay_ms), fail, calls: AtomicUsize::new(0) })
    }

    #[async_trait]
    impl Skill for Fake {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "fake"
        }
        fn parameters(&self) -> Value {
            json!({})
        }
        async fn call(&self, args: Value) -> Result<String> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            tokio::time::sleep(self.delay).await;
            if self.fail {
                bail!("source down");
            }
            Ok(format!("{} #{n}: {}", self.name, args["query"].as_str().unwrap_or("-")))
        }
        fn fallback_query(&self, _: &Value) -> Option<String> {
            Some("fallback query".into())
        }
    }

    fn policy(timeout_ms: u64, ttl_ms: u64) -> Policy {
        Policy { timeout: Duration::from_millis(timeout_ms), cache_ttl: Duration::from_millis(ttl_ms) }
    }

    fn web(fail: bool) -> Arc<Guarded> {
        Arc::new(Guarded::new(fake("web", 0, fail), None, policy(500, 0), None))
    }

    #[tokio::test]
    async fn timeout_falls_back_to_web_search() {
        let g = Guarded::new(fake("slow", 200, false), None, policy(20, 0), Some(web(false)));
        let out = g.run(json!({})).await;
        assert!(out.ok && out.fallback, "{out:?}");
        assert_eq!(out.text, "Live source unavailable; web results:\nweb #1: fallback query");
    }

    #[tokio::test]
    async fn total_failure_is_reported_honestly() {
        let g = Guarded::new(fake("broken", 0, true), None, policy(500, 0), Some(web(true)));
        let out = g.run(json!({})).await;
        assert!(!out.ok && !out.fallback);
        assert!(out.text.starts_with("Error: couldn't get that information (source down)"), "{}", out.text);
        assert!(out.text.contains("do not guess"));
    }

    #[tokio::test]
    async fn caches_within_ttl_only() {
        let skill = fake("cached", 0, false);
        let g = Guarded::new(skill.clone(), Some("custom".into()), policy(500, 50), None);
        assert_eq!(g.description(), "custom");
        let a = g.run(json!({"query": "x"})).await;
        let b = g.run(json!({"query": "x"})).await;
        assert!(!a.cached && b.cached);
        assert_eq!(a.text, b.text);
        assert!(!g.run(json!({"query": "y"})).await.cached);
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(!g.run(json!({"query": "x"})).await.cached);
        assert_eq!(skill.calls.load(Ordering::SeqCst), 3);
    }
}
