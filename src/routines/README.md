# routines

Turns JARVIS starts itself rather than the user.

| File | Does |
|---|---|
| `greeting.rs` | Startup greeting: a historical event, headlines and weather flags |

## Config

```toml
[greeting]
enabled       = false          # opt in
headlines     = 3
news_category = "top"
```

## Notes

- Built from skills with a fixed template, without an LLM round.
- Skips failed results and web-search fallbacks.
- Only used if ready by the time the models finish loading; startup never waits for it.
- Planned: reminders and scheduled briefings, once JARVIS runs as a background service.
