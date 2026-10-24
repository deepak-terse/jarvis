# channels

How a turn enters and leaves. The agent doesn't know which channel is in use; each passes its own `Sink`.

| File | Does |
|---|---|
| `terminal.rs` | `jarvis chat`; `Console` prints the reply stream |
| `voice/mod.rs` | `jarvis voice`: push-to-talk → stt → agent → tts → playback |
| `voice/push_to_talk.rs` | SPACE handling: hold, or press twice where key release isn't reported |
| `voice/text.rs` | Sentence splitting and cleanup for speech |
| `voice/speaker.rs` | TTS on its own thread, sentences in order |

The voice flow is described in [Implementation](../../docs/IMPLEMENTATION.md#voice-pipeline).

## Adding a channel

1. Add a folder or file here with a `run(agent, session, …)` function.
2. Implement `Sink` for its output.
3. Add a command in `app/commands.rs` and call it from `app/mod.rs`.

## Notes

- Persona `fillers` are spoken once per turn, only if a tool starts before anything was said.
- A web UI or avatar needs a runtime handle (sessions, event stream, cancellation) that doesn't exist yet.
