# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Monte Carlo simulator for the 2026-27 Trendyol Süper Lig (18 clubs, 34 matchdays, 306 fixtures). Rust/axum backend runs the simulation and serves a built React frontend as static files; an LLM (Kimi/Moonshot) turns natural-language "what if" scenarios into Elo adjustments that get re-simulated.

The local directory is still named `wc2026-sim`, but the GitHub repository is `superlig-2026-27-simulator` and the crate/binary are `superlig-sim`. The separate `worldcup-2026-simulator` repository and Railway project are different projects and must not be touched.

## Commands

### Backend (Rust, repo root)

```bash
cargo run --release             # serve on :3000 (reads .env for KIMI_API_KEY, PORT, RUST_LOG)
cargo test                      # all tests (inline #[cfg(test)] plus tests/calibration.rs)
cargo test <test_name>          # single test, e.g. cargo test simulate_is_deterministic_for_same_seed
cargo fmt -- --check            # CI formatting check
cargo clippy --all-targets -- -D warnings   # CI lint (warnings are hard errors)
cargo build --release
cargo run --release --example fit_dc        # refit Dixon-Coles against data/superlig_results.csv
cargo run --release --example arena         # backtest 10 candidate models on held-out seasons
cargo run --release --example log_forecast  # freeze predictions for unplayed fixtures (run before each matchday)
```

### Data acquisition (Python 3, no dependencies)

```bash
python3 scripts/fetch_fixtures.py   # tff.org -> data/fixtures_2026_27.json (306 fixtures)
python3 scripts/fetch_history.py    # MIT archive + official TFF -> chronological history
```

Both are offline/manual: the committed data files are what the binary embeds via `include_str!`/`include_bytes!`. After `fetch_history.py`, re-run `fit_dc`.

### Frontend (`frontend/`)

```bash
npm install
npm run dev            # Vite dev server on :5173, proxies /api to http://localhost:3001 (note: not :3000 — start the backend with PORT=3001 for the dev proxy to work, or run `npm run build` and hit the backend directly on :3000)
npx tsc --noEmit        # CI type check
npm run build           # tsc + vite build -> frontend/dist (served by the Rust binary in prod/docker)
```

CI (`.github/workflows/ci.yml`) runs stable backend checks, a Rust 1.87 MSRV check, RustSec/npm audits, and the frontend type check/build.

### Docker

`Dockerfile` is a 3-stage build: Rust backend builder (with a dependency-caching dummy-`main.rs` layer) → Node frontend builder → slim Debian runtime that copies the backend binary and `frontend/dist`. Deploys to Railway by auto-detecting the Dockerfile; health check path is `/api/health`.

The dummy-layer `rm -rf` globs and the binary copy path both key on the **package name**. Renaming the package without updating the Dockerfile breaks the deploy and nothing else — `cargo build` still passes.

## Architecture

### Request flow

`src/main.rs` builds one `AppState` (`Arc<AppState>`) holding:
- `world: Arc<RwLock<World>>` — the live simulation state (clubs, Elo ratings, fixture calendar, already-played results)
- `live_data: Arc<RwLock<Option<LiveData>>>` — cached scrape results
- `market: Arc<RwLock<Option<MarketSnapshot>>>` — last good bookmaker snapshot
- `simulation_slots: Arc<Semaphore>` — global CPU-heavy request bound
- `kimi_api_key: Option<String>`

