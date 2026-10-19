# Süper Lig 2026-27 Simulator

[English](README.md) | [Türkçe](README.tr.md)

[![CI](https://github.com/omercagatay/superlig-2026-27-simulator/actions/workflows/ci.yml/badge.svg)](https://github.com/omercagatay/superlig-2026-27-simulator/actions/workflows/ci.yml)

A full-stack Monte Carlo forecast for the 2026-27 Trendyol Süper Lig. It combines a Rust simulation engine, a React dashboard, live results from the Türkiye Futbol Federasyonu, and an optional Kimi-powered scenario analyzer.

> The probabilities and fair odds produced by this project are model estimates, not betting or financial advice.

The [model card](docs/model-card.md) ([in Turkish](docs/model-card.tr.md)) sets out what the model is, what it is and is not for, how it was evaluated, its results with intervals, and where it falls short.

## Highlights

- Simulates the full 306-fixture, 34-matchday double round-robin between all 18 clubs, 100–200,000 seasons in parallel with Rayon; the dashboard defaults to 50,000.
- Blends Elo, Dixon–Coles, and pi-ratings — the latter two fitted on 14 seasons of real Süper Lig results — into expected-goal estimates.
- Uses Dixon–Coles joint score sampling, which represents low-scoring and drawn matches better than independent Poissons.
- Locks confirmed results from the official TFF calendar and simulates only the remaining fixtures.
- Applies the Süper Lig's own classification rules, in which **head-to-head decides before goal difference**.
- Produces title, top-two, exact third/fourth, top-four, and relegation odds; an 18×18 finishing-position grid; a projected final table beside the current one; the fewest final points that gave a 50, 75 or 90% chance of the title, the top two, the top four or staying up in the simulated seasons; next-matchday forecasts; and pairwise "who finishes above whom" probabilities.
- Optionally converts natural-language scenarios into validated Elo overrides with Kimi and reruns the season (an API endpoint, off unless `SCENARIO_ENABLED=1`).
- Computes the default forecast once per state of the league and serves it from a cache; a bounded queue absorbs bursts of fresh simulations.
- Includes per-IP rate limits, request validation, compressed responses under a strict Content-Security-Policy, graceful shutdown, deterministic seeds, light/dark themes, a non-root Docker image, and GitHub Actions CI.

## Stack

| Layer | Technology |
|---|---|
| Backend/API | Rust 1.87+, Axum, Tokio |
| Simulation | Rayon, Rand, Dixon–Coles, pi-ratings, Elo/Poisson |
| Frontend | React 18, TypeScript, Vite |
| Live data | Türkiye Futbol Federasyonu (tff.org) fixture page |
| Historical data | MIT-licensed Club Football Match Data (2012-13 to 2024-25), openfootball (CC0) for the archive's missing 2024-25 matchdays 10-12, and official TFF results (2025-26) |
| Scenario analysis | Kimi via the Moonshot API |
| Deployment | Multi-stage Docker image; Railway-compatible |

## How the model works

The pure-Elo component converts rating difference and home advantage into expected goals:

```text
lambda_home = 1.35 × 10^(( Elo_home - Elo_away + 80) / 1200)
lambda_away = 1.35 × 10^((-Elo_home + Elo_away - 80) / 1200)
```

Home advantage is a property of the **fixture**, not of the club: every club gets the boost at its own ground, in both halves of the double round-robin.

By default those rates are blended with two models trained on real Süper Lig history:

- **Elo (0.5):** current club strength, computed from every top-flight result since 2012-13 and updated after each match this season, plus an 80-point home adjustment.
- **Dixon–Coles (0.3):** time-decayed attack/defence strengths and low-score correlation, fitted over 4,588 chronologically dated matches with a four-year half-life.
- **Pi-ratings (0.2):** sequential home/away strength updates from the same history.

Set `ENSEMBLE_WEIGHTS` to change the blend; `1,0,0` selects the pure-Elo model. When the Dixon–Coles weight is active, scorelines are drawn from its joint distribution.

After blending, the lower of a match's two expected-goals rates, when it is below 0.9, is lifted 70% of the way to 0.9, and the favourite's rate rises by half of that relative lift (when both are below 0.9, only the lower is lifted): heavy underdogs score more often than the rating gap implies. In the backtest this improves the both-teams-score forecast and nothing else (see [Backtest results](#backtest-results)).

Clubs promoted from the 1. Lig (Amedspor and Çorum for 2026-27) have no top-flight record, so the Dixon–Coles and pi-rating components give them league-average profiles; their Elo rating still distinguishes them.

**Ratings are uncertain, and the simulation says so.** Each simulated season draws every club's true strength once from a normal around its current Elo rating (`RATING_SIGMA`, 75 Elo points), held fixed for that season. Treating ratings as exact makes favourites' odds run hot and the tails too thin.

The sigma is calibrated, not guessed. The arena replays each held-out season from the model's own match probabilities and compares the spread of the resulting points table with the real one:

| | 2024-25 | 2025-26 |
|---|---:|---:|
| Played-match leader points | 92 | 77 |
| Actual played-match points spread (sd) | 17.3 | 15.2 |
| Model spread, σ=0 | 13.4 | 13.0 |
| Model spread, σ=75 | 15.0 | 14.4 |

Independent match sampling understates season-long strength persistence; a shared per-season strength draw closes about two-fifths of that gap on 2024-25 and two-thirds on 2025-26. It costs nothing per analytic match forecast because only season simulation draws it. The 2024-25 history leaves out the season's one administrative award (Galatasaray v Adana Demirspor, awarded 3-0), so this diagnostic deliberately compares points from played fixtures on both sides. The arena applies the draw to each match's win/loss odds, a different mechanism from the simulator's, which adds it to the Elo ratings; the [season-level evaluation](#season-level-evaluation) checks what the simulator itself produces.

**A known residual:** the model remains under-dispersed on 2024-25; a 92-point played-match leader occurs in roughly 14% of replays from the season's start (`EVAL_SEASON_SIM` in `examples/backtest.rs`). Read title odds as "who is most likely to win", not as a precise forecast of the winning points total.

**Ratings move with the season.** The Elo component starts from a preseason rating computed from the results history and is walked forward through every result played so far, in kick-off order: a `ELO_K`-scaled update with a square-root goal-difference multiplier, transferring rating from loser to winner. It is always recomputed from the baseline rather than compounded, so a repeated live refresh is a no-op. The Dixon–Coles and pi-rating components stay at their fitted values until refitted offline.

**Overrides move the whole model.** A manual override or a Kimi scenario asks for a club rating; its difference from the club's current Elo rating shifts the blended expected goals of every one of its matches, as the same Elo gap shifts the Elo component, so the Dixon–Coles and pi-rating parts follow it too. Overrides used to change the Elo component alone and so worked at about half strength. At preseason, +50 points now moves a club's expected season points by about 5.5 on average (20,000 simulated seasons; about 2.8 before). The shift applies after the underdog lift, so two clubs overridden to the same rating need not end up with the same projection. The embedded Dixon–Coles and pi-rating parameters themselves stay unchanged until the historical models are explicitly refitted.

### Classification rules

The table is ranked by:

1. Points
2. Head-to-head points
3. Head-to-head goal difference
4. Head-to-head goals scored
5. Overall goal difference
6. Overall goals scored
7. Play-off

Head-to-head precedes goal difference — this is not the FIFA/UEFA group-stage order. Head-to-head is applied **once** per block of clubs level on points; clubs still level after that pass fall through to overall goal difference rather than to a fresh mini-table among the remainder. The published rule does not specify this case, so it is a stated modelling assumption, covered by a dedicated test. A simulated season settles a tie that survives every rule with a random draw in place of the play-off.

TFF's regulation, as the press reports its text (tff.org could not be reached from the development environment), sets the order at the end of the season. Press sources differ on the table during the season: some say head-to-head counts once the clubs' meetings are complete, others that the weekly table uses overall goal difference until the season ends. The site follows the first reading, which agrees with the season-end rule: the current table applies head-to-head to clubs level on points only once every pair of them has played both its meetings; until then they are separated by overall goal difference, then goals scored. That is not a published rule, so it too is an assumption. The current table has no play-off to draw on either: clubs that no rule separates are listed by name, in Turkish alphabetical order.

The dashboard reports exact finishing bands—top two, third, fourth, and top four—rather than assigning UEFA competition names. Actual qualification depends on the Turkish Cup winner and that season's UEFA access list, so league position alone is not enough. The bottom three are relegated.

### Calibration

The model that ships (the default blend, at preseason, nothing played) is checked against every played Süper Lig match since 2012-13:

| | History (4,588 matches) | Model, per fixture | Model, simulated seasons |
|---|---:|---:|---:|
| Home wins | 45.4% | 44.9% | — |
| Draws | 25.9% | 24.8% | 24.4% |
| Away wins | 28.8% | 30.4% | — |
| Goals per match | 2.79 | 2.88 | 2.91 |
| Home goals per match | 1.57 | 1.63 | — |
| Away goals per match | 1.22 | 1.25 | — |

Home wins agree within the history's own sampling error. Draws are under-predicted by about one point (1.4 in simulated seasons, which also draw each season's strengths), and the missing draws turn up as away wins; goals are over-predicted by about 0.1 a match. These are known gaps, not fixed yet. `tests/calibration.rs` pins today's figures and fails the build if one moves (a rate by more than a point, goals by more than 0.05 a match) or if a gap grows past its stated band.

### Backtest results

`examples/backtest.rs` replays every season since 2016-17 match day by match day, pricing each match only with what was known before its date, under the production rules. The development seasons were used to build and tune the model. The two holdout seasons were kept out of that, except that they informed two decisions in the October 2026 tuning, so they are lightly used rather than untouched. Shipped model:

| | Development, 2016-17 to 2023-24 | Holdout, 2024-25 and 2025-26 |
|---|---:|---:|
| Matches | <!-- metric:dev_n:,d -->2,717<!-- /metric --> | <!-- metric:hold_n:,d -->647<!-- /metric --> |
| Result (home/draw/away) log-loss | <!-- metric:dev_1x2_ll:.4f -->0.9991<!-- /metric --> | <!-- metric:hold_1x2_ll:.4f -->0.9756<!-- /metric --> |
| Base-rate forecast's log-loss | <!-- metric:dev_1x2_ll_base:.4f -->1.0678<!-- /metric --> | <!-- metric:hold_1x2_ll_base:.4f -->1.0669<!-- /metric --> |
| Skill over base rates: result log-loss | <!-- metric:dev_1x2_skill:.1% -->6.4%<!-- /metric --> | <!-- metric:hold_1x2_skill:.1% -->8.6%<!-- /metric --> |
| Skill: ranked probability score | <!-- metric:dev_rpss:.1% -->10.2%<!-- /metric --> | <!-- metric:hold_rpss:.1% -->13.8%<!-- /metric --> |
| Skill: 3+ goals | <!-- metric:dev_ou25_skill:.1% -->1.4%<!-- /metric --> | <!-- metric:hold_ou25_skill:.1% -->2.4%<!-- /metric --> |
| Skill: both teams score | <!-- metric:dev_btts_skill:.2% -->0.06%<!-- /metric --> | <!-- metric:hold_btts_skill:.2% -->-0.12%<!-- /metric --> |
| Draws: forecast / happened | <!-- metric:dev_cal_draw_pred:.1% -->24.6%<!-- /metric --> / <!-- metric:dev_cal_draw_obs:.1% -->25.4%<!-- /metric --> | <!-- metric:hold_cal_draw_pred:.1% -->23.6%<!-- /metric --> / <!-- metric:hold_cal_draw_obs:.1% -->26.1%<!-- /metric --> |
| Goals per match: forecast / scored | <!-- metric:dev_goals_pred:.2f -->2.86<!-- /metric --> / <!-- metric:dev_goals_obs:.2f -->2.82<!-- /metric --> | <!-- metric:hold_goals_pred:.2f -->2.98<!-- /metric --> / <!-- metric:hold_goals_obs:.2f -->2.80<!-- /metric --> |
| Both teams score: forecast / happened | <!-- metric:dev_btts_pred:.1% -->54.8%<!-- /metric --> / <!-- metric:dev_btts_obs:.1% -->56.1%<!-- /metric --> | <!-- metric:hold_btts_pred:.1% -->55.2%<!-- /metric --> / <!-- metric:hold_btts_obs:.1% -->55.6%<!-- /metric --> |

Lower log-loss is better. Skill is the share of the base-rate forecast's loss that the model removes; the base rates come only from seasons before the one scored. The model clearly beats them on results and on goal totals, but its both-teams-score forecast is no better than the base rate. Draws come out slightly too rare in both periods, as in the calibration check above. Goals come out too high, by about 0.05 a match in the development seasons and 0.18 in the holdout seasons (the goals row above), and both-teams-score slightly too low, so the model splits goals too unevenly between the two sides.

The October 2026 tuning (faster in-season rating updates and the underdog lift), compared match by match with the model before it (`examples/compare.rs`; 95% intervals from resampling whole match dates):

| Change in loss, tuned minus before | Development | Holdout |
|---|---:|---:|
| Result log-loss | −0.0077 [−0.0122, −0.0031] | −0.0066 [−0.0170, +0.0043] |
| Ranked probability score | −0.0023 [−0.0037, −0.0009] | −0.0026 [−0.0056, +0.0005] |
| Both teams score log-loss | −0.0016 [−0.0026, −0.0006] | −0.0044 [−0.0081, −0.0008] |

On the development seasons the gain is statistically significant for results, RPS, both-teams-score and exact scores. On the holdout seasons only both-teams-score is; the result gain there is the same size but within the noise of <!-- metric:hold_n:,d -->647<!-- /metric --> matches. The faster rating updates are what improve results; the underdog lift improves only both-teams-score. The development seasons were used to choose the tuning, so their intervals are optimistic. The live 2026-27 season is the clean test.

A later sweep of 202 settings on the development seasons (the overall goal level, the low-score correction, and every blend weighting in steps of 0.1) found nothing worth shipping. The best setting changed result log-loss by −0.0006 [−0.0015, +0.0003], within the noise, and when the setting was chosen without seeing the season it was scored on, the gain disappeared (+0.0002). Scaling goals down closes the gap in goal totals but costs both-teams-score, and on the development seasons its gain did not hold up when it was chosen without seeing the season scored. A look at the holdout seasons after that decision found it improves exact scores there, where goals are over-predicted more, so the goal level stays an open question for the live season. A stronger low-score correction would bring draws to 25.3% without moving goal totals, but its gain is as small (−0.0003 [−0.0010, +0.0003]). No setting gives both-teams-score any skill.

### Against the market

`examples/market.rs` scores the same walk-forward forecasts against bookmaker prices: Bet365's pre-match odds from the MIT-licensed match archive, with the bookmaker's margin removed by Shin's method (chosen before the run, in the working notes; it was committed together with the results). This is offline evaluation only. The odds are not committed, and no bookmaker odds or market figure reaches the API or the dashboard; the API's `*_odds` fields are the model's own 1 / p, not prices.

| Result log-loss, matches with odds | Development | Holdout |
|---|---:|---:|
| Matches | <!-- metric:market:dev_n:,d -->2,714<!-- /metric --> | <!-- metric:market:hold_n:,d -->620<!-- /metric --> |
| Model | <!-- metric:market:dev_model_1x2_ll:.4f -->0.9993<!-- /metric --> | <!-- metric:market:hold_model_1x2_ll:.4f -->0.9754<!-- /metric --> |
| Market | <!-- metric:market:dev_market_1x2_ll:.4f -->0.9888<!-- /metric --> | <!-- metric:market:hold_market_1x2_ll:.4f -->0.9425<!-- /metric --> |
| Model minus market | <!-- metric:market:dev_diff_1x2_ll:+.4f -->+0.0106<!-- /metric --> [+0.005, +0.017] | <!-- metric:market:hold_diff_1x2_ll:+.4f -->+0.0328<!-- /metric --> [+0.017, +0.050] |

The market is clearly better in both periods (95% intervals from resampling whole match dates), on ranked probability score too, and less clearly on 3+ goals. Removing the margin proportionally, or taking the best price across bookmakers, gives the same picture. Mixing the model into the market does not help: the best weight for the model on the development seasons is <!-- metric:market:dev_pool_w:.2f -->0.07<!-- /metric --> (interval 0 to 0.21), and at that weight the mix does worse than the market alone on the holdout seasons. So no information the market lacks shows up out of sample; with this little data, that does not prove the model carries none.

Whether the gap opens late in the season is not settled either. On the development seasons it is small over the first ten matchdays and larger after that; on the holdout seasons it is large from the start. But the difference between the two periods (after matchday 10 minus matchdays 1-10) is <!-- metric:market:dev_md_late_minus_early_1x2_ll:+.4f -->+0.0106<!-- /metric --> [−0.002, +0.024] on the development seasons and <!-- metric:market:hold_md_late_minus_early_1x2_ll:+.4f -->-0.0100<!-- /metric --> [−0.043, +0.023] on the holdout seasons, within the noise in both.

The archive's odds are pre-match prices, not closing ones, and closing prices are sharper still, so the gap to the closing line is probably wider; comparing with closing prices is a follow-up. Of the live season, only <!-- metric:market:live_n -->19<!-- /metric --> matches have both a frozen forecast and odds so far: far too few to say anything. All were forecast by the August model, each <!-- metric:market:live_age_days_min:d -->0<!-- /metric --> to <!-- metric:market:live_age_days_max:d -->11<!-- /metric --> days before the match, which may put the model at a disadvantage against a pre-match price.

### Season-level evaluation

`examples/season_eval.rs` scores the season forecasts themselves (finishing positions, title, top four, relegation and final points) by replaying each past season from three points: before it starts, a third of the way in and two thirds in, each time with only the information available then. Over the <!-- metric:season:dev_n_seasons -->8<!-- /metric --> development seasons:

| | Preseason | A third played | Two thirds played |
|---|---:|---:|---:|
| Finishing-position RPS (lower is better) | <!-- metric:season:dev_s0_pos_rps:.4f -->0.1322<!-- /metric --> | <!-- metric:season:dev_s33_pos_rps:.4f -->0.1121<!-- /metric --> | <!-- metric:season:dev_s67_pos_rps:.4f -->0.0646<!-- /metric --> |
| Title, Brier skill over base rate | <!-- metric:season:dev_s0_title_bss:.3f -->0.052<!-- /metric --> | <!-- metric:season:dev_s33_title_bss:.3f -->0.307<!-- /metric --> | <!-- metric:season:dev_s67_title_bss:.3f -->0.500<!-- /metric --> |
| Top four, Brier skill | <!-- metric:season:dev_s0_top4_bss:.3f -->0.430<!-- /metric --> | <!-- metric:season:dev_s33_top4_bss:.3f -->0.584<!-- /metric --> | <!-- metric:season:dev_s67_top4_bss:.3f -->0.813<!-- /metric --> |
| Relegation, Brier skill | <!-- metric:season:dev_s0_rel_bss:.3f -->0.145<!-- /metric --> | <!-- metric:season:dev_s33_rel_bss:.3f -->0.319<!-- /metric --> | <!-- metric:season:dev_s67_rel_bss:.3f -->0.590<!-- /metric --> |
| Final points inside the 80% range | <!-- metric:season:dev_s0_cov80_pct:.1f -->70.4<!-- /metric -->% | <!-- metric:season:dev_s33_cov80_pct:.1f -->67.8<!-- /metric -->% | <!-- metric:season:dev_s67_cov80_pct:.1f -->73.0<!-- /metric -->% |
| Final points, CRPS in points (lower is better) | <!-- metric:season:dev_s0_pts_crps:.2f -->6.18<!-- /metric --> | <!-- metric:season:dev_s33_pts_crps:.2f -->5.04<!-- /metric --> | <!-- metric:season:dev_s67_pts_crps:.2f -->2.93<!-- /metric --> |

The forecasts improve as the season goes on, but they are over-confident: final-points ranges that should contain 80% of clubs contained about 70%, and simulated tables are less spread out than real ones (<!-- metric:season:dev_s0_table_sd_sim:.1f -->13.6<!-- /metric -->–<!-- metric:season:dev_s67_table_sd_sim:.1f -->14.4<!-- /metric --> points standard deviation against <!-- metric:season:dev_s0_table_sd_actual:.1f -->15.1<!-- /metric -->). The preseason title forecast is not distinguishable from simply using base rates (95% interval for its skill: −0.099 to 0.209, eight seasons). Treat the dashboard's ranges as somewhat too narrow. The two holdout seasons point both ways and settle nothing; their spread is also what `RATING_SIGMA` was tuned on (see above), so they are no independent test of it.

The CRPS scores the whole forecast distribution of each club's final points, in points (for a forecast of a single number it would be the plain miss), and rewards wider ranges only where the misses need them. Widening the ranges was tried and not shipped. Applying the per-season strength draw to the whole model, not only to the Elo ratings, brought the development seasons to nominal coverage (80% ranges covering 79.6–82.9% of clubs, in runs of 5,000 seasons per snapshot) and improved finishing-position RPS in 7 of the 8 seasons. Its points CRPS gain was weaker: better in only 5 of the 8 seasons, with an interval that includes no change, and it was no better two thirds of the way in. Those intervals also ignore that it was the pick of more than 47 settings tried. A rule written down before the holdout seasons were run (in the working notes; it was committed only after the result) said to adopt it unless their points CRPS or position RPS, each pooled over the three snapshots, got worse, and both did. The position-RPS difference was tiny and changed with the random seed; the CRPS difference decided it, worse at each seed tried and in both seasons. From a third of the way in, where the shipped forecasts had not been too narrow, it made them too wide. So the simulator is unchanged. Two seasons settle little, and the live 2026-27 season is the clean test.

### Model selection

The blend was chosen by backtest, not by fiat. `cargo run --release --example arena` pits candidate models against two held-out seasons (fit through 2023-24, tune on 2024-25; refit through 2024-25, judge on 2025-26), scored on log-loss, RPS, Brier, and accuracy. Test-season leaderboard (2025-26, 306 matches, log-loss per match):

| model | log-loss | RPS |
|---|---:|---:|
| Dixon–Coles, 1.5y half-life | 0.9994 | 0.1954 |
| logistic stack | 1.0003 | 0.1968 |
| **ensemble 0.5/0.3/0.2** | **1.0011** | **0.1965** |
| ensemble, validation-fitted weights | 1.0051 | 0.1970 |
| pi-ratings | 1.0061 | 0.1978 |
| Dixon–Coles, 4y half-life | 1.0065 | 0.1979 |
| Elo–Poisson | 1.0122 | 0.1988 |
| home-advantage baseline | 1.0866 | 0.2250 |

The arena prints twelve rows per season, nine fitted models and three baselines; the table shows a selection. Every fitted model clears the baselines by a wide margin. The production blend is 0.0017 log-loss behind the test winner, and 0.0150 ahead of it on validation (0.9828 against 0.9978). Weights fitted only to validation (0.4/0/0.6) win that season but fall behind on test, so the broader 0.5/0.3/0.2 blend stays.

The arena also builds a leak-free walk-forward multinomial logistic stack whose features are fitted only on earlier seasons. It scores 1.0003 on the test season, 0.0008 ahead of the ensemble, but 0.9978 on validation, 0.0150 behind it. An edge that small on one season does not justify replacing the simpler model with results-only features; richer inputs such as xG and lineups are not available here.

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
2. Explore the five pages: **Overview**, **Fixtures**, **Table**, **Probabilities**, and **Results**. Each page but Results opens with a line saying how far the season has got ("63 of 306 matches played · latest result 12 October"). **Table** switches between the current table and the projected final table; it opens on the current one once a result is in. **Probabilities** ends with the points needed: for the title, the top two, the top four and the top 15 (staying up), the fewest final points that gave a 50, 75 or 90% chance of that place or better in the simulated seasons. A run of a few hundred seasons is too small for some of these figures, which then show "–"; at up to about 200 seasons all are blank and the page says to run more simulations. Once matches have been scored, **Forecast accuracy** on **Overview** shows the hit rate, the log-loss and its difference from the base rates with a 95% interval and the number of matches, and the same for each model version separately (forecasts made by an earlier version of the model are scored apart from the current one's). Open **Forecast history** there to see every scored match's frozen forecast next to its result and the model that made it, matchday by matchday, with a chart of the log-loss so far against the base rates.
3. On **Fixtures**, pick results to assume (at most 20, the API's limit) and select **Recalculate**.
4. Select **Update live data** to pull the latest TFF results and recompute the forecast, fixture cards, and accuracy view. Results you assumed for matches that have since been played are dropped, with a notice.
5. Natural-language scenarios are not part of the dashboard. They are an optional API endpoint (`/api/scenario`, below) that is off unless the server sets `SCENARIO_ENABLED=1` and `KIMI_API_KEY`.

When the simulation queue is full (503), the page waits as long as the server asks, never more than 10 s at a time, and retries up to three times before showing a "try again in a moment" message. When the per-IP rate limit is hit (429), the page waits and retries if the limit frees up within 10 s; otherwise it says at once "Too many requests — try again in N s". Other failures show a short localized message, with the server's own text in parentheses when it sent one. The last forecast stays on screen. The fixture list and results stay available even when a simulation fails.

## API

| Endpoint | Method | Limit per IP | Description |
|---|---|---:|---|
| `/api/health` | `GET` | — | Service version, model configuration and `model_version`, last live refresh, `world_version`, and whether the default forecast is cached (`forecast_cached`). |
| `/api/forecast` | `GET` | 60/min | The default forecast (50,000 seasons, seed 12345, no overrides): the same JSON as `POST /api/simulate {}`, computed once per state of the league. |
| `/api/simulate` | `POST` | 30/min | Run a simulation with optional Elo overrides and pinned outcomes. Repeated identical requests are answered from a cache until the results or ratings change. |
| `/api/scenario` | `POST` | 10/min | Analyze a prompt with Kimi and rerun with its Elo overrides. Returns 404 unless `SCENARIO_ENABLED=1`. |
| `/api/refresh` | `POST` | 5/min | Fetch and apply the current TFF results. |
| `/api/live` | `GET` | — | Return the most recently cached live-data snapshot. |
| `/api/accuracy` | `GET` | 30/min | How the model has scored against predictions frozen before kick-off: hit rate, log-loss vs the base-rate baseline with a 95% interval for the difference, and calibration bands, pooled and for each model version (`by_model`), plus every scored match with its frozen forecast, the model that made it, and its own log-loss. |
| `/api/upcoming` | `GET` | 30/min | Home/draw/away forecasts for the next matchday's unplayed fixtures. |
| `/api/matches` | `GET` | 30/min | The full 306-fixture calendar: real scores for played games, 1X2 / over-under 2.5 / both-teams-to-score probabilities and fair odds for the rest; the table as it stands (`standings`); and how far the season has got (`as_of`: results recorded, fixtures, the latest result's date). |

`/api/accuracy`, `/api/upcoming`, and `/api/matches` share one 30/min allowance per IP; `/api/forecast` has its own 60/min allowance. A rate-limited request gets HTTP 429 with a `Retry-After` header.

Simulations run one at a time by default (`MAX_CONCURRENT_SIMULATIONS`). Up to `MAX_SIMULATION_QUEUE` further runs wait up to `SIM_QUEUE_TIMEOUT_SECS` for a turn. Past that, `/api/simulate`, `/api/forecast`, and `/api/scenario` answer HTTP 503 with `Retry-After: 10`. Cache hits, and requests identical to a run already in progress, never wait in the queue. A queued run whose clients have all disconnected leaves the queue without running.

The next matchday, used by `/api/upcoming` and as the calendar's `current_round`, is the round of the earliest unplayed fixture dated today or later (Istanbul time). One postponed match therefore does not hold it back. If a rescheduled match falls on the same day as a matchday, the round with more fixtures that day, played or not, is next, and a tie goes to the later round, normally the matchday. On that day the matchday stays next as long as it has a game still to come; once it is over, the rescheduled match is next. When only overdue fixtures remain, it is the earliest round that still has one; once the season is complete, the last round.

The API sends no CORS headers: the dashboard is served from the same origin, and the Vite development server proxies `/api`.

Responses, from the API and the dashboard alike, are compressed with gzip or brotli when the client accepts it (images and very small bodies excepted), and every response carries `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `X-Frame-Options: DENY` and a Content-Security-Policy that allows scripts, styles and requests from the site itself only, with no inline scripts and no framing.

`/api/simulate` also accepts `what_if`: up to 20 pinned outcomes for unplayed fixtures, e.g. `[{"home":"Gaziantep","away":"Galatasaray","outcome":"home"}]`. The *outcome* is fixed, not the scoreline: the scoreline is drawn from the model's score probabilities for that match, restricted to the pinned outcome, so goal difference stays honest. A pinned result then updates the simulated season's Elo ratings as the same real result would, and the season is played in kick-off order, so only later fixtures feel it. On the 2026-10-19 calendar (50,000 seasons), pinning a Fenerbahçe win in the round-9 Galatasaray v Fenerbahçe takes Galatasaray's title odds from 59.1% to 32.1%, between entering the match as a real 1-2 (34.5%) and 1-3 (29.8%). Pinning an already-played or non-existent fixture, or one fixture twice, is an error rather than a silent no-op.

Simulation requests accept 100–200,000 trials. Scenario prompts are limited to 2,000 characters, Elo overrides must name a known club and fall between 1,200 and 2,000; LLM scenarios return rating changes of at most ±150 points (a change that would carry a club outside 1,200–2,000 stops at the bound), and request bodies are limited to 1 MiB. The scenario's analysis text is capped at 600 characters. When the LLM call fails, the client gets a generic HTTP 502; the details go to the server log only.

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

`elo_overrides` contains requested ratings, not point deltas. Each is applied as its difference from the club's current Elo rating, on the whole model (see [How the model works](#how-the-model-works)); the Elo ratings themselves are not replaced.

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

Omit `SCENARIO_ENABLED` and `KIMI_API_KEY` if scenario analysis is not needed. The build context leaves out host build outputs, `node_modules`, `.env` files, Git metadata, documentation, and the market benchmark's odds (see `.dockerignore`).

The server runs as an unprivileged user (UID and GID 10001) that owns no files. On `docker stop` (SIGTERM) or Ctrl-C it stops accepting connections, gives open requests up to 5 seconds to finish, and exits without waiting for a simulation still in progress.

## Deploy to Railway

1. Create a Railway service from this GitHub repository, or run `railway init && railway up` from a clone.
2. Railway detects the root `Dockerfile` and builds the Rust backend and React frontend.
3. To keep scenario analysis available, set both `SCENARIO_ENABLED=1` and `KIMI_API_KEY`; without `SCENARIO_ENABLED`, `/api/scenario` answers 404.
4. Set `TRUST_PROXY=1` so rate limiting uses the client address supplied by Railway's sanitizing edge proxy.
5. Optionally customize `LIVE_REFRESH_MINUTES`, `ENSEMBLE_WEIGHTS`, `MAX_CONCURRENT_SIMULATIONS`, `MAX_SIMULATION_QUEUE`, `SIM_QUEUE_TIMEOUT_SECS`, and `RUST_LOG`.
6. Set the health-check path to `/api/health`.

The application reads Railway's injected `PORT` automatically and shuts down gracefully on the SIGTERM Railway sends when it stops a deployment. The scheduled data-refresh workflow (see [Refresh the data](#refresh-the-data)) also redeploys after committing new data, but only when the repository has a `RAILWAY_TOKEN` secret.

## Validation

The GitHub Actions workflow runs the same core checks:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
# The examples' own tests, which --all-targets builds without.
cargo clippy --examples --profile test -- -D warnings
cargo test --examples
cargo build --release
cargo audit

cd frontend
npm ci
npm audit
npx tsc --noEmit
npm run build
npm test            # needs Node.js 22.6+ (--experimental-strip-types)
cd ..

# The committed evaluation figures, which this README quotes, must still be
# what the code produces.
EVAL_HOLDOUT=1 EVAL_METRICS=/tmp/backtest.json cargo run --release --example backtest
python3 scripts/check_metrics.py diff /tmp/backtest.json data/metrics/backtest.json
EVAL_HOLDOUT=1 SEASON_METRICS=/tmp/season.json cargo run --release --example season_eval
python3 scripts/check_metrics.py diff /tmp/season.json data/metrics/season.json

# Offline tests for the fetch scripts and the metrics checker (no network),
# and every quoted figure in the READMEs, CLAUDE.md and the model card against
# data/metrics/.
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/check_metrics.py docs
```

The shipped model's figures in [Backtest results](#backtest-results), [Season-level evaluation](#season-level-evaluation) and the [model card](docs/model-card.md) are checked this way. Figures from tools that write no metrics file are not: the tuning comparison (`examples/compare.rs`), the season intervals (only printed), the later sweeps (the 202 match settings and the season-spread trials), the holdout comparison of the season-spread change, and the examples of overrides and pinned outcomes (one-off runs, dated where the calendar matters). If a change moves the checked figures, regenerate the committed files with `EVAL_HOLDOUT=1 EVAL_METRICS=data/metrics/backtest.json cargo run --release --example backtest` and `EVAL_HOLDOUT=1 SEASON_METRICS=data/metrics/season.json cargo run --release --example season_eval`, then update the quoted figures.

The market comparison in [Against the market](#against-the-market) is quoted from `data/metrics/market.json`, which CI cannot regenerate: its odds are fetched over the network and never committed. CI still checks every quoted figure against the committed file. To regenerate it, run `python3 scripts/fetch_fixtures.py` (so the live comparison sees every played match), `python3 scripts/fetch_odds.py`, the holdout backtest with `EVAL_DUMP=/tmp/prod.csv`, then `MARKET_METRICS=data/metrics/market.json cargo run --release --example market -- /tmp/prod.csv`.

## Refresh the data

The repository already includes the fixture calendar, historical results, and fitted Dixon–Coles parameters used at build time. To refresh them:

```bash
# Official 2026-27 calendar and results so far, from tff.org.
# Asserts the full double round-robin structure: 306 fixtures, 34 rounds,
# 9 per round, 153 pairs each played exactly twice. A postponed match that
# TFF lists without a date ("Ertelendi" or blank) keeps its committed date and
# kick-off, at most 18 per run; a week page that does not yield all 9 of its
# matches, or a date cell in an unknown format, stops the run. A committed
# result that disappears or changes stops the run before the file is written;
# set ALLOW_RESULT_CHANGES=1 for a genuine TFF correction or award.
python3 scripts/fetch_fixtures.py

# Chronological 2012-13 to 2024-25 results from the MIT-licensed Club Football
# Match Data archive, the archive's missing 2024-25 matchdays 10-12 from
# openfootball (CC0, reconciled against the archive's rows of that season),
# and the official TFF 2025-26 weekly archive. Both upstream files are pinned
# by SHA-256 (the archive by the slice of rows the script uses); a changed
# digest stops the run. Every season must be a complete double round-robin,
# bar one listed 2024-25 award, before the CSV is replaced.
python3 scripts/fetch_history.py
# The same, but reusing (and re-validating) the committed 2025-26 rows
# instead of fetching tff.org.
python3 scripts/fetch_history.py --keep-tff

# Bookmaker odds for the offline market benchmark only, from the same archive,
# reconciled with the history and pinned by SHA-256. Written to data/odds/,
# which is not committed; nothing in the app reads them. Run fetch_fixtures.py
# first: a 2026-27 odds row whose match the calendar does not show as played
# yet is skipped and counted.
python3 scripts/fetch_odds.py

# Refit Dixon–Coles against the refreshed history (~0.3 s).
cargo run --release --example fit_dc
```

Review the changed files under `data/` before committing a new fit.

The committed calendar is also refreshed automatically. A scheduled GitHub Actions workflow (`.github/workflows/refresh-data.yml`) runs on Thursdays at 15:17 UTC and Fridays at 06:00 UTC, and on demand. It fetches the calendar, freezes forecasts for upcoming fixtures, runs the test suite, and commits any changes. A failed run opens a "Weekly data refresh failed" issue, or comments on the open one, and the next successful run closes it. To accept a genuine TFF result correction, run the workflow by hand with `allow_result_changes`.

Data-source details worth knowing about:

- **Historical match dates are preserved.** Dixon–Coles recency weights and sequential pi-ratings both depend on chronology; the generator rejects partial season archives instead of silently fitting them.
- **The archive lacks three 2024-25 matchdays.** It has 314 of the season's 342 fixtures: the 27 matches of matchdays 10-12 and one administrative award are missing upstream. The generator adds the 27 from openfootball after checking that every archive row of that season agrees with it, and keeps the award out, since its 3-0 was not decided on the pitch.
- The redistributed archive-derived rows retain their upstream MIT notice in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md), which also credits openfootball (CC0 1.0).

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
│   ├── accuracy.rs       # Scoring the frozen forecast log, per model version
│   ├── model_version.rs  # Which model made a forecast, and its fingerprint
│   ├── odds.rs           # Fair odds (1 / p) for the API
│   ├── models.rs         # API request and response types
│   ├── validation.rs     # Request validation
│   └── rate_limit.rs     # Per-IP rate limiting
├── data/                 # Fixture calendar, historical results, fitted params, forecast log
│   └── metrics/          # Committed backtest, season-evaluation and market-benchmark figures
├── docs/                 # The model card, in English and Turkish: what the model is for, how it was evaluated, its limits
├── frontend/             # React and TypeScript dashboard
├── examples/             # Model fitting, backtest, season evaluation, comparison, market benchmark, forecast logging
├── scripts/              # Data-acquisition scripts, the metrics checker, their offline tests
├── tests/calibration.rs  # Checks the shipped blend against the real league
├── .github/workflows/    # CI and the scheduled data refresh
├── Dockerfile            # Production multi-stage image
├── LICENSE               # MIT
└── THIRD_PARTY_NOTICES.md # Data sources and their terms
```

## Data and model caveats

- Live refresh depends on tff.org and its current page format; the embedded baseline calendar remains available if a refresh fails.
- Fair odds are simply the inverse of simulated probabilities and do not include a bookmaker margin, liquidity, or market information.
- Promoted clubs are the least well-modelled: they have no top-flight history for the Dixon–Coles and pi-rating components, so their forecasts lean entirely on their Elo rating.
- The head-to-head-once rule, and the current table's wait for both meetings before head-to-head counts (see above), are stated assumptions, not published rules.
- The model loses to bookmaker prices (see [Against the market](#against-the-market)). It is not a basis for betting, and the dashboard shows no bookmaker odds or market figures.
- Scenario ratings are model-generated assumptions. Read the returned explanation and treat the output as exploratory.
- Season-level ranges are too narrow: in backtests, final-points ranges meant to contain 80% of clubs contained about 70%. Applying the strength draw to the whole model fixed that on the development seasons but made the holdout seasons worse, so it was not shipped (see [Season-level evaluation](#season-level-evaluation)).
- Draws are forecast slightly too rarely and goals slightly too often (see [Calibration](#calibration)).
- Forecast quality depends on ratings, historical-data coverage, model assumptions, and the number of trials.

## License

The code is released under the [MIT License](LICENSE). The data it ships keep their own terms, listed in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md): the match archive is MIT-licensed and openfootball CC0, and the 2025-26 results and the 2026-27 fixtures and results, taken from tff.org's public pages, are not covered by the MIT License.
