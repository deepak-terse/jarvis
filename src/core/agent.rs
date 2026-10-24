//! prompt → LLM → tool calls → loop → streamed reply. Input/output agnostic:
//! text and voice modes differ only in the `Sink` they pass in.

use super::{examples::ToolExamples, prompt, session::Session};
use crate::config::Persona;
use crate::llm::{ChatRequest, Completion, Llm, LlmEvent, Message, Prefix, Role};
use crate::skills::Registry;
use crate::storage::Storage;
use crate::telemetry::{ToolMetric, TurnMetrics, ms_since};
use anyhow::{Result, bail};
use regex::Regex;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

const MAX_TOOL_ROUNDS: usize = 3;

/// Where a reply goes as it streams.
pub trait Sink {
    fn text(&mut self, delta: &str);
    fn tool(&mut self, _name: &str, _args: &Value) {}
    fn end(&mut self) {}
}

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

pub struct Agent {
    llm: Arc<dyn Llm>,
    skills: Registry,
    personas: BTreeMap<String, Persona>,
    storage: Storage,
    examples: ToolExamples,
}

impl Agent {
    pub fn new(
        llm: Arc<dyn Llm>,
        skills: Registry,
        personas: BTreeMap<String, Persona>,
        storage: Storage,
        examples: ToolExamples,
    ) -> Self {
        Self { llm, skills, personas, storage, examples }
    }

    /// Whether `persona` may call `tool` (listed by the persona and enabled in config).
    fn can_use(&self, persona: &Persona, tool: &str) -> bool {
        persona.skills.iter().any(|s| s == tool) && self.skills.contains(tool)
    }

    /// Worked tool examples, then history: stable across turns, so their KV cache is reused.
    fn context(&self, persona: &Persona, history: &[Message]) -> Vec<Message> {
        let mut messages = self.examples.messages(|t| self.can_use(persona, t), &*self.llm);
        messages.extend_from_slice(history);
        messages
    }

    pub fn persona(&self, session: &Session) -> &Persona {
        &self.personas[&session.persona]
    }

    /// Pre-compute a persona's prompt prefix in the background.
    pub fn warm(&self, persona_id: &str) {
        match self.prefix(&self.personas[persona_id]) {
            Ok(prefix) => self.llm.warm(prefix),
            Err(e) => tracing::warn!("warm-up skipped: {e}"),
        }
    }

    pub fn log_metrics(&self, metrics: &TurnMetrics) {
        self.storage.log_metrics(metrics);
    }

    /// Handle one user input. Returns `None` if it was a built-in command.
    pub async fn respond(
        &self,
        session: &mut Session,
        input: &str,
        mode: &'static str,
        t0: Instant,
        sink: &mut dyn Sink,
    ) -> Result<Option<TurnMetrics>> {
        if let Some(reply) = self.command(session, input) {
            sink.text(&reply);
            sink.end();
            return Ok(None);
        }
        let persona = &self.personas[&session.persona];
        let prefix = self.prefix(persona)?;
        let mut metrics = TurnMetrics::new(mode, &persona.id, input);
        let mut messages = self.context(persona, session.history());
        messages.push(Message::new(Role::User, prompt::user_turn(input)));
        self.log(persona, Role::User, json!(input));

        let mut retried = false;
        let mut reply = String::new();
        for round in 0..=MAX_TOOL_ROUNDS {
            let req =
                ChatRequest { prefix: prefix.clone(), messages: messages.clone(), temperature: persona.temperature };
            let done = self.generate(req, t0, &mut metrics, sink).await?;
            let retry = done.tool_calls.is_empty() && !done.malformed.is_empty() && !retried;
            if (done.tool_calls.is_empty() && !retry) || round == MAX_TOOL_ROUNDS {
                reply = done.text;
                break;
            }
            messages.push(Message::new(Role::Assistant, done.raw));
            if retry {
                retried = true;
                messages.push(Message::new(Role::Tool, "Error: the tool call could not be parsed. Use the exact tool-call format with valid JSON, or answer directly."));
                continue;
            }
            for call in done.tool_calls {
                sink.tool(&call.name, &call.arguments);
                self.log(persona, Role::Assistant, json!({"tool_call": call}));
                let started = Instant::now();
                let out = self.skills.call(&persona.skills, &call.name, call.arguments).await;
                metrics.tools.push(ToolMetric {
                    name: call.name,
                    ms: ms_since(started),
                    ok: out.ok,
                    cached: out.cached,
                    fallback: out.fallback,
                });
                self.log(persona, Role::Tool, json!(out.text));
                messages.push(Message::new(Role::Tool, out.text));
            }
        }
        if reply.is_empty() {
            reply = "Sorry, I couldn't come up with an answer to that.".into();
            sink.text(&reply);
        }
        sink.end();
        self.log(persona, Role::Assistant, json!(reply));
        session.record(input, &reply);
        metrics.latency_ms = metrics.first_token_ms;
        Ok(Some(metrics))
    }

