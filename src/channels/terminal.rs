//! Text conversation in the terminal (`jarvis chat`).

use crate::core::agent::{Agent, Session};
use crate::core::contracts::events::Sink;
use anyhow::Result;
use serde_json::Value;
use std::io::Write;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;

/// Prints the reply to the terminal.
pub struct Console;

impl Sink for Console {
    fn text(&mut self, delta: &str) {
        print!("{delta}");
        let _ = std::io::stdout().flush();
    }
    fn tool(&mut self, name: &str, args: &Value) {
        print!("\x1b[2m[{name} {args}]\x1b[0m ");
        let _ = std::io::stdout().flush();
    }
    fn end(&mut self) {
        println!();
    }
}

pub async fn run(agent: &Agent, session: &mut Session, mut greeting: Option<oneshot::Receiver<String>>) -> Result<()> {
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
