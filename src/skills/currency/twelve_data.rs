//! Twelve Data currency conversion. Errors arrive as HTTP 200 with
//! `{"status": "error", "code", "message"}`, so the body is checked too.

use crate::skills::support::http::{ApiKey, fetch_json, lenient_f64};
use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Conversion {
    #[serde(deserialize_with = "lenient_f64")]
    pub rate: f64,
    /// The converted amount.
    #[serde(deserialize_with = "lenient_f64")]
    pub amount: f64,
    /// Unix seconds of the rate.
    pub timestamp: Option<i64>,
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

    pub async fn convert(&self, amount: f64, from: &str, to: &str) -> Result<Conversion> {
        let req = self.client.get(format!("{}/currency_conversion", self.base)).query(&[
            ("symbol", format!("{from}/{to}")),
            ("amount", amount.to_string()),
            ("apikey", self.key.get()?.to_string()),
        ]);
        let body: Value = fetch_json(req, "Twelve Data").await?;
        if body["status"] == "error" {
            bail!("Twelve Data error {}: {}", body["code"], body["message"].as_str().unwrap_or("unknown"));
        }
        Ok(serde_json::from_value(body)?)
    }
}