Routes (`src/handlers.rs`), each with its own per-IP rate limit (`src/rate_limit.rs`, sliding window):
- `POST /api/simulate` (30/min) — accepts validated `elo_overrides` and up to 20 `what_if` outcomes, clones the current `World`, and applies changes **to the clone only**. Pins fix the *outcome*, not the scoreline: `simulate_one_with` redraws (bounded at 64 attempts, then a minimal fallback scoreline) until the result matches.
- `POST /api/scenario` (10/min) — sends the prompt to Kimi (`src/llm.rs`), validates the returned Elo adjustments against club names/bounds (`src/validation.rs`), applies them to a cloned `World`, simulates, returns results plus the LLM's `analysis` text.
- `POST /api/refresh` (5/min) — validates all 306 TFF fixture rows before applying played results, then **does** mutate the shared `World` and caches the raw scrape. Partial or regressive snapshots keep the last good state.
- `GET /api/upcoming` (30/min) — home/draw/away probabilities for the next matchday's unplayed fixtures.
- `GET /api/matches` (30/min) — the whole calendar with per-fixture 1X2, over/under 2.5 and BTTS prices. Probabilities are exact sums over the Dixon-Coles scoreline table (`World::fixture_probs`), not Monte Carlo — the table truncates at 10 goals a side, so the 1X2 triple is renormalized to sum to exactly 100. Played fixtures carry the real score and no forecast (retrodicting with current ratings would mislead).
- `GET /api/coupon` (30/min) — `src/coupon.rs` compares the independently computed 1X2 probabilities with a fresh licensed-market snapshot for the next genuinely future matchday. It returns at most one guarded selection per fixture and three total, or an explicit unavailable/stale/no-value status. It never submits a wager or processes payment.
- `GET /api/live`, `GET /api/health` — read-only.

Everything not matching `/api/*` falls back to `ServeDir::new("frontend/dist")`, so in production this is a single binary serving both API and SPA.

### Simulation core (`src/sim.rs`)

`World::simulate` runs `n_sims` independent trials in parallel via `rayon`, each seeded deterministically (`config.seed.wrapping_add(i * 2654435761)`) so results are reproducible for a given seed — this determinism is asserted in tests, don't break it.

Per trial (`simulate_one`):
1. All 306 fixtures are played in calendar order. A fixture already recorded in `World.played` short-circuits and uses the real result; the rest are sampled from the ensemble λ.
2. `league::apply_result` folds each result into both clubs' `TeamRecord`.
3. `league::rank_table` ranks the final table (see below) and returns the finishing order.

Across trials, `simulate` uses Rayon fold/reduce accumulators instead of retaining every season. It aggregates position, title/top-two/third/fourth/top-four/relegation counts, record sums, fixed-size points histograms, and a flat n×n `pairwise_above` matrix. A separate deterministic sample season remains as a test anchor. The projected-table view is built from per-club **expected** records.

**Ratings are recomputed, never accumulated.** `World.elo_baseline` holds the preseason `data::elo()` ratings; `World::elo` is `refresh_ratings()` replaying every played result over that baseline in kick-off order. `update_from_live` calls it after inserting results. Doing it any other way (mutating `elo` in place) double-counts on the timer-driven refresh — there is a test for this.

**Home advantage is per fixture, not per club.** The World Cup version carried a `host: Vec<bool>` flag; `lam_pair(home, away)` now applies `HOME_ADV` to whichever club is at home in that fixture.

### League rules (`src/league.rs`)

Ranking order is **Points → head-to-head points → H2H GD → H2H GF → overall GD → overall GF → play-off**. Head-to-head comes *before* goal difference — this is not the FIFA group-stage order, and carrying the old one over is the single easiest way to get this wrong.

Head-to-head is applied **once** per block of clubs level on points. Clubs still level after that pass fall through to overall GD/GF, never to a fresh mini-table among the remaining pair. The published rule doesn't specify this; it's a stated assumption with a dedicated test (`head_to_head_is_applied_once_not_recursively`). Ranking lives in its own module, not in `sim.rs`, because it's the subtlest logic here and needs its own test surface.

### Data layer (`src/data.rs`)

18 clubs with ClubElo ratings, the `Fixture` struct, and `fixtures()` reading `data/fixtures_2026_27.json` via `include_str!`. Constants: `N_TEAMS`/`N_ROUNDS`/`N_FIXTURES`, `TOP_TWO_PLACES`/`TOP_FOUR_PLACES`/`RELEGATION_SPOTS`, and the Elo model's `BASE`/`D_DIV`/`HOME_ADV`. Do not turn the upper-table cutoffs into UEFA labels: qualification also depends on the cup and access list.

Changing the club list means re-running both Python scripts, refitting DC, and updating `TFF_NAMES` in `src/scraper.rs`.

### Strength-model ensemble (`src/dixoncoles.rs`, `src/piratings.rs`, `src/history.rs`)

