# adapters

Concrete engines and devices behind the contracts. Each depends only on `core/contracts`.

```text
adapters/
├── models/llama_cpp/   # Llm: embedded llama.cpp
├── stt/whisper.rs      # Stt: Whisper via sherpa-onnx
├── tts/kokoro.rs       # Tts: Kokoro via sherpa-onnx
└── audio/              # microphone, player, resample
```

## models/llama_cpp

| File | Does |
|---|---|
| `mod.rs` | `[llm]` config, `create()` |
| `engine.rs` | Model and context on a dedicated thread; jobs in, streamed events out |
| `cache.rs` | KV snapshot of each persona's prompt prefix |
| `template.rs` | Prompt format and tool-call syntax per model family (`qwen`, `qwen3`) |
| `tools.rs` | Tool-call parsing; hides `<think>` and `<tool_call>` spans from the stream |

```toml
[llm]
model        = "~/models/qwen-4b-q4.gguf"
template     = "qwen"          # "qwen3" for hybrid-thinking models
context_size = 8192
gpu_layers   = 99
flash_attn   = true
keep_warm    = true
max_tokens   = 1024
```

A new model family needs a `Template`. A new backend (e.g. an OpenAI-compatible server) is a sibling module implementing `Llm`, built in `app`.

## stt, tts

Each reads its settings from its `[capabilities.<slot>]` section. Whisper expects 16 kHz mono; Kokoro returns audio at its own rate.

## audio

Mic capture and playback with `cpal`. `AudioQueue` records when the first sample played, for the latency metric.

## Notes

- Whisper runs through sherpa-onnx, not whisper.cpp, to avoid a ggml symbol clash with llama.cpp. See [Architecture](../../docs/ARCHITECTURE.md#key-decisions).
- `llama-cpp-2` and `sherpa-onnx` are pinned. Check binding support before switching model architectures.
