# app

The assembly point. The only module that knows every concrete type.

| File | Does |
|---|---|
| `mod.rs` | Loads `.env` and config, builds model, skills, memory and metrics, starts a channel |
| `config.rs` | `Config`: collects each part's config section; loads personas |
| `commands.rs` | CLI: `chat`, `voice`, `bench`, `--persona`, `--config` |

## Config lookup

1. `--config <path>` or `JARVIS_CONFIG`
2. `./config.toml`
3. `~/.config/jarvis/config.toml`

`.env` is loaded from beside the chosen config. Persona and tool-example paths resolve against its folder; `~` expands to home.

## Notes

- Each config section's type lives with the part it configures; this module only assembles them.
- Unknown or disabled skills listed by a persona print a warning at startup.
- The LLM + STT + TTS smoke test lives here: `cargo test --release -- --ignored llm_stt_tts`.
