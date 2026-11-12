# Pre-registration: what the 2026-27 season will be used to test

Registered 12 November 2026, with 90 of the season's 306 results in the repository (matchdays 1 to 10).

The README and the model card say in several places that the live 2026-27 season is the clean test of this model. A season is a clean test only if what is tested, how it is scored and what follows from each outcome are fixed before the results are looked at. Twice already a rule in this project was written down before a run but committed only together with its result (the phase-4 holdout rule and the market benchmark's choice of Shin's method). This file is committed on its own, before any of the analyses below has been run on a 2026-27 result, so the git history shows the order.

## What has and has not been computed

As far as the repository records, and in the session that wrote this file:

- **Computed and published:** the shipped model's own live record (`/api/accuracy`: its frozen forecasts against results, 82 scored matches in the committed log) and the market benchmark's first 19 live matches (`data/metrics/market.json`).
- **Not computed:** any alternative setting priced on 2026-27 matches; the model's expected goals, draw rate or both-teams-score rate against what the season produced; any simulated season scored against the 2026-27 table.

The site's own table shows every club's goals, so the season's scoring rate has been visible to anyone who looked. The candidates below and their settings were all named in October 2026 from the seasons 2016-17 to 2025-26 and are not changed here.

## What is fixed

- **Code.** T1 to T4 run on the code of the commit that adds this file, checked out on its own. `2026-10b` and "production" mean that code with its constants. Only these changes are allowed, made together in one commit that touches nothing else:
  1. the 2026-27 rows appended to `data/superlig_results.csv`, with what `scripts/fetch_history.py` needs to write them;
  2. any 2026-27 award added to the lists that name awards (`EXCLUDED_AWARDS` in the script, `NOT_IN_CSV` in `src/history.rs`, `AWARDS_NOT_IN_CSV` in `examples/season_eval.rs`);
  3. one entry that gives `season_eval` three relegation places for 2026-27 (`data::RELEGATION_SPOTS`). It reads a season's relegated clubs from the next season's club list, which for the latest season is the same 18 clubs.

  The history is a model input, so the fingerprint test (`the_label_names_the_shipped_model_inputs`) fails on that commit. It is left failing for the analysis: 2026-27 is priced walk-forward, from earlier results only.
