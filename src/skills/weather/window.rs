//! Interpret everyday windows against the destination's clock, never the host clock.
use anyhow::{Result, bail};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};

#[derive(Debug)]
pub struct Window {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub current: bool,
}

pub fn resolve(when: &str, now: NaiveDateTime) -> Result<Window> {
    let text = when.trim().to_lowercase();
    // Resolve a day plus a period, e.g. tomorrow evening or Friday morning.
    let period = ["morning", "afternoon", "evening", "night"]
        .into_iter()
        .find(|p| text.ends_with(&format!(" {p}")) && !text.starts_with("this "));
    if let Some(period) = period {
        let day = text.strip_suffix(&format!(" {period}")).unwrap();
        let base = resolve(day, now)?;
        let (start, end) = match period {
            "morning" => (6, 12),
            "afternoon" => (12, 18),
            "evening" => (18, 24),
            _ => (18, 30),
        };
        let midnight = base.start.date().and_hms_opt(0, 0, 0).unwrap();
        let start = (midnight + Duration::hours(start)).max(now.date().and_hms_opt(now.hour(), 0, 0).unwrap());
        let end = midnight + Duration::hours(end);
        if start >= end {
            bail!("That time window has already passed at this location.");
        }
        return Ok(Window { start, end, current: false });
    }
    let today = now.date();
    let midnight = |d: NaiveDate| d.and_hms_opt(0, 0, 0).unwrap();
    if matches!(text.as_str(), "now" | "") {
        return Ok(Window { start: now, end: now + Duration::hours(1), current: true });
    }
    let (date, span) = match text.as_str() {
        "today" => (today, 1),
        "tomorrow" => (today + Duration::days(1), 1),
        "day after tomorrow" => (today + Duration::days(2), 1),
        "tonight" | "this evening" | "evening" | "this morning" | "morning" | "this afternoon" | "afternoon" => {
            (today, 1)
        }
        "this weekend" | "weekend" => {
            let day = today.weekday().num_days_from_monday();
            let offset = 5_u32.saturating_sub(day);
            (today + Duration::days(offset.into()), if day == 6 { 1 } else { 2 })
        }
        _ => {
            let weekdays = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
            let day_text = text.strip_prefix("on ").or_else(|| text.strip_prefix("this ")).unwrap_or(&text);
            if let Some(day) = weekdays.iter().position(|d| *d == day_text) {
                let offset = (day as i64 - today.weekday().num_days_from_monday() as i64).rem_euclid(7);
                (today + Duration::days(offset), 1)
            } else if let Ok(date) = NaiveDate::parse_from_str(&text, "%Y-%m-%d") {
                (date, 1)
            } else {
                bail!(
                    "Unsupported time window. Use now, today, tonight, tomorrow, this weekend, a weekday or YYYY-MM-DD; past weather is unavailable."
                );
            }
        }
    };
    if date < today {
        bail!("Past weather is unavailable.");
    }
    let (start_hour, end_hour) = match text.as_str() {
        "tonight" => (18, 30), // through 06:00 tomorrow
        "this evening" | "evening" => (18, 24),
        "this morning" | "morning" => (6, 12),
        "this afternoon" | "afternoon" => (12, 18),
        _ => (0, 24 * span),
    };
    let start = (midnight(date) + Duration::hours(start_hour)).max(now.date().and_hms_opt(now.hour(), 0, 0).unwrap());
    let end = midnight(date) + Duration::hours(end_hour);
    if start >= end {
        bail!("That time window has already passed at this location.");
    }
    Ok(Window { start, end, current: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_windows_and_past_dates() {
        let now = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap().and_hms_opt(20, 30, 0).unwrap();
        let night = resolve("tonight", now).unwrap();
        assert_eq!(night.start.hour(), 20);
        assert_eq!(night.end.to_string(), "2026-10-24 06:00:00");
        let weekend = resolve("weekend", now).unwrap();
        assert_eq!(weekend.start.to_string(), "2026-10-24 00:00:00");
        assert_eq!(weekend.end.to_string(), "2026-10-26 00:00:00");
        assert_eq!(resolve("on Friday", now).unwrap().start.date(), now.date());
        assert!(resolve("2026-10-22", now).is_err());
        assert!(resolve("yesterday", now).is_err());
        assert!(resolve("morning", now).is_err());
        assert_eq!(resolve("tomorrow evening", now).unwrap().start.to_string(), "2026-10-24 18:00:00");
        assert_eq!(resolve("Friday night", now).unwrap().end.to_string(), "2026-10-24 06:00:00");
    }
}
