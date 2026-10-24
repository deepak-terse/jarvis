# JARVIS — Local-First AI Voice Assistant (Plan v2)

## 1. Vision

A fast, privacy-first voice assistant that runs locally on the M3 Pro (18 GB) and handles day-to-day tasks: conversation, web search, and remembering facts about the user.

**Priorities:** Speed → Accuracy → Privacy → Minimal resource usage → Extensibility.

**Guiding principle:** The core stays small and stable. Models, capabilities, skills, and personas are swapped through config, not code changes to the core.

## 2. v1 scope

**In scope**
- Launched from the terminal: `jarvis chat` (text) and `jarvis voice` (push-to-talk).
- One embedded local LLM, swappable by changing the model path in config.
- Streamed responses, both as text and as speech.
- Web search through Tavily.
- User facts from `profile.toml` (e.g. location: Mumbai).
- Personas (JARVIS, FRIDAY, …), each with its own prompt, voice, and skills.
- Latency printed with each response: end of speech → first audio out.

**Out of scope for v1:** wake word, VAD, background service, cloud LLMs, MCP and third-party skills, external or Python capability processes, scheduler and reminders, vision, semantic (embedding) memory. See §12.

## 3. Architecture

| Component | Responsibility |
|---|---|
| **Core** | Agent loop: builds the prompt, streams from the LLM, runs tool calls, and drives the voice pipeline. Depends only on traits. |
| **LLM** | Embedded llama.cpp: model loading, token streaming, chat template, tool-call parsing, KV cache reuse. |
| **Capabilities** | Named by function (`stt`, `tts`, later `vad`, `vision`). Providers are compiled in and chosen by config. |
| **Skills** | Tools the LLM can call (`web_search`, …). Implemented in Rust behind a `Skill` trait and enabled in config. |
| **Personas** | TOML files defining prompt, voice, allowed skills, and sampling settings. |
| **Storage** | One data folder: profile facts, conversations, metrics. |

**Voice turn flow**

```
key press → record mic → key release [t0]
→ stt → agent (prompt → llm stream → tool calls → loop)
→ sentence splitter → clean text (strip markdown, citations, URLs)
→ tts → playback [t1]          latency = t1 − t0
```

Text mode uses the same agent; only input and output differ. The agent never knows whether input came from the keyboard or the mic.

## 4. Key decisions and reasoning

| Decision | Reasoning |
|---|---|
| **Embed llama.cpp in-process** | Lower memory (one process), direct control of the KV cache, single-binary distribution. HTTP overhead on localhost is negligible next to inference, so the benefit is memory and control rather than raw speed. Costs accepted: we parse tool calls ourselves, bindings lag upstream releases, and a llama.cpp crash takes down the whole app. |
| **One local LLM, no cloud** | Fits the privacy and speed priorities. Switching models is a config change (GGUF path + template name). Cloud support can be added later inside `llm/` without touching the core. |
| **Keep the current 4B model** | Already tested with web search on target hardware. Tool-calling accuracy is monitored as skills are added (§9). |
| **LLM is not a capability** | It is the decision-maker that uses capabilities and skills, so it gets its own module and config section. |
| **Capabilities named by function** | `stt`, `tts`, `vad`, `vision`. The technology (whisper, kokoro) is a provider choice in config, not part of the name. |
| **Compiled-in providers, no runtime plugins** | Rust has no stable ABI, so loading plugins at runtime is fragile. Adding a provider means a recompile; choosing between existing ones is config only. |
| **Skills behind a `Skill` trait + registry** | A new skill is a new folder plus one registry line; the core never changes. Config enables or disables skills and passes their settings. The trait matches MCP's tool shape (name, description, JSON schema, call), so MCP can be added later as a wrapper. |
| **Personas as files** | Adding a personality needs no code. With one LLM, a persona is prompt + voice + allowed skills + temperature. |
| **Facts in `profile.toml`, injected into the system prompt** | Simplest memory that answers questions like "weather outside" correctly. Hand-edited in v1. |
| **Push-to-talk** | Clear end-of-speech signal without tuning VAD; no echo problems because the mic is off during playback. |
| **Streaming end to end** | TTS starts on the first finished sentence while the LLM is still generating. This is the biggest win for perceived latency. |
| **Stable prompt prefix first** | Persona + profile + tool definitions come first and stay identical across turns, so their KV cache is reused and only new tokens are processed. Cached per persona. |
| **Tavily for search** | Search API built for LLM agents: one call returns ranked, pre-extracted snippets plus an optional short answer, so the local model writes the reply from compact context. Results are condensed (top 3, snippets capped, URLs dropped) to keep prompt processing fast. Trade-offs: a cloud call (query leaves the machine), API key needed, credits per search (`basic` = 1, `advanced` = 2). |
| **One Rust package, organized by meaning** | Folders follow the architecture, not crate convention. Split into crates later only if compile times demand it. |
| **Secrets in environment variables** | `config.toml` references env var names, so it is safe to share or commit. |
| **Models stored outside the repo** | Large, re-downloadable files; config holds their paths. |
| **Text mode before voice** | Tests the riskiest part (small-model tool calling) before investing in audio. |

