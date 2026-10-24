//! What a turn produces besides its reply: the streamed reply itself (`Sink`) and
//! per-turn stage timings (`TurnMetrics`) for whoever records them (`MetricsSink`).

use super::model::GenStats;
use serde::Serialize;
use serde_json::Value;
use std::time::Instant;

/// Where a reply goes as it streams.
pub trait Sink {
    fn text(&mut self, delta: &str);
    fn tool(&mut self, _name: &str, _args: &Value) {}
    fn end(&mut self) {}
}

/// Records finished turns (metrics log, dashboards, benchmarks).
pub trait MetricsSink: Send + Sync {
    fn record(&self, metrics: &TurnMetrics);
}

#[derive(Debug, Default, Serialize)]
pub struct TurnMetrics {
    pub ts: String,
    pub mode: &'static str,
    pub persona: String,
    pub input: String,
    pub stt_ms: Option<f64>,
    /// One entry per LLM round (a tool call adds a round).
    pub llm: Vec<GenStats>,
    /// Input → first visible token.
    pub first_token_ms: Option<f64>,
    pub tools: Vec<ToolMetric>,
    pub malformed_tool_calls: usize,
    pub tts_first_audio_ms: Option<f64>,
    /// End of speech → spoken "let me check" filler (voice turns that used a tool).
    pub filler_ms: Option<f64>,
    /// Headline: end of speech → first audio (voice) or Enter → first token (text).
    pub latency_ms: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct ToolMetric {
    pub name: String,
    pub ms: f64,
    pub ok: bool,
    /// Served from the skill's cache.
    pub cached: bool,
    /// The skill failed and web search answered instead.
    pub fallback: bool,
}

impl TurnMetrics {
    pub fn new(mode: &'static str, persona: &str, input: &str) -> Self {
        Self {
            ts: chrono::Local::now().to_rfc3339(),
            mode,
            persona: persona.into(),
            input: input.into(),
            ..Default::default()
        }
    }

    pub fn summary(&self) -> String {
        let mut s = match (self.latency_ms, self.mode) {
            (Some(l), "voice") => format!("{l:.0} ms speech end → first audio"),
            (Some(l), _) => format!("{l:.0} ms to first token"),
            (None, _) => "no output".into(),
        };
        if let Some(last) = self.llm.last().filter(|g| g.gen_ms > 0.0) {
            s += &format!(" · {:.1} tok/s", last.gen_tokens as f64 / (last.gen_ms / 1000.0));
        }
        for t in &self.tools {
            let note = match (t.cached, t.fallback, t.ok) {
                (true, ..) => " (cached)",
                (_, true, _) => " (fallback)",
                (.., false) => " (failed)",
                _ => "",
            };
            s += &format!(" · {} {:.0} ms{note}", t.name, t.ms);
        }
        s
    }
}

pub fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
