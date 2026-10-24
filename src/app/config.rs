//! Load and validate `config.toml` and persona files. Each section's type belongs to
//! the part it configures; this file only assembles them.

use crate::adapters::models::llama_cpp::LlmConfig;
use crate::core::agent::Persona;
use crate::core::contracts::settings::{ProviderConfig, expand};
use crate::memory::{SessionConfig, StorageConfig};
use crate::routines::greeting::GreetingConfig;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub persona: PersonaConfig,
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub capabilities: BTreeMap<String, ProviderConfig>,
    /// Skill name → its settings table; each skill parses its own settings.
    #[serde(default)]
    pub skills: BTreeMap<String, toml::Table>,
    #[serde(default)]
    pub greeting: GreetingConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersonaConfig {
    pub default: String,
    #[serde(default = "d_persona_dir")]
    pub dir: PathBuf,
    /// Tool-use examples shown to the model (relative to this file).
    #[serde(default = "d_tool_examples")]
    pub tool_examples: PathBuf,
}

fn d_tool_examples() -> PathBuf {
    "data/tool_examples.toml".into()
}
fn d_persona_dir() -> PathBuf {
    "personas".into()
}

impl Config {
    pub fn load(path: &Path) -> Result<(Self, BTreeMap<String, Persona>)> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut cfg: Config = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let base = path.parent().unwrap_or(Path::new("."));
        cfg.llm.model = expand(&cfg.llm.model);
        cfg.storage.dir = expand(&cfg.storage.dir);
        cfg.persona.dir = base.join(expand(&cfg.persona.dir));
        cfg.persona.tool_examples = base.join(expand(&cfg.persona.tool_examples));

        let personas = load_personas(&cfg.persona.dir)?;
        if !personas.contains_key(&cfg.persona.default) {
            bail!("default persona '{}' not found in {}", cfg.persona.default, cfg.persona.dir.display());
        }
        if !cfg.llm.model.exists() {
            bail!("LLM model not found: {}", cfg.llm.model.display());
        }
        Ok((cfg, personas))
    }

    /// Find the config file: `--config` / `$JARVIS_CONFIG`, else `./config.toml`, else `~/.config/jarvis/config.toml`.
    pub fn locate(explicit: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(path) = explicit {
            return Ok(path);
        }
        let candidates = [PathBuf::from("config.toml"), expand(Path::new("~/.config/jarvis/config.toml"))];
        candidates.iter().find(|p| p.is_file()).cloned().with_context(|| {
            format!(
                "no config found (looked in {} and {}); pass --config or set JARVIS_CONFIG",
                candidates[0].display(),
                candidates[1].display()
            )
        })
    }

    pub fn capability(&self, name: &str) -> Result<&ProviderConfig> {
        self.capabilities.get(name).with_context(|| format!("[capabilities.{name}] missing from config"))
    }
}

fn load_personas(dir: &Path) -> Result<BTreeMap<String, Persona>> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "toml") {
            let mut p: Persona = toml::from_str(&std::fs::read_to_string(&path)?)
                .with_context(|| format!("parsing {}", path.display()))?;
            p.id = path.file_stem().unwrap().to_string_lossy().to_lowercase();
            out.insert(p.id.clone(), p);
        }
    }
    Ok(out)
}
