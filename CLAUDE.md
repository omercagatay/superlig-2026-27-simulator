# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Monte Carlo simulator for the 2026-27 Trendyol Süper Lig (18 clubs, 34 matchdays, 306 fixtures). Rust/axum backend runs the simulation and serves a built React frontend as static files; an LLM (Kimi/Moonshot) turns natural-language "what if" scenarios into Elo adjustments that get re-simulated.

The local directory and GitHub repository are `superlig-2026-27-simulator`; the crate/binary are `superlig-sim`. The separate `worldcup-2026-simulator` repository and Railway project are different projects and must not be touched.

## Commands

### Backend (Rust, repo root)

```bash
cargo run --release             # serve on :3000 (reads .env; every variable is listed in .env.example)
cargo test                      # all tests (inline #[cfg(test)] plus tests/calibration.rs)
cargo test <test_name>          # single test, e.g. cargo test simulate_is_deterministic_for_same_seed
cargo fmt -- --check            # CI formatting check
cargo clippy --all-targets -- -D warnings   # CI lint (warnings are hard errors)
cargo build --release
cargo run --release --example fit_dc        # refit Dixon-Coles against data/superlig_results.csv
cargo run --release --example backtest      # walk-forward replay of the production model (see "Walk-forward backtest")
cargo run --release --example arena         # older frozen-season comparison of 10 candidate models
cargo run --release --example log_forecast  # freeze predictions for unplayed fixtures (run before each matchday)
```

### Data acquisition (Python 3, no dependencies)

```bash
python3 scripts/fetch_fixtures.py   # tff.org -> data/fixtures_2026_27.json (306 fixtures)
python3 scripts/fetch_history.py    # MIT archive + official TFF -> chronological history
python3 -m unittest discover -s scripts -p 'test_*.py'   # offline fetch_fixtures.py tests (CI `scripts` job)
```

Both are offline/manual: the committed data files are what the binary embeds via `include_str!`/`include_bytes!`. After `fetch_history.py`, re-run `fit_dc`.

`fetch_fixtures.py` treats the committed `data/fixtures_2026_27.json` as its reference. A postponed row with no date on TFF ("Ertelendi" or blank) keeps its committed date and kick-off, with a warning on stderr; it still fails if no committed date exists. The fallback cannot hide a broken week widget: every `&hafta=N` page must yield exactly 9 matched rows (`ROUND_SIZE`), dated or not; every fixture must be one of them; an undated row's date cell must be blank or a no-date marker (`NO_DATE_MARKERS`: a dash, "Ertelendi", "Belirlenmedi", ignoring case and accents); and at most `MAX_UNDATED` (18, two rounds' worth) fixtures may fall back in one run. Otherwise the run fails before writing. A committed result that vanishes or changes fails the run before the file is written, unless `ALLOW_RESULT_CHANGES=1` (a genuine TFF correction or award). It ends with a stderr summary of newly played, re-dated and re-timed fixtures. The comparison helpers are pure, and `scripts/test_fetch_fixtures.py` also replays `main()` against TFF-shaped pages built from the committed calendar.

### Frontend (`frontend/`)

```bash
npm install
npm run dev            # Vite dev server on :5173, proxies /api to http://localhost:3001 (note: not :3000 — start the backend with PORT=3001 for the dev proxy to work, or run `npm run build` and hit the backend directly on :3000)
npx tsc --noEmit        # CI type check
npm test                # node:test: src/clubs.test.mjs, src/calendar.test.mjs, src/retry.test.mjs, src/errors.test.mjs, src/history.test.mjs (Node >= 22.6, --experimental-strip-types)
npm run build           # tsc + vite build -> frontend/dist (served by the Rust binary in prod/docker)
```

