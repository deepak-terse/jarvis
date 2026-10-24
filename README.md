<div align="center">
<h1> JARVIS</h1>

[![Rust](https://img.shields.io/badge/Rust-2024-B7410E?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![AI: Local LLM](https://img.shields.io/badge/AI-llama.cpp%20%2F%20Qwen%203-orange)](https://github.com/ggml-org/llama.cpp)
[![Speech: sherpa-onnx](https://img.shields.io/badge/Speech-Whisper%20%2B%20Kokoro-2E7D32)](https://github.com/k2-fsa/sherpa-onnx)
[![Platform: macOS](https://img.shields.io/badge/Platform-macOS%20(Metal)-black?logo=apple&logoColor=white)](https://developer.apple.com/metal/)
</div>

> A fast, privacy-first voice assistant that runs on your own machine.

JARVIS listens, thinks and answers out loud using a local LLM, local speech-to-text and local text-to-speech. It answers live questions such as weather, news, stocks and currency through pluggable skills, and keeps every conversation in one local folder.

```text
Push-to-talk → Speech-to-text → Agent (LLM ⇄ Skills) → Sentences → Text-to-speech → Speaker
```

## The Problem

Voice assistants are either private or useful, rarely both.
Cloud assistants send everything you say to a server. Local ones are often slow, forget who you are, and can't answer anything that happened after their training.

The problem is not a lack of models. It is putting them together so they feel instant.

## The Solution

JARVIS runs the model, the speech engines and the agent loop in one local process, and streams everything: speech starts on the first finished sentence while the model is still writing the rest.

Live data comes from small, focused skills. Each has a time limit, a cache and a web-search fallback. Your voice never leaves the machine; only the query text does.

The core stays small. Models, speech engines, skills and personas are chosen in `config.toml`.

## What it does

- 🎙️ Push-to-talk voice and text chat with the same agent
- ⚡ Streams replies end to end and prints the latency of every response
- 🌦️ Answers live questions: weather, news, stocks, currency, Wikipedia, web search
- 🎭 Personas with their own prompt, voice, tone and allowed skills
- 🧠 Remembers facts about you and keeps recent conversation in context
- 🔒 Runs the LLM, STT and TTS locally; transcripts stay on your machine
- 📊 Benchmarks tool choice and latency with `jarvis bench`

## Built for

- People who want a voice assistant without sending their voice to the cloud
- Developers looking for a clean, local agent loop in Rust
- Anyone who wants to add skills, voices or models without rewriting the core

## Engineering Highlights

- Embedded llama.cpp — Model runs in-process on Metal; no server, single binary
- Prompt cache reuse — Persona, profile and tools form a stable prefix whose KV cache is reused across turns
- Streaming voice pipeline — Sentence-level TTS starts while the LLM is still generating
- Small-model tool calling — Worked examples, per-persona tool lists, malformed-call retry and a round limit
- Guarded skills — Every tool call gets a time limit, result cache, web-search fallback and an honest failure message
- Contracts-first design — The agent depends only on traits; models, skills, memory and speech engines plug in behind them
- Measured — Per-turn stage timings in JSONL and a tool-choice benchmark per category

## Tech stack

| Layer | Technology |
|---|---|
| Runtime | Rust, Tokio |
| LLM | llama.cpp via `llama-cpp-2` (Metal), Qwen 3 4B Instruct (Q4_K_M GGUF) |
| Speech-to-text | Whisper base.en via `sherpa-onnx` |
| Text-to-speech | Kokoro-82M via `sherpa-onnx` |
| Audio I/O | `cpal` |
| Live data | Tavily, WeatherAPI.com, Twelve Data, Wikipedia, RSS |
| Config and storage | TOML, JSONL |

## Getting started

### Prerequisites

- macOS on Apple Silicon (tested on M3 Pro, 18 GB)
- Rust 1.85+ (edition 2024)
- Xcode Command Line Tools and CMake (to build llama.cpp)
- Microphone permission for your terminal (voice mode)

### Download models

```bash
mkdir -p ~/models && cd ~/models
curl -L -o qwen-4b-q4.gguf https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf
curl -L https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-en-v0_19.tar.bz2 | tar xj && mv kokoro-en-v0_19 kokoro
mkdir -p whisper && curl -L https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-base.en.tar.bz2 | tar xj -C whisper
```

### Add API keys

```bash
cp .env.example .env
# Fill in TAVILY_API_KEY, WEATHER_API_KEY, TWELVE_DATA_API_KEY
```

`.env` is loaded from beside `config.toml` and ignored by Git. Exported variables take precedence. A missing key only affects the skills that need it.

### Run

```bash
cargo jarvis chat                    # text
cargo jarvis voice                   # push-to-talk: hold SPACE (press twice on older terminals)
cargo jarvis --persona friday chat
cargo jarvis bench                   # tool-choice accuracy and latency
```

`cargo jarvis` is a project alias for `cargo run --release --`. For a bare `jarvis` command: `jarvis() { cargo jarvis "$@"; }`.

In a session: `/persona <name>` (text) or say "switch to <name>" (voice). `/quit` exits text mode, `q` exits voice mode.

## Configuration

| File | What it sets |
|---|---|
| `config.toml` | Model, speech engines, skills and their settings, startup greeting, data folder |
| `personas/*.toml` | Name, prompt, voice, temperature, allowed skills, filler phrases |
| `~/jarvis/storage/profile.toml` | Facts about you, added to every prompt (created on first run) |
| `data/tool_examples.toml` | When to call which tool; tuned with `jarvis bench` |

```toml
# ~/jarvis/storage/profile.toml
name = "Tony"
location = "Mumbai, India"
country = "India"
units = "metric"
```

The startup greeting is off by default. Set `[greeting] enabled = true` to hear a historical event, headlines and weather flags at startup.

Skills, their providers and settings are listed in [skills](src/skills/README.md).

## Privacy

- LLM, speech-to-text and text-to-speech run locally
- Transcripts, metrics and your profile stay in `~/jarvis/storage/`
- Only skill queries (search text, place names, symbols) go to the skill providers

## Documentation

| Document | What it covers |
|---|---|
| [Architecture](docs/ARCHITECTURE.md) | System flow, layers, dependency rule, storage, key decisions |
| [Implementation](docs/IMPLEMENTATION.md) | Turn loop, prompt caching, tool calling, voice pipeline, metrics, known gaps |
| [Contributing](CONTRIBUTING.md) | Setup, where things go, checks before a change lands |
| [core](src/core/README.md) | Agent loop and the contracts everything plugs into |
| [skills](src/skills/README.md) | Available skills, settings, adding a skill |
| [memory](src/memory/README.md) | Conversation window, profile, transcripts, data folder |
| [capabilities](src/capabilities/README.md) | Speech-to-text and text-to-speech slots |
| [adapters](src/adapters/README.md) | llama.cpp, Whisper, Kokoro, audio devices |
| [channels](src/channels/README.md) | Text and voice interfaces |
| [routines](src/routines/README.md) | Startup greeting |
| [tooling](src/tooling/README.md) | Benchmark, metrics and logs |
| [app](src/app/README.md) | Config loading and wiring |

---

Built to make a voice assistant feel instant, without giving up privacy.
