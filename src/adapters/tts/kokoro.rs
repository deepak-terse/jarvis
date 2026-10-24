//! Kokoro-82M via sherpa-onnx.

use crate::core::contracts::capabilities::{Audio, Tts};
use crate::core::contracts::settings::{expand, settings};
use anyhow::{Context, Result};
use serde::Deserialize;
use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig};
use std::path::PathBuf;

/// Speaker order of `kokoro-en-v0_19`; override with `voices = [...]` for other releases.
const DEFAULT_VOICES: &[&str] = &[
    "af",
    "af_bella",
    "af_nicole",
    "af_sarah",
    "af_sky",
    "am_adam",
    "am_michael",
    "bf_emma",
    "bf_isabella",
    "bm_george",
    "bm_lewis",
];

#[derive(Deserialize)]
struct Settings {
    model_dir: PathBuf,
    voices: Option<Vec<String>>,
    #[serde(default = "d_speed")]
    speed: f32,
    #[serde(default = "d_threads")]
    threads: i32,
}

fn d_speed() -> f32 {
    1.0
}
fn d_threads() -> i32 {
    4
}

pub struct Kokoro {
    tts: OfflineTts,
    voices: Vec<String>,
    speed: f32,
}

impl Kokoro {
    pub fn new(table: &toml::Table) -> Result<Self> {
        let s: Settings = settings(table)?;
        let dir = expand(&s.model_dir);
        let path = |f: &str| Some(dir.join(f).to_string_lossy().into_owned());
        let mut cfg = OfflineTtsConfig::default();
        cfg.model.kokoro = OfflineTtsKokoroModelConfig {
            model: path("model.onnx"),
            voices: path("voices.bin"),
            tokens: path("tokens.txt"),
            data_dir: path("espeak-ng-data"),
            ..Default::default()
        };
        cfg.model.num_threads = s.threads;
        cfg.max_num_sentences = 1;
        let tts = OfflineTts::create(&cfg).with_context(|| format!("loading Kokoro from {}", dir.display()))?;
        let voices = s.voices.unwrap_or_else(|| DEFAULT_VOICES.iter().map(|v| v.to_string()).collect());
        Ok(Self { tts, voices, speed: s.speed })
    }
}

impl Tts for Kokoro {
    fn synthesize(&mut self, text: &str, voice: &str) -> Result<Audio> {
        let sid = self
            .voices
            .iter()
            .position(|v| v == voice)
            .with_context(|| format!("unknown Kokoro voice '{voice}' (available: {})", self.voices.join(", ")))?;
        let gen_cfg = GenerationConfig { sid: sid as i32, speed: self.speed, ..Default::default() };
        let audio = self
            .tts
            .generate_with_config(text, &gen_cfg, None::<fn(&[f32], f32) -> bool>)
            .context("Kokoro synthesis failed")?;
        Ok(Audio { samples: audio.samples().to_vec(), rate: audio.sample_rate() as u32 })
    }
}