CI (`.github/workflows/ci.yml`) runs stable backend checks (also `cargo clippy --examples --profile test -- -D warnings` and `cargo test --examples`, because `--all-targets` builds examples without their `#[cfg(test)]` modules, where `log_forecast`'s freeze-rule tests live), a Rust 1.87 MSRV check, RustSec/npm audits, the frontend type check, build and unit tests (Node 22.12), and a `scripts` job with the offline fetch-script tests.

### Docker

`Dockerfile` is a 3-stage build: Rust backend builder (with a dependency-caching dummy-`main.rs` layer) → Node frontend builder → slim Debian runtime that copies the backend binary and `frontend/dist`. Deploys to Railway by auto-detecting the Dockerfile; health check path is `/api/health`. `.github/workflows/deploy.yml` deploys every push to `main` once CI passes on it (`workflow_run`, checking out the commit CI tested): `railway up --ci` with the `RAILWAY_TOKEN` project-token secret and service `vars.RAILWAY_SERVICE` (default `superlig-sim`), then it polls `railway deployment list` until the newest deployment is `SUCCESS`, because the build finishing is not the new deployment taking traffic. Without the secret the job fails and says so. The refresh workflow deploys its own data commits, since a push made with `GITHUB_TOKEN` starts no CI run; both pin `@railway/cli` 5.70.0.

The dummy-layer `rm -rf` globs and the binary copy path both key on the **package name**. Renaming the package without updating the Dockerfile breaks the deploy and nothing else — `cargo build` still passes.

`.dockerignore` excludes `target*` (per-worktree target dirs included), `**/node_modules`, `frontend/dist`, `.git`, `.github`, `.claude`, `**/.env` and `**/.env.*`, Python caches, root `*.md` and `docs/`. Its patterns match from the context root, so a nested path needs `**/`. `node_modules/` alone never matched `frontend/node_modules`, and `COPY frontend/ ./` then laid the host's modules over the stage's `npm ci`.

## Architecture

### Request flow

`src/main.rs` builds one `AppState` (`Arc<AppState>`, via `AppState::new`) holding:
- `world: Arc<RwLock<World>>` — the live simulation state (clubs, Elo ratings, fixture calendar, already-played results)
- `world_version: Arc<AtomicU64>` — names one world state; bumped by `apply_live` only when a refresh changes `played` or `elo`. Written under the `world` write lock, read under its read lock.
- `kimi_api_key: Option<String>`
- `scenario_enabled: bool` — `SCENARIO_ENABLED=1`/`true`; off by default
- `live_data: Arc<RwLock<Option<LiveData>>>` — cached scrape results
- `simulation_queue: Arc<SimulationQueue>` — global CPU-heavy bound: `MAX_CONCURRENT_SIMULATIONS` slots (default 1); up to `MAX_SIMULATION_QUEUE` (8) distinct runs wait up to `SIM_QUEUE_TIMEOUT_SECS` (20) for one. A run beyond that, or one that times out, gets 503 with `Retry-After: 10` (a hint: on a 503 the site waits min(Retry-After, `MAX_WAIT_MS` 10 s) and retries, so `BUSY_RETRY_AFTER_SECS` must stay at or under `MAX_WAIT_MS` in `frontend/src/retry.ts`).
- `sim_cache: Arc<ResponseCache<SimKey, Bytes, ApiError>>` — serialized simulation responses (see "Response cache")

Routes (`src/handlers.rs`) sit in per-IP rate-limit groups (`src/rate_limit.rs`, sliding window). One `RateLimitLayer` is one bucket per client shared by every route in its group. The groups are wired in `router()` in `src/main.rs`; its test `the_forecast_has_its_own_allowance` pins which routes share a bucket. A 429 carries `Retry-After`: seconds until the oldest request in the window expires, at least 1. Refused requests are not counted. There is no CORS layer: the SPA is same-origin, and the Vite dev server proxies `/api`.
- `POST /api/simulate` (30/min) — accepts validated `elo_overrides` and up to 20 `what_if` outcomes, clones the current `World`, and applies changes **to the clone only**. Pins fix the *outcome*, not the scoreline: `simulate_one_with` redraws (bounded at 64 attempts, then a minimal fallback scoreline) until the result matches. Validated against the current world, then answered from `sim_cache`, by joining an identical run in flight, or by a fresh run. Only a fresh run takes a queue seat or slot, and a queued run gives its seat back without running once every client waiting for it has disconnected.
- `GET /api/forecast` (60/min, its own bucket: every page load asks for it and it is a cached byte copy) — the default forecast (`DEFAULT_N_SIMS` 50,000, `DEFAULT_SEED` 12345, nothing assumed): the same cache entry, so the same bytes, as `POST /api/simulate {}`. `spawn_forecast_warmup` computes it after every refresh attempt, failed ones included, or once at startup when `LIVE_REFRESH_MINUTES=0`. It is a no-op when already cached.
- `POST /api/scenario` (10/min) — 404 unless `scenario_enabled` (each call spends money on the LLM API; the UI doesn't call it). **Production must set `SCENARIO_ENABLED=1` to keep this endpoint.** Sends the prompt to Kimi (`src/llm.rs`), validates the returned Elo adjustments against club names/bounds (`src/validation.rs`), applies them to a cloned `World`, simulates through the queue (not cached), returns results plus the LLM's `analysis` text.
- `POST /api/refresh` (5/min) — validates all 306 TFF fixture rows before applying played results, then **does** mutate the shared `World` and caches the raw scrape. Partial or regressive snapshots keep the last good state.
- `GET /api/upcoming` (30/min, one bucket shared with `/api/matches` and `/api/accuracy`) — home/draw/away probabilities for the next matchday's unplayed fixtures (`World::next_round_on`, below).
- `GET /api/matches` (30/min) — the whole calendar with per-fixture 1X2, over/under 2.5 and BTTS prices. Probabilities are exact sums over the Dixon-Coles scoreline table (`World::fixture_probs`), not Monte Carlo — the table truncates at 10 goals a side, so the 1X2 triple is renormalized to sum to exactly 100. Played fixtures carry the real score and no forecast (retrodicting with current ratings would mislead). `current_round`, the UI's landing round, is `next_round_on` too.
- `GET /api/accuracy` (30/min) — see "Forecast accuracy".
- `GET /api/live`, `GET /api/health` — read-only, not rate-limited. Health reports the model and weights, `live_fetched_at`, `world_version` and `forecast_cached` (whether the default forecast is cached for the current version).

Everything not matching `/api/*` falls back to `ServeDir::new("frontend/dist")`, so in production this is a single binary serving both API and SPA.

### Response cache (`src/forecast_cache.rs`)

`ResponseCache` holds at most 32 (`SIM_CACHE_ENTRIES`) finished or in-flight results for the newest world version. A newer version drops every older entry. A caller still holding an older version gets a private flight: it gets its answer, but the answer is never published. `SimKey` is canonical: `n_sims`, seed, overrides sorted by club (rating bits) and pins sorted by fixture, so the same request in any order is one key.

Single flight: concurrent identical requests share one computation. It runs in its own spawned task, so a client that disconnects cannot cancel it for the others. A fresh run has two steps, `admit` (wait for a simulation slot) and `compute`. Each flight counts the callers waiting on it (an `Interest` drop guard). If that count reaches zero while the run is still in `admit`, the run gives up: its queue seat is freed, nothing is published, and the next caller for the key rejoins the flight and computes afresh. Once admitted, a run always finishes and publishes, even if every caller has left. Failures are not cached. Everyone waiting on a failed run gets its error, and the next new caller starts a fresh run. The default-forecast key is pinned and never evicted for space. Eviction drops failed and abandoned (never admitted, nobody waiting) entries first, then finished ones least-recently-used first, and runs in flight last. Under overload the newest entries hold 503s, and dropping a run in flight would let an identical request start a second one.

### Simulation core (`src/sim.rs`)

`World::simulate` runs `n_sims` independent trials in parallel via `rayon`, each seeded deterministically (`config.seed.wrapping_add(i * 2654435761)`) so results are reproducible for a given seed — this determinism is asserted in tests, don't break it.

Per trial (`simulate_one`):
1. All 306 fixtures are played in calendar order. A fixture already recorded in `World.played` short-circuits and uses the real result; the rest are sampled from the ensemble λ.
2. `league::apply_result` folds each result into both clubs' `TeamRecord`.
3. `league::rank_table` ranks the final table (see below) and returns the finishing order.

Across trials, `simulate` uses Rayon fold/reduce accumulators instead of retaining every season. It aggregates position, title/top-two/third/fourth/top-four/relegation counts, record sums, fixed-size points histograms, and a flat n×n `pairwise_above` matrix. A separate deterministic sample season remains as a test anchor. The projected-table view is built from per-club **expected** records. `models::projected_record` sets Pts = round(xPts), which is monotone down the table. It then fits integer W/D with W + D ≤ 34 and 3W + D = Pts, taking the valid draw count nearest round(xD). Rounding xPts and xD separately once asked a near-unbeaten leader for 35 results and wrapped the loss count. 101, the one total no 34-game record reaches, is shaded down to 100.

**Next matchday** (`World::next_round_on(today)`, with `today` from `sim::today_istanbul()`, UTC+3): the round of the earliest-dated unplayed fixture dated on or after today. With nothing unplayed dated today or later, the earliest round that still has an unplayed fixture. With everything played, the last round. When a rescheduled match shares that date with a matchday, every round with a fixture that day (played or not) that still has an unplayed fixture that day or before the next round starts competes (so its own game postponed for weeks does not hold the day): the round with more fixtures that day wins, and a tie goes to the later round, because a match moved onto a matchday is nearly always a postponed one from an earlier round (one brought forward from a later round would take the tie). So a matchday stays next after all of its games that day are in, while it still has games on later days, and a one-game Friday opener beats a rescheduled match on the same evening. A round with no game left to play drops out, so once a matchday is over, a rescheduled match still to come on its last day is next. A rescheduled match alone on its date is the next fixture, and its round is next. It used to be the lowest incomplete round, so one postponed match pinned `/api/upcoming` and the landing round for weeks. `upcoming_matches_on` / `build_matches_response_on` take the date for tests.

**Tests must not depend on the live season's state** in `data/fixtures_2026_27.json`; the scheduled refresh's test step went red for weeks because they did. Build the state with the `#[cfg(test)]` `World::with_played(n)`: the shipped calendar with its results replaced by its first `n` fixtures in date order, each given a fixed made-up score. Use `with_played(0)` for preseason and `with_played(sim::MID_SEASON)` (seven matchdays) for part-played. Date-sensitive tests set their own dates and pass `today`. `tests/calibration.rs` uses `preseason_world()`. The suite must pass with no results, every result, and a part-played calendar.

**Ratings are recomputed, never accumulated.** `World.elo_baseline` holds the preseason ratings from `sim::preseason_ratings()` (every result since 2012-13 replayed with `PRESEASON_K`; clubs with no top-flight result start at `NEW_CLUB_ELO`); `World::elo` is `refresh_ratings()` replaying every played result over that baseline in kick-off order. `update_from_live` calls it after inserting results. Doing it any other way (mutating `elo` in place) double-counts on the timer-driven refresh — there is a test for this.

**Home advantage is per fixture, not per club.** The World Cup version carried a `host: Vec<bool>` flag; `lam_pair(home, away)` now applies `HOME_ADV` to whichever club is at home in that fixture.

### League rules (`src/league.rs`)

Ranking order is **Points → head-to-head points → H2H GD → H2H GF → overall GD → overall GF → play-off**. Head-to-head comes *before* goal difference — this is not the FIFA group-stage order, and carrying the old one over is the single easiest way to get this wrong.

Head-to-head is applied **once** per block of clubs level on points. Clubs still level after that pass fall through to overall GD/GF, never to a fresh mini-table among the remaining pair. The published rule doesn't specify this; it's a stated assumption with a dedicated test (`head_to_head_is_applied_once_not_recursively`). Ranking lives in its own module, not in `sim.rs`, because it's the subtlest logic here and needs its own test surface.

### Data layer (`src/data.rs`)

The 18 clubs (`clubs()`; order defines indices), the `Fixture` struct, and `fixtures()` reading `data/fixtures_2026_27.json` via `include_str!`. Constants: `N_TEAMS`/`N_ROUNDS`/`N_FIXTURES`, `TOP_TWO_PLACES`/`TOP_FOUR_PLACES`/`RELEGATION_SPOTS`, the Elo model's `BASE`/`D_DIV`/`HOME_ADV`, in-season `ELO_K` and preseason `PRESEASON_K`/`NEW_CLUB_ELO`, and the underdog lift `UNDERDOG_PIVOT`/`UNDERDOG_LIFT`/`FAV_FOLLOW`. All model constants were chosen with `examples/backtest.rs`. Do not turn the upper-table cutoffs into UEFA labels: qualification also depends on the cup and access list.

Changing the club list means re-running both Python scripts, refitting DC, and updating `TFF_NAMES` in `src/scraper.rs`.

### Strength-model ensemble (`src/dixoncoles.rs`, `src/piratings.rs`, `src/history.rs`)

The simulation blends three strength models into each match's expected goals (λ), weighted by `ENSEMBLE_WEIGHTS` env var (`"elo,dc,pi"`, default `0.5,0.3,0.2`; `1,0,0` = pure Elo):
- **Elo-Poisson**: λ from Elo difference via `BASE`/`D_DIV`/`HOME_ADV`.
- **Dixon-Coles** attack/defense params, fit offline against `data/superlig_results.csv` with `cargo run --release --example fit_dc`, which writes `data/dc_params.json`; the server loads that file via `include_str!` at startup. The runtime uses DC λs and, when DC has weight, its joint scoreline distribution (ρ correction).
- **Pi-ratings** (Constantinou–Fenton), computed in one fast pass over the same history at startup (`src/piratings.rs`).

`World.ensemble: Option<Ensemble>` holds the blend; `None` (as in `World::new()` and most tests) means pure Elo. Club indices in DC/pi coincide with `World` indices because `history::TeamIndex::league()` is built from the same `data::clubs()` order, plus a trailing `"Other Club"` bucket.

After blending, `blend_lambdas` lifts any λ below `UNDERDOG_PIVOT` (0.9) by `UNDERDOG_LIFT` (0.7) of the gap, and raises the favourite's λ by `FAV_FOLLOW` (0.5) of that relative lift: heavy underdogs scored 0.88 goals where the blend said 0.66, and both-teams-to-score was badly underpredicted in mismatches.

**The bucket is a league-average departed club, not a promoted-club baseline.** It absorbs every match of every club outside the current 18 — relegated and defunct sides alike. Promoted clubs (Amedspor, Çorum for 2026-27) have α=β=0 in the DC fit, i.e. the neutral baseline; only their Elo distinguishes them.

Elo overrides from scenarios act through the Elo component only. `GET /api/health` reports the active model and weights.

### Walk-forward backtest (`examples/backtest.rs`)

The yardstick for every model change. It replays seasons 2016-17..2023-24 (development) match day by match day: each match is priced with the production rules (`sim::elo_delta`, `elo_lambdas`, `blend_lambdas`, `probs_from_lambdas`, `preseason_ratings_with`) using only earlier dates, and same-day results are revealed only after pricing. It prints `dev_*` metrics (1X2 log-loss, RPS, over/under 2.5, BTTS and exact-score log-loss). `EVAL_HOLDOUT=1` adds 2024-25 and 2025-26 (`hold_*`); `EVAL_SEASON_SIM=N` replays each season with the production simulator and compares the spread of final points; `EVAL_DUMP=file.csv` writes per-match rows. `EVAL_*` knobs (documented in the file header) override constants for screening; with none set it runs exactly what ships.

Results of the October 2026 tuning run (dev 1X2 log-loss 1.0068 → 0.9991; holdout 0.9805 → 0.9759, every aspect better on both, BTTS significantly): in-season `ELO_K` 20→60 with `D_DIV` 1600→1200, and the underdog lift. Most of the gap to closing odds opens after matchday 10, so in-season learning was the lever; the preseason prior is not where the error is. Tried and rejected (do not re-run without a new idea): promoted-club priors, regressing or widening preseason ratings, weekly in-season Dixon-Coles/pi refits (redundant once Elo is fast), early-season K schedules, phase-dependent blend weights, log/linear/no margin multipliers, goal-difference Elo (much worse), geometric blending, model-driven goal totals, pi-rating learning-rate changes, and a closeness-dependent draw boost (won on dev, failed the holdout). The holdout seasons informed two decisions in that run, so treat them as lightly used; the live 2026-27 season is the clean test.

### Model selection (`examples/arena.rs`)

The 0.5/0.3/0.2 ensemble weights are validated by backtest, not guessed: the arena fits every candidate on past seasons and scores them on held-out 2024-25 (validation) and 2025-26 (test). The production blend won the test season and is the only model in the top group on both; weights grid-fitted to the validation season overfit it. Re-run the arena before changing `ENSEMBLE_WEIGHTS` defaults. ML has been tried and lost: a walk-forward logistic stacker (in the arena, `logit-stack`) and sklearn GBM/RF/MLP (side experiment, `ARENA_DUMP_DIR=<dir>` dumps the feature CSV) all scored behind the ensemble on the 2025-26 test season — results-only features don't give ML room to win here. The arena freezes every rating for a whole season and its Elo is its own reimplementation; `examples/backtest.rs` supersedes it for anything production-facing.

### Scheduled refresh and scraper canary (`.github/workflows/refresh-data.yml`)

Runs Thursdays 15:17 UTC and Fridays 06:00 UTC (before the weekend's matches), plus `workflow_dispatch`. It re-runs `scripts/fetch_fixtures.py` and `log_forecast`, runs the test suite, and commits any data changes; a run with no new results or fixture changes commits and deploys nothing, because `log_forecast` leaves the log untouched when it has nothing new or refreshed. `log_forecast` never touches a fixture dated today (UTC) or earlier, so Thursday is the run that logs Friday's matches. GitHub starts scheduled runs hours late, hence the off-the-hour slot well before the UTC date turns. Its real job is the canary: the fetch script's structural assertions fail loudly if TFF changes its markup, including a week widget that stops giving dates (9 matched rows per week page; at most 18 postponed fixtures may keep their committed date), instead of the app quietly serving a stale calendar. So does its committed-result guard. The dispatch input `allow_result_changes` sets `ALLOW_RESULT_CHANGES=1` for a genuine correction.

The job has a `refresh-data` concurrency group (no cancel-in-progress), a 30-minute timeout and `Swatinem/rust-cache`. A failed, timed-out or cancelled run opens an issue titled "Weekly data refresh failed" naming the run and the step that failed or was interrupted. If one is already open, it comments there instead. The next green run closes it. The deploy step runs only when the commit step changed something and the job-level `HAS_RAILWAY_TOKEN` is true. A step's own `env` is not in scope for its `if`, which is why the old `env.RAILWAY_TOKEN` gate never fired. The token itself reaches only that step. Without the secret the step is skipped, since this project normally deploys by CLI.

### Forecast accuracy (`src/accuracy.rs`, `examples/log_forecast.rs`)

`log_forecast` writes the model's current 1X2 probabilities for every *unplayed* fixture into `data/forecast_log.json` and **never overwrites a frozen entry** (one whose fixture is played or whose date has arrived) — the last prediction recorded before kick-off is the one the model is judged on, and the committed file plus git history is the audit trail. `accuracy::report` joins that log against played results and reports hit rate, log-loss, the base-rate baseline, and calibration bands; `GET /api/accuracy` serves it. Each scored match also carries its frozen forecast (`home_pct`/`draw_pct`/`away_pct`, `logged_at`) and its own `log_loss` and `baseline_log_loss`; the headline figures are exactly their mean (`per_match_scores_are_the_aggregate_broken_down`). The Overview's `AccuracyPanel` shows them in a "Forecast history" section, closed by default (a `<details>`), with a running log-loss chart by matchday once two matchdays are scored.

`log_forecast` refreshes an entry only while its fixture is still in the future and unplayed; once the match date arrives, that prediction is frozen forever. So the thing being scored is the model's most recent view *before* kick-off, not a preseason guess about matchday 34. The scheduled refresh workflow runs it; a fixture already played when the log is written is skipped, so the tracker reads 0 scored until a logged matchday completes — the honest state, not a bug. An entry whose round, date and probabilities are unchanged is kept as is, `logged_at` included, and a run with nothing new or refreshed does not rewrite the file at all. Percentages are stored to 12 decimal places (`logged_pct`), and serde_json's `float_roundtrip` feature makes parsing exact, so a rewrite keeps every kept entry's digits (`the_shipped_log_rewrites_byte_for_byte`). A log that exists but does not parse stops the run instead of being treated as empty, so frozen entries can't be dropped by accident.

### No licensed betting sources

The app does not use Nesine or any other licensed betting operator, and must not. The Nesine bulletin fetch, the model-vs-bookmaker gap on `/api/matches`, and the daily coupon (`/api/coupon`) were removed on 2026-10-12 at the owner's instruction. The `*_odds` fields in the API are the model's own `1 / p` (`src/odds.rs`), not bookmaker prices; the UI does not show odds at all. To benchmark accuracy against the market, compare offline with football-data.co.uk closing odds (no Pinnacle for 2026-27; Betfair Exchange `BFEC*` and average `AvgC*` are the sharpest columns).

### Season dispersion (`examples/arena.rs`)

The arena also asks whether replayed tables spread as wide as the real played-match tables. With no shared season shock the 2024-25/2025-26 standard deviations are 13.1/13.3 versus 16.3/15.2 observed; `data::RATING_SIGMA` raises them to roughly 14.4/14.7. The 2024-25 archive omits unplayed administrative awards, so the diagnostic compares played-fixture points on both sides. `fixture_probs` still uses the point estimate, leaving per-match calibration untouched.

Residual known limitation: even at the calibrated sigma, an 86-point played-match leader in the validation season is only about a 7% event. Do not read title odds as a precise prediction of the winning points total.

### Calibration (`tests/calibration.rs`)

`BASE`/`D_DIV`/`HOME_ADV` are verified against the real league rather than inherited. The test compares simulated home-win and draw rates and mean points per club against 14 seasons of history and fails the build on drift. It runs on `preseason_world()` (preseason ratings, nothing played) so the check is of the model, not of how far the season has got. Current fit: 44.7% home wins (empirical 45.3%), 24.5% draws (empirical 25.8%).

### LLM scenario analysis (`src/llm.rs`)

Calls the Kimi/Moonshot chat completions API (`kimi-k2.6`, thinking disabled for latency) with a system prompt that enumerates all 18 valid club names and Elo-adjustment heuristics. Expects strict JSON back (`{"analysis": ..., "adjustments": {...}}`); `strip_fences` tolerates markdown code fences some models add. The LLM returns rating **changes**, not absolute ratings (self-computed ratings are not on ClubElo's scale); `validation::scenario_overrides` re-checks names, caps each change at ±150 and applies it to the current rating, then `validate_elo_overrides` enforces the 1200-2000 bound, since the LLM output isn't trusted.

The endpoint is off unless `SCENARIO_ENABLED` is set (see Request flow). The request sets `max_tokens` 800, and the analysis is trimmed to `MAX_ANALYSIS_CHARS` (600) on a character boundary. Upstream error bodies and raw model output (truncated to 2,000 characters) and rejected adjustments are only logged with `tracing::warn!`, never returned. Clients get `LlmError`'s generic messages as 502s: "unavailable" for an unreachable or refusing API, "unusable answer" for unparseable JSON or adjustments that fail validation.

### Frontend (`frontend/src/`)

`api.ts` mirrors the backend's JSON response shapes exactly (`SimResponse`, `TeamRow`, `PositionRow`, `TableRow`, `RivalryPair`, `UpcomingMatch`, `LiveData`/`PlayedMatch`) — when changing Rust response structs, update `api.ts` in the same change. Every call goes through its `request()` and `retry.ts`'s `sendWithRetry`. A 503 or 429 is retried up to 3 times; with no `Retry-After` it backs off 1/2/4 s. A 503's `Retry-After` is a hint, waited for at most `MAX_WAIT_MS` (10 s, equal to `BUSY_RETRY_AFTER_SECS` in `src/handlers.rs`, which must not exceed it; change them together). A 429's is exact: waited out when it is within 10 s, otherwise the call fails at once with `ApiError.retryAfter` set, since asking sooner would only be refused. Any other failure throws `ApiError` (status plus the backend's plain text; an HTML page from a proxy becomes `HTTP <status>`). `errors.ts`'s `describeError` turns a failure into a localized line: 503 busy, 429 with the seconds to wait, 400/422 settings not accepted, 502 on Update live data TFF unreachable, a fetch `TypeError` connection failed, anything else request failed. The backend's English text appears only as a muted parenthetical detail. `App.tsx` owns simulation, calendar, live-data, what-if and theme state and is otherwise a shell: `Header` (site name and sticky page tabs), one page component from `pages/` chosen by hash route (`#overview`, `#fixtures`, `#table`, `#probabilities`, `#results` → `OverviewPage`, `FixturesPage`, `TablePage`, `ProbabilitiesPage`, `ResultsPage`), `Footer` (sims/seed, run, live-data refresh, language and dark-mode toggles, disclaimer), and the `ClubDetail` dialog. There's no separate state management library.

`App.tsx` boots from `GET /api/forecast` and falls back to `POST /api/simulate {}` only on a 404 from an older backend. Run and Recalculate send `POST /api/simulate` with the current settings and picks. Forecast requests carry an id and an `AbortController`, so a late answer to an older request is dropped. Only Overview, Table and Probabilities wait on the simulation. Fixtures and Results read the `/api/matches` calendar (Results via `calendar.ts`'s `playedResults`, with the `/api/live` scrape only as a fallback), so they stay up when the simulation fails. Whenever a calendar arrives, what-if picks on fixtures it shows as played are dropped (`prunePlayedPicks`) with a one-off notice. A 400 on a run with picks (Run, Recalculate, or the re-simulation after Update live data; all go through `simulatePicks`) reloads the calendar once and asks again without the stale ones, because the server's timed refresh can record a picked match behind the page's back. Fixtures stops new picks at `calendar.ts`'s `MAX_WHAT_IF` (20), which must match `MAX_WHAT_IF` in `src/validation.rs`; existing picks can still be changed or cleared. `localStorage` reads and writes (theme, language) are wrapped in try/catch, and `components/ErrorBoundary.tsx` shows a localized reload prompt instead of a blank page.

The design is deliberately plain: system fonts, white (or neutral dark grey) background, one accent colour for interactive states and bars, red only for relegation, square tables, no decorative concept. It replaced a "matchday programme" design on 2026-10-13 because that one read as AI-generated; do not reintroduce themed metaphors, serif display type, paper textures or explainer copy under every heading. Colour tokens live on `:root` and `:root[data-theme="dark"]` in `styles.css` (nothing else hard-codes a colour except club colours, the form badges, and the theme-color meta (`index.html`, `App.tsx`'s `THEME_COLOR`) and inline favicon in `index.html`, which mirror `--bg` and `--accent`). All number, percent and date formatting goes through `clubs.ts` (Turkish `%59,9` vs English `59.9%`, `"–"` for zero, `"<%0,1"` for traces, `kickoffOrder` for fixture order) alongside the club colour map; it is pure so `clubs.test.mjs` can test it, as `calendar.ts`, `retry.ts`, `errors.ts` and `history.ts` are for `calendar.test.mjs`, `retry.test.mjs`, `errors.test.mjs` and `history.test.mjs`. `history.ts` backs the forecast history: matchday grouping (ordered with `kickoffOrder`, then home club in Turkish order), the running series, axis ticks, `readoutLeft` (keeps the chart readout inside the plot) and `fill()` for `{placeholder}` translations. In `styles.css`, the chart's dark-theme base-rate line (`--hist-base`) is `color-mix(--muted 65%, --bg)`, because plain `--muted` fails the normal-vision separation check against the dark accent (OKLab ΔE 12.9; 21.7 with the mix), and the history's percentage columns are 54px, not the overview's 44px, so the bold boxed figure fits. Shared pieces (`Swatch`, `ClubName`, `Bar`, `ResultLine`) live in `components/Common.tsx`; club names render through `ClubName` (`lang="tr"`). No odds, bookmaker or betting-market figures appear in the UI (say "3+ goals" / "both teams score", never market names).

## Data-source traps

These cost real debugging time; they are not obvious from the code:

- **tff.org serves windows-1254, not UTF-8.** Decoding as UTF-8 mangles every Turkish club name, which then silently fails every `TFF_NAMES` lookup and yields an empty scrape rather than an error. `src/scraper.rs` pins `encoding_rs::WINDOWS_1254`.
- **tff.org serves an incomplete TLS chain**, omitting the GlobalSign intermediate that signs its leaf. Browsers recover via the certificate's AIA URL; `reqwest`, `curl` and Python `urllib` do not, and fail with "unable to get local issuer certificate". This is TFF's misconfiguration, *not* a network block — do not diagnose it as one, and never reach for `-k`/`verify=False`/`danger_accept_invalid_certs`. The server embeds the intermediate (`data/tff_intermediate.pem`, added via `add_root_certificate`); the Python script AIA-fetches it pinned by SHA-256.
- **Historical chronology is model input.** The MIT-licensed Club Football Match Data archive supplies real dates through 2024-25; official TFF weekly pages supply 2025-26. `fetch_history.py` rejects partial downloads and unexpected season/week counts before writing.
- **The August season boundary is intentional.** It keeps the pandemic-delayed July 2020 matches in 2019-20.
- **Walkover awards are not results.** After the 6 February 2023 earthquake Hatayspor and Gaziantep withdrew and their 29 remaining 2022-23 fixtures were awarded 3-0 unplayed. `history::is_unplayed_award` drops them; they had been training Dixon-Coles, pi-ratings and Elo.
- **Club display names drift between sources.** An unmapped current club falls into the "Other Club" bucket and quietly weakens it; aliases plus `every_ever_present_club_has_full_season_coverage` are the regression net.
- **TFF is authoritative for current-season fixtures and results.** The live Rust scraper validates the complete fixture set before accepting any played-result snapshot.
