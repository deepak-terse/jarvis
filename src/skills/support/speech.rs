//! Formatting for answers that will be read aloud: plain numbers, "as of" times,
//! relative ages. No URLs; the source name is kept so the reply can cite it.

use chrono::{DateTime, Local, TimeZone};

/// "3:30 PM", "3:30 PM yesterday" or "21 October, 3:30 PM", relative to `now`.
pub fn as_of<Tz: TimeZone>(at: &DateTime<Tz>, now: &DateTime<Local>) -> String {
    let at = at.with_timezone(&Local);
    let time = at.format("%-I:%M %p");
    match (now.date_naive() - at.date_naive()).num_days() {
        0 => time.to_string(),
        1 => format!("{time} yesterday"),
        _ => format!("{}, {time}", at.format("%-d %B")),
    }
}

/// "5 minutes ago", "3 hours ago", "2 days ago".
pub fn ago<Tz: TimeZone>(at: &DateTime<Tz>, now: &DateTime<Local>) -> String {
    let mins = (now.clone().fixed_offset() - at.clone().fixed_offset()).num_minutes().max(0);
    let (n, unit) = match mins {
        0 => return "just now".into(),
        1..60 => (mins, "minute"),
        60..1440 => (mins / 60, "hour"),
        _ => (mins / 1440, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// Thousands-grouped with up to `decimals` places, trailing zeros trimmed: 2950.4 → "2,950.4".
pub fn number(v: f64, decimals: usize) -> String {
    let fixed = format!("{:.*}", decimals, v.abs());
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let frac = frac.trim_end_matches('0');
    let sign = if v < 0.0 && fixed.chars().any(|c| c.is_ascii_digit() && c != '0') { "-" } else { "" };
    if frac.is_empty() { format!("{sign}{grouped}") } else { format!("{sign}{grouped}.{frac}") }
}

/// "up 1.25 percent", "down 0.4 percent", "unchanged".
pub fn change_percent(p: f64) -> String {
    let rounded = (p * 100.0).round() / 100.0;
    if rounded == 0.0 {
        "unchanged".into()
    } else {
        format!("{} {} percent", if rounded > 0.0 { "up" } else { "down" }, number(rounded.abs(), 2))
    }
}

/// The first `n` sentences of `text`.
pub fn sentences(text: &str, n: usize) -> String {
    let mut end = text.len();
    let mut count = 0;
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        let next_is_space = bytes.get(i + 1).is_none_or(|b| b.is_ascii_whitespace());
        if matches!(c, '.' | '!' | '?') && next_is_space && !ends_with_abbreviation(&text[..i]) {
            count += 1;
            if count == n {
                end = i + 1;
                break;
            }
        }
    }
    text[..end].trim().to_string()
}

/// Initials and common abbreviations ("J. R. R.", "Dr.", "St.") are not sentence ends.
fn ends_with_abbreviation(before: &str) -> bool {
    let word = before.rsplit(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("");
    word.chars().count() == 1 || ["Mr", "Mrs", "Ms", "Dr", "St", "Jr", "Sr", "vs", "approx", "c"].contains(&word)
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.into() } else { s.chars().take(n).chain(['…']).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn numbers_read_cleanly() {
        assert_eq!(number(2950.4, 2), "2,950.4");
        assert_eq!(number(1234567.0, 2), "1,234,567");
        assert_eq!(number(-0.001, 2), "0");
        assert_eq!(number(-12.5, 1), "-12.5");
        assert_eq!(change_percent(1.2345), "up 1.23 percent");
        assert_eq!(change_percent(-0.4), "down 0.4 percent");
        assert_eq!(change_percent(0.001), "unchanged");
    }

    #[test]
    fn times_are_relative() {
        let now = Local::now();
        assert_eq!(ago(&(now - Duration::minutes(90)), &now), "1 hour ago");
        assert_eq!(ago(&(now - Duration::minutes(5)), &now), "5 minutes ago");
        assert_eq!(as_of(&now, &now), now.format("%-I:%M %p").to_string());
        assert!(as_of(&(now - Duration::days(1)), &now).ends_with("yesterday"));
    }

    #[test]
    fn sentences_skip_initials() {
        let t = "J. R. R. Tolkien was an English writer. He wrote The Hobbit. And more.";
        assert_eq!(sentences(t, 1), "J. R. R. Tolkien was an English writer.");
        assert_eq!(sentences(t, 2), "J. R. R. Tolkien was an English writer. He wrote The Hobbit.");
        assert_eq!(sentences("No full stop", 2), "No full stop");
    }
}
