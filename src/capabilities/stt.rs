//! Speech-to-text slot: `[capabilities.stt] provider` picks the adapter.

use super::Warm;
use crate::adapters::stt::whisper;
use crate::core::contracts::capabilities::Stt;
use crate::core::contracts::settings::ProviderConfig;
use anyhow::{Result, bail};

pub fn create(cfg: &ProviderConfig) -> Result<Warm<Box<dyn Stt>>> {
    let cfg = cfg.clone();
    Warm::new(cfg.keep_warm, move || -> Result<Box<dyn Stt>> {
        Ok(match cfg.provider.as_str() {
            "whisper" => Box::new(whisper::Whisper::new(&cfg.settings)?),
            other => bail!("unknown stt provider '{other}'"),
        })
    })
}
