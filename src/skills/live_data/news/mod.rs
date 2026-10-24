//! `news` skill: top recent headlines per category from RSS feeds listed in config.
//! Adding a `[skills.news.categories]` key makes a new category available to the model.

mod feeds;

use crate::config;
use crate::skills::Skill;
use crate::skills::common::speech::ago;
use crate::skills::common::{Policy, SkillContext};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use feeds::{FeedSettings, FeedSpec, Feeds, Headline};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

#[derive(Deserialize)]
struct Settings {
    /// Category name → feeds (URL strings or `{ url, name }` tables).
    categories: BTreeMap<String, Vec<FeedSpec>>,
    /// Used when the model doesn't name one; defaults to `top` if present, else the first category.
    default_category: Option<String>,
    #[serde(default = "d_refresh_mins")]
    refresh_mins: f64,
    #[serde(default = "d_max_headlines")]
    max_headlines: usize,
    /// Ignore stories older than this.
    #[serde(default = "d_max_age_hours")]
    max_age_hours: i64,
    /// Log a warning for feeds whose newest item is older than this.
    #[serde(default = "d_stale_after_hours")]
    stale_after_hours: i64,
    #[serde(default = "d_fetch_timeout_secs")]
    fetch_timeout_secs: f64,
    /// Added to fallback web searches, e.g. "latest tech news India".
    #[serde(default = "d_region")]
    region: String,
}

fn d_refresh_mins() -> f64 {
    15.0
}
fn d_max_headlines() -> usize {
    5
}
fn d_max_age_hours() -> i64 {
    36
}
fn d_stale_after_hours() -> i64 {
    48
}
fn d_fetch_timeout_secs() -> f64 {
    8.0
}
fn d_region() -> String {
    "India".into()
}

pub fn create(settings: &toml::Table, ctx: &SkillContext) -> Result<Arc<dyn Skill>> {
    let s: Settings = config::settings(settings)?;
    if s.categories.is_empty() {
        bail!("no news categories configured ([skills.news.categories])");
    }
    let default_category = match s.default_category {
        Some(c) if s.categories.contains_key(&c) => c,
        Some(c) => bail!("default_category '{c}' is not a configured category"),
        None if s.categories.contains_key("top") => "top".into(),
        None => s.categories.keys().next().unwrap().clone(),
    };
    tokio::runtime::Handle::try_current().context("the news skill needs a Tokio runtime")?;
    let feeds = Feeds::spawn(
        ctx.http.clone(),
        s.categories,
        FeedSettings {
            refresh: Duration::from_secs_f64(s.refresh_mins * 60.0),
            fetch_timeout: Duration::from_secs_f64(s.fetch_timeout_secs),
            stale_after: chrono::Duration::hours(s.stale_after_hours),
        },
    );
    let description = format!(
        "Today's top news headlines. Categories: {}. Use web_search for details about a particular story.",
        feeds.categories().collect::<Vec<_>>().join(", ")
    );
    Ok(Arc::new(News {
        feeds,
        description,
        default_category,
        max_headlines: s.max_headlines,
        max_age: chrono::Duration::hours(s.max_age_hours),
        region: s.region,
    }))
}

struct News {
    feeds: Arc<Feeds>,
    description: String,
    default_category: String,
    max_headlines: usize,
    max_age: chrono::Duration,
    region: String,
}

impl News {
    fn category<'a>(&'a self, args: &'a Value) -> &'a str {
        args["category"]
            .as_str()
            .map(str::trim)
            .filter(|c| self.feeds.categories().any(|k| k == *c))
            .unwrap_or(&self.default_category)
    }
}

#[async_trait]
impl Skill for News {
    fn name(&self) -> &str {
        "news"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        let categories: Vec<&str> = self.feeds.categories().collect();
        json!({
            "type": "object",
            "properties": {
                "category": { "type": "string", "enum": categories, "description": format!("Defaults to {}", self.default_category) }
            }
        })
    }

