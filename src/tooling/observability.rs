//! Diagnostics: tracing to JSONL and the per-turn metrics log.

use crate::core::contracts::events::{MetricsSink, TurnMetrics};
use crate::memory::append_jsonl;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing_subscriber::EnvFilter;

/// Diagnostic logs as JSONL; level via `RUST_LOG` (default: warnings, including llama.cpp's).
pub fn init(log: File) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt().json().with_env_filter(filter).with_writer(Mutex::new(log)).init();
}

/// Appends one JSON line per turn to `logs/metrics.jsonl`.
pub struct MetricsLog {
    path: PathBuf,
}

impl MetricsLog {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl MetricsSink for MetricsLog {
    fn record(&self, metrics: &TurnMetrics) {
        append_jsonl(&self.path, metrics);
    }
}