    /// One LLM round without history or tool execution: what would the model do first?
    pub async fn probe(&self, persona_id: &str, input: &str, metrics: &mut TurnMetrics) -> Result<Completion> {
        let persona = &self.personas[persona_id];
        let req = ChatRequest {
            prefix: self.prefix(persona)?,
            messages: {
                let mut m = self.context(persona, &[]);
                m.push(Message::new(Role::User, prompt::user_turn(input)));
                m
            },
            temperature: 0.0,
        };
        struct Silent;
        impl Sink for Silent {
            fn text(&mut self, _: &str) {}
        }
        self.generate(req, Instant::now(), metrics, &mut Silent).await
    }

    async fn generate(
        &self,
        req: ChatRequest,
        t0: Instant,
        metrics: &mut TurnMetrics,
        sink: &mut dyn Sink,
    ) -> Result<Completion> {
        let mut events = self.llm.chat(req);
        loop {
            match events.recv().await {
                Some(LlmEvent::Text(t)) => {
                    metrics.first_token_ms.get_or_insert_with(|| ms_since(t0));
                    sink.text(&t);
                }
                Some(LlmEvent::Done(done)) => {
                    metrics.llm.push(done.stats.clone());
                    metrics.malformed_tool_calls += done.malformed.len();
                    return Ok(done);
                }
                Some(LlmEvent::Error(e)) => bail!(e),
                None => bail!("LLM engine stopped"),
            }
        }
    }

    fn prefix(&self, persona: &Persona) -> Result<Prefix> {
        Ok(Prefix {
            cache_key: persona.id.clone(),
            system: prompt::system(
                persona,
                &self.storage.profile()?,
                &self.examples.routes(|t| self.can_use(persona, t)),
            ),
            tools: self.skills.specs(&persona.skills),
        })
    }

    /// Built-in commands, handled before the LLM: `/persona <id>` (text) or "switch to <id>" (voice).
    fn command(&self, session: &mut Session, input: &str) -> Option<String> {
        static PERSONA: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?i)^(/persona|switch to|persona)\s*(\w*)$").unwrap());
        let caps = PERSONA.captures(input.trim().trim_end_matches(['.', '!', '?']))?;
        let id = caps[2].to_lowercase();
        match self.personas.get(&id) {
            Some(p) => {
                session.persona = id.clone();
                self.warm(&id);
                Some(format!("Switched to {}.", p.name))
            }
            None if caps[1].starts_with('/') => {
                Some(format!("Available personas: {}", self.personas.keys().cloned().collect::<Vec<_>>().join(", ")))
            }
            None => None, // an ordinary sentence that happens to start with "switch to"
        }
    }

    fn log(&self, persona: &Persona, role: Role, content: Value) {
        self.storage.log_message(&json!({
            "ts": chrono::Local::now().to_rfc3339(), "persona": persona.id, "role": role, "content": content,
        }));
    }
}
