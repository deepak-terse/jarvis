# core

The brain and the contracts everything else plugs into.

```text
core/
├── contracts/     # stable traits and types; imports nothing else in the crate
└── agent/         # the agent loop; imports only contracts
```

## contracts

| File | Defines | Implemented by |
|---|---|---|
| `model.rs` | `Llm`, `Message`, `Prefix` (cache hint), `ChatRequest`, `LlmEvent`, `Completion` | `adapters/models` |
| `tools.rs` | `Skill`, `Tools`, `ToolSpec`, `ToolCall`, `ToolOutcome`, `Policy` | `skills` |
| `capabilities.rs` | `Stt`, `Tts`, `Audio` | `adapters/stt`, `adapters/tts` |
| `memory.rs` | `History`, `Memory` | `memory` |
| `events.rs` | `Sink`, `MetricsSink`, `TurnMetrics` | `channels`, `tooling` |
| `settings.rs` | `ProviderConfig`, `settings()`, `expand()` | used by every configurable part |

A change here ripples to every implementation. Keep it small and change it rarely.

## agent

| File | Does |
|---|---|
| `mod.rs` | `Agent`: one turn = prompt → LLM stream → tool calls → reply |
| `prompt.rs` | System prompt and the time-stamped user message |
| `examples.rs` | Tool routes and worked exchanges from `data/tool_examples.toml` |
| `policy.rs` | Which tools a persona may call, round limit, malformed-call retry |
| `persona.rs` | `Persona`, loaded from `personas/*.toml` |
| `session.rs` | Active persona + any `History` |

The turn flow and prompt caching are described in [Implementation](../../docs/IMPLEMENTATION.md#a-turn).

## Notes

- Prompt wording was tuned with `jarvis bench`. Re-run it after any change to `prompt.rs` or `examples.rs`.
- Keep per-turn data out of the system prompt, or the KV cache is recomputed every turn.
