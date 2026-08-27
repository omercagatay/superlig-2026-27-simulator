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
- Converts natural-language scenarios into validated Elo overrides with Kimi and reruns the season.
- Includes per-IP rate limits, request validation, deterministic seeds, light/dark themes, Docker support, and GitHub Actions CI.

## Stack

| Layer | Technology |
|---|---|
| Backend/API | Rust 1.87+, Axum, Tokio |
| Simulation | Rayon, Rand, Dixon–Coles, pi-ratings, Elo/Poisson |
| Frontend | React 18, TypeScript, Vite |
| Live data | Türkiye Futbol Federasyonu (tff.org) fixture page |
| Market odds | Nesine pre-match bulletin (comparison only, never a model input) |
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

- **Elo (0.5):** current club strength on the ClubElo scale, plus an 80-point home adjustment.
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

**A known residual:** the model remains somewhat under-dispersed on 2024-25; an 86-point played-match leader occurs in roughly 7% of σ≈75 replays. Read title odds as "who is most likely to win", not as a precise forecast of the winning points total.

**Ratings move with the season.** The Elo component starts from the preseason ClubElo baseline and is walked forward through every result played so far, in kick-off order: a `ELO_K`-scaled update with a square-root goal-difference multiplier, transferring rating from loser to winner. It is always recomputed from the baseline rather than compounded, so a repeated live refresh is a no-op. The Dixon–Coles and pi-rating components stay at their fitted values until refitted offline.

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

The baseline simulator does not require an API key. `KIMI_API_KEY` is needed only for natural-language scenarios.

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

Open <http://localhost:5173>. The first forecast starts automatically.

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
| `KIMI_API_KEY` | unset | Enables `/api/scenario`; obtain a key from the Moonshot platform. |
| `PORT` | `3000` | Backend HTTP port. Use `3001` with the Vite development server. |
| `RUST_LOG` | `superlig_sim=info` | Rust tracing filter. |
| `LIVE_REFRESH_MINUTES` | `30` | TFF refresh interval; `0` disables background refresh. |
| `MAX_CONCURRENT_SIMULATIONS` | `1` | Global cap on overlapping Rayon simulations; excess requests receive HTTP 429. |
| `ENSEMBLE_WEIGHTS` | `0.5,0.3,0.2` | Comma-separated Elo, Dixon–Coles, and pi-rating weights. |
| `TRUST_PROXY` | `0` | Trust `X-Forwarded-For` for rate limiting only behind a sanitizing reverse proxy. |

## Use the dashboard

1. Choose the simulation count and seed, then select **Run**. Reusing a seed makes the same configuration reproducible.
2. Explore the five tabs: **Forecast**, **Positions**, **Races**, **Table**, and **Live**.
3. Select **Update live data** to pull the latest TFF results and recompute the forecast, fixture cards, and accuracy view.
4. Enter a scenario such as `Galatasaray's first-choice keeper is suspended for five matches`. Kimi explains the effect, supplies validated club ratings, and starts a new simulation.

## API

| Endpoint | Method | Limit per IP | Description |
|---|---|---:|---|
| `/api/health` | `GET` | — | Service version, model configuration, and last live refresh. |
| `/api/simulate` | `POST` | 30/min | Run a baseline simulation with optional Elo overrides. |
| `/api/scenario` | `POST` | 10/min | Analyze a prompt with Kimi and rerun with its Elo overrides. |
| `/api/refresh` | `POST` | 5/min | Fetch and apply the current TFF results. |
| `/api/live` | `GET` | — | Return the most recently cached live-data snapshot. |
| `/api/accuracy` | `GET` | 30/min | How the model has scored against predictions frozen before kick-off: hit rate, log-loss vs the base-rate baseline, and calibration bands. |
| `/api/upcoming` | `GET` | 30/min | Home/draw/away forecasts for the next matchday's unplayed fixtures. |
| `/api/matches` | `GET` | 30/min | The full 306-fixture calendar: real scores for played games, 1X2 / over-under 2.5 / both-teams-to-score probabilities and fair odds for the rest, plus bookmaker prices and the model-vs-market gap where available. |

