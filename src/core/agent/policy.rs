//! Whether a tool call may happen and how many rounds a turn may take. How a single
//! call behaves (time limit, cache, fallback) is the tool set's business, not the agent's.

use super::persona::Persona;
use crate::core::contracts::model::Completion;
use crate::core::contracts::tools::Tools;

/// Tool rounds per turn before the agent stops and answers with what it has.
pub const MAX_TOOL_ROUNDS: usize = 3;

/// Whether `persona` may call `tool` (listed by the persona and enabled in config).
pub fn can_use(persona: &Persona, tools: &dyn Tools, tool: &str) -> bool {
    persona.skills.iter().any(|s| s == tool) && tools.contains(tool)
}

/// A turn gets one retry when the model tried to call a tool but the call couldn't be parsed.
pub fn retry_malformed(done: &Completion, already_retried: bool) -> bool {
    done.tool_calls.is_empty() && !done.malformed.is_empty() && !already_retried
}