The simulation blends three strength models into each match's expected goals (λ), weighted by `ENSEMBLE_WEIGHTS` env var (`"elo,dc,pi"`, default `0.5,0.3,0.2`; `1,0,0` = pure Elo):
- **Elo-Poisson**: λ from Elo difference via `BASE`/`D_DIV`/`HOME_ADV`.
- **Dixon-Coles** attack/defense params, fit offline against `data/superlig_results.csv` with `cargo run --release --example fit_dc`, which writes `data/dc_params.json`; the server loads that file via `include_str!` at startup. The runtime uses DC λs and, when DC has weight, its joint scoreline distribution (ρ correction).
- **Pi-ratings** (Constantinou–Fenton), computed in one fast pass over the same history at startup (`src/piratings.rs`).

`World.ensemble: Option<Ensemble>` holds the blend; `None` (as in `World::new()` and most tests) means pure Elo. Club indices in DC/pi coincide with `World` indices because `history::TeamIndex::league()` is built from the same `data::elo()` order, plus a trailing `"Other Club"` bucket.

**The bucket is a league-average departed club, not a promoted-club baseline.** It absorbs every match of every club outside the current 18 — relegated and defunct sides alike. Promoted clubs (Amedspor, Çorum for 2026-27) have α=β=0 in the DC fit, i.e. the neutral baseline; only their Elo distinguishes them.

Elo overrides from scenarios act through the Elo component only. `GET /api/health` reports the active model and weights.

### Model selection (`examples/arena.rs`)

The 0.5/0.3/0.2 ensemble weights are validated by backtest, not guessed: the arena fits every candidate on past seasons and scores them on held-out 2024-25 (validation) and 2025-26 (test). The production blend won the test season and is the only model in the top group on both; weights grid-fitted to the validation season overfit it. Re-run the arena before changing `ENSEMBLE_WEIGHTS` defaults. ML has been tried and lost: a walk-forward logistic stacker (in the arena, `logit-stack`) and sklearn GBM/RF/MLP (side experiment, `ARENA_DUMP_DIR=<dir>` dumps the feature CSV) all scored behind the ensemble on the 2025-26 test season — results-only features don't give ML room to win here. The arena's Elo is self-computed from the match history (ClubElo snapshots are unreachable), so its Elo component is the same family as production's, not identical ratings.

### Scheduled refresh and scraper canary (`.github/workflows/refresh-data.yml`)

