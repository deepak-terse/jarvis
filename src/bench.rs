//! `jarvis bench`: tool-choice accuracy and latency over a set of real questions,
//! reported per category.
//!
//! Default mode scores only the model's first decision (tools are not executed).
//! `--live` runs full turns with tools, and prints each answer plus tool latency,
//! cache hits and fallbacks for manual grading.

use crate::core::agent::{Agent, Sink};
use crate::core::session::Session;
use crate::telemetry::{ToolMetric, TurnMetrics};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

#[derive(Deserialize)]
struct Cases {
    case: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    prompt: String,
    #[serde(default = "d_category")]
    category: String,
    /// Acceptable first tool(s); empty or omitted for "answer directly".
    #[serde(default)]
    expect: Expect,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum Expect {
    #[default]
    None,
    One(String),
    Any(Vec<String>),
}

impl Expect {
    fn accepts(&self, chosen: &str) -> bool {
        match self {
            Self::None => chosen.is_empty(),
            Self::One(t) => t == chosen,
            Self::Any(ts) => ts.iter().any(|t| t == chosen) || (ts.is_empty() && chosen.is_empty()),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::One(t) if !t.is_empty() => t.clone(),
            Self::Any(ts) if !ts.is_empty() => ts.join("|"),
            _ => "(none)".into(),
        }
    }
}

fn d_category() -> String {
    "general".into()
}

#[derive(Default)]
struct Tally {
    total: usize,
    correct: usize,
    first_token: Vec<f64>,
    tool_ms: Vec<f64>,
    fallbacks: usize,
    cached: usize,
}

pub async fn run(agent: &Agent, persona: &str, file: &Path, live: bool) -> Result<()> {
    let cases: Cases =
        toml::from_str(&std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?)?;
    let mut by_category: BTreeMap<String, Tally> = BTreeMap::new();
    for case in &cases.case {
        let (m, answer) = if live {
            let mut session = Session::new(persona.into(), 0);
            let mut sink = Collect(String::new());
            let m = agent.respond(&mut session, &case.prompt, "bench", Instant::now(), &mut sink).await?;
            (m.context("bench prompt was handled as a built-in command")?, Some(sink.0))
        } else {
            let mut m = TurnMetrics::new("bench", persona, &case.prompt);
            let done = agent.probe(persona, &case.prompt, &mut m).await?;
            m.tools =
                done.tool_calls.iter().map(|c| ToolMetric { name: c.name.clone(), ..Default::default() }).collect();
            if !case.expect.accepts(done.tool_calls.first().map_or("", |c| c.name.as_str())) && !done.text.is_empty() {
                println!("    unexpected direct reply: {}", done.text.trim());
            }
            (m, None)
        };
        let chosen = m.tools.first().map_or(String::new(), |t| t.name.clone());
        let ok = case.expect.accepts(&chosen);
        let t = by_category.entry(case.category.clone()).or_default();
        t.total += 1;
        t.correct += ok as usize;
        t.first_token.extend(m.first_token_ms);
        t.tool_ms.extend(m.tools.iter().map(|x| x.ms));
        t.fallbacks += m.tools.iter().filter(|x| x.fallback).count();
        t.cached += m.tools.iter().filter(|x| x.cached).count();
        let stats = m.llm.first().cloned().unwrap_or_default();
        println!(
            "{} [{:<10}] {:<45} expected {:<16} got {:<12} prompt {:>5.0} ms ({} of {} cached)",
            if ok { "✓" } else { "✗" },
            truncate(&case.category, 10),
            truncate(&case.prompt, 45),
            case.expect.label(),
            or_none(&chosen),
            stats.prompt_ms,
            stats.cached_tokens,
            stats.prompt_tokens,
        );
        if let Some(answer) = answer {
            let tools: Vec<String> = m.tools.iter().map(|t| format!("{} {:.0} ms", t.name, t.ms)).collect();
            println!("    tools: {}\n    answer: {}", or_none(&tools.join(", ")), answer.trim().replace('\n', " "));
            println!("    length: {} words, {} characters", answer.split_whitespace().count(), answer.chars().count());
        }
    }

    println!(
        "\n{:<12} {:>9} {:>14} {:>12} {:>10} {:>7}",
        "category", "accuracy", "first token", "tool", "fallbacks", "cached"
    );
    let mut all = Tally::default();
    for (category, mut t) in by_category {
        print_row(&category, &mut t);
        all.total += t.total;
        all.correct += t.correct;
        all.first_token.append(&mut t.first_token);
        all.tool_ms.append(&mut t.tool_ms);
        all.fallbacks += t.fallbacks;
        all.cached += t.cached;
    }
    print_row("all", &mut all);
    Ok(())
}

fn print_row(category: &str, t: &mut Tally) {
    let tool = if t.tool_ms.is_empty() { "-".into() } else { format!("{:.0} ms", median(&mut t.tool_ms)) };
    println!(
        "{:<12} {:>4}/{:<4} {:>11.0} ms {:>12} {:>10} {:>7}",
        category,
        t.correct,
        t.total,
        median(&mut t.first_token),
        tool,
        t.fallbacks,
        t.cached
    );
}

struct Collect(String);

impl Sink for Collect {
    fn text(&mut self, delta: &str) {
        self.0.push_str(delta);
    }
}

fn or_none(s: &str) -> &str {
    if s.is_empty() { "(none)" } else { s }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.into() } else { s.chars().take(n - 1).chain(['…']).collect() }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    if v.is_empty() { 0.0 } else { v[v.len() / 2] }
}