    /// Headlines are already in memory after the first refresh; the timeout only bounds that first wait.
    fn policy(&self) -> Policy {
        Policy { timeout: Duration::from_secs(10), cache_ttl: Duration::ZERO }
    }

    fn fallback_query(&self, args: &Value) -> Option<String> {
        let category = self.category(args);
        let topic = if category == "top" { String::new() } else { format!("{category} ") };
        Some(format!("latest {topic}news {}", self.region))
    }

    async fn call(&self, args: Value) -> Result<String> {
        let category = self.category(&args);
        self.feeds.ready().await;
        let headlines = self.feeds.headlines(category, self.max_age, Utc::now());
        if headlines.is_empty() {
            bail!("no recent {category} headlines");
        }
        Ok(format_headlines(category, &headlines[..headlines.len().min(self.max_headlines)], Local::now()))
    }
}

fn format_headlines(category: &str, headlines: &[Headline], now: DateTime<Local>) -> String {
    let mut out = if category == "top" { "Top headlines:".to_string() } else { format!("Top {category} headlines:") };
    for (i, h) in headlines.iter().enumerate() {
        let age = h.published.map(|p| format!(", {}", ago(&p, &now))).unwrap_or_default();
        let _ = write!(out, "\n{}. {} ({}{age})", i + 1, h.title, h.source);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(toml: &str) -> toml::Table {
        toml::from_str(toml).unwrap()
    }

    #[tokio::test]
    async fn categories_come_from_config() {
        let cfg = settings(
            r#"[categories]
               tech = ["http://127.0.0.1:9/tech.xml"]
               top = ["http://127.0.0.1:9/top.xml"]"#,
        );
        let skill = create(&cfg, &SkillContext::for_tests()).unwrap();
        assert_eq!(skill.parameters()["properties"]["category"]["enum"], json!(["tech", "top"]));
        assert!(skill.description().contains("Categories: tech, top."));
        assert_eq!(skill.fallback_query(&json!({})).unwrap(), "latest news India");
        assert_eq!(skill.fallback_query(&json!({"category": "tech"})).unwrap(), "latest tech news India");
        // An unreachable feed yields no headlines, which the guard turns into a web-search fallback.
        assert_eq!(skill.call(json!({"category": "tech"})).await.unwrap_err().to_string(), "no recent tech headlines");
    }

    #[tokio::test]
    async fn rejects_unknown_default_category() {
        let cfg = settings("default_category = \"world\"\n[categories]\ntop = [\"http://127.0.0.1:9/a\"]");
        assert!(create(&cfg, &SkillContext::for_tests()).is_err());
    }

    #[test]
    fn formats_numbered_headlines_with_source_and_age() {
        let now = Local::now();
        let hs = [
            Headline {
                title: "Sensex hits record".into(),
                source: "Mint".into(),
                published: Some(now.to_utc() - chrono::Duration::hours(2)),
            },
            Headline { title: "Monsoon update".into(), source: "The Hindu".into(), published: None },
        ];
        assert_eq!(
            format_headlines("business", &hs, now),
            "Top business headlines:\n1. Sensex hits record (Mint, 2 hours ago)\n2. Monsoon update (The Hindu)"
        );
    }

    /// Fetches the feeds in config.toml. `cargo test -- --ignored live --nocapture`
    #[tokio::test]
    #[ignore]
    async fn news_feeds_live() {
        let cfg: toml::Table = toml::from_str(&std::fs::read_to_string("config.toml").unwrap()).unwrap();
        let table = cfg["skills"]["news"].as_table().unwrap();
        let skill = create(table, &SkillContext::for_tests()).unwrap();
        for category in table["categories"].as_table().unwrap().keys() {
            let out = skill.call(json!({"category": category})).await;
            println!("{category}: {out:#?}\n");
            assert!(out.is_ok());
        }
    }
}
