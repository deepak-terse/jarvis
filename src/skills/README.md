# skills

Tools the model can call. Each skill is a folder implementing `Skill` from `core/contracts`, enabled by a `[skills.<name>]` section in `config.toml`.

```text
skills/
├── registry.rs     # config name → skill; serves them to the agent as `Tools`
├── execution/      # time limit, cache, web-search fallback for every skill
├── support/        # HTTP + API keys, phrasing for speech
├── context.rs      # what a skill is built with: HTTP client, profile, paths
└── <name>/         # one folder per skill, provider clients beside it
```

## Available skills

| Skill | Answers | Source | Key |
|---|---|---|---|
| `web_search` | Anything current; fallback for every other skill | Tavily | `TAVILY_API_KEY` |
| `news` | Headlines by category, refreshed in the background | RSS feeds in config | — |
| `knowledge` | "Who is", "what is" | Wikipedia | — |
| `about_today` | Notable events on this day | Wikipedia | — |
| `stocks` | Stock and index prices by company name | Twelve Data | `TWELVE_DATA_API_KEY` |
| `currency` | Conversions with live rates | Twelve Data | `TWELVE_DATA_API_KEY` |
| `weather` | Now, later, tomorrow, weekend, air quality, alerts | WeatherAPI.com | `WEATHER_API_KEY` |

## Shared settings

Every `[skills.<name>]` section accepts these, on top of its own settings:

| Key | Default | Does |
|---|---|---|
| `enabled` | `true` | `false` skips the skill entirely |
| `timeout_secs` | per skill (8) | Give up after this long |
| `cache_ttl_secs` | per skill (0) | Reuse answers for identical arguments; 0 is off |
| `fallback` | `true` | On failure or timeout, answer with `web_search` |
| `description` | built in | Override the tool description the model sees |

If the fallback fails too, the model is told to say it couldn't get the answer instead of guessing. The flow is in [Implementation](../../docs/IMPLEMENTATION.md#skill-guard).

## Adding a skill

1. Create `<name>/mod.rs` implementing `Skill`, with a `create(settings, ctx)` function. Put provider clients next to it (e.g. `twelve_data.rs`).
2. Add one line to `create` in `registry.rs`.
3. Add a `[skills.<name>]` section to `config.toml` and the name to the personas that should use it.
4. Add routes and a worked example to `data/tool_examples.toml`, and cases to `bench/cases.toml`. Run `jarvis bench`.

Import only `core/contracts` and `skills` itself (`support`, `SkillContext`). The skill implements the lookup; time limits, caching and fallback come from `execution/`.

## Design principles

- **One clear tool per kind of question.** The model picks the tool and passes specific arguments. No hidden routing behind one search tool.
- **Answers built for speech.** Short, plain sentences with units and an "as of" time. No URLs; the source name is kept for "according to …".
- **Fast and honest.** Every skill has a time limit and a fallback. No guessing.
- **Cache while fresh.** Seconds to minutes for prices, a day for encyclopedic facts.
- **Config over code.** Providers, keys, feeds, limits and enabled skills are all set in config.
- **Personas see only the tools they need,** to keep the small model accurate.

## Skill notes

**news**
- Categories and feeds live in `[skills.news.categories]`; a new category is offered to the model automatically
- Feeds refresh every `refresh_mins` in the background; questions are answered from memory
- Duplicate stories across feeds are removed; feeds older than `stale_after_hours` are flagged in logs

**stocks**
- Users say company names, not tickers. Config `aliases` win, then search; learned names are saved to `<storage>/skills/stock_symbols.toml`
- Indices must be listed in `aliases`; Twelve Data's search doesn't return them
- `exchanges` sets preferred exchanges; `any_exchange` allows others (e.g. Apple)

**weather**
- Default place and units come from `profile.toml` (`location`, `country`, `units`)
- Understands now, tonight, tomorrow, weekend, weekdays, ISO dates and periods like "tomorrow evening", in the destination's timezone. Tonight is 18:00–06:00; evening is 18:00–24:00
- Past dates and dates outside the forecast are reported as unavailable
- Caches the whole provider response per place for `bundle_cache_ttl_secs` (10 min), so follow-ups are instant. Keep `cache_ttl_secs = 0` so time windows are re-evaluated
- Set `forecast_days` to your [WeatherAPI plan](https://www.weatherapi.com/pricing): Free 3, Starter 7, max 14
- Air quality uses [US EPA categories 1–6](https://www.weatherapi.com/docs/air-quality/), not India's AQI
- [Alerts](https://www.weatherapi.com/docs/weather-alerts/) aren't guaranteed for India; no alerts is not an all-clear
- `[skills.weather.thresholds]` sets when flags are raised; `city_thresholds` overrides them per city

## Testing

```bash
cargo test skills                              # deterministic, includes local HTTP mocks
cargo test -- --ignored live --nocapture       # live API checks; needs keys
cargo jarvis bench --live                      # full turns, answers printed for review
```

## Roadmap

| Next | Answers | Note |
|---|---|---|
| Sports | Results, fixtures, live scores | Needs good cricket coverage |
| Places | "Good café near me", opening hours | Off by default: shares location with the provider |
| Commute, flights, trains | Travel times and status | When the bench shows search failing on them |
| MCP bridge | Any MCP server's tools | Third-party skills without recompiling |

Any skill that isn't clearly better than web search alone on the bench gets disabled.
