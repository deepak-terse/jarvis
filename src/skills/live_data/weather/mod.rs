//! WeatherAPI: cache the complete bundle by location; give the model only the asked window.
mod report;
mod window;

use crate::config;
use crate::skills::Skill;
use crate::skills::common::http::ApiKey;
use crate::skills::common::{Policy, SkillContext};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Deserialize)]
#[serde(default)]
struct Settings {
    api_key_env: String,
    base_url: String,
    forecast_days: u8,
    bundle_cache_ttl_secs: u64,
    units: String,
    thresholds: Thresholds,
    city_thresholds: BTreeMap<String, Thresholds>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            api_key_env: "WEATHER_API_KEY".into(),
            base_url: "https://api.weatherapi.com/v1".into(),
            forecast_days: 3,
            bundle_cache_ttl_secs: 600,
            units: "metric".into(),
            thresholds: Thresholds::default(),
            city_thresholds: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(default)]
struct Thresholds {
    rain_chance: f64,
    feels_like_c: f64,
    us_epa_category: f64,
    uv: f64,
    heavy_rain_mm_per_hour: f64,
}
impl Default for Thresholds {
    fn default() -> Self {
        Self { rain_chance: 50., feels_like_c: 35., us_epa_category: 3., uv: 6., heavy_rain_mm_per_hour: 7.5 }
    }
}

pub fn create(settings: &toml::Table, ctx: &SkillContext) -> Result<Arc<dyn Skill>> {
    let settings: Settings = config::settings(settings)?;
    anyhow::ensure!(
        (1..=14).contains(&settings.forecast_days),
        "forecast_days must be between 1 and 14 (and supported by your plan)"
    );
    for t in std::iter::once(&settings.thresholds).chain(settings.city_thresholds.values()) {
        anyhow::ensure!(
            (0. ..=100.).contains(&t.rain_chance)
                && (1. ..=6.).contains(&t.us_epa_category)
                && t.uv >= 0.
                && t.heavy_rain_mm_per_hour > 0.
                && t.feels_like_c.is_finite(),
            "invalid weather thresholds"
        );
    }
    let units = ctx.profile_str("units").unwrap_or(&settings.units).to_string();
    anyhow::ensure!(matches!(units.as_str(), "metric" | "imperial"), "weather units must be metric or imperial");
    Ok(Arc::new(Weather {
        key: ApiKey::from_env(&settings.api_key_env, "weather"),
        http: ctx.http.clone(),
        home: ctx.profile_str("location").unwrap_or_default().into(),
        country: ctx
            .profile_str("country")
            .or_else(|| ctx.profile_str("location").and_then(|s| s.rsplit_once(',').map(|(_, c)| c.trim())))
            .unwrap_or_default()
            .into(),
        settings,
        units,
        cache: Mutex::new(HashMap::new()),
    }))
}

struct Weather {
    key: ApiKey,
    http: reqwest::Client,
    home: String,
    country: String,
    settings: Settings,
    units: String,
    cache: Mutex<HashMap<String, (Instant, Value)>>,
}

