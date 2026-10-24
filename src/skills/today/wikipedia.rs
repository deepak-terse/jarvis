//! Wikipedia "On this day": editor-selected historical events for a date.

use crate::skills::support::http::fetch_json;
use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OnThisDay {
    #[serde(default)]
    pub selected: Vec<Event>,
}

#[derive(Debug, Deserialize)]
pub struct Event {
    pub text: String,
    pub year: Option<i32>,
}

pub struct Wikipedia {
    client: reqwest::Client,
    language: String,
}

impl Wikipedia {
    pub fn new(client: reqwest::Client, language: &str) -> Self {
        Self { client, language: language.into() }
    }

    pub async fn on_this_day(&self, date: NaiveDate) -> Result<OnThisDay> {
        let url = format!(
            "https://{}.wikipedia.org/api/rest_v1/feed/onthisday/selected/{:02}/{:02}",
            self.language,
            date.month(),
            date.day()
        );
        fetch_json(self.client.get(url), "Wikipedia").await
    }
}
