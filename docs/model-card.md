# Model card: Süper Lig 2026-27 forecast model

[English](model-card.md) | [Türkçe](model-card.tr.md)

The model behind this repository's forecasts, as it ships under the label `2026-10b` (19 October 2026). The headings follow Mitchell et al., "Model Cards for Model Reporting" (FAT* 2019). Every figure taken from `data/metrics/` sits in a hidden marker that `python3 scripts/check_metrics.py docs` checks in CI. Any other figure says what produced it and is not gated. [CLAUDE.md](../CLAUDE.md) has the detail behind each section.

## Model details

| | |
|---|---|
| Label | `2026-10b` (`LABEL` in `src/model_version.rs`). Earlier labels: `2026-08` (ClubElo preseason prior, in-season K 20, `D_DIV` 1600, no underdog lift) and `2026-10a` (the shipped rules on the history that still lacked 2024-25 matchdays 10-12). |
| Fingerprint | `FINGERPRINT` `0xd46b_9fb0_56c6_d09c`, an FNV-1a hash of every input that moves a 1X2 forecast (the model constants, default weights, club list, `dc_params.json` and the results history), and `PROBE` `0x4522_bdaa_bfe1_e492`, a hash of the model's own preseason forecasts for every pairing. Tests pin both, so a changed model cannot ship under this label. |
| Shipped | 2026-10-19: the October 2026 tuning, run on the corrected history, Dixon-Coles refit. |
| Owner | Çağatay (GitHub `omercagatay`), repository `omercagatay/superlig-2026-27-simulator`. Questions and problems: GitHub issues. |
| License | Code: MIT (`LICENSE`). Data keep their own terms (`THIRD_PARTY_NOTICES.md`): the match archive is MIT, openfootball CC0, and the 2025-26 results and 2026-27 fixtures and results taken from tff.org are not covered by the MIT License. |

**Match model.** Each fixture gets an expected-goals rate (λ) per side, a weighted blend of three strength models fitted to Süper Lig results (`ENSEMBLE_WEIGHTS`, default `0.5,0.3,0.2`):

| Component | Weight | How it rates clubs |
|---|---:|---|
| Elo-Poisson | 0.5 | λ<sub>home</sub> = 1.35 × 10^((gap + 80) / 1200) and λ<sub>away</sub> = 1.35 × 10^(−(gap + 80) / 1200), gap = home Elo − away Elo, 80 the home advantage. Preseason ratings replay every top-flight result since 2012-13 at K 20; a club with none starts at 1450. During the season each result moves the ratings at K 60 with a square-root goal-difference multiplier, recomputed from the preseason baseline on every refresh. |
| Dixon-Coles | 0.3 | Attack, defence, home advantage and the low-score correction ρ, fitted offline (`examples/fit_dc.rs`, four-year half-life, `data/dc_params.json`). Its joint score table, up to 10 goals a side, supplies the scoreline distribution. Frozen until refit. |
| Pi-ratings | 0.2 | Constantinou-Fenton home and away ratings, one pass over the same history at startup. Frozen during the season. |

After blending, the underdog lift raises the lower λ of the pair, when it is below 0.9, by 70% of its gap to 0.9, and the favourite's λ by half of that relative lift; when both are below 0.9, only the lower is lifted. Match probabilities (home/draw/away, 3+ goals, both teams score, scorelines) are exact sums over the score table, not Monte Carlo.

**Season simulator.** 50,000 seasons by default (seed 12345; 100 to 200,000 on request). Recorded results are fixed; every other fixture is drawn from the blend. Each club draws one strength shock, N(0, 75) Elo points, held for the whole simulated season (`RATING_SIGMA`). Simulated results do not move ratings (a cold simulation); an outcome the user pins does, as the same real result would. Final tables follow the league rules, head-to-head before goal difference, with a random draw standing in for the play-off. Outputs: finishing-position probabilities, title, top-two, third, fourth, top-four and relegation chances, a projected table from expected records, the points needed for each place, and who finishes above whom.

**What-if tools.** A rating override, or a natural-language scenario turned into rating changes by an LLM (an API endpoint, off by default), shifts the whole blend for that club. These are user assumptions layered on the model, not part of it, and the evaluation below never uses them.

## Intended use

- **Primary:** probabilistic forecasts for people following the 2026-27 Süper Lig: title, top-four and relegation chances, the projected final table, the next matchday's home/draw/away chances, the points that have been enough for each place, and what-if exploration.
- **Secondary:** a reproducible example of evaluating a sports forecast match by match, season by season and against the market.

Out of scope:

- **Betting or any financial decision.** The model loses to bookmaker prices on both the development and the holdout seasons; the live sample is too small to say ([Against the market](#against-the-market-offline)). It is no basis for a bet. The app, its API and its UI carry no bookmaker odds and no market figures. The market benchmark is offline evaluation only: its odds are never committed and nothing in the app reads them. The API's `*_odds` fields are the model's own 1 / p, not prices, and the dashboard shows no odds at all.
- **European places.** Qualification also depends on the cup winner and the season's access list, so the site gives finishing bands (top two, third, fourth, top four), not competition names.
- **Exact points totals.** Season ranges were too narrow on the development seasons ([Season level](#season-level-season-evaluation)); title odds say who is likely to win, not with how many points.
- **Other leagues, divisions or seasons** without refitting, and anything below club level: the model sees results only, never lineups, injuries, transfers or xG.
- **LLM scenarios** as forecasts: they are model-generated assumptions, for exploration.

## Factors

Measured or known to move the model's accuracy:

- **Season phase.** Forecasts sharpen as results come in (season-level table below). The gap to the market by matchday band is in [Against the market](#against-the-market-offline).
- **Promoted clubs** (Amedspor and Çorum for 2026-27) have no top-flight history: Dixon-Coles and pi-ratings rate them league-average, so only their Elo rating separates them.
- **Mismatches.** Underdogs score more than the blend says. With the underdog lift off (`EVAL_LIFT=0 EVAL_HOLDOUT=1` backtest with `EVAL_DUMP`, taking the lower expected-goals rate of each match where it is below 0.9; a one-off, not gated), that rate averaged 0.776 over 572 dev and holdout matches, and those sides scored 0.953 goals a match (dev alone 0.788 against 0.930, 430 matches).
- **Season scoring level** moves between seasons (2.64 to 3.05 goals a match since 2012-13, from `tests/calibration.rs`), and nothing in the model tracks it within a season: 2025-26 scored 2.654 a match against 2.988 forecast (the backtest's per-season line, not gated).

The model uses no personal or demographic data; clubs are the only units.

## Data

**History** (`data/superlig_results.csv`, embedded at build time): every Süper Lig match from 2012-13 to 2025-26, 4,588 played matches once awards are dropped.

| Seasons | Source | Notes |
|---|---|---|
| 2012-13 to 2024-25 | Club Football Match Data archive (MIT; built from Football-Data.co.uk) | real dates; the slice used is pinned by SHA-256 (`ARCHIVE_SLICE_SHA256`) |
| 2024-25 matchdays 10-12 (27 matches) | openfootball `2024-25/tr.1.json` (CC0) | missing upstream; every archive row of the season must agree with it; pinned by SHA-256 |
| 2025-26 | tff.org weekly pages | windows-1254 decoding, TFF's incomplete TLS chain handled with a pinned intermediate |

Fixes: the 29 walkovers awarded after the February 2023 earthquake stay in the file and are dropped at load; the 2024-25 Galatasaray v Adana Demirspor award is kept out at source; Karagümrük's two spellings are merged; every season must be a complete double round-robin bar those awards; the season boundary in August keeps the July 2020 matches in 2019-20. Details: [CLAUDE.md, "Data-source traps"](../CLAUDE.md#data-source-traps).

**Live season:** `data/fixtures_2026_27.json`, TFF's 306-fixture calendar with results, refreshed by `scripts/fetch_fixtures.py` (scheduled on Thursdays and Fridays) and by the server's scraper (every 30 minutes by default), which validates all 306 rows before applying any result.

**Evaluation only:** Bet365 and best-price pre-match odds from the same archive, written by `scripts/fetch_odds.py` to `data/odds/`, which `.gitignore` and `.dockerignore` exclude. Nothing in `src/` or `frontend/` reads them.

## Evaluation design

| Evaluation | Tool | What is scored | Matches or seasons | Gate |
|---|---|---|---|---|
| Walk-forward backtest | `examples/backtest.rs` | each match priced with the production rules from earlier dates only; same-day results revealed after pricing | dev 2016-17 to 2023-24 (<!-- metric:dev_n:,d -->2,717<!-- /metric -->), holdout 2024-25 and 2025-26 (<!-- metric:hold_n:,d -->647<!-- /metric -->) | `backtest.json`, rerun and diffed in CI |
| Season-level evaluation | `examples/season_eval.rs` | positions, title, top four, relegation and final points, from nothing played (s0), a third (s33) and two thirds (s67) | <!-- metric:season:dev_n_seasons -->8<!-- /metric --> dev and <!-- metric:season:hold_n_seasons -->2<!-- /metric --> holdout seasons, <!-- metric:season:n_sims:,d -->2,000<!-- /metric --> simulated seasons each | `season.json`, rerun and diffed in CI |
| Calibration check | `tests/calibration.rs` | the shipped blend at preseason against the history | 4,588 matches | pinned in the test |
| Live tracker | `examples/log_forecast.rs`, `src/accuracy.rs` | the last forecast frozen before kick-off, scored per model label | 2026-27 | not gated (`GET /api/accuracy`) |
| Market benchmark | `scripts/fetch_odds.py`, `examples/market.rs` | the backtest's forecasts against Bet365 pre-match odds, margin removed by Shin's method | dev <!-- metric:market:dev_n:,d -->2,714<!-- /metric -->, holdout <!-- metric:market:hold_n:,d -->620<!-- /metric -->, live | `market.json`, markers checked in CI, regenerated by hand |

- **The holdout is lightly used, not untouched.** It informed two decisions in the October 2026 tuning, it vetoed the phase-4 dispersion change, and `RATING_SIGMA` (75) was chosen on its table spread. The live 2026-27 season is the clean test.
- **Statistics.** Model comparisons are paired, match by match, with a 95% date-block bootstrap interval (10,000 resamples; a date's matches share one rating state), a Diebold-Mariano-style t and a 95% season-cluster t interval (seasons − 1 degrees of freedom, so one for the holdout). Skill is the share of a base-rate forecast's loss the model removes, the base rates taken only from seasons before the one scored. Season figures carry 95% season-cluster bootstrap intervals, rough over 8 seasons.
- **Metrics.** Match level: 1X2 log-loss (the primary score), RPS, log-loss for over/under 2.5, both teams score and the exact score, calibration in the large and logistic recalibration slopes. Season level: position RPS and points CRPS decide; Brier skill for title, top four and relegation; interval coverage, PIT and the error ratio are diagnostics.

## Quantitative results

### Match level (walk-forward backtest)

| | Dev | Holdout |
|---|---:|---:|
| Matches | <!-- metric:dev_n:,d -->2,717<!-- /metric --> | <!-- metric:hold_n:,d -->647<!-- /metric --> |
| 1X2 log-loss (base rates) | <!-- metric:dev_1x2_ll:.4f -->0.9991<!-- /metric --> (<!-- metric:dev_1x2_ll_base:.4f -->1.0678<!-- /metric -->) | <!-- metric:hold_1x2_ll:.4f -->0.9756<!-- /metric --> (<!-- metric:hold_1x2_ll_base:.4f -->1.0669<!-- /metric -->) |
| 1X2 log-loss skill | <!-- metric:dev_1x2_skill:.1% -->6.4%<!-- /metric --> | <!-- metric:hold_1x2_skill:.1% -->8.6%<!-- /metric --> |
| RPS (skill) | <!-- metric:dev_rps:.4f -->0.2046<!-- /metric --> (<!-- metric:dev_rpss:.1% -->10.2%<!-- /metric -->) | <!-- metric:hold_rps:.4f -->0.1946<!-- /metric --> (<!-- metric:hold_rpss:.1% -->13.8%<!-- /metric -->) |
| Over/under 2.5 log-loss (skill) | <!-- metric:dev_ou25_ll:.4f -->0.6821<!-- /metric --> (<!-- metric:dev_ou25_skill:.1% -->1.4%<!-- /metric -->) | <!-- metric:hold_ou25_ll:.4f -->0.6737<!-- /metric --> (<!-- metric:hold_ou25_skill:.1% -->2.4%<!-- /metric -->) |
| Both teams score log-loss (base rates) | <!-- metric:dev_btts_ll:.4f -->0.6858<!-- /metric --> (<!-- metric:dev_btts_ll_base:.4f -->0.6862<!-- /metric -->) | <!-- metric:hold_btts_ll:.4f -->0.6876<!-- /metric --> (<!-- metric:hold_btts_ll_base:.4f -->0.6868<!-- /metric -->) |
| Exact-score log-loss | <!-- metric:dev_score_ll:.3f -->2.946<!-- /metric --> | <!-- metric:hold_score_ll:.3f -->2.921<!-- /metric --> |
| Hit rate (most likely result) | <!-- metric:dev_acc:.1% -->51.7%<!-- /metric --> | <!-- metric:hold_acc:.1% -->54.1%<!-- /metric --> |

Calibration, mean forecast against observed rate:

| | Dev forecast | Dev observed | Holdout forecast | Holdout observed |
|---|---:|---:|---:|---:|
| Home win | <!-- metric:dev_cal_home_pred:.1% -->45.4%<!-- /metric --> | <!-- metric:dev_cal_home_obs:.1% -->45.2%<!-- /metric --> | <!-- metric:hold_cal_home_pred:.1% -->45.9%<!-- /metric --> | <!-- metric:hold_cal_home_obs:.1% -->45.4%<!-- /metric --> |
| Draw | <!-- metric:dev_cal_draw_pred:.1% -->24.6%<!-- /metric --> | <!-- metric:dev_cal_draw_obs:.1% -->25.4%<!-- /metric --> | <!-- metric:hold_cal_draw_pred:.1% -->23.6%<!-- /metric --> | <!-- metric:hold_cal_draw_obs:.1% -->26.1%<!-- /metric --> |
| Away win | <!-- metric:dev_cal_away_pred:.1% -->30.0%<!-- /metric --> | <!-- metric:dev_cal_away_obs:.1% -->29.4%<!-- /metric --> | <!-- metric:hold_cal_away_pred:.1% -->30.5%<!-- /metric --> | <!-- metric:hold_cal_away_obs:.1% -->28.4%<!-- /metric --> |
| Goals per match | <!-- metric:dev_goals_pred:.3f -->2.865<!-- /metric --> | <!-- metric:dev_goals_obs:.3f -->2.816<!-- /metric --> | <!-- metric:hold_goals_pred:.3f -->2.979<!-- /metric --> | <!-- metric:hold_goals_obs:.3f -->2.802<!-- /metric --> |
| 3+ goals | <!-- metric:dev_ou25_pred:.1% -->54.3%<!-- /metric --> | <!-- metric:dev_ou25_obs:.1% -->53.2%<!-- /metric --> | <!-- metric:hold_ou25_pred:.1% -->56.5%<!-- /metric --> | <!-- metric:hold_ou25_obs:.1% -->54.1%<!-- /metric --> |
| Both teams score | <!-- metric:dev_btts_pred:.1% -->54.8%<!-- /metric --> | <!-- metric:dev_btts_obs:.1% -->56.1%<!-- /metric --> | <!-- metric:hold_btts_pred:.1% -->55.2%<!-- /metric --> | <!-- metric:hold_btts_obs:.1% -->55.6%<!-- /metric --> |

Recalibration slopes (1 is calibrated, below 1 too spread): home <!-- metric:dev_cal_home_slope:.3f -->0.996<!-- /metric --> ± <!-- metric:dev_cal_home_slope_se:.3f -->0.064<!-- /metric -->, draw <!-- metric:dev_cal_draw_slope:.3f -->1.397<!-- /metric --> ± <!-- metric:dev_cal_draw_slope_se:.3f -->0.247<!-- /metric -->, away <!-- metric:dev_cal_away_slope:.3f -->1.079<!-- /metric --> ± <!-- metric:dev_cal_away_slope_se:.3f -->0.070<!-- /metric --> on dev; <!-- metric:hold_cal_home_slope:.3f -->0.894<!-- /metric --> ± <!-- metric:hold_cal_home_slope_se:.3f -->0.110<!-- /metric -->, <!-- metric:hold_cal_draw_slope:.3f -->0.614<!-- /metric --> ± <!-- metric:hold_cal_draw_slope_se:.3f -->0.293<!-- /metric -->, <!-- metric:hold_cal_away_slope:.3f -->1.163<!-- /metric --> ± <!-- metric:hold_cal_away_slope_se:.3f -->0.130<!-- /metric --> on the holdout. None is distinguishable from 1. Draws are forecast too rarely and goals too often in both periods; both teams scoring has no skill over the base rate.

The calibration check (`tests/calibration.rs`, the shipped blend at preseason against the 4,588 history matches; printed by `cargo test --test calibration -- --nocapture`, pinned in the test, not in a metrics file) finds the same: home wins 44.9% per fixture against 45.4%; draws 24.8% per fixture and 24.4% per simulated season against 25.9%; away wins 30.4% against 28.8%; goals 2.878 a match per fixture and 2.915 per season against 2.787.

### Season level (season evaluation)

Pooled over the seasons; position RPS and points CRPS: lower is better.

| | Dev s0 | Dev s33 | Dev s67 | Holdout s0 | Holdout s33 | Holdout s67 |
|---|---:|---:|---:|---:|---:|---:|
| Position RPS | <!-- metric:season:dev_s0_pos_rps:.4f -->0.1322<!-- /metric --> | <!-- metric:season:dev_s33_pos_rps:.4f -->0.1121<!-- /metric --> | <!-- metric:season:dev_s67_pos_rps:.4f -->0.0646<!-- /metric --> | <!-- metric:season:hold_s0_pos_rps:.4f -->0.1283<!-- /metric --> | <!-- metric:season:hold_s33_pos_rps:.4f -->0.0648<!-- /metric --> | <!-- metric:season:hold_s67_pos_rps:.4f -->0.0485<!-- /metric --> |
| Points CRPS (points) | <!-- metric:season:dev_s0_pts_crps:.2f -->6.18<!-- /metric --> | <!-- metric:season:dev_s33_pts_crps:.2f -->5.04<!-- /metric --> | <!-- metric:season:dev_s67_pts_crps:.2f -->2.93<!-- /metric --> | <!-- metric:season:hold_s0_pts_crps:.2f -->5.80<!-- /metric --> | <!-- metric:season:hold_s33_pts_crps:.2f -->2.97<!-- /metric --> | <!-- metric:season:hold_s67_pts_crps:.2f -->2.13<!-- /metric --> |
| Title, Brier skill | <!-- metric:season:dev_s0_title_bss:.3f -->0.052<!-- /metric --> | <!-- metric:season:dev_s33_title_bss:.3f -->0.307<!-- /metric --> | <!-- metric:season:dev_s67_title_bss:.3f -->0.500<!-- /metric --> | <!-- metric:season:hold_s0_title_bss:.3f -->0.717<!-- /metric --> | <!-- metric:season:hold_s33_title_bss:.3f -->0.933<!-- /metric --> | <!-- metric:season:hold_s67_title_bss:.3f -->0.581<!-- /metric --> |
| Top four, Brier skill | <!-- metric:season:dev_s0_top4_bss:.3f -->0.430<!-- /metric --> | <!-- metric:season:dev_s33_top4_bss:.3f -->0.584<!-- /metric --> | <!-- metric:season:dev_s67_top4_bss:.3f -->0.813<!-- /metric --> | <!-- metric:season:hold_s0_top4_bss:.3f -->0.610<!-- /metric --> | <!-- metric:season:hold_s33_top4_bss:.3f -->0.900<!-- /metric --> | <!-- metric:season:hold_s67_top4_bss:.3f -->0.933<!-- /metric --> |
| Relegation, Brier skill | <!-- metric:season:dev_s0_rel_bss:.3f -->0.145<!-- /metric --> | <!-- metric:season:dev_s33_rel_bss:.3f -->0.319<!-- /metric --> | <!-- metric:season:dev_s67_rel_bss:.3f -->0.590<!-- /metric --> | <!-- metric:season:hold_s0_rel_bss:.3f -->-0.101<!-- /metric --> | <!-- metric:season:hold_s33_rel_bss:.3f -->0.520<!-- /metric --> | <!-- metric:season:hold_s67_rel_bss:.3f -->0.687<!-- /metric --> |
| Chance given to the champion | <!-- metric:season:dev_s0_p_champion:.0% -->19%<!-- /metric --> | <!-- metric:season:dev_s33_p_champion:.0% -->38%<!-- /metric --> | <!-- metric:season:dev_s67_p_champion:.0% -->57%<!-- /metric --> | <!-- metric:season:hold_s0_p_champion:.0% -->63%<!-- /metric --> | <!-- metric:season:hold_s33_p_champion:.0% -->82%<!-- /metric --> | <!-- metric:season:hold_s67_p_champion:.0% -->62%<!-- /metric --> |
| Final points inside the 80% range | <!-- metric:season:dev_s0_cov80_pct:.1f -->70.4<!-- /metric -->% | <!-- metric:season:dev_s33_cov80_pct:.1f -->67.8<!-- /metric -->% | <!-- metric:season:dev_s67_cov80_pct:.1f -->73.0<!-- /metric -->% | <!-- metric:season:hold_s0_cov80_pct:.1f -->73.0<!-- /metric -->% | <!-- metric:season:hold_s33_cov80_pct:.1f -->86.5<!-- /metric -->% | <!-- metric:season:hold_s67_cov80_pct:.1f -->81.1<!-- /metric -->% |
| Final points inside the 50% range | <!-- metric:season:dev_s0_cov50_pct:.1f -->42.1<!-- /metric -->% | <!-- metric:season:dev_s33_cov50_pct:.1f -->42.8<!-- /metric -->% | <!-- metric:season:dev_s67_cov50_pct:.1f -->50.0<!-- /metric -->% | <!-- metric:season:hold_s0_cov50_pct:.1f -->48.6<!-- /metric -->% | <!-- metric:season:hold_s33_cov50_pct:.1f -->75.7<!-- /metric -->% | <!-- metric:season:hold_s67_cov50_pct:.1f -->51.4<!-- /metric -->% |
| PIT SD (uniform: 0.289) | <!-- metric:season:dev_s0_pit_sd:.3f -->0.318<!-- /metric --> | <!-- metric:season:dev_s33_pit_sd:.3f -->0.325<!-- /metric --> | <!-- metric:season:dev_s67_pit_sd:.3f -->0.302<!-- /metric --> | <!-- metric:season:hold_s0_pit_sd:.3f -->0.305<!-- /metric --> | <!-- metric:season:hold_s33_pit_sd:.3f -->0.240<!-- /metric --> | <!-- metric:season:hold_s67_pit_sd:.3f -->0.270<!-- /metric --> |
| Realized error / forecast SD | <!-- metric:season:dev_s0_pts_err_ratio:.2f -->1.26<!-- /metric --> | <!-- metric:season:dev_s33_pts_err_ratio:.2f -->1.30<!-- /metric --> | <!-- metric:season:dev_s67_pts_err_ratio:.2f -->1.17<!-- /metric --> | <!-- metric:season:hold_s0_pts_err_ratio:.2f -->1.27<!-- /metric --> | <!-- metric:season:hold_s33_pts_err_ratio:.2f -->0.82<!-- /metric --> | <!-- metric:season:hold_s67_pts_err_ratio:.2f -->0.85<!-- /metric --> |
| Table SD, simulated / actual | <!-- metric:season:dev_s0_table_sd_sim:.1f -->13.6<!-- /metric --> / <!-- metric:season:dev_s0_table_sd_actual:.1f -->15.1<!-- /metric --> | <!-- metric:season:dev_s33_table_sd_sim:.1f -->14.3<!-- /metric --> / <!-- metric:season:dev_s33_table_sd_actual:.1f -->15.1<!-- /metric --> | <!-- metric:season:dev_s67_table_sd_sim:.1f -->14.4<!-- /metric --> / <!-- metric:season:dev_s67_table_sd_actual:.1f -->15.1<!-- /metric --> | <!-- metric:season:hold_s0_table_sd_sim:.1f -->15.2<!-- /metric --> / <!-- metric:season:hold_s0_table_sd_actual:.1f -->16.4<!-- /metric --> | <!-- metric:season:hold_s33_table_sd_sim:.1f -->16.8<!-- /metric --> / <!-- metric:season:hold_s33_table_sd_actual:.1f -->16.4<!-- /metric --> | <!-- metric:season:hold_s67_table_sd_sim:.1f -->17.4<!-- /metric --> / <!-- metric:season:hold_s67_table_sd_actual:.1f -->16.4<!-- /metric --> |

On dev the forecasts are over-confident at s0 and s33: the 80% coverage intervals (printed by `season_eval`, not gated) are 63.6-76.7 and 60.3-76.0, short of 80. At s67 the 80% ranges still fall short (66.9-79.1), but 50% coverage is nominal and the PIT SD and error-ratio intervals (0.278-0.326 and 0.996-1.325) reach their targets. These are percentile bootstrap intervals over 8 seasons, which are anti-conservative, so read them as if anything too narrow. The preseason title forecast is not distinguishable from the base rate (Brier skill interval −0.099 to 0.209). The two holdout seasons are not over-confident from s33 on, but two seasons settle nothing, and their spread is what `RATING_SIGMA` was chosen on.

### Against the market (offline)

Offline evaluation only: none of these odds or figures reach the app, its API or its UI. The market line is Bet365's pre-match 1X2 and over/under 2.5 odds from the MIT archive (it has no closing prices), the margin removed by Shin's method, chosen before the run (in the session notes; like the phase-4 holdout rule, it was committed together with the results, not before). Proportional normalization and the best price of about 17 books are checks; over/under takes the same two methods on its two prices. Positive model − market: the market has the lower loss. Intervals are the printed ones, not gated.

| | Dev | Holdout |
|---|---:|---:|
| Matches with odds | <!-- metric:market:dev_n:,d -->2,714<!-- /metric --> of <!-- metric:dev_n:,d -->2,717<!-- /metric --> | <!-- metric:market:hold_n:,d -->620<!-- /metric --> of <!-- metric:hold_n:,d -->647<!-- /metric --> (the archive's 2024-25 gap) |
| 1X2 log-loss, model / market | <!-- metric:market:dev_model_1x2_ll:.4f -->0.9993<!-- /metric --> / <!-- metric:market:dev_market_1x2_ll:.4f -->0.9888<!-- /metric --> | <!-- metric:market:hold_model_1x2_ll:.4f -->0.9754<!-- /metric --> / <!-- metric:market:hold_market_1x2_ll:.4f -->0.9425<!-- /metric --> |
| Model − market, 1X2 log-loss | <!-- metric:market:dev_diff_1x2_ll:+.4f -->+0.0106<!-- /metric --> [+0.00465, +0.01661], season t [+0.00588, +0.01522] | <!-- metric:market:hold_diff_1x2_ll:+.4f -->+0.0328<!-- /metric --> [+0.01744, +0.05018], season t [+0.00392, +0.06173] |
| Model − market, RPS | <!-- metric:market:dev_diff_rps:+.4f -->+0.0032<!-- /metric --> (bootstrap clear of 0) | <!-- metric:market:hold_diff_rps:+.4f -->+0.0111<!-- /metric --> (bootstrap clear of 0, one-df season t not) |
| Model − market, over/under 2.5 | <!-- metric:market:dev_diff_ou25_ll:+.4f -->+0.0063<!-- /metric --> (bootstrap clear of 0, season t not) | <!-- metric:market:hold_diff_ou25_ll:+.4f -->+0.0092<!-- /metric --> (not significant) |
| Bet365 margin, 1X2 | <!-- metric:market:dev_b365_margin:.1% -->6.4%<!-- /metric --> | <!-- metric:market:hold_b365_margin:.1% -->7.2%<!-- /metric --> |
| Best-price 1X2 books at or below 1 (shares printed by `market.rs`, not gated) | <!-- metric:market:dev_max_book_le1_1x2:,d -->580<!-- /metric --> (21.4%) | <!-- metric:market:hold_max_book_le1_1x2:,d -->26<!-- /metric --> (4.2%) |

The proportional and best-price lines agree with Shin's to 0.002 on every aspect. Where a best-price book sums to 1 or less (one book's stale price), Shin's method has no solution and the proportional line stands in; that share falls from 44.1% of matches in 2016-17 to 0.7% in 2025-26 (printed per season by `market.rs`). That noise is why Bet365, not the best price, is the primary line. The dev seasons chose the model's constants, which favours the model there, and it still loses.

Calibration, model / market / observed on the matches with odds: draws <!-- metric:market:dev_cal_draw_model:.1% -->24.6%<!-- /metric --> / <!-- metric:market:dev_cal_draw_market:.1% -->25.2%<!-- /metric --> / <!-- metric:market:dev_cal_draw_obs:.1% -->25.4%<!-- /metric --> on dev and <!-- metric:market:hold_cal_draw_model:.1% -->23.6%<!-- /metric --> / <!-- metric:market:hold_cal_draw_market:.1% -->23.9%<!-- /metric --> / <!-- metric:market:hold_cal_draw_obs:.1% -->26.6%<!-- /metric --> on the holdout; 3+ goals <!-- metric:market:dev_cal_over_model:.1% -->54.3%<!-- /metric --> / <!-- metric:market:dev_cal_over_market:.1% -->53.6%<!-- /metric --> / <!-- metric:market:dev_cal_over_obs:.1% -->53.2%<!-- /metric --> and <!-- metric:market:hold_cal_over_model:.1% -->56.5%<!-- /metric --> / <!-- metric:market:hold_cal_over_market:.1% -->54.3%<!-- /metric --> / <!-- metric:market:hold_cal_over_obs:.1% -->53.7%<!-- /metric -->. The market's draws are closer, and its P(3+ goals) is only slightly above the observed rate (dev +0.4, holdout +0.6 points), the model's well above it (+1.1, +2.8; differences of the figures above, not gated).

By matchday band (matchday = 1 + the two clubs' mean games before the match), model − market 1X2 log-loss and the band's share of the summed gap:

| Band | Dev matches | Dev gap | Dev share | Holdout matches | Holdout gap | Holdout share |
|---|---:|---:|---:|---:|---:|---:|
| 1-10 | <!-- metric:market:dev_md01_10_n:,d -->766<!-- /metric --> | <!-- metric:market:dev_md01_10_diff_1x2_ll:+.4f -->+0.0029<!-- /metric --> | <!-- metric:market:dev_md01_10_gap_share:.1% -->7.9%<!-- /metric --> | <!-- metric:market:hold_md01_10_n:,d -->171<!-- /metric --> | <!-- metric:market:hold_md01_10_diff_1x2_ll:+.4f -->+0.0401<!-- /metric --> | <!-- metric:market:hold_md01_10_gap_share:.1% -->33.7%<!-- /metric --> |
| 11-22 | <!-- metric:market:dev_md11_22_n:,d -->912<!-- /metric --> | <!-- metric:market:dev_md11_22_diff_1x2_ll:+.4f -->+0.0135<!-- /metric --> | <!-- metric:market:dev_md11_22_gap_share:.1% -->42.9%<!-- /metric --> | <!-- metric:market:hold_md11_22_n:,d -->211<!-- /metric --> | <!-- metric:market:hold_md11_22_diff_1x2_ll:+.4f -->+0.0059<!-- /metric --> | <!-- metric:market:hold_md11_22_gap_share:.1% -->6.1%<!-- /metric --> |
| 23 to the end | <!-- metric:market:dev_md23_end_n:,d -->1,036<!-- /metric --> | <!-- metric:market:dev_md23_end_diff_1x2_ll:+.4f -->+0.0136<!-- /metric --> | <!-- metric:market:dev_md23_end_gap_share:.1% -->49.2%<!-- /metric --> | <!-- metric:market:hold_md23_end_n:,d -->238<!-- /metric --> | <!-- metric:market:hold_md23_end_diff_1x2_ll:+.4f -->+0.0515<!-- /metric --> | <!-- metric:market:hold_md23_end_gap_share:.1% -->60.2%<!-- /metric --> |

The contrast that asks whether the gap opens after matchday 10 (mean gap after matchday 10 minus the mean gap in matchdays 1-10) separates neither set: dev <!-- metric:market:dev_md_late_minus_early_1x2_ll:+.4f -->+0.0106<!-- /metric --> [−0.00221, +0.02350] (season t [−0.00099, +0.02221]), holdout <!-- metric:market:hold_md_late_minus_early_1x2_ll:+.4f -->-0.0100<!-- /metric --> [−0.04269, +0.02268].

Forecast encompassing: the maximum-likelihood weight of the model in (1 − w) · market + w · model is <!-- metric:market:dev_pool_w:.3f -->0.073<!-- /metric --> on dev (season-cluster bootstrap [0.000, 0.208]; the seasons' own w run from 0 to 0.48), an in-sample gain of 0.00007. At that weight the pool is worse than the market alone on the holdout by <!-- metric:market:hold_pool_diff_1x2_ll:+.4f -->+0.0012<!-- /metric --> [+0.00012, +0.00228]. No information the market lacks shows up out of sample; with these sample sizes that does not show the model carries none.

### Live 2026-27

On 2026-10-19, 63 of 306 fixtures were played. The tracker had scored 55 frozen forecasts, every one made by the `2026-08` model: log-loss −0.054 against the base rates, 95% interval −0.123 to +0.015, matches treated as independent (`GET /api/accuracy` that day; not gated). `2026-10b` has no scored forecast yet.

Against the market, <!-- metric:market:live_n -->19<!-- /metric --> played matches have both a frozen forecast and Bet365 odds (all `2026-08` forecasts; the archive's 2026-27 rows ended on 2026-08-31): model <!-- metric:market:live_model_1x2_ll:.4f -->1.0014<!-- /metric -->, market <!-- metric:market:live_market_1x2_ll:.4f -->0.9981<!-- /metric -->, model − market <!-- metric:market:live_diff_1x2_ll:+.4f -->+0.0033<!-- /metric --> (normal interval [−0.09021, +0.09680]). Each forecast was frozen <!-- metric:market:live_age_days_min:d -->0<!-- /metric --> to <!-- metric:market:live_age_days_max:d -->11<!-- /metric --> days (median <!-- metric:market:live_age_days_median:d -->4<!-- /metric -->) before the match date the odds row carries, so it may have known less than the pre-match price did, which handicaps the model. <!-- metric:market:live_no_forecast:d -->8<!-- /metric --> odds rows have no frozen forecast (round 1, played before the first log run) and <!-- metric:market:live_no_odds:d -->36<!-- /metric --> scored forecasts have no odds row. Nothing to conclude yet.

## Negative results

- **The October 2026 tuning is not significant on the holdout.** Raising in-season K from 20 to 60 with `D_DIV` 1600 to 1200, and adding the underdog lift, improved every aspect on dev (1X2 −0.0077 [−0.0122, −0.0031]), but on the holdout only both-teams-score improved significantly (−0.0044 [−0.0081, −0.0008]); 1X2 moved −0.0066 [−0.0170, +0.0043]. Measured apart, the K and `D_DIV` change carries the 1X2 gain and makes holdout both-teams-score worse; the lift helps only both-teams-score. (`examples/compare.rs`; not gated.)
- **The phase-4 match sweep shipped nothing.** 202 dev settings of goal level, `BASE`, ρ and the blend weights: the best beat production by −0.0006 [−0.0015, +0.0003], and chosen by nested leave-one-season-out selection it scored +0.0002. A goal scale or `BASE` 1.30 closes the dev goal gap but costs both-teams-score and does not survive that selection; a fixed ρ near −0.10 would bring dev draws to 25.3% (observed <!-- metric:dev_cal_draw_obs:.1% -->25.4%<!-- /metric -->) for a 1X2 change within the noise. No setting gives both-teams-score skill. (One-off sweep, not gated.)
- **The whole-model season shock was vetoed.** Putting the 75-point shock on the whole blend brought dev to nominal coverage and improved pooled position RPS in 7 of 8 seasons, but its points CRPS gain spanned zero and it was the pick of 47+ settings. The holdout rule (written in the session notes before the run, committed after it) said to adopt it unless pooled points CRPS or position RPS got worse; both did (points CRPS +0.064, better in 0 of 2 seasons; position RPS +0.00039, tiny and seed-sensitive). Production keeps the Elo shock. Hot in-season updates and weekly drift were tried as well and not shipped. (`season_eval` and `compare --season`; not gated.)
- **The market beats the model** in both periods, and mixing the model in does not help out of sample (above).
- **An unsourced rationale.** The tuning rested on the claim that most of the gap to closing odds opens after matchday 10. Nothing in the repository sourced it, and the market benchmark neither confirms nor refutes it: its early-against-late contrast separates neither set, and it uses pre-match, not closing, odds.
- **Model selection.** In the arena (`examples/arena.rs`, frozen-season, not gated) the production blend wins neither held-out season, though each season's leader does worse than it on the other; a walk-forward logistic stacker edges it on the test season and trails it on validation, and gradient boosting, random forests and neural nets (a side experiment, not re-run on the corrected history) scored behind it on 2025-26: results-only features leave machine learning little room.
- **Tried and rejected** (do not re-run without a new idea): promoted-club priors, regressing or widening preseason ratings, weekly in-season Dixon-Coles and pi refits, early-season K schedules, phase-dependent blend weights, log, linear or no margin multipliers, goal-difference Elo, geometric blending, model-driven goal totals, pi-rating learning-rate changes, and a closeness-dependent draw boost (won on dev, failed the holdout).

## Known limitations

- **Draws are under-predicted** (dev <!-- metric:dev_cal_draw_pred:.1% -->24.6%<!-- /metric --> against <!-- metric:dev_cal_draw_obs:.1% -->25.4%<!-- /metric -->, holdout <!-- metric:hold_cal_draw_pred:.1% -->23.6%<!-- /metric --> against <!-- metric:hold_cal_draw_obs:.1% -->26.1%<!-- /metric -->), and the missing draws go to away wins.
- **Goals are over-predicted**, more on the holdout (<!-- metric:hold_goals_pred:.3f -->2.979<!-- /metric --> against <!-- metric:hold_goals_obs:.3f -->2.802<!-- /metric -->) than on dev (<!-- metric:dev_goals_pred:.3f -->2.865<!-- /metric --> against <!-- metric:dev_goals_obs:.3f -->2.816<!-- /metric -->). The goal level is an open item for the live season.
- **Both teams score has no skill**: goals are split too unevenly between the sides, a structural issue rather than a constant to tune.
- **Season ranges are over-confident on dev** (80% ranges cover about 70% of clubs) **but not on the holdout** from a third of the season on; two holdout seasons cannot settle which holds, and their spread chose `RATING_SIGMA`.
- **The market beats the model**, and its odds here are pre-match, not closing: the gap to the closing line is probably wider. Closing odds (football-data.co.uk) are a follow-up that needs network access to www.football-data.co.uk.
- **The live sample is small**: 55 scored forecasts, all from an older model, and 19 with odds.
- **Tie-break assumptions.** Head-to-head is applied once per block of clubs level on points, never recursively, which the regulation does not specify. The table during the season applies head-to-head within a level block only once every pair in it has played both meetings. TFF's regulation, as the press reports its text (tff.org was not reachable from the development environment), sets the season-end order; press sources differ on the table during the season, some saying head-to-head counts once the clubs' meetings are complete, others that the weekly table uses overall goal difference until the season ends. The site follows the first reading, which agrees with the season-end rule, so this is an assumption too.
- **Promoted clubs** are separated only by their Elo rating, and Dixon-Coles and pi-ratings stay frozen through the season.
- **Results only**: no lineups, injuries, transfers or xG.

## Ethical considerations

- **Gambling.** Forecasts of football matches can be read as betting tips. This model is not one: it loses to bookmaker prices, the app shows no bookmaker odds or market figures, and the market benchmark exists only to measure the model offline. The app's bookmaker features (a licensed operator's bulletin fetch, a model-against-bookmaker gap and a daily coupon) were removed on 2026-10-12 at the owner's instruction ([CLAUDE.md, "No licensed betting sources"](../CLAUDE.md#no-licensed-betting-sources)).
- **Data.** Match results only, no personal data. Third-party data keep their licences (`THIRD_PARTY_NOTICES.md`).

## How to reproduce

```bash
cargo test                                        # unit tests and tests/calibration.rs
EVAL_HOLDOUT=1 EVAL_METRICS=data/metrics/backtest.json cargo run --release --example backtest
EVAL_HOLDOUT=1 SEASON_METRICS=data/metrics/season.json cargo run --release --example season_eval
# Market benchmark: the odds come over the network and are never committed.
python3 scripts/fetch_fixtures.py                 # first, so the live join sees every played fixture
python3 scripts/fetch_odds.py
EVAL_HOLDOUT=1 EVAL_DUMP=/tmp/prod.csv cargo run --release --example backtest
MARKET_METRICS=data/metrics/market.json cargo run --release --example market -- /tmp/prod.csv
# Paired comparisons of two backtest or season dumps.
cargo run --release --example compare -- A.csv B.csv
cargo run --release --example compare -- --season A.csv B.csv
# Every quoted figure against data/metrics/.
python3 scripts/check_metrics.py docs
```

**Numbers gate.** CI reruns the backtest and the season evaluation and requires `data/metrics/backtest.json` and `season.json` to match exactly; it cannot rerun the market benchmark, so `market.json` is regenerated by hand. `check_metrics.py docs` then checks every marker in README.md, README.tr.md, CLAUDE.md, this card and its Turkish version (`docs/model-card.tr.md`) against the committed files. A change that moves a match forecast also bumps the model label (`src/model_version.rs`); any change that moves a forecast regenerates the metrics files and the figures quoted here. Details: [CLAUDE.md, "Numbers gate"](../CLAUDE.md#walk-forward-backtest-examplesbacktestrs) and ["Forecast accuracy"](../CLAUDE.md#forecast-accuracy-srcaccuracyrs-exampleslog_forecastrs).
