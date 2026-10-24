//! HTTP helpers shared by skill providers: API keys from the environment and
//! JSON requests with readable errors.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// An API key read from an environment variable named in config. A missing key is
/// a warning at startup and a clear error on use, so other skills keep working.
pub struct ApiKey {
    env: String,
    value: Option<String>,
}

impl ApiKey {
    pub fn from_env(env: &str, skill: &str) -> Self {
        let value = std::env::var(env).ok().filter(|v| !v.trim().is_empty());
        if value.is_none() {
            eprintln!("warning: {env} is not set; {skill} will fall back or fail");
        }
        Self { env: env.into(), value }
    }

    pub fn get(&self) -> Result<&str> {
        self.value.as_deref().with_context(|| format!("{} is not set", self.env))
    }
}

/// Send a request and decode a JSON body; non-2xx statuses become `"<source> returned <status>: <body>"`.
pub async fn fetch_json<T: DeserializeOwned>(req: reqwest::RequestBuilder, source: &str) -> Result<T> {
    let resp = req.send().await.with_context(|| format!("{source} request failed"))?;
    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        bail!("{source} returned {status}: {}", super::speech::truncate(detail.trim(), 200));
    }
    resp.json::<T>().await.with_context(|| format!("unexpected {source} response"))
}

/// Deserialize a number some APIs send as a JSON string (`"2950.40"`) or number.
pub fn lenient_f64<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        N(f64),
        S(String),
    }
    match Num::deserialize(d)? {
        Num::N(n) => Ok(n),
        Num::S(s) => s.trim().parse().map_err(serde::de::Error::custom),
    }
}
