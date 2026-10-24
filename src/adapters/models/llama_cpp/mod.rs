//! Embedded llama.cpp: model loading, token streaming, chat template, tool-call
//! parsing and KV-cache reuse, all behind the `Llm` contract.

mod cache;
mod engine;
mod template;
mod tools;

use crate::core::contracts::model::Llm;
use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

/// The `[llm]` config section.
#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub model: PathBuf,
    pub template: String,
    #[serde(default = "d_ctx")]
    pub context_size: u32,
    #[serde(default = "d_gpu")]
    pub gpu_layers: u32,
    #[serde(default = "yes")]
    pub flash_attn: bool,
    #[serde(default = "yes")]
    pub keep_warm: bool,
    #[serde(default = "d_max_tokens")]
    pub max_tokens: u32,
}

fn yes() -> bool {
    true
}
fn d_ctx() -> u32 {
    8192
}
fn d_gpu() -> u32 {
    99
}
fn d_max_tokens() -> u32 {
    1024
}

pub fn create(cfg: &LlmConfig) -> Result<Arc<dyn Llm>> {
    Ok(Arc::new(engine::LlamaEngine::spawn(cfg)?))
}
