# JARVIS

Local-first voice assistant (see [PLAN.md](PLAN.md)).

From this folder, use the project-local Cargo alias (builds as needed):

```sh
cargo jarvis chat
cargo jarvis voice
cargo jarvis --persona friday chat
cargo jarvis bench
```

For bare `jarvis` in your current terminal session, define a shell function:

```sh
jarvis() { cargo jarvis "$@"; }
jarvis chat
```

This function is temporary and uses the Cargo alias in this project; no global installation is needed.
The commands below assume that function or an installed `jarvis` binary.

```sh
jarvis chat                    # text
jarvis voice                   # push-to-talk (hold SPACE; press-to-toggle on terminals without key-release events)
jarvis --persona friday chat
jarvis bench                   # tool-choice accuracy + latency (bench/cases.toml)
```

In a session: `/persona <name>` (text) or say "switch to <name>" (voice). `/quit` exits text mode, `q` exits voice mode.

The startup briefing is disabled by default. Set `[greeting] enabled = true` in `config.toml` to enable it.

## Setup

```sh
cd ~/models
curl -L -o qwen-4b-q4.gguf https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf
curl -L https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-en-v0_19.tar.bz2 | tar xj && mv kokoro-en-v0_19 kokoro
mkdir -p whisper && curl -L https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-base.en.tar.bz2 | tar xj -C whisper
```

From the repository directory, configure API keys and build:

```sh
cp .env.example .env
# Edit .env: TWELVE_DATA_API_KEY, TAVILY_API_KEY, WEATHER_API_KEY.
cargo build --release && ./target/release/jarvis chat
```

JARVIS loads `.env` beside the selected `config.toml` before starting. Exported environment variables
take precedence. `.env` is ignored by Git; keep real keys there and only commit `.env.example`.
These are the only API keys currently required by the live-data providers; other skills and local models need no keys.
Missing keys leave the affected skills unavailable or using their configured fallback.

Facts about you go in `~/jarvis/storage/profile.toml` (created on first run). The terminal needs microphone permission for voice mode.

## Extending

| Add a… | Do this |
|---|---|
| Persona | Drop a TOML file in `personas/` |
| Skill | Implement `Skill` in `src/skills/<group>/<name>/` (e.g. `live_data/`), add one line to the group's `create` |
| Skill group | Add `src/skills/<group>/mod.rs` with a `create`, and call it from `skills::create` |
| News category or feed | Edit `[skills.news.categories]` in `config.toml`; no code |
| Stock index | Add a `[skills.stocks.aliases]` entry (symbols: `api.twelvedata.com/indices?country=India`) |
| Tool-choice examples | Edit `data/tool_examples.toml`, then re-run `jarvis bench` |
| Search backend | Implement `SearchProvider`, add one line to `web_search::create` |
| STT / TTS provider | Implement `Stt` / `Tts`, add one line to the capability's `create` |
| Model family | Implement `Template` (prompt format + tool-call parsing) in `llm/template.rs` |

## Deviation from the plan

**STT runs Whisper through sherpa-onnx, not whisper.cpp.** The Phase 0 spike showed `whisper-rs` and
`llama-cpp-2` each statically embed their own ggml (v0.9.5 vs v0.26). The linker only warns about the duplicate
symbols, so one copy silently replaces the other, which would crash or corrupt memory at runtime. sherpa-onnx is
already the TTS runtime, so this keeps a single binary with a single ggml. A whisper.cpp `Stt` adapter can return once the
bindings share a ggml (`llama-cpp-sys-2` has a `system-ggml` feature).

Smoke test (LLM + STT + TTS in one process): `cargo test --release -- --ignored`.

## Live-data skills

`web_search`, `news` (RSS), `knowledge` (Wikipedia), `about_today` (Wikipedia historical events), `stocks` and `currency` (Twelve Data), and `weather` (WeatherAPI.com).
Every `[skills.<name>]` section accepts `enabled`, `timeout_secs`, `cache_ttl_secs`, `fallback` and `description`. A failing or slow
skill falls back to `web_search`. If that fails too, the model is told to say it couldn't get the information.
Live API checks: `cargo test -- --ignored live --nocapture`.

## Weather

Set `WEATHER_API_KEY` in `.env`. Set your default place and units in `~/jarvis/storage/profile.toml`:

```toml
location = "Mumbai, India"
country = "India"
units = "metric"                 # or "imperial"
```

Ask about now, tonight, tomorrow, this weekend, a weekday or an ISO date, including another city.
Periods such as "tomorrow evening" work too. All windows use the destination timezone;
tonight covers 18:00–06:00 and evening covers 18:00–24:00. Today's windows cover the remaining hours.
Past dates, incomplete windows and dates outside the returned forecast are explicitly unavailable.
Answers include the resolved place and the provider's local observation time.

`[skills.weather]` in `config.toml` controls forecast days, units, timeouts and thresholds.
The [Free plan includes 3 forecast days and Starter includes 7](https://www.weatherapi.com/pricing);
set `forecast_days` to match your subscription, up to 14. Actual returned coverage is checked.
Air quality uses the [US EPA categories 1–6](https://www.weatherapi.com/docs/air-quality/), rather than India's AQI;
WeatherAPI also supplies UK DEFRA bands. Missing AQ data stays unavailable.
[Government alerts are returned when available](https://www.weatherapi.com/docs/weather-alerts/),
but the provider does not explicitly guarantee Indian coverage; no returned alerts is not an all-clear.

The skill caches the whole bundle for 10 minutes per requested place, so different follow-up questions reuse it.
`bundle_cache_ttl_secs` controls this cache; leave the shared argument cache (`cache_ttl_secs`) at zero
so time windows are re-evaluated each call. Only place queries, coordinates, the API key and fixed data options
go to WeatherAPI. Failures use the configured web-search fallback. Unknown places and unsupported dates
return an explicit limitation. The greeting includes at most one weather sentence, only when flags are raised.
Normal replies use structured facts phrased by the selected persona.

Run `cargo test weather` for deterministic weather checks, including local HTTP mocks.
Run `jarvis bench --live` for shared weather and edge cases: it prints answers, word/character counts and latency
for manual correctness and brevity review. Live checks need your API keys and model files.
