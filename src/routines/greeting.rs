//! Startup greeting: a notable historical event and a few top headlines.
//! Built in the background from the `about_today` and `news` skills with a fixed
//! template (no LLM round), so it never delays startup or competes with a turn.

use crate::skills::{Registry, ToolOutcome};
use chrono::{Local, Timelike};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::oneshot;

/// The `[greeting]` config section.
#[derive(Debug, Clone, Deserialize)]
pub struct GreetingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "d_headlines")]
    pub headlines: usize,
    /// News category to take headlines from (empty: the news skill's default).
    #[serde(default)]
    pub news_category: String,
}

impl Default for GreetingConfig {
    fn default() -> Self {
        Self { enabled: false, headlines: d_headlines(), news_category: String::new() }
    }
}

fn d_headlines() -> usize {
    3
}

/// Start preparing the greeting; poll the receiver with `try_recv` when ready to use it.
pub fn spawn(skills: Registry, cfg: &GreetingConfig, user_name: Option<String>) -> Option<oneshot::Receiver<String>> {
    if !cfg.enabled {
        return None;
    }
    let (tx, rx) = oneshot::channel();
    let (headlines, category) = (cfg.headlines, cfg.news_category.clone());
    tokio::spawn(async move {
        let news_args = if category.is_empty() { json!({}) } else { json!({ "category": category }) };
        let (today, news, weather) = tokio::join!(
            skills.invoke("about_today", json!({})),
            skills.invoke("news", news_args),
            skills.invoke("weather", json!({"when":"today", "highlights_only":true}))
        );
        let mut text = compose(user_name.as_deref(), Local::now().hour(), usable(today), usable(news), headlines);
        if let Some(line) = usable(weather).and_then(|w| weather_line(&w)) {
            text += &format!(" {line}");
        }
        let _ = tx.send(text);
    });
    Some(rx)
}

/// Only first-hand results: skip failures and web-search fallbacks.
fn usable(outcome: Option<ToolOutcome>) -> Option<String> {
    outcome.filter(|o| o.ok && !o.fallback).map(|o| o.text)
}

/// A single short line, only for first-hand significant weather. The normal tool
/// response stays structured; this template follows the rest of the greeting.
fn weather_line(text: &str) -> Option<String> {
    let data: serde_json::Value = serde_json::from_str(text).ok()?;
    if data["status"] != "ok" {
        return None;
    }
    let flags = data["flags"].as_array()?;
    let parts: Vec<String> = flags
        .iter()
        .take(2)
        .filter_map(|flag| {
            Some(match flag["kind"].as_str()? {
                "official_alert" => "an official weather alert applies today".into(),
                "heavy_rain" => "heavy rain is forecast".into(),
                "rain" => {
                    format!("rain is likely from {}", flag["first_local_time"].as_str()?.split_whitespace().last()?)
                }
                "heat" => format!(
                    "it may feel like {:.0} degrees {}",
                    flag["value"].as_f64()?,
                    if flag["unit"] == "feels_like_f" { "Fahrenheit" } else { "Celsius" }
                ),
                "poor_air_quality" => "air quality is poor on the US EPA scale".into(),
                "high_uv" => "UV is high".into(),
                _ => return None,
            })
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "Weather in {} as of {}: {}.",
        data["place"]["city"].as_str()?,
        data["as_of"].as_str()?,
        parts.join(" and ")
    ))
}

/// Reads the line formats produced by the `about_today` and `news` skills.
fn compose(name: Option<&str>, hour: u32, today: Option<String>, news: Option<String>, headlines: usize) -> String {
    let part = match hour {
        5..12 => "morning",
        12..17 => "afternoon",
        _ => "evening",
    };
    let mut out = match name {
        Some(n) => format!("Good {part}, {n}."),
        None => format!("Good {part}."),
    };
    if let Some(today) = today {
        let line = |prefix: &str| today.lines().find_map(|l| l.strip_prefix(prefix)).map(str::trim);
        if let Some(event) = line("- ") {
            out += &format!(" On this day: {event}");
        }
    }
    let titles: Vec<&str> = news
        .as_deref()
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(". ").filter(|(n, _)| n.parse::<usize>().is_ok()).map(|(_, rest)| rest))
        .map(|t| t.rsplit_once(" (").map_or(t, |(title, _)| title)) // drop "(source, age)"
        .take(headlines)
        .collect();
    if !titles.is_empty() {
        out += " In the news: ";
        out += &titles.iter().map(|t| t.trim_end_matches(['.', ' '])).collect::<Vec<_>>().join(". ");
        out.push('.');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_and_headlines() {
        let today = "Today is Sunday, 8 November 2026.\nOn this day (source: Wikipedia):\n- In 1950, A.";
        let news =
            "Top headlines:\n1. Sensex hits record (Mint, 2 hours ago)\n2. Rain in Mumbai (The Hindu)\n3. Third (X)";
        assert_eq!(
            compose(Some("Tony"), 9, Some(today.into()), Some(news.into()), 2),
            "Good morning, Tony. On this day: In 1950, A. In the news: Sensex hits record. Rain in Mumbai."
        );
    }

    #[test]
    fn degrades_gracefully() {
        let today = "Today is Friday, 23 October 2026.\nHistorical events are unavailable right now.";
        assert_eq!(compose(None, 20, Some(today.into()), None, 3), "Good evening.");
        assert_eq!(compose(None, 13, None, None, 3), "Good afternoon.");
    }
}

#[cfg(test)]
mod weather_tests {
    use super::*;
    #[test]
    fn weather_only_when_flags_exist() {
        assert!(weather_line(r#"{"status":"ok","flags":[]}"#).is_none());
        assert!(weather_line(r#"{"status":"unavailable"}"#).is_none());
        assert!(weather_line("Live source unavailable; web results: rain").is_none());
        let line = weather_line(r#"{"status":"ok","place":{"city":"Pune"},"as_of":"2026-10-23 09:00","flags":[{"kind":"heat","value":38,"unit":"feels_like_c"}]}"#).unwrap();
        assert!(line.contains("Pune") && line.contains("38 degrees Celsius") && line.contains("09:00"));
    }
}
