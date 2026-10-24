//! Wikipedia: relevance search for the best page, then its REST summary.

use crate::skills::common::http::fetch_json;
use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    pages: Vec<Page>,
}

#[derive(Deserialize)]
struct Page {
    key: String,
}

#[derive(Debug, Deserialize)]
pub struct Summary {
    #[serde(rename = "type", default)]
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub extract: String,
}

pub struct Wikipedia {
    client: reqwest::Client,
    /// e.g. `https://en.wikipedia.org`
    base: String,
}

impl Wikipedia {
    pub fn new(client: reqwest::Client, language: &str) -> Self {
        Self { client, base: format!("https://{language}.wikipedia.org") }
    }

    pub async fn summary(&self, topic: &str) -> Result<Summary> {
        let search =
            self.client.get(format!("{}/w/rest.php/v1/search/page", self.base)).query(&[("q", topic), ("limit", "1")]);
        let key = fetch_json::<Search>(search, "Wikipedia").await?.pages.into_iter().next();
        let key = key.with_context(|| format!("no Wikipedia article found for '{topic}'"))?.key;

        let mut url = reqwest::Url::parse(&format!("{}/api/rest_v1/page/summary", self.base))?;
        url.path_segments_mut().map_err(|_| anyhow::anyhow!("bad Wikipedia URL"))?.push(&key);
        let summary: Summary = fetch_json(self.client.get(url), "Wikipedia").await?;
        if summary.kind == "disambiguation" {
            bail!("'{topic}' is ambiguous on Wikipedia");
        }
        if summary.extract.trim().is_empty() {
            bail!("Wikipedia article '{}' has no summary", summary.title);
        }
        Ok(summary)
    }
}
