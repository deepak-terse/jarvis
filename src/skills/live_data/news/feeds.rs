//! RSS/Atom feeds kept fresh in the background, so headline questions never wait
//! on the network. Each feed keeps its last good items if a refresh fails.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::watch;

/// A feed in config: a bare URL, or `{ url, name }` to override the source name.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FeedSpec {
    Url(String),
    Named { url: String, name: String },
}

impl FeedSpec {
    pub fn url(&self) -> &str {
        match self {
            Self::Url(url) | Self::Named { url, .. } => url,
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            Self::Url(_) => None,
            Self::Named { name, .. } => Some(name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Headline {
    pub title: String,
    pub source: String,
    pub published: Option<DateTime<Utc>>,
}

pub struct FeedSettings {
    pub refresh: Duration,
    pub fetch_timeout: Duration,
    pub stale_after: chrono::Duration,
}

/// Category → feeds, plus the latest items per feed URL.
pub struct Feeds {
    categories: BTreeMap<String, Vec<FeedSpec>>,
    items: RwLock<HashMap<String, Vec<Headline>>>,
    ready: watch::Receiver<bool>,
}

impl Feeds {
    /// Start refreshing in the background (needs a Tokio runtime).
    pub fn spawn(client: reqwest::Client, categories: BTreeMap<String, Vec<FeedSpec>>, s: FeedSettings) -> Arc<Self> {
        let (ready_tx, ready) = watch::channel(false);
        let feeds = Arc::new(Self { categories, items: RwLock::default(), ready });
        let weak = Arc::downgrade(&feeds);
        tokio::spawn(async move {
            // Stops once the skill (and with it the last strong reference) is dropped.
            while let Some(feeds) = weak.upgrade() {
                feeds.refresh(&client, &s).await;
                let _ = ready_tx.send(true);
                drop(feeds);
                tokio::time::sleep(s.refresh).await;
            }
        });
        feeds
    }

    pub fn categories(&self) -> impl Iterator<Item = &str> {
        self.categories.keys().map(String::as_str)
    }

    /// Resolves once the first refresh has finished (instantly afterwards).
    pub async fn ready(&self) {
        let _ = self.ready.clone().wait_for(|r| *r).await;
    }

    /// Newest-first, de-duplicated headlines for a category, ignoring items older than `max_age`.
    pub fn headlines(&self, category: &str, max_age: chrono::Duration, now: DateTime<Utc>) -> Vec<Headline> {
        let items = self.items.read().unwrap();
        let mut all: Vec<Headline> = self
            .categories
            .get(category)
            .into_iter()
            .flatten()
            .filter_map(|f| items.get(f.url()))
            .flatten()
            .filter(|h| h.published.is_none_or(|p| now - p <= max_age))
            .cloned()
            .collect();
        all.sort_by_key(|h| std::cmp::Reverse(h.published)); // newest first, undated last
        dedupe(all)
    }

    async fn refresh(&self, client: &reqwest::Client, s: &FeedSettings) {
        let unique: HashMap<&str, &FeedSpec> = self.categories.values().flatten().map(|f| (f.url(), f)).collect();
        let mut tasks = tokio::task::JoinSet::new();
        for spec in unique.into_values() {
            let (client, spec, timeout) = (client.clone(), spec.clone(), s.fetch_timeout);
            tasks.spawn(async move {
                let result = fetch(&client, &spec, timeout).await;
                (spec, result)
            });
        }
        let now = Utc::now();
        while let Some(Ok((spec, result))) = tasks.join_next().await {
            match result {
                Ok(items) => {
                    let newest = items.iter().filter_map(|h| h.published).max();
                    if newest.is_some_and(|n| now - n > s.stale_after) {
                        tracing::warn!(feed = spec.url(), "news feed looks stale: newest item {}", newest.unwrap());
                    } else if items.is_empty() {
                        tracing::warn!(feed = spec.url(), "news feed returned no items");
                    }
                    self.items.write().unwrap().insert(spec.url().to_string(), items);
                }
                Err(e) => tracing::warn!(feed = spec.url(), "news feed failed: {e:#}"),
            }
        }
    }
}

async fn fetch(client: &reqwest::Client, spec: &FeedSpec, timeout: Duration) -> anyhow::Result<Vec<Headline>> {
    let resp = client.get(spec.url()).timeout(timeout).send().await?.error_for_status()?;
    let feed = feed_rs::parser::parse(resp.bytes().await?.as_ref())?;
    let source = spec
        .name()
        .map(str::to_string)
        .or_else(|| feed.title.as_ref().map(|t| clean(&t.content)))
        .unwrap_or_else(|| spec.url().to_string());
    Ok(feed
        .entries
        .into_iter()
        .filter_map(|e| {
            let title = clean(&e.title?.content);
            (!title.is_empty()).then(|| Headline {
                title,
                source: source.clone(),
                published: e.published.or(e.updated),
            })
        })
        .collect())
}

/// Strip markup and collapse whitespace in feed text.
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Drop stories already covered by an earlier (newer) headline: the same story from
/// two outlets shares most of its significant words.
fn dedupe(headlines: Vec<Headline>) -> Vec<Headline> {
    let mut kept: Vec<(Headline, HashSet<String>)> = Vec::new();
    for h in headlines {
        let words = significant_words(&h.title);
        let duplicate = kept.iter().any(|(_, seen)| {
            let shared = words.intersection(seen).count();
            let smaller = words.len().min(seen.len());
            smaller > 0 && (shared == smaller || (smaller >= 3 && shared as f64 / smaller as f64 >= 0.6))
        });
        if !duplicate {
            kept.push((h, words));
        }
    }
    kept.into_iter().map(|(h, _)| h).collect()
}

fn significant_words(title: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "from", "that", "this", "into", "over", "after", "amid", "says", "said", "will",
        "has", "have", "are", "was", "its", "his", "her", "who", "what", "why", "how", "new", "news", "live",
        "updates",
    ];
    title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP.contains(w))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(title: &str, source: &str, hours_ago: i64) -> Headline {
        Headline {
            title: title.into(),
            source: source.into(),
            published: Some(Utc::now() - chrono::Duration::hours(hours_ago)),
        }
    }

    #[test]
    fn removes_cross_outlet_duplicates() {
        let out = dedupe(vec![
            h("RBI keeps repo rate unchanged at 6.5% for eighth time", "ET", 1),
            h("RBI keeps repo rate unchanged at 6.5 per cent", "Hindu", 2),
            h("India beat Australia by 6 wickets in Perth Test", "Cricinfo", 2),
            h("Sensex rises 400 points", "Mint", 3),
        ]);
        let titles: Vec<_> = out.iter().map(|h| h.source.as_str()).collect();
        assert_eq!(titles, ["ET", "Cricinfo", "Mint"]);
    }

    #[test]
    fn parses_rss_and_strips_markup() {
        let xml = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Test  Feed</title>
            <item><title><![CDATA[<b>Big</b>   story]]></title><pubDate>Fri, 23 Oct 2026 08:00:00 GMT</pubDate></item>
            <item><title></title></item></channel></rss>"#;
        let feed = feed_rs::parser::parse(xml.as_bytes()).unwrap();
        assert_eq!(clean(&feed.title.unwrap().content), "Test Feed");
        assert_eq!(clean(&feed.entries[0].title.as_ref().unwrap().content), "Big story");
    }

    #[test]
    fn feed_spec_accepts_url_or_named_table() {
        #[derive(Deserialize)]
        struct C {
            feeds: Vec<FeedSpec>,
        }
        let c: C =
            toml::from_str(r#"feeds = ["https://a.example/rss", { url = "https://b.example/rss", name = "B News" }]"#)
                .unwrap();
        assert_eq!((c.feeds[0].url(), c.feeds[0].name()), ("https://a.example/rss", None));
        assert_eq!((c.feeds[1].url(), c.feeds[1].name()), ("https://b.example/rss", Some("B News")));
    }
}
