//! Tool definitions as the LLM sees them, plus format-agnostic helpers for
//! pulling tool calls out of raw model output.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP-shaped tool description: name, description, JSON schema.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Extract `open … close` blocks containing `{"name", "arguments"}` JSON.
/// An unterminated final block (generation stopped early) is still attempted.
pub fn parse_tagged(raw: &str, open: &str, close: &str) -> (Vec<ToolCall>, Vec<String>) {
    let (mut calls, mut bad) = (Vec::new(), Vec::new());
    let mut rest = raw;
    while let Some(start) = rest.find(open) {
        let body = &rest[start + open.len()..];
        let (block, next) = match body.find(close) {
            Some(end) => (&body[..end], &body[end + close.len()..]),
            None => (body, ""),
        };
        match serde_json::from_str::<ToolCall>(block.trim()) {
            Ok(mut call) => {
                // Some models emit arguments as a JSON-encoded string.
                if let Value::String(s) = &call.arguments {
                    call.arguments = serde_json::from_str(s).unwrap_or(Value::Null);
                }
                calls.push(call);
            }
            Err(_) => bad.push(block.trim().to_string()),
        }
        rest = next;
    }
    (calls, bad)
}

/// Hides marked spans (e.g. `<think>…</think>`, `<tool_call>…</tool_call>`) from a
/// token stream, holding back just enough text to recognise markers split across tokens.
pub struct StreamFilter {
    spans: &'static [(&'static str, &'static str)],
    buf: String,
    inside: Option<usize>,
    visible: String,
}

impl StreamFilter {
    pub fn new(spans: &'static [(&'static str, &'static str)]) -> Self {
        Self { spans, buf: String::new(), inside: None, visible: String::new() }
    }

    /// Feed raw text; returns the newly visible part.
    pub fn push(&mut self, text: &str) -> String {
        self.buf.push_str(text);
        let mut out = String::new();
        loop {
            match self.inside {
                None => {
                    let hit = self
                        .spans
                        .iter()
                        .enumerate()
                        .filter_map(|(i, (open, _))| self.buf.find(open).map(|pos| (pos, i)))
                        .min();
                    if let Some((pos, i)) = hit {
                        out.push_str(&self.buf[..pos]);
                        self.buf.drain(..pos + self.spans[i].0.len());
                        self.inside = Some(i);
                        continue;
                    }
                    let hold = self.spans.iter().map(|(open, _)| partial_suffix(&self.buf, open)).max().unwrap_or(0);
                    let cut = self.buf.len() - hold;
                    out.push_str(&self.buf[..cut]);
                    self.buf.drain(..cut);
                }
                Some(i) => {
                    let close = self.spans[i].1;
                    if let Some(pos) = self.buf.find(close) {
                        self.buf.drain(..pos + close.len());
                        self.inside = None;
                        continue;
                    }
                    let cut = self.buf.len() - partial_suffix(&self.buf, close);
                    self.buf.drain(..cut);
                }
            }
            break;
        }
        self.visible.push_str(&out);
        out
    }

    /// Flush held-back text at end of stream; returns (newly visible, all visible).
    pub fn finish(mut self) -> (String, String) {
        let tail = if self.inside.is_none() { std::mem::take(&mut self.buf) } else { String::new() };
        self.visible.push_str(&tail);
        (tail, self.visible)
    }
}

/// Length of the longest suffix of `s` that is a proper prefix of `marker`.
fn partial_suffix(s: &str, marker: &str) -> usize {
    (1..marker.len().min(s.len() + 1)).rev().find(|&k| s.ends_with(&marker[..k])).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPANS: &[(&str, &str)] = &[("<think>", "</think>"), ("<tool_call>", "</tool_call>")];

    #[test]
    fn hides_spans_split_across_chunks() {
        let mut f = StreamFilter::new(SPANS);
        let chunks = ["Hi <thi", "nk>secret</th", "ink>there <tool", "_call>{}</tool_call> end"];
        let shown: String = chunks.iter().map(|c| f.push(c)).collect();
        let (tail, all) = f.finish();
        assert_eq!(shown + &tail, "Hi there  end");
        assert_eq!(all, "Hi there  end");
    }

    #[test]
    fn parses_calls_and_reports_malformed() {
        let raw = r#"<tool_call>
{"name": "web_search", "arguments": {"query": "weather Mumbai"}}
</tool_call><tool_call>{"name": oops}</tool_call>"#;
        let (calls, bad) = parse_tagged(raw, "<tool_call>", "</tool_call>");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments["query"], "weather Mumbai");
        assert_eq!(bad.len(), 1);
    }
}
