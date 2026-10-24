use super::{Settings, window};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};

fn number(value: &Value, key: &str) -> Option<f64> {
    value[key].as_f64()
}
fn range(values: &[&Value], key: &str) -> Value {
    let numbers: Vec<_> = values.iter().filter_map(|v| number(v, key)).collect();
    if numbers.is_empty() {
        return Value::Null;
    }
    json!({"min":numbers.iter().copied().reduce(f64::min), "max":numbers.iter().copied().reduce(f64::max), "samples":numbers.len()})
}
fn maximum(values: &[&Value], key: &str) -> Option<f64> {
    values.iter().filter_map(|v| number(v, key)).reduce(f64::max)
}
fn local_hour(value: &Value, tz: Tz) -> Option<NaiveDateTime> {
    value["time_epoch"]
        .as_i64()
        .and_then(|t| DateTime::from_timestamp(t, 0))
        .map(|t| t.with_timezone(&tz).naive_local())
}
fn alert_time(value: &Value, key: &str, tz: Tz) -> Option<DateTime<Utc>> {
    let text = value[key].as_str()?;
    DateTime::parse_from_rfc3339(text).map(|t| t.with_timezone(&Utc)).ok().or_else(|| {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
            .ok()
            .and_then(|t| tz.from_local_datetime(&t).single())
            .map(|t| t.with_timezone(&Utc))
    })
}

