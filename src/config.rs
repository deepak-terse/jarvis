//! Load and validate `config.toml` and persona files.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
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

/// Startup greeting: a historical event plus a few headlines, prepared in the background.
#[derive(Debug, Clone, Deserialize)]
pub struct GreetingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "d_greeting_headlines")]
    pub headlines: usize,
    /// News category to take headlines from (empty: the news skill's default).
    #[serde(default)]
    pub news_category: String,
}

impl Default for GreetingConfig {
    fn default() -> Self {
        Self { enabled: false, headlines: d_greeting_headlines(), news_category: String::new() }
    }
}

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

#[derive(Debug, Clone, Deserialize)]
pub struct PersonaConfig {
    pub default: String,
    #[serde(default = "d_persona_dir")]
    pub dir: PathBuf,
    /// Tool-use examples shown to the model (relative to this file).
    #[serde(default = "d_tool_examples")]
    pub tool_examples: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "d_history")]
    pub history_messages: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self { history_messages: d_history() }
    }
}

/// A capability slot (`stt`, `tts`, …): which provider fills it, plus provider-specific settings.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub provider: String,
    #[serde(default = "yes")]
    pub keep_warm: bool,
    #[serde(flatten)]
    pub settings: toml::Table,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub dir: PathBuf,
}

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
fn d_greeting_headlines() -> usize {
    3
}
fn d_history() -> usize {
    20
}
fn d_temp() -> f32 {
    0.7
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

/// Deserialize a provider/skill settings table into its typed settings struct.
pub fn settings<T: DeserializeOwned>(table: &toml::Table) -> Result<T> {
    Ok(toml::Value::Table(table.clone()).try_into()?)
}

/// Expand a leading `~` to the home directory.
pub fn expand(path: &Path) -> PathBuf {
    match (path.strip_prefix("~"), std::env::var_os("HOME")) {
        (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => path.to_path_buf(),
    }
}
