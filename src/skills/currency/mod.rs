//! `currency` skill: convert an amount between currencies at the live rate.

mod twelve_data;

use crate::core::contracts::settings as config;
use crate::skills::Skill;
use crate::skills::support::http::ApiKey;
use crate::skills::support::speech::{as_of, number};
use crate::skills::{Policy, SkillContext};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use twelve_data::{Conversion, TwelveData};

#[derive(Deserialize)]
struct Settings {
    #[serde(default = "d_key_env")]
    api_key_env: String,
    #[serde(default = "d_base_url")]
    base_url: String,
    /// Target currency when the user doesn't name one.
    #[serde(default = "d_home_currency")]
    home_currency: String,
}

fn d_key_env() -> String {
    "TWELVE_DATA_API_KEY".into()
}
fn d_base_url() -> String {
    "https://api.twelvedata.com".into()
}
fn d_home_currency() -> String {
    "INR".into()
}

pub fn create(settings: &toml::Table, ctx: &SkillContext) -> Result<Arc<dyn Skill>> {
    let s: Settings = config::settings(settings)?;
    let api = TwelveData::new(ctx.http.clone(), ApiKey::from_env(&s.api_key_env, "currency"), &s.base_url);
    let home = parse_code(&s.home_currency).context("home_currency must be a 3-letter ISO code")?;
    let description = format!("Convert money between currencies at the live exchange rate (default target {home}).");
    Ok(Arc::new(Currency { api, home, description }))
}

struct Currency {
    api: TwelveData,
    home: String,
    description: String,
}

#[async_trait]
impl Skill for Currency {
    fn name(&self) -> &str {
        "currency"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "amount": { "type": "number", "description": "Amount to convert; 1 for just the rate" },
                "from": { "type": "string", "description": "ISO currency code, e.g. USD for dollars, EUR for euros, GBP for pounds" },
                "to": { "type": "string", "description": format!("ISO currency code; defaults to {}", self.home) }
            },
            "required": ["from"]
        })
    }

    fn policy(&self) -> Policy {
        Policy { timeout: Duration::from_secs(5), cache_ttl: Duration::from_secs(600) }
    }

    fn fallback_query(&self, args: &Value) -> Option<String> {
        let from = args["from"].as_str()?;
        Some(format!("{from} to {} exchange rate today", args["to"].as_str().unwrap_or(&self.home)))
    }

    async fn call(&self, args: Value) -> Result<String> {
        let amount = match &args["amount"] {
            Value::Null => 1.0,
            v => v
                .as_f64()
                .or_else(|| v.as_str()?.replace(',', "").trim().parse().ok())
                .context("'amount' must be a number")?,
        };
        let from = args["from"].as_str().and_then(parse_code).context("'from' must be a 3-letter currency code")?;
        let to = match args["to"].as_str() {
            Some(t) => parse_code(t).context("'to' must be a 3-letter currency code")?,
            None => self.home.clone(),
        };
        if from == to {
            bail!("'from' and 'to' are both {from}");
        }
        let conversion = self.api.convert(amount, &from, &to).await?;
        Ok(format_conversion(amount, &from, &to, &conversion, Local::now()))
    }
}

fn parse_code(code: &str) -> Option<String> {
    let code = code.trim().to_uppercase();
    (code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic())).then_some(code)
}

fn format_conversion(amount: f64, from: &str, to: &str, c: &Conversion, now: DateTime<Local>) -> String {
    let when = c
        .timestamp
        .and_then(|t| DateTime::from_timestamp(t, 0))
        .map(|t| format!(", as of {}", as_of(&t, &now)))
        .unwrap_or_default();
    format!(
        "{} {from} = {} {to} (rate {}{when}). Source: Twelve Data.",
        number(amount, 2),
        number(c.amount, 2),
        number(c.rate, 4)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_reads_naturally() {
        let now = Local::now();
        let c: Conversion = serde_json::from_value(
            json!({"symbol": "USD/INR", "rate": 84.0712, "amount": 8407.12, "timestamp": now.timestamp()}),
        )
        .unwrap();
        assert_eq!(
            format_conversion(100.0, "USD", "INR", &c, now),
            format!("100 USD = 8,407.12 INR (rate 84.0712, as of {}). Source: Twelve Data.", now.format("%-I:%M %p"))
        );
    }

    #[tokio::test]
    async fn validates_codes_before_calling_the_api() {
        let skill = create(&toml::Table::new(), &SkillContext::for_tests()).unwrap();
        let err = |args| async { skill.call(args).await.unwrap_err().to_string() };
        assert_eq!(err(json!({"from": "dollars"})).await, "'from' must be a 3-letter currency code");
        assert_eq!(err(json!({"from": "inr"})).await, "'from' and 'to' are both INR");
        assert_eq!(err(json!({"from": "USD", "amount": "lots"})).await, "'amount' must be a number");
        assert_eq!(skill.fallback_query(&json!({"from": "USD"})).unwrap(), "USD to INR exchange rate today");
    }

    /// Needs TWELVE_DATA_API_KEY. `cargo test -- --ignored live --nocapture`
    #[tokio::test]
    #[ignore]
    async fn twelve_data_currency_live() {
        let skill = create(&toml::Table::new(), &SkillContext::for_tests()).unwrap();
        let out = skill.call(json!({"amount": 100, "from": "USD"})).await.unwrap();
        println!("{out}");
        assert!(out.contains("INR"));
    }
}
