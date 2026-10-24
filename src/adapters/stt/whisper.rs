//! Whisper via sherpa-onnx (ONNX Runtime). whisper.cpp is avoided because its
//! bundled ggml clashes with llama.cpp's when both are linked into one binary.

use crate::core::contracts::capabilities::{SAMPLE_RATE, Stt};
use crate::core::contracts::settings::{expand, settings};
use anyhow::{Context, Result};
use serde::Deserialize;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineWhisperModelConfig};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Settings {
    /// Directory with `*-encoder.onnx`, `*-decoder.onnx`, `*-tokens.txt` (int8 preferred).
    model: PathBuf,
    language: Option<String>,
    #[serde(default = "d_threads")]
    threads: i32,
}

fn d_threads() -> i32 {
    4
}

pub struct Whisper {
    recognizer: OfflineRecognizer,
}

impl Whisper {
    pub fn new(table: &toml::Table) -> Result<Self> {
        let s: Settings = settings(table)?;
        let dir = expand(&s.model);
        let mut cfg = OfflineRecognizerConfig::default();
        cfg.model_config.whisper = OfflineWhisperModelConfig {
            encoder: Some(find(&dir, &["encoder.int8.onnx", "encoder.onnx"])?),
            decoder: Some(find(&dir, &["decoder.int8.onnx", "decoder.onnx"])?),
            language: s.language,
            task: Some("transcribe".into()),
            tail_paddings: -1,
            ..Default::default()
        };
        cfg.model_config.tokens = Some(find(&dir, &["tokens.txt"])?);
        cfg.model_config.num_threads = s.threads;
        let recognizer =
            OfflineRecognizer::create(&cfg).with_context(|| format!("loading Whisper from {}", dir.display()))?;
        Ok(Self { recognizer })
    }
}

impl Stt for Whisper {
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, pcm);
        self.recognizer.decode(&stream);
        Ok(stream.get_result().map(|r| r.text.trim().to_string()).unwrap_or_default())
    }
}

/// First file in `dir` whose name ends with one of `suffixes` (in preference order).
fn find(dir: &Path, suffixes: &[&str]) -> Result<String> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    suffixes
        .iter()
        .find_map(|suf| names.iter().find(|n| n.ends_with(suf)))
        .map(|n| dir.join(n).to_string_lossy().into_owned())
        .with_context(|| format!("no *{} in {}", suffixes[0], dir.display()))
}
