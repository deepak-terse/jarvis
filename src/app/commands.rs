//! The command line: `jarvis chat`, `jarvis voice`, `jarvis bench`.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// JARVIS — local-first voice assistant.
#[derive(Parser)]
pub struct Cli {
    /// Config file [default: ./config.toml, then ~/.config/jarvis/config.toml]
    #[arg(long, env = "JARVIS_CONFIG")]
    pub config: Option<PathBuf>,
    /// Persona to start with (defaults to `[persona] default`).
    #[arg(long)]
    pub persona: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
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