pub(super) fn build(
    data: &Value,
    args: &Value,
    units: &str,
    settings: &Settings,
    now: DateTime<Utc>,
) -> Result<String> {
    let tz: Tz = data["location"]["tz_id"]
        .as_str()
        .context("WeatherAPI timezone missing")?
        .parse()
        .context("Unknown location timezone")?;
    let local_now = now.with_timezone(&tz).naive_local();
    let place = json!({"city":data["location"]["name"], "region":data["location"]["region"], "country":data["location"]["country"], "timezone":tz.name()});
    let as_of = &data["current"]["last_updated"];
    let unavailable =
        |reason: String| Ok(json!({"status":"unavailable", "place":place, "as_of":as_of, "reason":reason}).to_string());
    let when = args["when"].as_str().unwrap_or("now");
    let window = match window::resolve(when, local_now) {
        Ok(w) => w,
        Err(e) => return unavailable(e.to_string()),
    };
    let days = data["forecast"]["forecastday"].as_array().context("Forecast missing")?;
    let all_hours: Vec<_> = days.iter().filter_map(|d| d["hour"].as_array()).flatten().collect();
    let hours: Vec<_> = all_hours
        .iter()
        .copied()
        .filter(|h| local_hour(h, tz).is_some_and(|t| t >= window.start && t < window.end))
        .collect();
    if !window.current {
        let last = all_hours.iter().filter_map(|h| local_hour(h, tz)).max();
        if hours.first().and_then(|h| local_hour(h, tz)).is_none_or(|t| t > window.start)
            || last.is_none_or(|t| t + chrono::Duration::hours(1) < window.end)
        {
            return unavailable(format!(
                "The full requested window is outside the returned forecast range ({} through {}). No prediction available.",
                days.first().unwrap()["date"],
                days.last().unwrap()["date"]
            ));
        }
        // Detect holes, including partial day responses. DST changes may duplicate/skip local hours;
        // provider epoch times must still cover every real hour.
        let epochs: Vec<_> = hours.iter().filter_map(|h| h["time_epoch"].as_i64()).collect();
        if epochs.len() != hours.len() || epochs.windows(2).any(|w| w[1] - w[0] != 3600) {
            return unavailable("Hourly forecast coverage is incomplete for that window.".into());
        }
    }
    let city = data["location"]["name"].as_str().unwrap_or_default();
    let thresholds = settings
        .city_thresholds
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(city))
        .map(|(_, v)| v)
        .unwrap_or(&settings.thresholds);
    let values: Vec<&Value> = if window.current { vec![&data["current"]] } else { hours.clone() };
    let current_hour: Vec<_> = all_hours
        .iter()
        .copied()
        .filter(|h| h["time_epoch"].as_i64().is_some_and(|at| at <= now.timestamp() && now.timestamp() < at + 3600))
        .collect();
    let imperial = units == "imperial";
    let temp_key = if imperial { "temp_f" } else { "temp_c" };
    let feels_key = if imperial { "feelslike_f" } else { "feelslike_c" };
    let wind_key = if imperial { "wind_mph" } else { "wind_kph" };
    let precip_key = if imperial { "precip_in" } else { "precip_mm" };
    let rain_hours: Vec<_> = hours
        .iter()
        .filter(|h| number(h, "chance_of_rain").is_some_and(|n| n >= thresholds.rain_chance))
        .map(|h| json!({"local_time":h["time"], "chance_percent":h["chance_of_rain"], "precipitation":h[precip_key]}))
        .collect();
    let aqi =
        values.iter().filter_map(|v| v["air_quality"]["us-epa-index"].as_u64()).max().filter(|n| (1..=6).contains(n));
    let aqi_labels = ["Good", "Moderate", "Unhealthy for sensitive groups", "Unhealthy", "Very unhealthy", "Hazardous"];
    let mut flags = Vec::new();
    let mut alerts = Vec::new();
    if let Some(items) = data["alerts"]["alert"].as_array() {
        for alert in items {
            let effective = alert_time(alert, "effective", tz);
            let expires = alert_time(alert, "expires", tz);
            if let (Some(start), Some(end)) = (effective, expires)
                && start.with_timezone(&tz).naive_local() < window.end
                && end.with_timezone(&tz).naive_local() > window.start
            {
                alerts.push(json!({"event":alert["event"], "headline":alert["headline"], "severity":alert["severity"], "effective":alert["effective"], "expires":alert["expires"]}));
            }
        }
    }
    if !alerts.is_empty() {
        flags.push(json!({"kind":"official_alert", "count":alerts.len()}));
    }
    if !window.current {
        flag_max(&mut flags, &values, "precip_mm", thresholds.heavy_rain_mm_per_hour, "heavy_rain", "mm_per_hour");
    }
    if !window.current && !rain_hours.is_empty() {
        flags.push(json!({"kind":"rain", "first_local_time":rain_hours.first().unwrap()["local_time"], "last_local_time":rain_hours.last().unwrap()["local_time"], "threshold_percent":thresholds.rain_chance}));
    } else if window.current && number(&data["current"], "precip_mm").is_some_and(|n| n > 0.) {
        flags.push(json!({"kind":"rain", "local_time":as_of, "precipitation":data["current"][precip_key], "unit":if imperial {"in"} else {"mm"}}));
    }
    flag_max(&mut flags, &values, "feelslike_c", thresholds.feels_like_c, "heat", "feels_like_c");
    if aqi.is_some_and(|n| n as f64 >= thresholds.us_epa_category) {
        flags.push(json!({"kind":"poor_air_quality", "scale":"US EPA category (1–6)", "category":aqi, "label":aqi.map(|n| aqi_labels[n as usize - 1])}));
    }
    flag_max(&mut flags, &values, "uv", thresholds.uv, "high_uv", "uv_index");
    flags.truncate(3);
    if imperial {
        for flag in &mut flags {
            let (factor, offset, unit) = match flag["kind"].as_str() {
                Some("heat") => (1.8, 32., "feels_like_f"),
                Some("heavy_rain") => (1. / 25.4, 0., "in_per_hour"),
                _ => continue,
            };
            for field in ["value", "threshold"] {
                if let Some(n) = flag[field].as_f64() {
                    flag[field] = json!(n * factor + offset);
                }
            }
            flag["unit"] = json!(unit);
        }
    }
    let astro: Vec<_> = days
        .iter()
        .filter(|d| {
            d["date"].as_str().is_some_and(|date| {
                date >= window.start.date().to_string().as_str()
                    && date <= (window.end - chrono::Duration::seconds(1)).date().to_string().as_str()
            })
        })
        .map(|d| json!({"date":d["date"], "sunrise":d["astro"]["sunrise"], "sunset":d["astro"]["sunset"]}))
        .collect();
    let mut result = json!({
        "status":"ok", "source":"WeatherAPI.com", "place":place, "as_of":as_of,
        "local_now":local_now.to_string(), "window":{"requested":when,"start":window.start.to_string(),"end":window.end.to_string()},
        "units":{"temperature":if imperial {"F"} else {"C"}, "wind":if imperial {"mph"} else {"km/h"}, "precipitation":if imperial {"in"} else {"mm"}},
        "flags":flags, "interest":args["metric"].as_str().unwrap_or("overview"),
        "metrics":{
            "temperature":range(&values,temp_key), "feels_like":range(&values,feels_key),
            "humidity_percent":range(&values,"humidity"), "wind":range(&values,wind_key),
            "uv_index":range(&values,"uv"), "precipitation":range(&values,precip_key), "precipitation_basis":if window.current {"provider current observation"} else {"hourly forecast"},
            "rain_chance_percent":if window.current {range(&current_hour,"chance_of_rain")} else {range(&hours,"chance_of_rain")},
            "rain_probability_basis":if window.current {"forecast for the current hour, not an observation"} else {"hourly forecast for the requested window"},
            "air_quality":{"scale":"US EPA category (1–6), not numeric AQI or Indian AQI", "worst_category":aqi, "label":aqi.map(|n| aqi_labels[n as usize-1]), "available_samples":values.iter().filter(|v| v["air_quality"]["us-epa-index"].is_number()).count()},
            "conditions": values.iter().filter_map(|v| v["condition"]["text"].as_str()).collect::<std::collections::BTreeSet<_>>()
        },
        "rain_hours":if window.current {vec![]} else {rain_hours}, "astronomy":astro,
        "official_alerts":alerts,
        "alert_coverage":"Only provider-returned alerts with valid times overlapping the window. No returned alerts does not confirm absence of warnings; India coverage is not guaranteed.",
        "missing_data":"null metrics are unavailable; do not infer them. Forecast AQ may cover fewer days than weather."
    });
    if args["highlights_only"].as_bool() == Some(true) {
        result
            .as_object_mut()
            .unwrap()
            .retain(|k, _| matches!(k.as_str(), "status" | "place" | "as_of" | "flags" | "units" | "window"));
    }
    Ok(result.to_string())
}

