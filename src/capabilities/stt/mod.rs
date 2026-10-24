//! Speech-to-text.

mod whisper;

use super::Warm;
use crate::config::ProviderConfig;
use anyhow::{Result, bail};

pub trait Stt: Send {
    /// Transcribe 16 kHz mono PCM.
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String>;
}

pub const SAMPLE_RATE: u32 = 16_000;

pub fn create(cfg: &ProviderConfig) -> Result<Warm<Box<dyn Stt>>> {
    let cfg = cfg.clone();
    Warm::new(cfg.keep_warm, move || -> Result<Box<dyn Stt>> {
        Ok(match cfg.provider.as_str() {
            "whisper" => Box::new(whisper::Whisper::new(&cfg.settings)?),
            other => bail!("unknown stt provider '{other}'"),
        })
    })
}
