//! Tool-use examples from `data/tool_examples.toml`: one-line routes for the system
//! prompt, and worked exchanges placed before the conversation. Only entries for
//! tools the persona can use are included.

use super::prompt;
use crate::core::contracts::model::{Llm, Message, Role};
use crate::core::contracts::tools::ToolCall;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct ToolExamples {
    #[serde(default)]
    route: Vec<Route>,
    #[serde(default)]
    example: Vec<Example>,
}

#[derive(Debug, Deserialize)]
struct Route {
    ask: Phrases,
    /// `None`: answer directly.
    tool: Option<String>,
    #[serde(default)]
    arguments: toml::Table,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Phrases {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct Example {
    tool: String,
    user: String,
    #[serde(default)]
    arguments: toml::Table,
    result: String,
    reply: String,
}

impl ToolExamples {
    /// A missing file means no examples; a malformed one is an error.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            tracing::warn!("tool examples file {} not found", path.display());
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Route lines for the system prompt, e.g. `"any tech news" → news {"category": "tech"}`.
    pub fn routes(&self, can_use: impl Fn(&str) -> bool) -> Vec<String> {
        self.route
            .iter()
            .filter(|r| r.tool.as_deref().is_none_or(&can_use))
            .map(|r| {
                let asks = match &r.ask {
                    Phrases::One(p) => format!("\"{p}\""),
                    Phrases::Many(ps) => ps.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", "),
                };
                match &r.tool {
                    Some(tool) => format!("{asks} → {tool} {}", json(&r.arguments)),
                    None => format!("{asks} → no tool, answer directly"),
                }
            })
            .collect()
    }

    /// Worked exchanges (user → tool call → result → reply) in the model's own syntax.
    pub fn messages(&self, can_use: impl Fn(&str) -> bool, llm: &dyn Llm) -> Vec<Message> {
        let mut out = Vec::new();
        for (i, e) in self.example.iter().filter(|e| can_use(&e.tool)).enumerate() {
            let call = ToolCall { name: e.tool.clone(), arguments: json(&e.arguments) };
            out.push(Message::new(Role::User, prompt::stamp(&e.user, &format!("9:{i:02} AM"))));
            out.push(Message::new(Role::Assistant, llm.format_tool_call(&call)));
            out.push(Message::new(Role::Tool, e.result.trim()));
            out.push(Message::new(Role::Assistant, e.reply.trim()));
        }
        out
    }
}

fn json(table: &toml::Table) -> serde_json::Value {
    serde_json::to_value(table).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_by_persona_tools() {
        let ex = ToolExamples::load("data/tool_examples.toml".as_ref()).unwrap();
        let only_search = |t: &str| t == "web_search";
        let routes = ex.routes(only_search);
        assert!(routes.iter().all(|r| r.contains("→ web_search") || r.contains("no tool")), "{routes:#?}");
        assert!(routes.contains(
            &"\"what's 15 times 4\", \"what time is it\", \"tell me a joke\" → no tool, answer directly".to_string()
        ));
        let all = ex.routes(|_| true);
        assert!(
            all.contains(&"\"50 euros in rupees\" → currency {\"amount\":50,\"from\":\"EUR\"}".to_string()),
            "{all:#?}"
        );
        assert!(all.contains(&"\"anything special today\" → about_today {}".to_string()));
    }
}