impl Weather {
    // Strip reqwest URLs from errors: the provider authenticates with a query key.
    async fn request(&self, endpoint: &str, params: &[(&str, String)]) -> Result<Value> {
        let response = self
            .http
            .get(format!("{}/{endpoint}.json", self.settings.base_url.trim_end_matches('/')))
            .query(&[("key", self.key.get()?)])
            .query(params)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("WeatherAPI request failed: {}", e.without_url()))?;
        let status = response.status();
        let body: Value = response.json().await.map_err(|_| anyhow::anyhow!("WeatherAPI returned invalid JSON"))?;
        if body["error"]["code"].as_i64() == Some(1006) && endpoint == "search" {
            return Ok(json!([]));
        }
        if !status.is_success() || body.get("error").is_some() {
            // Only a numeric code is exposed; error messages can reflect the input/key.
            bail!(
                "WeatherAPI returned HTTP {} (code {}); check API key, plan and location",
                status.as_u16(),
                body["error"]["code"].as_i64().unwrap_or(0)
            );
        }
        Ok(body)
    }

    async fn bundle(&self, location: &str) -> Result<Value> {
        let cache_key = location.to_lowercase();
        let mut cache = self.cache.lock().await;
        let ttl = Duration::from_secs(self.settings.bundle_cache_ttl_secs);
        cache.retain(|_, (at, _)| at.elapsed() < ttl);
        if let Some((_, data)) = cache.get(&cache_key) {
            let same_day =
                data["location"]["tz_id"].as_str().and_then(|s| s.parse::<chrono_tz::Tz>().ok()).is_some_and(|tz| {
                    data["forecast"]["forecastday"][0]["date"].as_str()
                        == Some(Utc::now().with_timezone(&tz).date_naive().to_string().as_str())
                });
            if same_day {
                return Ok(data.clone());
            }
        }
        let matches = self.request("search", &[("q", location.into())]).await?;
        let matches = matches.as_array().context("WeatherAPI search returned invalid data")?;
        if matches.is_empty() {
            return Ok(json!({"unavailable": "Unknown location; ask for a city and country."}));
        }
        // Exact city first, then the user's country among equally named cities.
        let city = location.split(',').next().unwrap_or(location).trim();
        let candidates: Vec<_> =
            matches.iter().filter(|m| m["name"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(city))).collect();
        let pool: Vec<_> = if candidates.is_empty() { matches.iter().collect() } else { candidates };
        let qualifiers: Vec<_> = location.split(',').skip(1).map(str::trim).filter(|s| !s.is_empty()).collect();
        let qualified: Vec<_> = pool
            .iter()
            .copied()
            .filter(|m| {
                qualifiers.iter().all(|q| {
                    ["region", "country"]
                        .iter()
                        .any(|field| m[*field].as_str().is_some_and(|s| s.eq_ignore_ascii_case(q)))
                })
            })
            .collect();
        if !qualifiers.is_empty() && qualified.is_empty() {
            return Ok(
                json!({"unavailable":"Could not match that city and region/country. Please clarify the location."}),
            );
        }
        let pool = if qualifiers.is_empty() { pool } else { qualified };
        let selected = pool
            .iter()
            .find(|m| {
                !location.contains(',') && m["country"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(&self.country))
            })
            .copied()
            .unwrap_or(pool[0]);
        let lat = selected["lat"].as_f64().context("WeatherAPI location missing latitude")?;
        let lon = selected["lon"].as_f64().context("WeatherAPI location missing longitude")?;
        let data = self
            .request(
                "forecast",
                &[
                    ("q", format!("{lat},{lon}")),
                    ("days", self.settings.forecast_days.to_string()),
                    ("aqi", "yes".into()),
                    ("alerts", "yes".into()),
                ],
            )
            .await?;
        // Validate before caching. No absent metrics become zero.
        anyhow::ensure!(
            data["location"]["tz_id"].is_string()
                && data["current"]["last_updated"].is_string()
                && data["forecast"]["forecastday"].as_array().is_some_and(|d| !d.is_empty()),
            "WeatherAPI response is incomplete"
        );
        cache.insert(cache_key, (Instant::now(), data.clone()));
        Ok(data)
    }
}

