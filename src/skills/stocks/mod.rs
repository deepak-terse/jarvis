//! `stocks` skill: live price and day change for a company or index, named as the
//! user says it. Indian exchanges are preferred by default.

mod symbols;
mod twelve_data;

use crate::core::contracts::settings as config;
use crate::skills::Skill;
use crate::skills::support::http::ApiKey;
use crate::skills::support::speech::{as_of, change_percent, number};
use crate::skills::{Policy, SkillContext};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use symbols::{Listing, Symbols, best_match};
use twelve_data::{Quote, TwelveData};

#[derive(Deserialize)]
struct Settings {
    #[serde(default = "d_key_env")]
    api_key_env: String,
    #[serde(default = "d_base_url")]
    base_url: String,
    /// Preferred exchanges, best first.
    #[serde(default = "d_exchanges")]
    exchanges: Vec<String>,
    /// Allow listings on other exchanges when none of the preferred ones match (e.g. "Apple").
    #[serde(default = "yes")]
    any_exchange: bool,
    /// Spoken name → `SYMBOL` or `SYMBOL:EXCHANGE`; checked before searching.
    #[serde(default)]
    aliases: BTreeMap<String, String>,
}

fn d_key_env() -> String {
    "TWELVE_DATA_API_KEY".into()
}
fn d_base_url() -> String {
    "https://api.twelvedata.com".into()
}
fn d_exchanges() -> Vec<String> {
    vec!["NSE".into(), "BSE".into()]
}
fn yes() -> bool {
    true
}

pub fn create(settings: &toml::Table, ctx: &SkillContext) -> Result<Arc<dyn Skill>> {
    let s: Settings = config::settings(settings)?;
    let default_exchange = s.exchanges.first().context("exchanges must not be empty")?;
    let symbols = Symbols::load(ctx.state_dir.join("stock_symbols.toml"), &s.aliases, default_exchange)?;
    let api = TwelveData::new(ctx.http.clone(), ApiKey::from_env(&s.api_key_env, "stocks"), &s.base_url);
    Ok(Arc::new(Stocks { api, symbols, exchanges: s.exchanges, any_exchange: s.any_exchange }))
}

struct Stocks {
    api: TwelveData,
    symbols: Symbols,
    exchanges: Vec<String>,
    any_exchange: bool,
}

impl Stocks {
    async fn resolve(&self, company: &str) -> Result<Listing> {
        if let Some(listing) = self.symbols.lookup(company) {
            return Ok(listing);
        }
        let matches = self.api.search(company).await?;
        let best = best_match(company, &matches, &self.exchanges, self.any_exchange)
            .with_context(|| format!("no listing found for '{company}'"))?;
        let listing = Listing { symbol: best.symbol.clone(), exchange: best.exchange.clone() };
        self.symbols.learn(company, listing.clone());
        Ok(listing)
    }
}

#[async_trait]
impl Skill for Stocks {
    fn name(&self) -> &str {
        "stocks"
    }

    fn description(&self) -> &str {
        "Live share price and today's change for a company or a market index (e.g. Reliance, Infosys, Nifty, Sensex)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "company": { "type": "string", "description": "Company or index name as the user said it, e.g. \"Tata Motors\" or \"Nifty\"" } },
            "required": ["company"]
        })
    }

    fn policy(&self) -> Policy {
        Policy { timeout: Duration::from_secs(6), cache_ttl: Duration::from_secs(60) }
    }

    fn fallback_query(&self, args: &Value) -> Option<String> {
        args["company"].as_str().map(|c| format!("{c} share price today"))
    }

    async fn call(&self, args: Value) -> Result<String> {
        let company = args["company"].as_str().map(str::trim).filter(|c| !c.is_empty());
        let company = company.context("missing 'company' argument")?;
        let listing = self.resolve(company).await?;
        let quote = self.api.quote(&listing.symbol, &listing.exchange).await?;
        if quote.close <= 0.0 {
            bail!("no price available for {}", listing.symbol);
        }
        Ok(format_quote(&quote, Local::now()))
    }
}

fn format_quote(q: &Quote, now: DateTime<Local>) -> String {
    let price = match currency_word(&q.currency) {
        Some(word) => format!("{} {word}", number(q.close, 2)),
        None if q.currency.is_empty() => number(q.close, 2), // indices have no currency
        None => format!("{} {}", number(q.close, 2), q.currency),
    };
    let points = if q.change == 0.0 {
        String::new()
    } else {
        format!(", {}{}", if q.change > 0.0 { "+" } else { "" }, number(q.change, 2))
    };
    let when = q
        .timestamp
        .and_then(|t| DateTime::from_timestamp(t, 0))
        .map(|t| format!(", as of {}", as_of(&t, &now)))
        .unwrap_or_default();
    let market = if q.is_market_open { "Market open" } else { "Market closed" };
    format!(
        "{} ({} on {}): {price}, {} today{points}. {market}{when}. Source: Twelve Data.",
        q.name,
        q.symbol,
        q.exchange,
        change_percent(q.percent_change)
    )
}

fn currency_word(code: &str) -> Option<&'static str> {
    Some(match code {
        "INR" => "rupees",
        "USD" => "US dollars",
        "EUR" => "euros",
        "GBP" => "pounds",
        "JPY" => "yen",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_reads_naturally() {
        let now = Local::now();
        let q: Quote = serde_json::from_value(json!({
            "symbol": "RELIANCE", "name": "Reliance Industries Limited", "exchange": "NSE", "mic_code": "XNSE",
            "currency": "INR", "datetime": "2026-10-23", "timestamp": now.timestamp(),
            "open": "2920.00", "close": "2950.40", "change": "35.15", "percent_change": "1.2057",
            "is_market_open": false
        }))
        .unwrap();
        assert_eq!(
            format_quote(&q, now),
            format!(
                "Reliance Industries Limited (RELIANCE on NSE): 2,950.4 rupees, up 1.21 percent today, +35.15. \
                 Market closed, as of {}. Source: Twelve Data.",
                now.format("%-I:%M %p")
            )
        );
    }

    #[test]
    fn index_has_no_currency() {
        let q: Quote = serde_json::from_value(json!({
            "symbol": "NSEI", "name": "NIFTY 50", "exchange": "NSE", "close": 24512.3,
            "change": "-80.5", "percent_change": "-0.33"
        }))
        .unwrap();
        assert_eq!(
            format_quote(&q, Local::now()),
            "NIFTY 50 (NSEI on NSE): 24,512.3, down 0.33 percent today, -80.5. Market closed. Source: Twelve Data."
        );
    }

    /// Needs TWELVE_DATA_API_KEY. Checks NSE stock, an index and symbol search on your plan.
    /// `cargo test -- --ignored live --nocapture`
    #[tokio::test]
    #[ignore]
    async fn twelve_data_stocks_live() {
        let cfg: toml::Table = toml::from_str(&std::fs::read_to_string("config.toml").unwrap()).unwrap();
        let table = cfg["skills"]["stocks"].as_table().unwrap();
        let mut ctx = SkillContext::for_tests();
        ctx.state_dir = tempfile::tempdir().unwrap().keep();
        let skill = create(table, &ctx).unwrap();
        for company in ["Reliance Industries", "Infosys", "Nifty", "Sensex", "Apple"] {
            println!("{company}: {:?}", skill.call(json!({ "company": company })).await);
        }
    }
}
