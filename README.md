# Süper Lig 2026-27 Simulator

[English](README.md) | [Türkçe](README.tr.md)

[![CI](https://github.com/omercagatay/superlig-2026-27-simulator/actions/workflows/ci.yml/badge.svg)](https://github.com/omercagatay/superlig-2026-27-simulator/actions/workflows/ci.yml)

A full-stack Monte Carlo forecast for the 2026-27 Trendyol Süper Lig. It combines a Rust simulation engine, a React dashboard, live results from the Türkiye Futbol Federasyonu, and an optional Kimi-powered scenario analyzer.

> The probabilities and fair odds produced by this project are model estimates, not betting or financial advice.

## Highlights

- Simulates the full 306-fixture, 34-matchday double round-robin between all 18 clubs, 100–200,000 seasons in parallel with Rayon; the dashboard defaults to 50,000.
- Blends Elo, Dixon–Coles, and pi-ratings — the latter two fitted on 14 seasons of real Süper Lig results — into expected-goal estimates.
- Uses Dixon–Coles joint score sampling, which represents low-scoring and drawn matches better than independent Poissons.
- Locks confirmed results from the official TFF calendar and simulates only the remaining fixtures.
- Applies the Süper Lig's own classification rules, in which **head-to-head decides before goal difference**.
- Produces title, top-two, exact third/fourth, top-four, and relegation odds; an 18×18 finishing-position grid; a projected final table; next-matchday forecasts; and pairwise "who finishes above whom" probabilities.
- Optionally converts natural-language scenarios into validated Elo overrides with Kimi and reruns the season (an API endpoint, off unless `SCENARIO_ENABLED=1`).
- Computes the default forecast once per state of the league and serves it from a cache; a bounded queue absorbs bursts of fresh simulations.
- Includes per-IP rate limits, request validation, deterministic seeds, light/dark themes, Docker support, and GitHub Actions CI.

## Stack

| Layer | Technology |
|---|---|
| Backend/API | Rust 1.87+, Axum, Tokio |
| Simulation | Rayon, Rand, Dixon–Coles, pi-ratings, Elo/Poisson |
| Frontend | React 18, TypeScript, Vite |
| Live data | Türkiye Futbol Federasyonu (tff.org) fixture page |
| Historical data | MIT-licensed Club Football Match Data (2012-13 to 2024-25) plus official TFF results (2025-26) |
| Scenario analysis | Kimi via the Moonshot API |
| Deployment | Multi-stage Docker image; Railway-compatible |

## How the model works

The pure-Elo component converts rating difference and home advantage into expected goals:

```text
lambda_home = 1.35 × 10^(( Elo_home - Elo_away + 80) / 1600)
lambda_away = 1.35 × 10^((-Elo_home + Elo_away - 80) / 1600)
```

Home advantage is a property of the **fixture**, not of the club: every club gets the boost at its own ground, in both halves of the double round-robin.

By default those rates are blended with two models trained on real Süper Lig history:

- **Elo (0.5):** current club strength, computed from every top-flight result since 2012-13 and updated after each match this season, plus an 80-point home adjustment.
- **Dixon–Coles (0.3):** time-decayed attack/defence strengths and low-score correlation, fitted over 4,590 chronologically dated matches with a four-year half-life.
- **Pi-ratings (0.2):** sequential home/away strength updates from the same history.

Set `ENSEMBLE_WEIGHTS` to change the blend; `1,0,0` selects the pure-Elo model. When the Dixon–Coles weight is active, scorelines are drawn from its joint distribution.

Clubs promoted from the 1. Lig (Amedspor and Çorum for 2026-27) have no top-flight record, so the Dixon–Coles and pi-rating components give them league-average profiles; their Elo rating still distinguishes them.

**Ratings are uncertain, and the simulation says so.** Each simulated season draws every club's true strength once from a normal around its current rating (`RATING_SIGMA`, 75 Elo points), held fixed for that season. Treating ratings as exact makes favourites' odds run hot and the tails too thin.

The sigma is calibrated, not guessed. The arena replays each held-out season from the model's own match probabilities and compares the spread of the resulting points table with the real one:

| | 2024-25 | 2025-26 |
|---|---:|---:|
| Played-match leader points | 86 | 77 |
| Actual played-match points spread (sd) | 16.3 | 15.2 |
| Model spread, σ=0 | 13.1 | 13.3 |
| Model spread, σ=75 | ~14.4 | ~14.7 |

Independent match sampling understates season-long strength persistence; a shared per-season strength draw closes most of that gap. It costs nothing per analytic match forecast because only season simulation draws it. The 2024-25 archive excludes unplayed administrative awards, so this diagnostic deliberately compares points from played fixtures on both sides.

**A known residual:** the model remains somewhat under-dispersed on 2024-25; an 86-point played-match leader occurs in roughly 12% of replays from the season's start (`EVAL_SEASON_SIM` in `examples/backtest.rs`). Read title odds as "who is most likely to win", not as a precise forecast of the winning points total.

**Ratings move with the season.** The Elo component starts from a preseason rating computed from the results history and is walked forward through every result played so far, in kick-off order: a `ELO_K`-scaled update with a square-root goal-difference multiplier, transferring rating from loser to winner. It is always recomputed from the baseline rather than compounded, so a repeated live refresh is a no-op. The Dixon–Coles and pi-rating components stay at their fitted values until refitted offline.

Manual overrides and Kimi scenarios update the Elo component. The embedded Dixon–Coles and pi-rating parameters stay unchanged until the historical models are explicitly refitted.

### Classification rules

The table is ranked by:

1. Points
2. Head-to-head points
3. Head-to-head goal difference
4. Head-to-head goals scored
5. Overall goal difference
6. Overall goals scored
7. Play-off

Head-to-head precedes goal difference — this is not the FIFA/UEFA group-stage order. Head-to-head is applied **once** per block of clubs level on points; clubs still level after that pass fall through to overall goal difference rather than to a fresh mini-table among the remainder. The published rule does not specify this case, so it is a stated modelling assumption, covered by a dedicated test.

The dashboard reports exact finishing bands—top two, third, fourth, and top four—rather than assigning UEFA competition names. Actual qualification depends on the Turkish Cup winner and that season's UEFA access list, so league position alone is not enough. The bottom three are relegated.

### Calibration

The Elo constants are verified against the real league rather than inherited:

| | Empirical (14 seasons) | Simulated |
|---|---:|---:|
| Home goals per match | 1.566 | — |
| Away goals per match | 1.222 | — |
| Home wins | 45.3% | 44.7% |
| Draws | 25.8% | 24.5% |

`tests/calibration.rs` fails the build if these drift apart.

### Model selection

The blend was chosen by backtest, not by fiat. `cargo run --release --example arena` pits candidate models against two held-out seasons (fit through 2023-24, tune on 2024-25; refit through 2024-25, judge on 2025-26), scored on log-loss, RPS, Brier, and accuracy. Test-season leaderboard (2025-26, 306 matches, log-loss per match):

| model | log-loss | RPS |
|---|---:|---:|
| Dixon–Coles, 1.5y half-life | 0.9916 | 0.1925 |
| **ensemble 0.5/0.3/0.2** | **0.9949** | **0.1943** |
| logistic stack | 0.9974 | 0.1952 |
| Dixon–Coles, 4y half-life | 0.9988 | 0.1950 |
| ensemble, validation-fitted weights | 1.0018 | 0.1957 |
| Elo–Poisson | 1.0049 | 0.1963 |
| pi-ratings | 1.0076 | 0.1981 |
| home-advantage baseline | 1.0865 | 0.2250 |

Every fitted model clears the baselines by a wide margin. The production blend is within 0.0033 log-loss of the test winner while remaining competitive on validation. Weights fitted only to validation (0.4/0/0.6) win that season but fall behind on test, so the broader 0.5/0.3/0.2 blend stays.

The arena also builds a leak-free walk-forward multinomial logistic stack whose features are fitted only on earlier seasons. It scores 0.9974 on the test season: respectable, but behind the statistical ensemble. Results-only features do not currently justify replacing the simpler model; richer inputs such as xG and lineups are not available here.

## Run locally

### Prerequisites

- Rust 1.87 or later
- Node.js 20.19 or later with npm

The baseline simulator does not require an API key. `KIMI_API_KEY` is needed only for natural-language scenarios, which are also off unless `SCENARIO_ENABLED=1`.

### Development mode

The Vite development server proxies `/api` to port `3001`, so run the backend on that port.

Terminal 1:

```bash
git clone https://github.com/omercagatay/superlig-2026-27-simulator.git
cd superlig-2026-27-simulator
cp .env.example .env
PORT=3001 cargo run --release
```

Terminal 2:

```bash
cd superlig-2026-27-simulator/frontend
npm ci
npm run dev
```

Open <http://localhost:5173>. The page loads the server's default forecast, which the backend computes once after startup. The first load can take a few seconds while that run finishes.

### Production-like local build

Build the frontend first; Axum then serves `frontend/dist` together with the API on port `3000`.

```bash
cd frontend
npm ci
npm run build
cd ..
cargo run --release
```

Open <http://localhost:3000>.

## Configuration

Copy `.env.example` to `.env` and adjust these values as needed:

| Variable | Default | Purpose |
|---|---:|---|
| `KIMI_API_KEY` | unset | Key for `/api/scenario`; obtain one from the Moonshot platform. The endpoint also needs `SCENARIO_ENABLED`. |
| `SCENARIO_ENABLED` | `0` | `1` or `true` turns on `/api/scenario`, which spends money on every call; otherwise it answers 404. The dashboard does not use it. |
| `PORT` | `3000` | Backend HTTP port. Use `3001` with the Vite development server. |
| `RUST_LOG` | `superlig_sim=info` | Rust tracing filter. |
| `LIVE_REFRESH_MINUTES` | `30` | TFF refresh interval; `0` disables background refresh. Each refresh attempt ends by warming the default forecast; with `0` it is warmed once at startup. |
| `MAX_CONCURRENT_SIMULATIONS` | `1` | Simulations that may run at once (Rayon already uses every core inside one run). |
| `MAX_SIMULATION_QUEUE` | `8` | Further distinct simulations allowed to wait for a free slot; beyond that the server answers 503 with `Retry-After`. |
| `SIM_QUEUE_TIMEOUT_SECS` | `20` | How long a queued simulation waits for a slot before the server answers 503. |
| `ENSEMBLE_WEIGHTS` | `0.5,0.3,0.2` | Comma-separated Elo, Dixon–Coles, and pi-rating weights. |
| `TRUST_PROXY` | `0` | Trust `X-Forwarded-For` for rate limiting only behind a sanitizing reverse proxy. |

## Use the dashboard

1. The page opens on the server's default forecast (50,000 seasons, seed 12345). To change it, choose the simulation count and seed, then select **Run**. Reusing a seed makes the same configuration reproducible.
2. Explore the five tabs: **Forecast**, **Positions**, **Races**, **Table**, and **Live**.
3. Select **Update live data** to pull the latest TFF results and recompute the forecast, fixture cards, and accuracy view. Results you assumed for matches that have since been played are dropped, with a notice.
4. Natural-language scenarios are not part of the dashboard. They are an optional API endpoint (`/api/scenario`, below) that is off unless the server sets `SCENARIO_ENABLED=1` and `KIMI_API_KEY`.

When the server is busy, the page retries a few times and then shows a "try again in a moment" message; the last forecast stays on screen. The fixture list and results stay available even when a simulation fails.

## API

| Endpoint | Method | Limit per IP | Description |
|---|---|---:|---|
| `/api/health` | `GET` | — | Service version, model configuration, last live refresh, `world_version`, and whether the default forecast is cached (`forecast_cached`). |
| `/api/forecast` | `GET` | 30/min | The default forecast (50,000 seasons, seed 12345, no overrides): the same JSON as `POST /api/simulate {}`, computed once per state of the league. |
| `/api/simulate` | `POST` | 30/min | Run a simulation with optional Elo overrides and pinned outcomes. Repeated identical requests are answered from a cache until the results or ratings change. |
| `/api/scenario` | `POST` | 10/min | Analyze a prompt with Kimi and rerun with its Elo overrides. Returns 404 unless `SCENARIO_ENABLED=1`. |
| `/api/refresh` | `POST` | 5/min | Fetch and apply the current TFF results. |
| `/api/live` | `GET` | — | Return the most recently cached live-data snapshot. |
| `/api/accuracy` | `GET` | 30/min | How the model has scored against predictions frozen before kick-off: hit rate, log-loss vs the base-rate baseline, and calibration bands. |
| `/api/upcoming` | `GET` | 30/min | Home/draw/away forecasts for the next matchday's unplayed fixtures. |
| `/api/matches` | `GET` | 30/min | The full 306-fixture calendar: real scores for played games, 1X2 / over-under 2.5 / both-teams-to-score probabilities and fair odds for the rest. |

`/api/forecast`, `/api/accuracy`, `/api/upcoming`, and `/api/matches` share one 30/min allowance per IP. A rate-limited request gets HTTP 429 with a `Retry-After` header.

Simulations run one at a time by default (`MAX_CONCURRENT_SIMULATIONS`). Up to `MAX_SIMULATION_QUEUE` further runs wait up to `SIM_QUEUE_TIMEOUT_SECS` for a turn. Past that, `/api/simulate`, `/api/forecast`, and `/api/scenario` answer HTTP 503 with `Retry-After: 10`. Cache hits, and requests identical to a run already in progress, never wait in the queue.

The next matchday, used by `/api/upcoming` and as the calendar's `current_round`, is the round of the earliest unplayed fixture dated today or later (Istanbul time). One postponed match therefore does not hold it back. When only overdue fixtures remain, it is the earliest round that still has one; once the season is complete, the last round.

The API sends no CORS headers: the dashboard is served from the same origin, and the Vite development server proxies `/api`.

`/api/simulate` also accepts `what_if`: up to 20 pinned outcomes for unplayed fixtures, e.g. `[{"home":"Gaziantep","away":"Galatasaray","outcome":"home"}]`. The *outcome* is fixed, not the scoreline — the model keeps drawing until the result matches, so goal difference stays honest. Pinning an already-played or non-existent fixture is an error rather than a silent no-op.

Simulation requests accept 100–200,000 trials. Scenario prompts are limited to 2,000 characters, Elo overrides must name a known club and fall between 1,200 and 2,000; LLM scenarios return rating changes of at most ±150 points, and request bodies are limited to 1 MiB. The scenario's analysis text is capped at 600 characters. When the LLM call fails, the client gets a generic HTTP 502; the details go to the server log only.

### Default forecast

```bash
curl http://localhost:3000/api/forecast
```

### Baseline simulation

```bash
curl -X POST http://localhost:3000/api/simulate \
  -H 'Content-Type: application/json' \
  -d '{"n_sims":50000,"seed":12345}'
```

### Simulation with a manual rating override

`elo_overrides` contains replacement ratings, not point deltas.

```bash
curl -X POST http://localhost:3000/api/simulate \
  -H 'Content-Type: application/json' \
  -d '{"n_sims":50000,"seed":12345,"elo_overrides":{"Trabzonspor":1720}}'
```

### Natural-language scenario

Requires `SCENARIO_ENABLED=1` and `KIMI_API_KEY` on the server.

```bash
curl -X POST http://localhost:3000/api/scenario \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"Fenerbahçe sacks its manager","n_sims":50000,"seed":12345}'
```

## Docker

```bash
docker build -t superlig-sim .
docker run --rm -p 3000:3000 \
  -e SCENARIO_ENABLED=1 \
  -e KIMI_API_KEY=your_key \
  superlig-sim
```

Omit `SCENARIO_ENABLED` and `KIMI_API_KEY` if scenario analysis is not needed. The build context leaves out host build outputs, `node_modules`, `.env` files, Git metadata, and documentation (see `.dockerignore`).

## Deploy to Railway

1. Create a Railway service from this GitHub repository, or run `railway init && railway up` from a clone.
2. Railway detects the root `Dockerfile` and builds the Rust backend and React frontend.
3. To keep scenario analysis available, set both `SCENARIO_ENABLED=1` and `KIMI_API_KEY`; without `SCENARIO_ENABLED`, `/api/scenario` answers 404.
4. Set `TRUST_PROXY=1` so rate limiting uses the client address supplied by Railway's sanitizing edge proxy.
5. Optionally customize `LIVE_REFRESH_MINUTES`, `ENSEMBLE_WEIGHTS`, `MAX_CONCURRENT_SIMULATIONS`, `MAX_SIMULATION_QUEUE`, `SIM_QUEUE_TIMEOUT_SECS`, and `RUST_LOG`.
6. Set the health-check path to `/api/health`.

The application reads Railway's injected `PORT` automatically. The scheduled data-refresh workflow (see [Refresh the data](#refresh-the-data)) also redeploys after committing new data, but only when the repository has a `RAILWAY_TOKEN` secret.

## Validation

The GitHub Actions workflow runs the same core checks:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
cargo audit

cd frontend
npm ci
npm audit
npx tsc --noEmit
npm run build
npm test            # needs Node.js 22.6+ (--experimental-strip-types)
cd ..

# Offline tests for the fixture fetch script (no network).
python3 -m unittest discover -s scripts -p 'test_*.py'
```

## Refresh the data

The repository already includes the fixture calendar, historical results, and fitted Dixon–Coles parameters used at build time. To refresh them:

```bash
# Official 2026-27 calendar and results so far, from tff.org.
# Asserts the full double round-robin structure: 306 fixtures, 34 rounds,
# 9 per round, 153 pairs each played exactly twice. A postponed match that
# TFF lists without a date keeps its committed date and kick-off. A committed
# result that disappears or changes stops the run before the file is written;
# set ALLOW_RESULT_CHANGES=1 for a genuine TFF correction or award.
python3 scripts/fetch_fixtures.py

# Chronological 2012-13 to 2024-25 results from the MIT-licensed Club Football
# Match Data archive, plus the official TFF 2025-26 weekly archive. The script
# validates every expected season/week count before replacing the CSV.
python3 scripts/fetch_history.py

# Refit Dixon–Coles against the refreshed history (~0.3 s).
cargo run --release --example fit_dc
```

Review the changed files under `data/` before committing a new fit.

The committed calendar is also refreshed automatically. A scheduled GitHub Actions workflow (`.github/workflows/refresh-data.yml`) runs on Thursdays at 15:17 UTC and Fridays at 06:00 UTC, and on demand. It fetches the calendar, freezes forecasts for upcoming fixtures, runs the test suite, and commits any changes. A failed run opens a "Weekly data refresh failed" issue, or comments on the open one, and the next successful run closes it. To accept a genuine TFF result correction, run the workflow by hand with `allow_result_changes`.

Data-source details worth knowing about:

- **Historical match dates are preserved.** Dixon–Coles recency weights and sequential pi-ratings both depend on chronology; the generator rejects partial season archives instead of silently fitting them.
- The redistributed archive-derived rows retain their upstream MIT notice in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

- **tff.org serves its pages as windows-1254**, not UTF-8. Decoding them as UTF-8 mangles every Turkish club name.
- **tff.org serves an incomplete TLS chain**, omitting the GlobalSign intermediate that signs its certificate. Browsers recover by chasing the certificate's AIA URL; `curl`, Python `urllib`, and `reqwest` do not, and fail with "unable to get local issuer certificate". The fetch script performs the AIA fetch itself (pinned by SHA-256) and the server embeds the same intermediate at `data/tff_intermediate.pem`. Certificate verification stays on throughout.

## Project structure

```text
.
├── src/
│   ├── main.rs           # Axum server, configuration, and background refresh
│   ├── sim.rs            # Season simulation and parallel Monte Carlo engine
│   ├── league.rs         # Table records and Süper Lig classification rules
│   ├── data.rs           # Clubs, ratings, and the official fixture calendar
│   ├── dixoncoles.rs     # Dixon–Coles fitting and joint score probabilities
│   ├── piratings.rs      # Historical pi-rating model
│   ├── history.rs        # Historical result loading and club normalization
│   ├── scraper.rs        # TFF live-result ingestion
│   ├── handlers.rs       # API handlers and the simulation queue
│   ├── forecast_cache.rs # Simulation responses cached per league state
│   ├── llm.rs            # Kimi scenario analysis
│   ├── models.rs         # API request and response types
│   ├── validation.rs     # Request validation
│   └── rate_limit.rs     # Per-IP rate limiting
├── data/                 # Fixture calendar, historical results, fitted params
├── frontend/             # React and TypeScript dashboard
├── examples/             # Model fitting utility
├── scripts/              # Data-acquisition scripts and their offline tests
├── tests/calibration.rs  # Guards the Elo constants against the real league
├── .github/workflows/    # CI and the scheduled data refresh
└── Dockerfile            # Production multi-stage image
```

## Data and model caveats

- Live refresh depends on tff.org and its current page format; the embedded baseline calendar remains available if a refresh fails.
- Fair odds are simply the inverse of simulated probabilities and do not include a bookmaker margin, liquidity, or market information.
- Promoted clubs are the least well-modelled: they have no top-flight history for the Dixon–Coles and pi-rating components, so their forecasts lean entirely on their Elo rating.
- The head-to-head-once rule (see above) is a stated assumption, not a published one.
- Scenario ratings are model-generated assumptions. Read the returned explanation and treat the output as exploratory.
- Forecast quality depends on ratings, historical-data coverage, model assumptions, and the number of trials.
