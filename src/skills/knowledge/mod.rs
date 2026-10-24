//! `knowledge` skill: "who is X" / "what is Y" from Wikipedia summaries, which are
//! faster and more reliable than search snippets for encyclopedic facts.

mod wikipedia;

use crate::core::contracts::settings as config;
use crate::skills::Skill;
use crate::skills::support::speech::sentences;
use crate::skills::{Policy, SkillContext};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use wikipedia::{Summary, Wikipedia};

#[derive(Deserialize)]
struct Settings {
    /// Wikipedia language edition.
    #[serde(default = "d_language")]
    language: String,
    #[serde(default = "d_max_sentences")]
    max_sentences: usize,
}

fn d_language() -> String {
    "en".into()
}
fn d_max_sentences() -> usize {
    3
}

pub fn create(settings: &toml::Table, ctx: &SkillContext) -> Result<Arc<dyn Skill>> {
    let settings: Settings = config::settings(settings)?;
    let wiki = Wikipedia::new(ctx.http.clone(), &settings.language);
    Ok(Arc::new(Knowledge { wiki, max_sentences: settings.max_sentences }))
}

struct Knowledge {
    wiki: Wikipedia,
    max_sentences: usize,
}

#[async_trait]
impl Skill for Knowledge {
    fn name(&self) -> &str {
        "knowledge"
    }

    fn description(&self) -> &str {
        "Look up a person, place, organisation, thing or concept on Wikipedia. Use it for every \"who is\", \
         \"who was\", \"what is\" or \"tell me about\" question, so the answer is accurate and up to date."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "topic": { "type": "string", "description": "The person, place or thing, e.g. \"Ratan Tata\" or \"black hole\"" } },
            "required": ["topic"]
        })
    }

    fn policy(&self) -> Policy {
        Policy { timeout: Duration::from_secs(5), cache_ttl: Duration::from_secs(24 * 3600) }
    }

    fn fallback_query(&self, args: &Value) -> Option<String> {
        args["topic"].as_str().map(|t| format!("who or what is {t}"))
    }

    async fn call(&self, args: Value) -> Result<String> {
        let topic = args["topic"].as_str().filter(|t| !t.trim().is_empty()).context("missing 'topic' argument")?;
        Ok(format_summary(&self.wiki.summary(topic.trim()).await?, self.max_sentences))
    }
}

fn format_summary(s: &Summary, max_sentences: usize) -> String {
    let about = s.description.as_deref().map(|d| format!(" ({d})")).unwrap_or_default();
    format!("Source: Wikipedia, article \"{}\"{about}.\n{}", s.title, sentences(s.extract.trim(), max_sentences))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_is_short_and_attributed() {
        let s: Summary = serde_json::from_value(json!({
            "type": "standard",
            "title": "Ratan Tata",
            "description": "Indian industrialist (1937–2024)",
            "extract": "Ratan Naval Tata was an Indian industrialist and philanthropist. He was chairman of Tata Group. He died in 2024. More text.",
            "content_urls": {"desktop": {"page": "https://en.wikipedia.org/wiki/Ratan_Tata"}}
        }))
        .unwrap();
        let out = format_summary(&s, 2);
        assert_eq!(
            out,
            "Source: Wikipedia, article \"Ratan Tata\" (Indian industrialist (1937–2024)).\n\
             Ratan Naval Tata was an Indian industrialist and philanthropist. He was chairman of Tata Group."
        );
    }

    /// `cargo test -- --ignored live --nocapture`
    #[tokio::test]
    #[ignore]
    async fn wikipedia_live() {
        let skill = create(&toml::Table::new(), &SkillContext::for_tests()).unwrap();
        let out = skill.call(json!({"topic": "Ratan Tata"})).await.unwrap();
        println!("{out}");
        assert!(out.contains("Tata"));
    }
}
