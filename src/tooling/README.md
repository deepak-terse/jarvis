# tooling

Development and diagnostics.

| File | Does |
|---|---|
| `bench.rs` | `jarvis bench`: tool-choice accuracy and latency per category |
| `observability.rs` | Tracing to `logs/jarvis.jsonl`; `MetricsLog` writes `logs/metrics.jsonl` |

## Benchmark

```bash
cargo jarvis bench                  # first decision only, tools not run
cargo jarvis bench --live           # full turns with tools; prints answers for manual review
cargo jarvis bench path/cases.toml  # custom cases
```

Cases live in `bench/cases.toml`:

```toml
[[case]]
prompt   = "50 euros in rupees"
category = "currency"
expect   = "currency"          # or ["news", "web_search"], or omit for "answer directly"
```

Run it after changing the prompt, tool examples, tool descriptions or the model.

## Logs

- `RUST_LOG=info` (default `warn`) controls tracing
- Metric fields are listed in [Implementation](../../docs/IMPLEMENTATION.md#metrics)
