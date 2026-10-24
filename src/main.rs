mod audio;
mod bench;
mod capabilities;
mod config;
mod core;
mod llm;
mod skills;
mod storage;
mod telemetry;

use crate::config::Config;
use crate::core::agent::{Agent, Console};
use crate::core::session::Session;
use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};

type Greeting = tokio::sync::oneshot::Receiver<String>;

/// JARVIS — local-first voice assistant.
#[derive(Parser)]
struct Cli {
    /// Config file [default: ./config.toml, then ~/.config/jarvis/config.toml]
    #[arg(long, env = "JARVIS_CONFIG")]
    config: Option<PathBuf>,
    /// Persona to start with (defaults to `[persona] default`).
    #[arg(long)]
    persona: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Text conversation.
    Chat,
    /// Push-to-talk voice conversation.
    Voice,
    /// Tool-choice accuracy and latency benchmark.
    Bench {
        /// Cases file [default: bench/cases.toml next to the config]
        file: Option<PathBuf>,
        /// Run full turns with tools executed and print each answer for manual grading.
        #[arg(long)]
        live: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
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
    let storage = storage::Storage::open(&cfg.storage.dir)?;
    telemetry::init(storage.trace_log()?);

    let config_dir = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let skill_ctx = skills::SkillContext::new(storage.profile_table()?, config_dir, storage.skills_dir())?;
    let skills = skills::Registry::from_config(&cfg.skills, &skill_ctx)?;
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
            crate::core::greeting::spawn(skills.clone(), &cfg.greeting, name)
        }
        Command::Bench { .. } => None,
    };

    eprintln!("Loading {}…", cfg.llm.model.display());
    let llm = llm::create(&cfg.llm)?;
    let examples = crate::core::examples::ToolExamples::load(&cfg.persona.tool_examples)?;
    let agent = Agent::new(llm, skills, personas, storage, examples);
    let mut session = Session::new(persona, cfg.session.history_messages);
    if cfg.llm.keep_warm {
        agent.warm(&session.persona);
    }

    match cli.command {
        Command::Chat => chat(&agent, &mut session, greeting).await,
        Command::Voice => crate::core::voice::run(&agent, &mut session, &cfg, greeting).await,
        Command::Bench { file, live } => {
            let file = file.unwrap_or_else(|| config_path.parent().unwrap_or(Path::new(".")).join("bench/cases.toml"));
            bench::run(&agent, &session.persona, &file, live).await
        }
    }
}

async fn chat(agent: &Agent, session: &mut Session, mut greeting: Option<Greeting>) -> Result<()> {
    println!("JARVIS text mode — /persona <name> to switch, /quit to exit.");
    if let Some(text) = greeting.as_mut().and_then(|g| g.try_recv().ok()) {
        println!("\n{}> {text}", agent.persona(session).name);
    }
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        print!("\nyou> ");
        std::io::stdout().flush()?;
        let Some(line) = lines.next_line().await? else { break };
        let input = line.trim();
        match input {
            "" => continue,
            "/quit" | "/exit" => break,
            _ => {}
        }
        let t0 = Instant::now(); // Enter pressed
        print!("{}> ", agent.persona(session).name);
        match agent.respond(session, input, "text", t0, &mut Console).await {
            Ok(Some(metrics)) => {
                println!("\x1b[2m  ⏱ {}\x1b[0m", metrics.summary());
                agent.log_metrics(&metrics);
            }
            Ok(None) => {}
            Err(e) => eprintln!("error: {e:#}"),
        }
    }
    Ok(())
}
