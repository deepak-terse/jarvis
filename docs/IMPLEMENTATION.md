# Implementation

Feature-level flows, the reasoning behind them, and their tradeoffs. Module details live in each folder's README.

## A turn

`core/agent` runs every turn the same way, whether it came from text or voice.

1. **Built-in commands first.** `/persona <id>` or "switch to <id>" is handled before the LLM sees it.
2. **Build the prompt.**
   - **Prefix** (stable): persona prompt + date + profile facts + tool routes + tool definitions
   - **Messages:** worked tool examples → history window → user input stamped with `[now: <time>. …]`
3. **Stream from the LLM.** Visible text goes to the `Sink` as it arrives.
4. **Run tool calls.** Each call goes through the persona's tool list and the skill guard. Results are added as tool messages and the LLM runs again.
5. **Stop** when the model answers without a tool call, or after 3 tool rounds.
6. **Record** only the user input and the final reply in history. Tool traffic stays inside the turn to keep the context small.

| Situation | Behaviour |
|---|---|
| Tool call can't be parsed | One retry with an error message asking for the exact format |
| Model asks for a tool the persona can't use | `Error: unknown tool` returned as text, so the model can recover |
| Model produces no answer | A fixed apology instead of silence |

## Prompt and KV cache

Prompt processing is the slowest step on a small local model, so the prompt is built to be reused.

- **Stable prefix.** Nothing per-turn goes in the system prompt. Only the date is there; the clock time rides on the user message.
- **Per-persona snapshots.** The engine keeps a KV snapshot of each persona's prefix, keyed by persona id and checked against the exact tokens.
- **Longest common prefix.** Tokens already in the KV cache that match the start of the new prompt are kept; only the rest is processed. If less than the persona prefix matches, its snapshot is restored first.
- **Warm-up.** The default persona's prefix is computed at startup, and a persona's prefix is computed again on switch, in the background.

```text
Turn 2 prompt: [ prefix (cached) | examples + history (cached) | new user input ]
                                                                  ↑ only this is processed
```

Tradeoff: any change to the profile, tools or persona prompt invalidates that persona's snapshot.

## Tool calling on a small model

A 4B model needs help to pick the right tool.

- **Routes** in the system prompt: one line per example, e.g. `"50 euros in rupees" → currency {"amount":50,"from":"EUR"}`
- **Worked exchanges** before the conversation: user → tool call → result → reply, in the model's own syntax
- **Reminder on the user message**, because small models weigh the end of the prompt most
- **Per-persona tool lists**, so each persona sees only the tools it needs
- **Template per model family** (`qwen`, `qwen3`): prompt format, tool-call syntax, and spans hidden from the stream (`<think>`, `<tool_call>`)

All of it is tuned with `jarvis bench`, which scores the model's first decision per category.

## Skill guard

Every skill is wrapped by `skills/execution` with the settings from its config section.

```text
call(args)
  → cache hit (same arguments, within cache_ttl_secs)? → return cached
  → run skill with timeout_secs
  → failed or timed out? → web_search with the skill's fallback query
  → that failed too?     → "Error: couldn't get that information … do not guess."
```

- Cache keys are the canonical arguments JSON
- Each skill decides its own fallback query (e.g. "Reliance share price")
- Outcomes record `ok`, `cached` and `fallback` for metrics

## Voice pipeline

`channels/voice` turns the reply stream into speech while it is still being written.

1. **Record.** Hold SPACE (or press twice where the terminal can't report key release). Audio is resampled to 16 kHz.
2. **Transcribe** with the `stt` slot.
3. **Split into sentences** as text streams in: newline, or `.!?;:` followed by a space. Decimals like `28.5` don't split. Runs over 200 characters split at a comma.
4. **Clean** each sentence: drop markdown, citations like `[1]`, and URLs.
5. **Synthesize** on a dedicated TTS thread, in order, into the audio queue.
6. **Filler.** When a tool starts before anything was said, speak one of the persona's filler phrases ("Let me check.").

The mic is off during playback, so there is no echo handling.

## Memory

- **History window:** last `history_messages` user/assistant messages, dropping whole exchanges
- **Profile:** `profile.toml` flattened to `key: value` lines in the system prompt; hand-edited
- **Transcript:** every input, tool call, tool result and reply appended to a JSONL file per session

## Startup greeting

`routines/greeting` builds a greeting in the background while models load.

- Calls `about_today`, `news` and `weather` in parallel
- Fills a fixed template, with no LLM round, so it never competes with a turn
- Uses only first-hand results; failures and web-search fallbacks are skipped
- Spoken only if ready by the time the speech models finish loading

## Model lifecycle

`keep_warm` decides memory against latency per model.

| `keep_warm` | Behaviour |
|---|---|
| `true` | Loaded at startup and kept |
| `false` | Loaded on first use, released at the end of the turn |

## Metrics

Each turn logs one line to `logs/metrics.jsonl` and prints a summary.

| Field | Meaning |
|---|---|
| `latency_ms` | Voice: end of speech → first audio. Text: Enter → first token |
| `first_token_ms` | Input → first visible token |
| `stt_ms`, `tts_first_audio_ms` | Speech stage timings |
| `filler_ms` | End of speech → spoken filler |
| `llm` | Per round: cached tokens, prompt tokens and time, generated tokens and time |
| `tools` | Per call: name, time, ok, cached, fallback |
| `malformed_tool_calls` | Tool calls that couldn't be parsed |

## Known gaps

| Gap | Effect | Planned fix |
|---|---|---|
| `Stt` and `Tts` are synchronous | Network providers (e.g. Google) can't call async HTTP from the voice loop or the TTS thread | Make the speech contracts async |
| HTTP and API-key helpers live in `skills/support` | Adapters can't reuse them | Move to a shared module |
| Persona `voice` is a Kokoro voice id | Switching TTS provider means editing every persona | Voice ids per provider |
| No runtime handle | A web UI or avatar would need to drive the agent directly | Runtime API with sessions, event stream and cancellation |
| Profile is hand-edited | JARVIS can't learn new facts | `remember` skill |
