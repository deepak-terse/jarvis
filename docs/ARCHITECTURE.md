# Architecture

High-level design of how JARVIS is structured and how a turn flows through it.

## Design principles

- **Priorities:** speed → accuracy → privacy → minimal resource use → extensibility
- **Small, stable core.** Models, speech engines, skills and personas are swapped through config, not changes to the agent.
- **Contracts in the middle.** The agent depends only on traits; everything else implements or consumes them.
- **One local process.** LLM, STT and TTS run in-process; only skill queries leave the machine.

## System flow

```text
Voice                                              Text
key press → record mic → key release [t0]          Enter [t0]
    ↓                                                  ↓
Speech-to-text (Whisper)                               │
    ↓                                                  ↓
Agent ── prompt (stable prefix + history + input)
    ↓
LLM stream (llama.cpp) ──→ tool calls? ──→ Skills (guarded) ──→ back to LLM (≤ 3 rounds)
    ↓
Sink: terminal text  ·  voice: sentence splitter → clean → TTS (Kokoro) → speaker [t1]

latency = t1 − t0  (voice: first audio · text: first token)
```

Text and voice use the same agent. Only the `Sink` they pass in differs.

## Component structure

```text
jarvis/
├── config.toml               # model, speech engines, skills, greeting, data folder
├── personas/                 # one TOML per persona
├── data/tool_examples.toml   # when-to-call-which-tool examples
├── bench/cases.toml          # benchmark questions
└── src/
    ├── main.rs               # CLI entry → app
    ├── app/                  # config loading, wiring, commands
    ├── core/
    │   ├── contracts/        # traits and shared types
    │   └── agent/            # agent loop, prompt, policy, personas, session
    ├── skills/               # tools the model can call
    ├── memory/               # history window, profile, transcripts, data folder
    ├── capabilities/         # stt and tts slots
    ├── adapters/             # llama.cpp, Whisper, Kokoro, audio devices
    ├── channels/             # terminal and voice
    ├── routines/             # startup greeting
    └── tooling/              # benchmark, tracing, metrics log
```

Each folder has its own README with local details.

## Layers

| Layer | Role | Contracts |
|---|---|---|
| [core/contracts](../src/core/README.md) | The stable surface: traits and shared types | — |
| [core/agent](../src/core/README.md) | The brain: prompt, LLM stream, tool calls, policy | uses all |
| [skills](../src/skills/README.md) | Tools the model can call, each guarded | `Skill`, served as `Tools` |
| [memory](../src/memory/README.md) | What JARVIS remembers | `History`, `Memory` |
| [capabilities](../src/capabilities/README.md) | Speech slots, provider picked by config | `Stt`, `Tts` |
| [adapters](../src/adapters/README.md) | Concrete engines and devices | implement `Llm`, `Stt`, `Tts` |
| [channels](../src/channels/README.md) | How a turn enters and leaves | `Sink` |
| [routines](../src/routines/README.md) | Turns JARVIS starts itself | — |
| [tooling](../src/tooling/README.md) | Benchmark, tracing, metrics | `MetricsSink` |
| [app](../src/app/README.md) | Builds everything from config and connects it | — |

### Dependency rule

- `core/contracts` imports nothing else in the crate
- `core/agent` imports only contracts
- `skills`, `memory`, `adapters` import only contracts
- `capabilities` adds adapters; `routines` adds skills
- `channels` adds the agent, capabilities and audio
- Only `app` imports everything

Keeping to this rule lets each folder become its own crate later without moving files.

## Storage model

All AI context lives in one folder, so it is easy to back up, inspect or delete. Models live outside it.

```text
~/jarvis/storage/
├── profile.toml          # user facts, added to every system prompt
├── conversations/        # one JSONL transcript per session
├── skills/               # state skills keep between runs (e.g. learned stock symbols)
└── logs/
    ├── metrics.jsonl     # per-turn stage timings
    └── jarvis.jsonl      # tracing (level via RUST_LOG)
```

## Key decisions

| Decision | Reasoning |
|---|---|
| Embed llama.cpp in-process | One process, lower memory, direct control of the KV cache, single binary. Cost: we parse tool calls ourselves, and a llama.cpp crash ends the app. |
| LLM is not a capability | It is the decision-maker that uses capabilities and skills, so it has its own contract and config section. |
| Capabilities named by function | `stt`, `tts`, later `vad`, `vision`. The engine (Whisper, Kokoro) is a provider choice in config. |
| Compiled-in providers | Rust has no stable ABI, so runtime plugins are fragile. Choosing between providers is config; adding one is a recompile. |
| Protocols for outside plugins | Third-party skills, models and speech engines are planned through MCP, an OpenAI-compatible API and Wyoming, not Rust plugins. |
| Skills match MCP's tool shape | Name, description, JSON schema, call. An MCP bridge can wrap external servers as skills. |
| Stable prompt prefix first | Persona + profile + tools stay identical across turns, so their KV cache is reused. Per-turn data (clock time) rides on the user message. |
| Streaming end to end | TTS starts on the first finished sentence. The biggest win for perceived latency. |
| Push-to-talk | Clear end of speech without VAD tuning; no echo, since the mic is off during playback. |
| Whisper via sherpa-onnx | `whisper-rs` and `llama-cpp-2` each embed their own ggml, and the duplicate symbols silently clash. sherpa-onnx already runs TTS, so one binary has one ggml. |
| Secrets in environment variables | `config.toml` names the variables, so it is safe to share. |

## Risks

| Risk | Mitigation |
|---|---|
| Small model makes bad tool calls as skills grow | Per-persona tool lists, worked examples, malformed-call retry, round limit, `jarvis bench` per category |
| Tool-call format differs between model families | Isolated in the llama.cpp adapter's template, picked by `llm.template` |
| More tools slow the prompt and lower accuracy | Persona tool lists; later, per-turn tool selection |
| Search results contain prompt injection | The prompt treats tool output as data, never instructions |
| Memory pressure (LLM + STT + TTS) | `keep_warm` per model loads on demand and releases after each turn |
| Bindings lag new model architectures | Pinned versions; check support before switching models |
