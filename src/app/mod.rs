//! The assembly point: reads config, builds the configured model, skills, memory and
//! capabilities, connects them to the agent, and runs the chosen channel. The only
//! module that knows every concrete type.

pub mod commands;
mod config;

use crate::adapters::models::llama_cpp;
use crate::channels::{terminal, voice};
use crate::core::agent::{Agent, Session, ToolExamples};
use crate::core::contracts::tools::Tools;
use crate::memory::{Store, Window};
use crate::routines::greeting;
use crate::skills::{Registry, SkillContext};
use crate::tooling::{bench, observability};
use anyhow::{Result, bail};
use commands::{Cli, Command};
use config::Config;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn start(cli: Cli) -> Result<()> {
    let config_path = Config::locate(cli.config.clone())?;
    // Load secrets before starting runtime threads. Exported variables win.
    let env_path = config_path.parent().unwrap_or(Path::new(".")).join(".env");
    match dotenvy::from_path(&env_path) {
        Ok(()) => {}
        Err(dotenvy::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => bail!("could not load {}; check its permissions and KEY=value syntax", env_path.display()),
    }
    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(run(cli, config_path))
}

async fn run(cli: Cli, config_path: PathBuf) -> Result<()> {
    let (cfg, personas) = Config::load(&config_path)?;
    let store = Store::open(&cfg.storage.dir)?;
    observability::init(store.trace_log()?);

    let skill_ctx = SkillContext::new(store.profile_table()?, store.skills_dir())?;
    let skills = Registry::from_config(&cfg.skills, &skill_ctx)?;
    for p in personas.values() {
        for s in p.skills.iter().filter(|s| !skills.contains(s)) {
            eprintln!("warning: persona '{}' lists unknown or disabled skill '{s}'", p.id);
        }
    }
    let persona = cli.persona.map_or_else(|| cfg.persona.default.clone(), |p| p.to_lowercase());
    if !personas.contains_key(&persona) {
        bail!("unknown persona '{persona}' (available: {})", personas.keys().cloned().collect::<Vec<_>>().join(", "));
    }

    // Prepared while the models load; never waited on.
    let greeting = match cli.command {
        Command::Chat | Command::Voice => {
            let name = skill_ctx.profile_str("name").map(str::to_string);
            greeting::spawn(skills.clone(), &cfg.greeting, name)
        }
        Command::Bench { .. } => None,
    };

    eprintln!("Loading {}…", cfg.llm.model.display());
    let llm = llama_cpp::create(&cfg.llm)?;
    let examples = ToolExamples::load(&cfg.persona.tool_examples)?;
    let metrics = Arc::new(observability::MetricsLog::new(store.metrics_log()));
    let agent = Agent::new(llm, Arc::new(skills), Arc::new(store), metrics, personas, examples);
    let mut session = Session::new(persona, Box::new(Window::new(cfg.session.history_messages)));
    if cfg.llm.keep_warm {
        agent.warm(&session.persona);
    }

    match cli.command {
        Command::Chat => terminal::run(&agent, &mut session, greeting).await,
        Command::Voice => {
            voice::run(&agent, &mut session, cfg.capability("stt")?, cfg.capability("tts")?, greeting).await
        }
        Command::Bench { file, live } => {
            let file = file.unwrap_or_else(|| config_path.parent().unwrap_or(Path::new(".")).join("bench/cases.toml"));
            bench::run(&agent, &session.persona, &file, live).await
        }
    }
}

/// Phase 0 smoke test: LLM, STT and TTS loaded and running in one process.
/// `cargo test --release -- --ignored` (needs the models from config.toml).
#[cfg(test)]
mod smoke {
    use super::config::Config;
    use crate::adapters::audio::resample;
    use crate::adapters::models::llama_cpp;
    use crate::capabilities::{stt, tts};
    use crate::core::contracts::capabilities::SAMPLE_RATE;
    use crate::core::contracts::model::{ChatRequest, LlmEvent, Message, Prefix, Role};
    use crate::core::contracts::settings::expand;

    #[tokio::test]
    #[ignore]
    async fn llm_stt_tts_in_one_process() {
        let (cfg, _) = Config::load("config.toml".as_ref()).unwrap();
        let llm = llama_cpp::create(&cfg.llm).unwrap();
        let mut stt = stt::create(cfg.capability("stt").unwrap()).unwrap();
        let mut tts = tts::create(cfg.capability("tts").unwrap()).unwrap();

        let audio = tts.get().unwrap().synthesize("Hello there, how is the weather today?", "af_bella").unwrap();
        let pcm = resample(&audio.samples, audio.rate, SAMPLE_RATE);
        let heard = stt.get().unwrap().transcribe(&pcm).unwrap().to_lowercase();
        assert!(heard.contains("weather"), "round-trip transcript: {heard}");

        let wav_dir = cfg.capability("stt").unwrap().settings["model"].as_str().unwrap().to_string();
        let wav = sherpa_onnx::Wave::read(&format!("{}/test_wavs/0.wav", expand(wav_dir.as_ref()).display())).unwrap();
        println!("whisper: {}", stt.get().unwrap().transcribe(wav.samples()).unwrap());

        let prefix = Prefix { cache_key: "t".into(), system: "Be brief.".into(), tools: vec![] };
        let req = ChatRequest { prefix, messages: vec![Message::new(Role::User, heard)], temperature: 0.0 };
        let mut rx = llm.chat(req);
        while let Some(ev) = rx.recv().await {
            if let LlmEvent::Done(c) = ev {
                assert!(!c.text.is_empty());
                println!("llm: {}", c.text);
                break;
            }
            assert!(!matches!(ev, LlmEvent::Error(_)), "{ev:?}");
        }
    }
}
