# memory

What JARVIS remembers. Implements `History` and `Memory` from `core/contracts`.

| File | Does |
|---|---|
| `session.rs` | `Window`: last N user/assistant messages, dropping whole exchanges |
| `profile.rs` | Loads `profile.toml` and flattens it to `key: value` lines |
| `transcript.rs` | Names the JSONL transcript for each session |
| `store.rs` | `Store`: the data folder; implements `Memory` |

## Config

```toml
[session]
history_messages = 20          # sliding window of past user/assistant messages

[storage]
dir = "~/jarvis/storage"
```

The folder layout is in [Architecture](../../docs/ARCHITECTURE.md#storage-model).

## Notes

- Tool calls and results go to the transcript, not the history window.
- The profile is read on every turn, so edits apply on the next turn (the persona prefix is recomputed once). Skills read it once at startup.
- A new backend (SQLite, embeddings) implements the same traits; the agent doesn't change.
