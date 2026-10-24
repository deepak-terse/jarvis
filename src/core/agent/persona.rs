//! A persona: prompt, voice, allowed tools and sampling settings, loaded from `personas/*.toml`.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Persona {
    /// File stem, used for `/persona <id>` and as the KV-cache key.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub voice: String,
    #[serde(default = "d_temp")]
    pub temperature: f32,
    #[serde(default)]
    pub skills: Vec<String>,
    /// Short phrases spoken while a tool runs ("Let me check."); empty disables.
    #[serde(default)]
    pub fillers: Vec<String>,
    pub prompt: String,
}

fn d_temp() -> f32 {
    0.7
}
