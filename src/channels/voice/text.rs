//! Turning streamed model text into speakable sentences.

use regex::Regex;
use std::sync::LazyLock;

/// Byte index just past the first speakable sentence in `buf`, if complete.
/// Boundaries: newline, or `.!?;:` followed by whitespace. Very long runs split at a comma/space.
pub fn sentence_end(buf: &str) -> Option<usize> {
    let mut chars = buf.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let boundary = c == '\n' || (".!?;:".contains(c) && chars.peek().is_some_and(|(_, n)| n.is_whitespace()));
        if boundary && buf[..i].trim().len() >= 2 {
            return Some(i + c.len_utf8());
        }
    }
    if buf.len() > 200 {
        return buf.rfind(", ").or_else(|| buf.rfind(' ')).map(|i| i + 1);
    }
    None
}

/// Make model text speakable: drop markdown, citations and URLs.
pub fn clean(text: &str) -> String {
    static RULES: LazyLock<[(Regex, &str); 6]> = LazyLock::new(|| {
        [
            (Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap(), "$1"), // [text](url) → text
            (Regex::new(r"\[\d+(?:[,\s]*\d+)*\]").unwrap(), ""),   // [1], [2, 3]
            (Regex::new(r"https?://\S+").unwrap(), ""),            // bare URLs
            (Regex::new(r"(?m)^\s*(?:[-*•]|\d+\.)\s+|[*_`#>|~]").unwrap(), ""), // bullets, emphasis, headings
            (Regex::new(r"\s+").unwrap(), " "),
            (Regex::new(r" ([.,!?;:])").unwrap(), "$1"), // space left before punctuation
        ]
    });
    RULES.iter().fold(text.to_string(), |s, (re, rep)| re.replace_all(&s, *rep).into_owned()).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sentences_but_not_decimals() {
        let buf = "It is 28.5 degrees. Humid too";
        let end = sentence_end(buf).unwrap();
        assert_eq!(&buf[..end], "It is 28.5 degrees.");
        assert_eq!(sentence_end("Humid too"), None);
    }

    #[test]
    fn cleans_markdown_citations_urls() {
        let s = "**Sunny** in [Mumbai](https://x.com) today [1][2]. See https://weather.com\n- High: 31°C";
        assert_eq!(clean(s), "Sunny in Mumbai today. See High: 31°C");
    }
}