#[async_trait]
impl Skill for Weather {
    fn name(&self) -> &str {
        "weather"
    }
    fn description(&self) -> &str {
        "Current weather and forecasts, rain/umbrella/running decisions, air quality, UV, humidity and sunrise/sunset. Defaults to profile location. Resolves time windows in the destination timezone; rejects past/out-of-range dates."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object", "properties": {
            "location":{"type":"string", "description":"City, optionally region/country; omit for profile location. Only place information, never the user's question."},
            "when":{"type":"string", "description":"now (default), today, tonight (18:00–06:00), this evening (18:00–24:00), morning, afternoon, tomorrow, day after tomorrow, this weekend, weekday or YYYY-MM-DD; day plus period such as tomorrow evening. Pass past dates too, so the skill can reject them."},
            "metric":{"type":"string", "enum":["overview","rain","temperature","air_quality","uv","humidity","sunrise","sunset","running"], "description":"Specific interest; umbrella maps to rain."},
            "units":{"type":"string", "enum":["metric","imperial"]},
            "highlights_only":{"type":"boolean", "description":"Internal greeting use; true returns only significant flags."}
        }})
    }
    fn policy(&self) -> Policy {
        // Bundle cache, rather than argument cache: time windows are re-evaluated on every call.
        Policy { timeout: Duration::from_secs(6), cache_ttl: Duration::ZERO }
    }
    fn fallback_query(&self, args: &Value) -> Option<String> {
        if args["highlights_only"].as_bool() == Some(true) {
            return None;
        }
        let when = args["when"].as_str().unwrap_or("now");
        // Never let search bypass a known temporal limitation. Allow a one-day
        // margin for destination dates differing from UTC when the source fails.
        if window::resolve(when, Utc::now().naive_utc()).is_err() {
            return None;
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(when, "%Y-%m-%d")
            && date >= Utc::now().date_naive() + chrono::Duration::days(i64::from(self.settings.forecast_days) + 1)
        {
            return None;
        }
        let location = args["location"].as_str().unwrap_or(&self.home).trim();
        if location.is_empty() {
            return None;
        }
        Some(format!(
            "weather {} {} {}",
            location,
            args["when"].as_str().unwrap_or("now"),
            args["metric"].as_str().unwrap_or("overview")
        ))
    }
    async fn call(&self, args: Value) -> Result<String> {
        if let Err(err) = window::resolve(args["when"].as_str().unwrap_or("now"), Utc::now().naive_utc()) {
            // Syntax-only rejections are independent of the destination clock.
            // Date/range checks happen below once its timezone is known.
            if err.to_string().starts_with("Unsupported time window") {
                return Ok(json!({"status":"unavailable", "reason":err.to_string(), "as_of":Utc::now().to_rfc3339()})
                    .to_string());
            }
        }
        let location = args["location"].as_str().unwrap_or(&self.home).trim();
        if location.is_empty() {
            return Ok(json!({"status":"unavailable", "reason":"No profile location. Ask which city to use.", "as_of":Utc::now().to_rfc3339()}).to_string());
        }
        anyhow::ensure!(
            location.len() <= 200 && !location.chars().any(char::is_control),
            "location must be a city and optional country"
        );
        let units = args["units"].as_str().unwrap_or(&self.units);
        anyhow::ensure!(matches!(units, "metric" | "imperial"), "units must be metric or imperial");
        let data = self.bundle(location).await?;
        if let Some(reason) = data.get("unavailable") {
            return Ok(json!({"status":"unavailable", "reason":reason, "as_of":Utc::now().to_rfc3339()}).to_string());
        }
        report::build(&data, &args, units, &self.settings, Utc::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A real local HTTP server verifies request boundaries and caching without paid calls.
    fn server(replies: Vec<(u16, Value)>) -> (String, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let handle = std::thread::spawn(move || {
            for (status, body) in replies {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 4096];
                while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let req = String::from_utf8(bytes).unwrap();
                assert!(req.starts_with("GET /search.json?") || req.starts_with("GET /forecast.json?"));
                assert!(!req.contains("umbrella") && !req.contains("conversation"));
                if req.starts_with("GET /forecast") {
                    assert!(req.contains("aqi=yes") && req.contains("alerts=yes") && req.contains("days=3"));
                    // Country-preferred match, rather than first result in another country.
                    assert!(req.contains("q=19%2C72"));
                }
                calls.fetch_add(1, Ordering::SeqCst);
                let body = body.to_string();
                write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        (base, count, handle)
    }
    fn weather(base: String) -> Weather {
        Weather {
            // An existing non-secret variable avoids mutating the process environment in tests.
            key: ApiKey::from_env("PATH", "weather"),
            http: reqwest::Client::new(),
            home: "Mumbai".into(),
            country: "India".into(),
            units: "metric".into(),
            settings: Settings { base_url: base, ..Default::default() },
            cache: Mutex::new(HashMap::new()),
        }
    }
    #[tokio::test]
    async fn caches_bundle_across_questions_and_prefers_country() {
        let mut data = report::tests::fixture();
        data["forecast"]["forecastday"][0]["date"] =
            json!(Utc::now().with_timezone(&"Asia/Kolkata".parse::<chrono_tz::Tz>().unwrap()).date_naive().to_string());
        let (base, count, handle) = server(vec![
            (
                200,
                json!([{"name":"Mumbai","country":"Other","lat":1,"lon":2},{"name":"Mumbai","country":"India","lat":19,"lon":72}]),
            ),
            (200, data),
        ]);
        let w = weather(base);
        let first: Value = serde_json::from_str(&w.call(json!({"metric":"rain"})).await.unwrap()).unwrap();
        let second: Value = serde_json::from_str(&w.call(json!({"metric":"humidity"})).await.unwrap()).unwrap();
        assert_eq!(first["place"], second["place"]);
        assert_eq!(second["interest"], "humidity");
        assert_eq!(count.load(Ordering::SeqCst), 2);
        handle.join().unwrap();
    }
    #[tokio::test]
    async fn unknown_and_missing_home_never_become_guesses() {
        let (base, _, handle) = server(vec![(200, json!([]))]);
        let mut w = weather(base);
        let out: Value = serde_json::from_str(&w.call(json!({"location":"unknown"})).await.unwrap()).unwrap();
        assert_eq!(out["status"], "unavailable");
        handle.join().unwrap();
        w.home.clear();
        let out: Value = serde_json::from_str(&w.call(json!({})).await.unwrap()).unwrap();
        assert!(out["reason"].as_str().unwrap().contains("which city"));
        assert!(w.fallback_query(&json!({})).is_none());
    }
    #[tokio::test]
    async fn provider_errors_do_not_expose_key_or_response_body() {
        let (base, _, handle) = server(vec![(403, json!({"error":{"code":2006,"message":"secret echoed here"}}))]);
        let w = weather(base);
        let err = w.call(json!({})).await.unwrap_err().to_string();
        assert!(err.contains("2006"));
        assert!(!err.contains("secret") && !err.contains("key="));
        assert_eq!(
            w.fallback_query(&json!({"when":"tonight","metric":"rain"})).unwrap(),
            "weather Mumbai tonight rain"
        );
        assert!(w.fallback_query(&json!({"highlights_only":true})).is_none());
        handle.join().unwrap();
    }
    #[tokio::test]
    async fn absent_key_is_clear_and_config_validates() {
        let mut w = weather("http://127.0.0.1:1".into());
        w.key = ApiKey::from_env("JARVIS_WEATHER_TEST_UNSET_KEY", "weather");
        assert!(w.call(json!({})).await.unwrap_err().to_string().contains("JARVIS_WEATHER_TEST_UNSET_KEY is not set"));
        let ctx = SkillContext::for_tests();
        let bad: toml::Table = toml::from_str("forecast_days = 15").unwrap();
        assert!(create(&bad, &ctx).is_err());
    }
}