- **Data.** The calendar (`data/fixtures_2026_27.json`) at the first commit on or after 23 May 2027, the last scheduled matchday, in which all 306 fixtures have a result. If there is none by 30 June 2027, the calendar as committed on that date, with the unplayed fixtures left out and listed. A later correction does not re-run the analysis.
- **Awards.** A match played to the final whistle and later awarded counts with its score on the pitch in the match-level scores (taken from the calendar's git history if the award replaced it) and with the official score in the table. A match abandoned and later resumed counts as played. A match abandoned or never started and then awarded is left out of the match-level scores and counted in the table. Each such match is listed in the write-up.
- **The final table** is the one `league::rank_table` computes from those results, as `season_eval` computes every other season's. A points deduction or any other difference from the official order is listed, and if the official champion, top four or relegated clubs differ from the computed ones, both are reported.
- **Timing.** One analysis, after that calendar is committed. No candidate is run on 2026-27 matches before then. The shipped model's own record stays public throughout, and `data/metrics/market.json` may be regenerated as usual; neither is a look at a candidate.
- **Two records of production.** T1 to T3 score the backtest's walk-forward replay of production, which prices every match from the results before its date. T5 and the scores of the published season forecasts use what was actually logged at the time: `data/forecast_log.json`, and `data/season_log.json`, which the change set this file opens adds. Those two read the logs as committed on the analysis date, with the tools of that date. The two records are reported separately and not mixed.
- **Everything is reported.** Each item below is written up in the model card whatever it shows.

## T1. The October 2026 tuning (the one test with a decision)

- **A:** the model the tuning started from, defined by the knobs `EVAL_K=20 EVAL_DDIV=1600 EVAL_LIFT=0`. It is not the model that logged the August forecasts, which rated clubs from a ClubElo prior. **B:** production.
- **Runs:** `EVAL_SEASONS=2027 EVAL_K=20 EVAL_DDIV=1600 EVAL_LIFT=0 EVAL_DUMP=A.csv cargo run --release --example backtest`, the same without the three knobs for `B.csv`, then `cargo run --release --example compare -- A.csv B.csv --resamples 10000 --seed 12345`. `compare` files a season outside its holdout pair under `dev`, so the line to read is headed `dev seasons 2026-27..2026-27`.
- **Score:** `1x2_ll`, the row's B − A with its 95% date-block bootstrap interval as printed.
- **Rule:** the tuning is shipped, so the default is to keep it. It is reverted for 2027-28, both halves together (`ELO_K` 20, `D_DIV` 1600, `UNDERDOG_LIFT` 0), only if both printed endpoints of that interval are above +0.00000: only if production is distinguishably worse than the model it replaced. Any other outcome keeps it.
- **Also reported, no decision:** the other four rows (`rps`, `ou25_ll`, `btts_ll`, `score_ll`), and the two halves apart as in October (`EVAL_LIFT=0` against production for the lift; A against `EVAL_LIFT=0` for the rating speed).

## Reported without a decision

Fixed in advance so that they cannot be chosen after the fact. Nothing ships on their 2026-27 result alone, for the reason the next section gives.

- **T2. The goal level.** Production (A) against `EVAL_GOALS=0.98` (B), run and compared as T1. Rows read: `score_ll`, with `1x2_ll` and `btts_ll`.
- **T3. Draws.** Production (A) against `EVAL_RHO=-0.10` (B), the same way. Row read: `1x2_ll`. With it, the forecast and observed draw rate of each run, from the backtest's summary.
- **T4. Season spread.** `EVAL_SEASONS=2027 SEASON_SIMS=5000 SEASON_SEED=12345 SEASON_DUMP=A.csv cargo run --release --example season_eval` for production, the same with `SEASON_SHOCK=blend` for B, then `compare --season A.csv B.csv`. Read: B − A of points CRPS and position RPS, pooled over the three snapshots and per snapshot. These are point estimates. With one season `compare` has no season-cluster interval to give, and the bootstrap interval it prints has zero width and is not an interval.
- **T5. The market.** The live section of `examples/market.rs` as it stands on the analysis date, run on production's `EVAL_HOLDOUT=1` dump: the frozen forecasts against the Bet365 pre-match price, margin removed with Shin's method, `1x2_ll`, on every match with both. Reported with its counts (`live_n`, `live_no_forecast`, `live_no_odds`, `live_unplayed`) and with the two things that weaken it: the forecasts come from three model labels, and each was frozen some days before the price it is compared with.
- **Calibration in the large.** From production's backtest summary for the season: mean expected goals, forecast draw rate, P(3+ goals) and P(both score) against what happened.
- **The published season forecasts.** For each snapshot of `data/season_log.json` not marked `reconstructed`: the Brier scores of the title, top-four and relegation chances and the root mean square error of expected points, against the final table.

## What one more season can show

A league-wide setting moves a whole season's scores together, so the unit of evidence is the season, not the match. On the ten seasons 2016-17 to 2025-26, each season's mean B − A was (a one-off script over the runs' `EVAL_DUMP` files; `compare` prints the pooled figures, not these):

| Comparison | Mean of the ten seasons | SD across seasons | Range | B better in |
| --- | --- | --- | --- | --- |
| T1, `1x2_ll`: production − before the tuning | −0.0073 | 0.0081 | −0.0181 to +0.0018 | 7 of 10 |
| T2, `score_ll`: goal scale 0.98 − production | −0.0009 | 0.0030 | −0.0060 to +0.0045 | 7 of 10 |
| T3, `1x2_ll`: ρ −0.10 − production | −0.0004 | 0.0010 | −0.0019 to +0.0009 | 5 of 10 |

So one season's difference is a single draw from a spread at least as wide as the effect:

- **T1.** The date-block interval on about 306 matches will be roughly ±0.015. The revert bar therefore sits nearly three season-SDs above the ten-season mean, so it trips on a tuning that has gone clearly wrong and not on an ordinary bad season. The live season cannot confirm a gain of the size seen before; it adds one season to the count.
- **T2.** The holdout's gain from the goal scale (−0.0029 pooled over 2024-25 and 2025-26, the figure behind "it improves exact scores there" in the README) was 2025-26 alone: −0.0060 there, −0.0001 in 2024-25. A season's sign follows its scoring level. `compare`'s date-block interval treats that level as fixed, so on a single low-scoring season it would clear zero without the long-run goal level being wrong. That is why T2 carries no decision.
- **T3.** No power.
- **T4.** On the development seasons the season-to-season SD of the points-CRPS difference was about 0.08 against a pooled difference of −0.03.

A setting of this kind changes when the season-cluster interval over every scored season, 2016-17 onwards, excludes zero. For T2 and T3 the ten seasons so far are nowhere near that, and one more cannot bring it about. A result that does not reach a bar is not evidence that the effect is absent. It leaves the model as it is and the question where the model card has it.

## Anything else is exploratory

Any other analysis of 2026-27 results, including a different candidate, score, subset or interval for the questions above, is exploratory. It is run only after the registered analysis and is labelled as exploratory wherever it is quoted. It cannot change the shipped model on its own; it can only nominate a candidate for a registered test on later seasons.
