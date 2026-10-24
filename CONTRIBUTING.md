# Contributing

How to work on JARVIS: setup, where things go, and what to check before a change lands.

Read [Architecture](docs/ARCHITECTURE.md) first. It explains the layers and the one rule that keeps them independent.

## Setup

Follow [Getting started](README.md#getting-started), then:

```bash
cargo build
cargo test
```

Dependencies are built optimised even in dev builds (llama.cpp and ONNX are too slow otherwise), so the first build takes a few minutes.

## Where things go

| To add… | Go to |
|---|---|
| A skill | [skills](src/skills/README.md#adding-a-skill) |
| An STT or TTS engine | [capabilities](src/capabilities/README.md#adding-a-provider) |
| A model family or backend | [adapters](src/adapters/README.md#modelsllama_cpp) |
| An interface (web, avatar) | [channels](src/channels/README.md#adding-a-channel) |
| A memory backend | [memory](src/memory/README.md) |
| A persona | A TOML file in `personas/`; no code |
| A news feed or stock alias | `config.toml`; no code |
| A contract change | [core](src/core/README.md); expect it to touch every implementation |

## Rules

- **Respect the dependency rule.** `core/contracts` imports nothing; the agent imports only contracts; only `app` imports everything. See [Architecture](docs/ARCHITECTURE.md#dependency-rule).
- **Config over code.** New behaviour that a user might tune goes in its config section, with a sensible default.
- **Each part owns its config.** A config section's type lives with the part it configures.
- **Secrets in environment variables.** Config names the variable (`api_key_env`), never the key.
- **Stream, don't wait.** New contracts and adapters should stream results, not return them all at once.
- **Keep the prompt prefix stable.** No per-turn data in the system prompt.
- **Match the surrounding code.** `rustfmt.toml` sets a 120-column width; doc comments explain why, not what.

## Before a change lands

```bash
cargo fmt
cargo clippy --all-targets
cargo test
```

| If you changed… | Also run |
|---|---|
| Prompt, tool examples, tool descriptions or the model | `cargo jarvis bench` and compare accuracy per category |
| A skill's provider code | `cargo test -- --ignored live --nocapture` (needs keys) |
| llama.cpp, Whisper or Kokoro code | `cargo test --release -- --ignored llm_stt_tts` (needs models) |
| Voice code | `cargo jarvis voice` and check the latency line |

## Commits

- One logical change per commit
- Message says what changed and why, in the present tense
- Never commit `.env`, models or `storage/`; `.gitignore` covers them

## Roadmap

Open problems are listed in [known gaps](docs/IMPLEMENTATION.md#known-gaps). Beyond those:

| Item | Revisit when |
|---|---|
| VAD and wake word | Push-to-talk feels limiting |
| Background service, reminders | The voice loop is stable |
| MCP bridge for third-party skills | The built-in skill set is stable |
| OpenAI-compatible model adapter | A task needs more than the local model |
| Semantic memory (embeddings) | Profile and history outgrow prompt-based recall |
| Vision, OCR | After the voice loop is solid |
| Split folders into workspace crates | Compile times or outside contributors call for it |
