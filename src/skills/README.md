# JARVIS — Live Data Skills (Feature Plan)

## Goal

Answer most everyday live-data questions quickly and accurately by voice. Dedicated sources handle structured data, RSS handles headlines, and web search covers everything else and acts as the fallback. Weather is implemented in `live_data/weather` using WeatherAPI.com; see the root README for setup, forecast limits, thresholds and tests.

## Guiding principles

- **One clear tool per kind of question.** The LLM chooses the tool and passes specific arguments (company name, team, category). There's no hidden routing behind a single search tool.
- **Answers built for speech.** Short, plain sentences with units and an "as of" time. No URLs, but the source name is kept so JARVIS can say "according to …".
- **Fast and honest.** Every skill has a time limit. If its source fails or is slow, it falls back to web search; if that also fails, JARVIS says it couldn't get the information instead of guessing.
- **Repeat questions are instant.** Results are cached for as long as the data stays fresh (seconds to minutes for prices and scores, a day for encyclopedic facts).
- **Everything configurable.** Providers, API keys (via environment variables), sources, RSS feeds, time limits, and which skills are enabled are all set in config, with no code changes needed.
- **Personas see only the tools they need,** to keep the small model accurate.

## Skills

**1. Web search (improve existing)**
The fallback for anything not covered below and the default for "why" and "what happened" questions. JARVIS should be able to ask for recent results only (today, this week, …) and indicate whether the question is general, news, or finance. Results should favor sources from the user's country, taken from the profile.

**2. News headlines (RSS)**
Answers "what's the news" and "any tech/business/sports news today" with the top few recent headlines in a category. Feeds are grouped into categories in config and can be added, removed, or changed at any time; a new category becomes available to JARVIS automatically. Feeds refresh in the background so headline questions are answered without waiting on the network. Duplicate stories across feeds are removed, and feeds that stop updating are flagged in the logs. Detailed questions about a story go to web search.

**3. Stocks and indices**
Answers "how's Reliance doing" or "where's Nifty today." The user says company names, not tickers, so JARVIS resolves the name and remembers the mapping. Indian exchanges are the default.

**4. Currency conversion**
Answers "100 dollars in rupees" with live rates.

**5. Quick knowledge**
Answers "who is X" and "what is Y" from Wikipedia summaries, which are faster and more reliable than search snippets for encyclopedic facts.

**6. About today**
Answers "what's special about today" with notable historical events from Wikipedia. Festival and holiday questions use web search.

**7. Sports**
Answers last results, upcoming fixtures, and live scores for a team or league. Cricket coverage is a requirement, given the user's location.

**8. Places**
Answers "is there a good café near me" or "when does X close." Uses the profile location by default. **Disabled by default** until the privacy trade-off (sharing location with the provider) and cost are accepted.

## Core behaviors

- **Time and place awareness:** JARVIS always knows the current date, time, timezone, and user location, so it writes precise queries and can judge whether information is current.
- **Daily greeting:** at startup, JARVIS greets with a notable historical event and a few top headlines. This is prepared in the background and never delays startup.
- **Spoken filler:** when a lookup starts, JARVIS says a short phrase ("Let me check") right away, so the user isn't waiting in silence.
- **No lookups for computable things:** time, date, unit conversions, and arithmetic are answered directly.

## Rollout

| Phase | Features | Done when |
|---|---|---|
| **1. Foundation** | Web search improvements, time/place awareness, spoken filler, fallback and caching behavior, a test set of ~50 real questions | Baseline accuracy and latency recorded per category |
| **2. News + knowledge** | RSS headlines, quick knowledge, about today, daily greeting | Headlines answered instantly; greeting mentions a historical event |
| **3. Finance** | Stocks and indices, currency conversion | Company names and conversions answered correctly |
| **4. Sports + places** | Sports, places (behind a toggle) | Clearly better than web search alone on the test set |
| **5. Later** | Commute time, flight and train status | Test set shows these failing through search |

## Check before building

- The finance provider covers NSE/BSE on your plan, with workable rate limits.
- The sports provider has live scores and good cricket coverage on your plan; if not, choose a cricket-specific source.
- Whether higher-depth web search is worth the extra cost and latency, measured in phase 1.
- Places: current API version, billing, and whether sharing location is acceptable.

## Measuring success

Rerun the test set after each phase. For each category, track whether the right tool was chosen, whether the answer was correct, time to the filler, time to the answer, and how often the fallback was used. Any skill that isn't clearly better than web search alone gets disabled in config.
