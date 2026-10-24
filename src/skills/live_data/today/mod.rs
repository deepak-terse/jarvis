//! `about_today` skill: notable historical events from Wikipedia.

mod wikipedia;

use crate::config;
use crate::skills::Skill;
use crate::skills::common::{Policy, SkillContext};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{Local, NaiveDate};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wikipedia::{Event, Wikipedia};

#[derive(Deserialize)]
struct Settings {
    #[serde(default = "d_max_events")]
    max_events: usize,
    #[serde(default = "d_language")]
    language: String,
    /// Give up on Wikipedia after this long.
    #[serde(default = "d_events_timeout")]
    events_timeout_secs: f64,
}

fn d_max_events() -> usize {
    3
}
fn d_language() -> String {
    "en".into()
}
fn d_events_timeout() -> f64 {
    4.0
}

pub fn create(settings: &toml::Table, ctx: &SkillContext) -> Result<Arc<dyn Skill>> {
    let s: Settings = config::settings(settings)?;
    Ok(Arc::new(AboutToday {
        wiki: Wikipedia::new(ctx.http.clone(), &s.language),
        events_timeout: Duration::from_secs_f64(s.events_timeout_secs),
        max_events: s.max_events,
        events_cache: Mutex::default(),
    }))
}

struct AboutToday {
    wiki: Wikipedia,
    events_timeout: Duration,
    max_events: usize,
    /// On-this-day events for the most recently asked date (they never change).
    events_cache: Mutex<Option<(NaiveDate, Arc<Vec<Event>>)>>,
}

impl AboutToday {
    async fn events(&self, date: NaiveDate) -> Result<Arc<Vec<Event>>> {
        if let Some((d, events)) = self.events_cache.lock().unwrap().as_ref()
            && *d == date
        {
            return Ok(events.clone());
        }
        let fetched = tokio::time::timeout(self.events_timeout, self.wiki.on_this_day(date))
            .await
            .map_err(|_| anyhow::anyhow!("Wikipedia timed out"))??;
        let events = Arc::new(fetched.selected);
        *self.events_cache.lock().unwrap() = Some((date, events.clone()));
        Ok(events)
    }
}

#[async_trait]
impl Skill for AboutToday {
    fn name(&self) -> &str {
        "about_today"
    }

    fn description(&self) -> &str {
        "Notable historical events for today or another date. Use it for \
         \"what's special about today\" and \"on this day in history\". For festivals or holidays, use web_search."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "date": { "type": "string", "description": "YYYY-MM-DD; omit for today" } }
        })
    }

    fn policy(&self) -> Policy {
        Policy { timeout: Duration::from_secs(6), cache_ttl: Duration::ZERO }
    }

    fn fallback_query(&self, args: &Value) -> Option<String> {
        Some(format!("historical events on {}", args["date"].as_str().unwrap_or("today")))
    }

    async fn call(&self, args: Value) -> Result<String> {
        let today = Local::now().date_naive();
        let date =
            args["date"].as_str().and_then(|d| NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").ok()).unwrap_or(today);
        let events = self.events(date).await.inspect_err(|e| tracing::warn!("on-this-day: {e:#}"));
        Ok(format_day(date, date == today, events.as_deref().map(Vec::as_slice), self.max_events))
    }
}

fn format_day(date: NaiveDate, is_today: bool, events: Result<&[Event], &anyhow::Error>, max_events: usize) -> String {
    let mut out = format!("{} {}.\n", if is_today { "Today is" } else { "Date:" }, date.format("%A, %-d %B %Y"));
    match events {
        Ok(events) if !events.is_empty() => {
            out.push_str("On this day (source: Wikipedia):");
            for e in events.iter().take(max_events) {
                let year = e.year.map(|y| format!("In {y}, ")).unwrap_or_default();
                let _ = write!(out, "\n- {year}{}", e.text.trim());
            }
        }
        Ok(_) => out.push_str("No historical events listed."),
        Err(_) => out.push_str("Historical events are unavailable right now."),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn historical_events() {
        let events: Vec<Event> = serde_json::from_value(json!([
            {"text": "Event A.", "year": 1950, "pages": []},
            {"text": "Event B.", "year": 2001},
            {"text": "Event C.", "year": 1800}
        ]))
        .unwrap();
        let out = format_day(d("2026-11-08"), true, Ok(&events), 2);
        assert_eq!(
            out,
            "Today is Sunday, 8 November 2026.\n\
             On this day (source: Wikipedia):\n- In 1950, Event A.\n- In 2001, Event B."
        );
    }

    #[test]
    fn reports_unavailable_when_wikipedia_fails() {
        let err = anyhow::anyhow!("down");
        let out = format_day(d("2026-10-23"), false, Err(&err), 3);
        assert_eq!(out, "Date: Friday, 23 October 2026.\nHistorical events are unavailable right now.");
    }

    /// `cargo test -- --ignored live --nocapture`
    #[tokio::test]
    #[ignore]
    async fn on_this_day_live() {
        let mut ctx = SkillContext::for_tests();
        ctx.config_dir = std::env::current_dir().unwrap();
        let out = create(&toml::Table::new(), &ctx).unwrap().call(json!({})).await.unwrap();
        println!("{out}");
        assert!(out.contains("source: Wikipedia"), "{out}");
    }
}