`/api/simulate` also accepts `what_if`: up to 20 pinned outcomes for unplayed fixtures, e.g. `[{"home":"Gaziantep","away":"Galatasaray","outcome":"home"}]`. The *outcome* is fixed, not the scoreline — the model keeps drawing until the result matches, so goal difference stays honest. Pinning an already-played or non-existent fixture is an error rather than a silent no-op.

Simulation requests accept 100–200,000 trials. Scenario prompts are limited to 2,000 characters, Elo overrides must name a known club and fall between 1,200 and 2,000 (the ClubElo club scale is compressed relative to international Elo), and request bodies are limited to 1 MiB.

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

```bash
curl -X POST http://localhost:3000/api/scenario \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"Fenerbahçe sacks its manager","n_sims":50000,"seed":12345}'
```

## Docker

```bash
docker build -t superlig-sim .
docker run --rm -p 3000:3000 \
  -e KIMI_API_KEY=your_key \
  superlig-sim
```

Omit `KIMI_API_KEY` if scenario analysis is not needed.

## Deploy to Railway

1. Create a Railway service from this GitHub repository, or run `railway init && railway up` from a clone.
2. Railway detects the root `Dockerfile` and builds the Rust backend and React frontend.
3. Add `KIMI_API_KEY` if scenario analysis should be enabled.
4. Set `TRUST_PROXY=1` so rate limiting uses the client address supplied by Railway's sanitizing edge proxy.
5. Optionally customize `LIVE_REFRESH_MINUTES`, `ENSEMBLE_WEIGHTS`, and `RUST_LOG`.
6. Set the health-check path to `/api/health`.

The application reads Railway's injected `PORT` automatically.

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
npm run build
```

## Refresh the data

The repository already includes the fixture calendar, historical results, and fitted Dixon–Coles parameters used at build time. To refresh them:

```bash
# Official 2026-27 calendar and results so far, from tff.org.
# Asserts the full double round-robin structure: 306 fixtures, 34 rounds,
# 9 per round, 153 pairs each played exactly twice.
python3 scripts/fetch_fixtures.py

# Chronological 2012-13 to 2024-25 results from the MIT-licensed Club Football
# Match Data archive, plus the official TFF 2025-26 weekly archive. The script
# validates every expected season/week count before replacing the CSV.
python3 scripts/fetch_history.py

# Refit Dixon–Coles against the refreshed history (~0.3 s).
cargo run --release --example fit_dc
```

Review the changed files under `data/` before committing a new fit.

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
│   ├── handlers.rs       # API handlers
│   ├── llm.rs            # Kimi scenario analysis
│   ├── models.rs         # API request and response types
│   ├── validation.rs     # Request validation
│   └── rate_limit.rs     # Per-IP rate limiting
├── data/                 # Fixture calendar, historical results, fitted params
├── frontend/             # React and TypeScript dashboard
├── examples/             # Model fitting utility
├── scripts/              # Data-acquisition scripts
├── tests/calibration.rs  # Guards the Elo constants against the real league
├── .github/workflows/    # CI configuration
└── Dockerfile            # Production multi-stage image
```

## Data and model caveats

- Live refresh depends on tff.org and its current page format; the embedded baseline calendar remains available if a refresh fails.
- Fair odds are simply the inverse of simulated probabilities and do not include a bookmaker margin, liquidity, or market information.
- Promoted clubs are the least well-modelled: they have no top-flight history for the Dixon–Coles and pi-rating components, so their forecasts lean entirely on their Elo rating.
- The head-to-head-once rule (see above) is a stated assumption, not a published one.
- Scenario ratings are model-generated assumptions. Read the returned explanation and treat the output as exploratory.
- Forecast quality depends on ratings, historical-data coverage, model assumptions, and the number of trials.