A weekly job (Fridays 06:00 UTC, before the weekend's matches) re-runs `scripts/fetch_fixtures.py` and `log_forecast`, runs the test suite, and commits any data changes. Its real job is the canary: the fetch script's structural assertions fail loudly if TFF changes its markup, instead of the app quietly serving a stale calendar. The deploy step is skipped unless a `RAILWAY_TOKEN` secret exists, since this project normally deploys by CLI.

### Forecast accuracy (`src/accuracy.rs`, `examples/log_forecast.rs`)

`log_forecast` writes the model's current 1X2 probabilities for every *unplayed* fixture into `data/forecast_log.json` and **never overwrites an existing entry** — the first prediction recorded is the one the model is judged on, and the committed file plus git history is the audit trail. `accuracy::report` joins that log against played results and reports hit rate, log-loss, the base-rate baseline, and calibration bands; `GET /api/accuracy` serves it.

`log_forecast` refreshes an entry only while its fixture is still in the future and unplayed; once the match date arrives, that prediction is frozen forever. So the thing being scored is the model's most recent view *before* kick-off, not a preseason guess about matchday 34. The weekly workflow runs it; a fixture already played when the log is written is skipped, so the tracker reads 0 scored until a logged matchday completes — the honest state, not a bug.

### Market odds (`src/market.rs`)

Nesine's public pre-match bulletin (`cdnbulten.nesine.com`, league code 584 = Süper Lig, market type 1 = 1X2) is fetched on the same timer as the TFF scrape and cached in `AppState.market`. Nothing from it feeds the season simulation: `/api/matches` reports the model-minus-market probability gap, while `/api/coupon` filters independently computed model probabilities against fresh prices.

Every failure degrades to "no market data" rather than an error: a failed fetch keeps the previous snapshot, and an unmapped club name drops that fixture. Bookmaker club names carry shifting corporate suffixes, so `canonical_club` compares on an ASCII-folded, suffix-stripped form and refuses ambiguous matches — attaching real prices to the wrong fixture is worse than showing none.

`src/coupon.rs` refuses snapshots older than 90 minutes, ignores fixtures whose Turkey-time kick-off has passed, considers only the next active round, and requires all four thresholds: model ≥30%, model-minus-margin-free-market edge ≥2 points, `p × odds ≥1.02`, and odds ≤4.00. It emits no more than three selections. Do not weaken these safeguards to keep the panel populated: `NoValue` is an intended result. The six operator links are plain/non-affiliate and carry a checked-on date; re-verify their first-party legal statements before changing that date or list. The UI must retain the 18+, no-guarantee, independence caveat, and YEDAM 115 language.

### Season dispersion (`examples/arena.rs`)

The arena also asks whether replayed tables spread as wide as the real played-match tables. With no shared season shock the 2024-25/2025-26 standard deviations are 13.1/13.3 versus 16.3/15.2 observed; `data::RATING_SIGMA` raises them to roughly 14.4/14.7. The 2024-25 archive omits unplayed administrative awards, so the diagnostic compares played-fixture points on both sides. `fixture_probs` still uses the point estimate, leaving per-match calibration untouched.

Residual known limitation: even at the calibrated sigma, an 86-point played-match leader in the validation season is only about a 7% event. Do not read title odds as a precise prediction of the winning points total.

### Calibration (`tests/calibration.rs`)

ClubElo's scale is compressed relative to international Elo, so `BASE`/`D_DIV`/`HOME_ADV` are verified against the real league rather than inherited. The test compares simulated home-win and draw rates and mean points per club against 14 seasons of history and fails the build on drift. Current fit: 44.7% home wins (empirical 45.3%), 24.5% draws (empirical 25.8%).

### LLM scenario analysis (`src/llm.rs`)

Calls the Kimi/Moonshot chat completions API (`kimi-k2.6`, thinking disabled for latency) with a system prompt that enumerates all 18 valid club names and Elo-adjustment heuristics. Expects strict JSON back (`{"analysis": ..., "adjustments": {...}}`); `strip_fences` tolerates markdown code fences some models add. `validate_elo_overrides` re-checks names and the 1200-2000 bound server-side since the LLM output isn't trusted.

### Frontend (`frontend/src/`)

`api.ts` mirrors the backend's JSON response shapes exactly (`SimResponse`, `TeamRow`, `PositionRow`, `TableRow`, `RivalryPair`, `UpcomingMatch`, `DailyCouponResponse`, `LiveData`) — when changing Rust response structs, update `api.ts` in the same change. `App.tsx` owns simulation state and drives the child components (`ForecastView`, `ResultsTable`, `PositionGrid`, `LeagueTable`, `DailyCouponView`, `LiveStats`); there's no separate state management library.

## Data-source traps

These cost real debugging time; they are not obvious from the code:

- **tff.org serves windows-1254, not UTF-8.** Decoding as UTF-8 mangles every Turkish club name, which then silently fails every `TFF_NAMES` lookup and yields an empty scrape rather than an error. `src/scraper.rs` pins `encoding_rs::WINDOWS_1254`.
- **tff.org serves an incomplete TLS chain**, omitting the GlobalSign intermediate that signs its leaf. Browsers recover via the certificate's AIA URL; `reqwest`, `curl` and Python `urllib` do not, and fail with "unable to get local issuer certificate". This is TFF's misconfiguration, *not* a network block — do not diagnose it as one, and never reach for `-k`/`verify=False`/`danger_accept_invalid_certs`. The server embeds the intermediate (`data/tff_intermediate.pem`, added via `add_root_certificate`); the Python script AIA-fetches it pinned by SHA-256.
- **Historical chronology is model input.** The MIT-licensed Club Football Match Data archive supplies real dates through 2024-25; official TFF weekly pages supply 2025-26. `fetch_history.py` rejects partial downloads and unexpected season/week counts before writing.
- **The August season boundary is intentional.** It keeps the pandemic-delayed July 2020 matches in 2019-20.
- **Club display names drift between sources.** An unmapped current club falls into the "Other Club" bucket and quietly weakens it; aliases plus `every_ever_present_club_has_full_season_coverage` are the regression net.
- **TFF is authoritative for current-season fixtures and results.** The live Rust scraper validates the complete fixture set before accepting any played-result snapshot.