## 5. Tech stack

| Layer | Technology |
|---|---|
| Runtime | Rust + Tokio |
| LLM | llama.cpp embedded via Rust bindings (e.g. `llama-cpp-2`), Metal |
| Model | Qwen 4B, quantized GGUF |
| STT | whisper.cpp (`whisper-rs`) |
| TTS | Kokoro-82M via sherpa-onnx |
| Audio I/O | `cpal` |
| Web search | Tavily Search API (`reqwest`) |
| Config | `serde` + `toml`; `toml_edit` if JARVIS writes to the profile later |
| Telemetry | `tracing`, logged to JSONL |
| Storage | TOML (facts) + JSONL (conversations, metrics) |

## 6. Repository structure

```
jarvis/
├── Cargo.toml
├── config.toml
├── personas/
│   ├── jarvis.toml
│   └── friday.toml
├── src/
│   ├── main.rs               # CLI: `jarvis chat`, `jarvis voice`, --persona
│   ├── core/
│   │   ├── agent.rs          # prompt → llm → tool calls → streamed reply
│   │   ├── session.rs        # conversation state, history window
│   │   ├── prompt.rs         # persona + profile + tools → system prompt
│   │   └── voice.rs          # push-to-talk → stt → agent → sentences → tts
│   ├── llm/
│   │   ├── engine.rs         # load model, context, stream tokens, keep warm
│   │   ├── cache.rs          # KV reuse for stable prefix, per persona
│   │   ├── template.rs       # chat template + tool definitions
│   │   └── tools.rs          # parse tool calls from raw output
│   ├── capabilities/
│   │   ├── stt/  (mod.rs: trait + provider selection, whisper.rs)
│   │   └── tts/  (mod.rs: trait + provider selection, kokoro.rs)
│   ├── skills/
│   │   ├── mod.rs            # Skill trait + registry
│   │   └── web_search/tavily.rs
│   ├── audio/                # mic capture, playback (plumbing, not a capability)
│   ├── storage/              # profile, conversations, metrics I/O
│   ├── telemetry.rs          # stage timings, printed total
│   └── config.rs             # load + validate config and personas
└── storage/                  # default data dir, gitignored
```

Folders for `vad/` and `vision/` are created when those are built, not before.

## 7. Configuration

```toml
# config.toml
[llm]
model        = "~/models/qwen-4b-q4.gguf"
template     = "qwen"          # chat template + tool-call parser
context_size = 8192
gpu_layers   = 99
flash_attn   = true
keep_warm    = true

[persona]
default = "jarvis"

[capabilities.stt]
provider = "whisper"
model    = "~/models/whisper/ggml-base.en.bin"

[capabilities.tts]
provider  = "kokoro"
model_dir = "~/models/kokoro"

[skills.web_search]
enabled     = true
provider    = "tavily"
api_key_env = "TAVILY_API_KEY"

[storage]
dir = "~/jarvis/storage"
```

