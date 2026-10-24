//! Twelve Data: symbol search and real-time quotes. Errors arrive as HTTP 200 with
//! `{"status": "error", "code", "message"}`, so the body is checked too.

use crate::skills::support::http::{ApiKey, fetch_json, lenient_f64};
use anyhow::{Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Match {
    pub symbol: String,
    pub instrument_name: String,
    pub exchange: String,
    #[serde(default)]
    pub instrument_type: String,
}

#[derive(Deserialize)]
struct Matches {
    #[serde(default)]
    data: Vec<Match>,
}

#[derive(Debug, Deserialize)]
pub struct Quote {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    #[serde(default)]
    pub currency: String,
    /// Unix seconds of the last update.
    pub timestamp: Option<i64>,
    #[serde(deserialize_with = "lenient_f64")]
    pub close: f64,
    #[serde(deserialize_with = "lenient_f64")]
    pub change: f64,
    #[serde(deserialize_with = "lenient_f64")]
    pub percent_change: f64,
    #[serde(default)]
    pub is_market_open: bool,
}

pub struct TwelveData {
    client: reqwest::Client,
    key: ApiKey,
    base: String,
}

impl TwelveData {
    pub fn new(client: reqwest::Client, key: ApiKey, base: &str) -> Self {
        Self { client, key, base: base.trim_end_matches('/').into() }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Match>> {
        Ok(self.get::<Matches>("symbol_search", &[("symbol", query), ("outputsize", "20")]).await?.data)
    }

    pub async fn quote(&self, symbol: &str, exchange: &str) -> Result<Quote> {
        self.get("quote", &[("symbol", symbol), ("exchange", exchange)]).await
    }

    async fn get<T: DeserializeOwned>(&self, endpoint: &str, params: &[(&str, &str)]) -> Result<T> {
        let req =
            self.client.get(format!("{}/{endpoint}", self.base)).query(params).query(&[("apikey", self.key.get()?)]);
        let body: Value = fetch_json(req, "Twelve Data").await?;
        if body["status"] == "error" {
            bail!("Twelve Data error {}: {}", body["code"], body["message"].as_str().unwrap_or("unknown"));
        }
        Ok(serde_json::from_value(body)?)
    }
}
