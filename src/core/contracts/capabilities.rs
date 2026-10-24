//! Capabilities: senses and voice used by channels (`stt`, `tts`, later `vad`, `vision`).

use anyhow::Result;

/// Sample rate `Stt::transcribe` expects.
pub const SAMPLE_RATE: u32 = 16_000;

pub trait Stt: Send {
    /// Transcribe 16 kHz mono PCM.
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String>;
}

pub struct Audio {
    pub samples: Vec<f32>,
    pub rate: u32,
}

pub trait Tts: Send {
    fn synthesize(&mut self, text: &str, voice: &str) -> Result<Audio>;
}