```toml
# personas/friday.toml
name        = "FRIDAY"
voice       = "af_bella"
temperature = 0.9
skills      = ["web_search"]
prompt      = """
You are FRIDAY, a witty, informal assistant. Keep replies short and spoken-friendly.
"""
```

**Persona switching:** `--persona` at launch, plus a built-in command (`/persona friday` in text; a fixed phrase in voice) handled by the core before the LLM sees it.

## 8. Storage layout

```
storage/
├── profile.toml          # user facts, added to every system prompt
├── conversations/        # one JSONL file per session
└── logs/
    └── metrics.jsonl     # per-turn stage timings
```

All AI context lives in one folder, so it is easy to back up, inspect, or delete. Models live outside it. Transcripts are stored locally only; the one outbound flow is search queries to Tavily.

## 9. Telemetry

- **Printed per response:** end of speech (key release) → first audio played. In text mode: Enter → first token.
- **Logged per turn:** STT time, LLM prompt-processing time, time to first token, tokens/sec, tool call duration (Tavily), TTS time to first audio.
- **Tracked as skills grow:** whether the model chose the right tool, so accuracy loss on the 4B model is caught early.

## 10. Implementation phases

| Phase | Deliverable | Done when |
|---|---|---|
| **0. Build spike** | Minimal binary linking llama.cpp, whisper.cpp, and sherpa-onnx with Metal | All three load and run in one process with no ggml symbol conflicts |
| **1. Text agent** | Config, embedded LLM with streaming, template + tool parsing, personas, profile injection, web search skill, conversation log, time-to-first-token | `jarvis chat` answers "what's the weather outside" for Mumbai through search, streamed |
| **2. Voice** | Audio I/O, push-to-talk, STT, sentence-level streaming TTS, text cleaning, latency print | `jarvis voice` replies out loud with the latency shown |
| **3. Tuning** | KV prefix cache per persona, persona switching command, latency and tool-accuracy benchmarks | Stable numbers on target hardware; repeat turns skip reprocessing the system prompt |

## 11. Risks and mitigations

| Risk | Mitigation |
|---|---|
| whisper.cpp and llama.cpp ggml clash when linked together | Phase 0 spike before any feature work |
| Small model makes bad tool calls as skills grow | Per-persona skill allowlists, tool-accuracy tracking, retry on malformed calls, cap on tool iterations |
| Tool-call format differs between model families | Isolated in `llm/tools.rs`, selected by `template` in config |
| Bindings lag new model architectures | Pin versions; check binding support before switching models |
| llama.cpp crash kills the app | Acceptable for terminal v1; revisit before the background service |
| Search results contain prompt injection | Treat tool output as data in the prompt, not instructions |
| Memory pressure (LLM + STT + TTS on 18 GB) | Measure in Phase 0/2; `keep_warm` per model is configurable |

## 12. Deferred, and what would trigger it

| Item | Revisit when |
|---|---|
| JARVIS writing new facts (`remember` skill) | Hand-editing the profile becomes tedious. **Open decision.** |
| VAD and wake word | Push-to-talk feels limiting |
| Scheduler and reminders | Core moves to event-driven; needs background service to fire reliably |
| Background service (launchd / Windows) | Voice loop is stable |
| MCP and third-party skills | Built-in skill set is stable; wrap the `Skill` trait |
| External / Python capabilities | A provider is needed that has no Rust option |
| Cloud LLMs | A task needs more than the local model can do; add inside `llm/` |
| Semantic memory (embeddings, `sqlite-vec`) | Profile and conversations outgrow keyword or prompt-based recall |
| Vision, OCR | After the voice loop is solid |
