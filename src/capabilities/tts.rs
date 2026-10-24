//! Text-to-speech slot: `[capabilities.tts] provider` picks the adapter.

use super::Warm;
use crate::adapters::tts::kokoro;
use crate::core::contracts::capabilities::Tts;
use crate::core::contracts::settings::ProviderConfig;
use anyhow::{Result, bail};

pub fn create(cfg: &ProviderConfig) -> Result<Warm<Box<dyn Tts>>> {
    let cfg = cfg.clone();
    Warm::new(cfg.keep_warm, move || -> Result<Box<dyn Tts>> {
        Ok(match cfg.provider.as_str() {
            "kokoro" => Box::new(kokoro::Kokoro::new(&cfg.settings)?),
            other => bail!("unknown tts provider '{other}'"),
        })
    })
}