fn flag_max(flags: &mut Vec<Value>, values: &[&Value], key: &str, threshold: f64, kind: &str, unit: &str) {
    if let Some(value) = maximum(values, key).filter(|v| *v >= threshold) {
        flags.push(json!({"kind":kind,"value":value,"unit":unit,"threshold":threshold}));
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate};

    pub fn fixture() -> Value {
        let tz: Tz = "Asia/Kolkata".parse().unwrap();
        let first = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let days: Vec<_> = (0..3).map(|d| {
            let date = first + Duration::days(d);
            let hours: Vec<_> = (0..24).map(|h| {
                let local = date.and_hms_opt(h, 0, 0).unwrap();
                json!({"time":local.format("%Y-%m-%d %H:%M").to_string(), "time_epoch":tz.from_local_datetime(&local).unwrap().timestamp(),
                    "temp_c":28., "temp_f":82.4, "feelslike_c":29., "feelslike_f":84.2,
                    "wind_kph":12., "wind_mph":7.46, "humidity":65, "uv":2., "precip_mm":0., "precip_in":0.,
                    "chance_of_rain":if d == 0 && h == 21 {70} else {0},
                    "air_quality":{"us-epa-index":1}, "condition":{"text":"Clear"}})
            }).collect();
            json!({"date":date.to_string(), "day":{"maxtemp_c":29.}, "astro":{"sunrise":"06:30 AM","sunset":"06:10 PM"},"hour":hours})
        }).collect();
        json!({"location":{"name":"Mumbai","region":"Maharashtra","country":"India","tz_id":"Asia/Kolkata"},
            "current":{"last_updated":"2026-10-23 17:15","temp_c":28.,"temp_f":82.4,"feelslike_c":29.,"feelslike_f":84.2,"uv":2,"humidity":65,"precip_mm":0,"air_quality":{"us-epa-index":1},"condition":{"text":"Clear"}},
            "forecast":{"forecastday":days},"alerts":{"alert":[]}})
    }
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-23T12:00:00Z").unwrap().with_timezone(&Utc)
    }
    fn output(data: &Value, args: Value) -> Value {
        serde_json::from_str(&build(data, &args, "metric", &Settings::default(), now()).unwrap()).unwrap()
    }
    #[test]
    fn filters_asked_window_and_overnight_rain() {
        let mut data = fixture();
        data["forecast"]["forecastday"][1]["hour"][2]["chance_of_rain"] = json!(80);
        data["forecast"]["forecastday"][1]["hour"][13]["feelslike_c"] = json!(42);
        let evening = output(&data, json!({"when":"this evening"}));
        assert_eq!(evening["rain_hours"].as_array().unwrap().len(), 1);
        let night = output(&data, json!({"when":"tonight"}));
        assert_eq!(night["rain_hours"].as_array().unwrap().len(), 2);
        assert_eq!(night["metrics"]["temperature"]["samples"], 12);
        assert!(!night["flags"].as_array().unwrap().iter().any(|f| f["kind"] == "heat"));
        let tomorrow = output(&data, json!({"when":"tomorrow"}));
        assert_eq!(tomorrow["metrics"]["temperature"]["samples"], 24);
        assert!(tomorrow["flags"].as_array().unwrap().iter().any(|f| f["kind"] == "heat"));
        assert_eq!(tomorrow["as_of"], "2026-10-23 17:15");
        assert_eq!(tomorrow["place"]["timezone"], "Asia/Kolkata");
    }
    #[test]
    fn fractional_clock_seconds_do_not_exclude_the_current_forecast_hour() {
        let data = fixture();
        for time in
            ["2026-10-23T12:00:00.123456789Z", "2026-10-23T15:00:00.123456789Z", "2026-10-23T18:15:59.999999999Z"]
        {
            let now = DateTime::parse_from_rfc3339(time).unwrap().with_timezone(&Utc);
            for when in ["today", "tonight"] {
                let out: Value = serde_json::from_str(
                    &build(&data, &json!({"when":when}), "metric", &Settings::default(), now).unwrap(),
                )
                .unwrap();
                assert_eq!(out["status"], "ok", "{time}, {when}: {out}");
            }
        }
    }
    #[test]
    fn refuses_past_beyond_range_and_partial_coverage() {
        let mut data = fixture();
        for when in ["yesterday", "2026-10-22", "2026-10-26", "Monday"] {
            assert_eq!(output(&data, json!({"when":when}))["status"], "unavailable", "{when}");
        }
        data["forecast"]["forecastday"][1]["hour"].as_array_mut().unwrap().remove(0);
        assert_eq!(output(&data, json!({"when":"tomorrow"}))["status"], "unavailable");
    }
    #[test]
    fn calm_day_missing_aqi_and_imperial_units() {
        let mut data = fixture();
        data["current"].as_object_mut().unwrap().remove("air_quality");
        let current = output(&data, json!({}));
        assert!(current["flags"].as_array().unwrap().is_empty());
        assert!(current["metrics"]["air_quality"]["worst_category"].is_null());
        assert_eq!(current["metrics"]["rain_chance_percent"]["max"], 0.0);
        data["current"]["feelslike_c"] = json!(40);
        data["current"]["feelslike_f"] = json!(104);
        let imperial: Value =
            serde_json::from_str(&build(&data, &json!({}), "imperial", &Settings::default(), now()).unwrap()).unwrap();
        assert_eq!(imperial["metrics"]["temperature"]["max"], 82.4);
        assert_eq!(imperial["flags"][0]["value"], 104.);
        assert_eq!(imperial["flags"][0]["unit"], "feels_like_f");
    }
    #[test]
    fn alert_overlap_thresholds_and_flag_limit() {
        let mut data = fixture();
        data["alerts"]["alert"] = json!([
            {"effective":"2026-10-23T18:00:00+05:30","expires":"2026-10-24T03:00:00+05:30","event":"Storm"},
            {"effective":"2026-10-22T18:00:00+05:30","expires":"2026-10-23T03:00:00+05:30","event":"Expired"},
            {"event":"Unknown timing"}
        ]);
        for h in data["forecast"]["forecastday"][0]["hour"].as_array_mut().unwrap() {
            h["feelslike_c"] = json!(37);
            h["uv"] = json!(8);
            h["air_quality"]["us-epa-index"] = json!(4);
        }
        let out = output(&data, json!({"when":"tonight"}));
        assert_eq!(out["official_alerts"].as_array().unwrap().len(), 1);
        assert_eq!(out["flags"].as_array().unwrap().len(), 3);
        assert_eq!(out["flags"][0]["kind"], "official_alert");
        let mut settings = Settings::default();
        settings
            .city_thresholds
            .insert("mumbai".into(), super::super::Thresholds { feels_like_c: 40., ..Default::default() });
        let out: Value =
            serde_json::from_str(&build(&data, &json!({"when":"tonight"}), "metric", &settings, now()).unwrap())
                .unwrap();
        assert!(!out["flags"].as_array().unwrap().iter().any(|f| f["kind"] == "heat"));
        assert!(out["flags"].as_array().unwrap().iter().any(|f| f["kind"] == "poor_air_quality"));
    }
    #[test]
    fn destination_day_differs_from_utc_and_dst_is_supported() {
        let mut data = fixture();
        data["location"]["tz_id"] = json!("America/New_York");
        let at = DateTime::parse_from_rfc3339("2026-10-24T01:00:00Z").unwrap().with_timezone(&Utc);
        let out: Value =
            serde_json::from_str(&build(&data, &json!({}), "metric", &Settings::default(), at).unwrap()).unwrap();
        assert_eq!(out["local_now"], "2026-10-23 21:00:00");
        let tz: Tz = "America/New_York".parse().unwrap();
        let first = tz.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap().with_timezone(&Utc);
        let hours: Vec<_> =
            (0..25).map(|i| json!({"time_epoch":(first + Duration::hours(i)).timestamp(),"temp_c":10})).collect();
        data["forecast"]["forecastday"] = json!([{"date":"2026-11-01","hour":hours}]);
        let out: Value = serde_json::from_str(
            &build(&data, &json!({"when":"today"}), "metric", &Settings::default(), first).unwrap(),
        )
        .unwrap();
        assert_eq!(out["status"], "ok");
        assert_eq!(out["metrics"]["temperature"]["samples"], 25);
    }
}
