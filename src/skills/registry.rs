//! Config names → skills. Builds every enabled `[skills.<name>]` section, wraps each
//! in its execution guard, and serves them to the agent as `Tools`.

use super::execution::{CommonSettings, Guarded};
use super::{Skill, SkillContext, ToolOutcome};
use super::{currency, knowledge, news, stocks, today, weather, web_search};
use crate::core::contracts::settings as config;
use crate::core::contracts::tools::{ToolSpec, Tools};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The skill other skills fall back to when their own source fails.
const FALLBACK_SKILL: &str = "web_search";

/// Maps config names to implementations. A new skill adds one line here.
fn create(name: &str, settings: &toml::Table, ctx: &SkillContext) -> Result<Arc<dyn Skill>> {
    match name {
        "web_search" => web_search::create(settings, ctx),
        "news" => news::create(settings, ctx),
        "knowledge" => knowledge::create(settings, ctx),
        "about_today" => today::create(settings, ctx),
        "stocks" => stocks::create(settings, ctx),
        "currency" => currency::create(settings, ctx),
        "weather" => weather::create(settings, ctx),
        _ => bail!("unknown skill '{name}'"),
    }
}

/// Cheap to clone: skills are shared.
#[derive(Default, Clone)]
pub struct Registry {
    skills: BTreeMap<String, Arc<Guarded>>,
}

impl Registry {
    pub fn from_config(cfg: &BTreeMap<String, toml::Table>, ctx: &SkillContext) -> Result<Self> {
        let mut enabled = Vec::new();
        for (name, table) in cfg {
            let common: CommonSettings = config::settings(table).with_context(|| format!("[skills.{name}]"))?;
            if common.enabled() {
                let skill = create(name, table, ctx).with_context(|| format!("[skills.{name}]"))?;
                enabled.push((name.clone(), skill, common));
            }
        }
        let guard = |skill: &Arc<dyn Skill>, common: &CommonSettings, fallback| {
            Arc::new(Guarded::new(skill.clone(), common.description.clone(), common.policy(skill.policy()), fallback))
        };
        let web = enabled.iter().find(|(n, ..)| n == FALLBACK_SKILL).map(|(_, s, c)| guard(s, c, None));
        let mut skills = BTreeMap::new();
        for (name, skill, common) in &enabled {
            let guarded = match &web {
                Some(web) if name == FALLBACK_SKILL => web.clone(),
                _ => guard(skill, common, web.clone().filter(|_| common.fallback != Some(false))),
            };
            skills.insert(name.clone(), guarded);
        }
        Ok(Self { skills })
    }

    /// Run a skill directly (startup greeting and other internal callers); `None` if it isn't enabled.
    pub async fn invoke(&self, name: &str, args: Value) -> Option<ToolOutcome> {
        Some(self.skills.get(name)?.run(args).await)
    }

    fn allowed<'a>(&'a self, allowed: &'a [String]) -> impl Iterator<Item = &'a Arc<Guarded>> {
        allowed.iter().filter_map(|n| self.skills.get(n))
    }
}

#[async_trait]
impl Tools for Registry {
    fn contains(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    fn specs(&self, allowed: &[String]) -> Vec<ToolSpec> {
        self.allowed(allowed)
            .map(|s| ToolSpec {
                name: s.name().into(),
                description: s.description().into(),
                parameters: s.parameters(),
            })
            .collect()
    }

    async fn call(&self, allowed: &[String], name: &str, args: Value) -> ToolOutcome {
        match self.allowed(allowed).find(|s| s.name() == name) {
            Some(skill) => skill.run(args).await,
            None => ToolOutcome::error(format!("Error: unknown tool '{name}'")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The shipped config.toml, with API keys pointed at unset variables.
    fn shipped_config() -> BTreeMap<String, toml::Table> {
        let cfg: toml::Table = toml::from_str(&std::fs::read_to_string("config.toml").unwrap()).unwrap();
        let mut skills: BTreeMap<String, toml::Table> = cfg["skills"].clone().try_into().unwrap();
        for table in skills.values_mut().filter(|t| t.contains_key("api_key_env")) {
            table.insert("api_key_env".into(), "JARVIS_TEST_UNSET_KEY".into());
        }
        skills
    }

    #[tokio::test]
    async fn shipped_config_builds_every_skill() {
        let registry = Registry::from_config(&shipped_config(), &SkillContext::for_tests()).unwrap();
        let persona: toml::Table = toml::from_str(&std::fs::read_to_string("personas/jarvis.toml").unwrap()).unwrap();
        let allowed: Vec<String> = persona["skills"].clone().try_into().unwrap();
        let names: Vec<String> = registry.specs(&allowed).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["web_search", "news", "knowledge", "about_today", "stocks", "currency", "weather"]);
    }

    #[tokio::test]
    async fn failing_skill_falls_back_then_reports_honestly() {
        let registry = Registry::from_config(&shipped_config(), &SkillContext::for_tests()).unwrap();
        let all = vec!["stocks".to_string()];
        // No Twelve Data key → fallback to web_search, which has no key either → honest error.
        let out = registry.call(&all, "stocks", json!({"company": "Reliance"})).await;
        assert!(!out.ok && !out.fallback, "{out:?}");
        assert!(out.text.contains("do not guess"), "{}", out.text);
        // Personas can only call the tools they list.
        assert!(registry.call(&all, "currency", json!({"from": "USD"})).await.text.contains("unknown tool"));
    }

    #[tokio::test]
    async fn config_disables_and_overrides() {
        let mut cfg = shipped_config();
        cfg.get_mut("currency").unwrap().insert("enabled".into(), false.into());
        cfg.get_mut("knowledge").unwrap().insert("description".into(), "Custom wording.".into());
        let registry = Registry::from_config(&cfg, &SkillContext::for_tests()).unwrap();
        assert!(!registry.contains("currency"));
        assert_eq!(registry.specs(&["knowledge".into()])[0].description, "Custom wording.");
        cfg.insert("nonsense".into(), toml::Table::new());
        let err = Registry::from_config(&cfg, &SkillContext::for_tests()).err().unwrap();
        assert_eq!(format!("{err:#}"), "[skills.nonsense]: unknown skill 'nonsense'");
    }
}
