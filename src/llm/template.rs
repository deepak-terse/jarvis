//! Chat templates: how a model family wants prompts, tools and tool calls formatted.
//! Selected by `llm.template` in config.

use super::tools::{self, ToolCall, ToolSpec};
use super::{Message, Role};
use anyhow::{Result, bail};

pub trait Template: Send {
    /// The stable prompt prefix: system text + tool definitions.
    fn prefix(&self, system: &str, tools: &[ToolSpec]) -> String;
    /// Conversation messages followed by the assistant generation prompt.
    fn messages(&self, messages: &[Message]) -> String;
    /// Marked spans to hide from the user-visible stream.
    fn hidden_spans(&self) -> &'static [(&'static str, &'static str)];
    /// Extract tool calls; returns (calls, malformed blocks).
    fn parse_tool_calls(&self, raw: &str) -> (Vec<ToolCall>, Vec<String>);
    /// A tool call as the model would write it (for worked examples in the prompt).
    fn tool_call(&self, call: &ToolCall) -> String;
}

pub fn create(name: &str) -> Result<Box<dyn Template>> {
    Ok(match name {
        "qwen" => Box::new(Qwen { no_think: false }),
        "qwen3" => Box::new(Qwen { no_think: true }),
        other => bail!("unknown chat template '{other}' (available: qwen, qwen3)"),
    })
}

/// ChatML with Hermes-style `<tool_call>` JSON (Qwen 2.5 / Qwen 3).
struct Qwen {
    /// Pre-fill an empty think block so hybrid-reasoning models answer directly.
    no_think: bool,
}

impl Template for Qwen {
    fn prefix(&self, system: &str, tools: &[ToolSpec]) -> String {
        let mut s = format!("<|im_start|>system\n{}", system.trim());
        if !tools.is_empty() {
            s.push_str(
                "\n\n# Tools\n\nYou may call one or more functions to assist with the user query.\n\n\
                 You are provided with function signatures within <tools></tools> XML tags:\n<tools>",
            );
            for t in tools {
                // Key order as in Qwen's own chat template (serde_json would sort them alphabetically).
                s.push_str(&format!(
                    "\n{{\"type\": \"function\", \"function\": {{\"name\": {}, \"description\": {}, \"parameters\": {}}}}}",
                    serde_json::Value::from(t.name.as_str()),
                    serde_json::Value::from(t.description.as_str()),
                    t.parameters
                ));
            }
            s.push_str(
                "\n</tools>\n\nFor each function call, return a json object with function name and arguments \
                 within <tool_call></tool_call> XML tags:\n<tool_call>\n\
                 {\"name\": <function-name>, \"arguments\": <args-json-object>}\n</tool_call>",
            );
        }
        s.push_str("<|im_end|>\n");
        s
    }

    fn messages(&self, messages: &[Message]) -> String {
        let mut s = String::new();
        for m in messages {
            let body = sanitize(&m.content);
            match m.role {
                Role::User => s.push_str(&format!("<|im_start|>user\n{body}<|im_end|>\n")),
                Role::Assistant => s.push_str(&format!("<|im_start|>assistant\n{body}<|im_end|>\n")),
                Role::Tool => {
                    s.push_str(&format!("<|im_start|>user\n<tool_response>\n{body}\n</tool_response><|im_end|>\n"))
                }
            }
        }
        s.push_str("<|im_start|>assistant\n");
        if self.no_think {
            s.push_str("<think>\n\n</think>\n\n");
        }
        s
    }

    fn hidden_spans(&self) -> &'static [(&'static str, &'static str)] {
        &[("<think>", "</think>"), ("<tool_call>", "</tool_call>")]
    }

    fn parse_tool_calls(&self, raw: &str) -> (Vec<ToolCall>, Vec<String>) {
        tools::parse_tagged(raw, "<tool_call>", "</tool_call>")
    }

    fn tool_call(&self, call: &ToolCall) -> String {
        let name = serde_json::Value::from(call.name.as_str());
        format!("<tool_call>\n{{\"name\": {name}, \"arguments\": {}}}\n</tool_call>", call.arguments)
    }
}

/// Neutralise control-token syntax in user/tool text so it is treated as data,
/// not as chat structure (prompt-injection guard).
fn sanitize(text: &str) -> String {
    text.replace("<|", "< |").replace("|>", "| >")
}
