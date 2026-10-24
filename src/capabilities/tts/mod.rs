//! Text-to-speech.

mod kokoro;

use super::Warm;
use crate::config::ProviderConfig;
use anyhow::{Result, bail};

pub struct Audio {
    pub samples: Vec<f32>,
    pub rate: u32,
}

pub trait Tts: Send {
    fn synthesize(&mut self, text: &str, voice: &str) -> Result<Audio>;
}

pub fn create(cfg: &ProviderConfig) -> Result<Warm<Box<dyn Tts>>> {
    let cfg = cfg.clone();
    Warm::new(cfg.keep_warm, move || -> Result<Box<dyn Tts>> {
        Ok(match cfg.provider.as_str() {
            "kokoro" => Box::new(kokoro::Kokoro::new(&cfg.settings)?),
            other => bail!("unknown tts provider '{other}'"),
        })
    })
}
