# capabilities

Slots named by function. Each picks an adapter by `provider` and holds it according to `keep_warm`.

| File | Does |
|---|---|
| `mod.rs` | `Warm<T>`: load at startup and keep, or load on demand and release after each turn |
| `stt.rs` | Speech-to-text slot |
| `tts.rs` | Text-to-speech slot |

## Config

```toml
[capabilities.stt]
provider  = "whisper"
model     = "~/models/whisper/sherpa-onnx-whisper-base.en"
keep_warm = true               # default

[capabilities.tts]
provider  = "kokoro"
model_dir = "~/models/kokoro"
```

Keys other than `provider` and `keep_warm` go to the adapter.

## Adding a provider

1. Implement `Stt` or `Tts` in `adapters/stt/` or `adapters/tts/`.
2. Add one line to the `match` in `stt.rs` or `tts.rs`.
3. Set `provider` in `config.toml`.

## Notes

- `Stt` and `Tts` are synchronous today. Network providers need async contracts first; see [known gaps](../../docs/IMPLEMENTATION.md#known-gaps).
- Planned: `vad`, `vision`. Their folders are added when they are built.
