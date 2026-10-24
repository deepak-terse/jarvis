//! Tavily search: ranked snippets plus an optional short answer, condensed into a
//! compact digest for the local model (URLs dropped: unspeakable and token-heavy).

use super::{SearchProvider, SearchRequest};
use crate::core::contracts::settings as config;
use crate::skills::SkillContext;
use crate::skills::support::http::{ApiKey, fetch_json};
use crate::skills::support::speech::truncate;
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::fmt::Write;

const SNIPPET_CHARS: usize = 500;

#[derive(Deserialize)]
struct Settings {
    #[serde(default = "d_key_env")]
    api_key_env: String,
    /// `basic` (1 credit), `fast`, `ultra-fast`, or `advanced` (2 credits).
    #[serde(default = "d_depth")]
    search_depth: String,
    /// `general`, `news` or `finance`.
    #[serde(default = "d_topic")]
    topic: String,
    #[serde(default = "d_max_results")]
    max_results: u8,
    #[serde(default = "d_answer")]
    include_answer: bool,
    /// Boost results from this country (general topic only). Defaults to the profile's
    /// `country`, or the last part of its `location`; set to "" to disable.
    country: Option<String>,
}

fn d_key_env() -> String {
    "TAVILY_API_KEY".into()
}
fn d_depth() -> String {
    "basic".into()
}
fn d_topic() -> String {
    "general".into()
}
fn d_max_results() -> u8 {
    3
}
fn d_answer() -> bool {
    true
}

#[derive(Deserialize)]
struct Response {
    answer: Option<String>,
    #[serde(default)]
    results: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    title: String,
    content: String,
    published_date: Option<String>,
}

pub struct Tavily {
    client: reqwest::Client,
    api_key: ApiKey,
    settings: Settings,
    country: Option<String>,
}

impl Tavily {
    pub fn new(table: &toml::Table, ctx: &SkillContext) -> Result<Self> {
        let settings: Settings = config::settings(table)?;
        let api_key = ApiKey::from_env(&settings.api_key_env, "web_search");
        let country = match settings.country.as_deref() {
            Some(c) => Some(c.to_string()),
            None => ctx
                .profile_str("country")
                .or_else(|| ctx.profile_str("location").and_then(|l| l.rsplit(',').next()))
                .map(str::to_string),
        }
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty());
        Ok(Self { client: ctx.http.clone(), api_key, settings, country })
    }
}

#[async_trait]
impl SearchProvider for Tavily {
    async fn search(&self, req: &SearchRequest<'_>) -> Result<String> {
        let key = self.api_key.get()?;
        let s = &self.settings;
        let topic = req.topic.unwrap_or(&s.topic);
        let mut body = json!({
            "query": req.query,
            "search_depth": s.search_depth,
            "topic": topic,
            "max_results": s.max_results,
            "include_answer": s.include_answer,
        });
        if let Some(range) = req.time_range {
            body["time_range"] = range.into();
        }
        if let Some(country) = self.country.as_deref().filter(|_| topic == "general") {
            body["country"] = country.into();
        }
        let req = self.client.post("https://api.tavily.com/search").bearer_auth(key).json(&body);
        Ok(digest(&fetch_json::<Response>(req, "Tavily").await?))
    }
}

fn digest(r: &Response) -> String {
    let mut out = String::new();
    if let Some(answer) = r.answer.as_deref().filter(|a| !a.trim().is_empty()) {
        let _ = writeln!(out, "Answer: {}\n", answer.trim());
    }
    if r.results.is_empty() {
        out.push_str("No results found.");
    } else {
        out.push_str("Sources:");
        for (i, hit) in r.results.iter().enumerate() {
            let date = hit.published_date.as_deref().map(|d| format!(" ({d})")).unwrap_or_default();
            let _ =
                write!(out, "\n{}. {}{date}: {}", i + 1, hit.title.trim(), truncate(hit.content.trim(), SNIPPET_CHARS));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_compact_and_url_free() {
        let r: Response = serde_json::from_value(json!({
            "query": "weather mumbai",
            "answer": "Mumbai is 31°C and humid.",
            "results": [
                {"title": "Mumbai Weather", "url": "https://x.com", "content": "Humid, 31°C, light rain.", "score": 0.9},
                {"title": "Forecast", "url": "https://y.com", "content": "x".repeat(900), "score": 0.5}
            ],
            "response_time": 1.2
        }))
        .unwrap();
        let d = digest(&r);
        assert!(d.starts_with("Answer: Mumbai is 31°C and humid.\n\nSources:\n1. Mumbai Weather: Humid"));
        assert!(!d.contains("https://"));
        assert!(d.len() < 700);
    }

    fn configured(key_env: &str) -> Tavily {
        let cfg: toml::Table = toml::from_str(&std::fs::read_to_string("config.toml").unwrap()).unwrap();
        let mut table = cfg["skills"]["web_search"].as_table().unwrap().clone();
        table.insert("api_key_env".into(), key_env.into());
        Tavily::new(&table, &SkillContext::for_tests()).unwrap()
    }

    fn query(q: &str) -> SearchRequest<'_> {
        SearchRequest { query: q, time_range: None, topic: None }
    }

    #[test]
    fn country_comes_from_profile_location() {
        let mut ctx = SkillContext::for_tests();
        ctx.profile = toml::from_str(r#"location = "Mumbai, India""#).unwrap();
        assert_eq!(Tavily::new(&toml::Table::new(), &ctx).unwrap().country.as_deref(), Some("india"));
        let off: toml::Table = toml::from_str(r#"country = """#).unwrap();
        assert_eq!(Tavily::new(&off, &ctx).unwrap().country, None);
    }

    #[test]
    fn reads_settings_from_config_toml() {
        let t = configured("JARVIS_TEST_UNSET_KEY");
        assert_eq!((t.settings.search_depth.as_str(), t.settings.topic.as_str()), ("basic", "general"));
        assert_eq!(t.settings.max_results, 3);
        assert!(t.settings.include_answer);
    }

    #[tokio::test]
    async fn missing_key_is_a_clear_error() {
        let err = configured("JARVIS_TEST_UNSET_KEY").search(&query("weather")).await.unwrap_err();
        assert_eq!(err.to_string(), "JARVIS_TEST_UNSET_KEY is not set");
    }

    /// Live API. Real search if TAVILY_API_KEY is set, otherwise checks a rejected key surfaces as an error.
    /// `cargo test -- --ignored tavily_live --nocapture`
    #[tokio::test]
    #[ignore]
    async fn tavily_live() {
        if std::env::var("TAVILY_API_KEY").is_ok() {
            let out = configured("TAVILY_API_KEY").search(&query("current weather in Mumbai")).await.unwrap();
            println!("{out}");
            assert!(out.contains("Sources:"));
        } else {
            unsafe { std::env::set_var("JARVIS_TEST_BOGUS_KEY", "tvly-invalid") };
            let err = configured("JARVIS_TEST_BOGUS_KEY").search(&query("weather")).await.unwrap_err().to_string();
            println!("{err}");
            assert!(err.starts_with("Tavily returned 401"), "{err}");
        }
    }
}
