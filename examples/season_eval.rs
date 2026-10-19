//! Season-level evaluation of the production forecast.
//!
//! The walk-forward backtest (`examples/backtest.rs`) scores single matches.
//! This scores what the site headlines: finishing positions, the title, the
//! top four, relegation and final points.
//!
//! For every scored season and three snapshots of it (nothing played, a third
//! and two thirds of its fixtures played) it rebuilds the `World` production
//! would have held on that date and replays the rest of the season
//! `SEASON_SIMS` times with `World::simulate_one_dispersed`, the production
//! simulator (`sim::Dispersion::PRODUCTION`: a mean-preserving
//! `data::RATING_SIGMA` shock on the whole model drawn per trial, the
//! shipped ensemble) unless the dispersion knobs below say otherwise:
//!   - clubs and fixtures are the season's own, from the embedded history;
//!   - results dated before the snapshot are `played`, the rest simulated;
//!   - ratings are production's: `sim::preseason_ratings_with` over every
//!     earlier result (`PRESEASON_K`, unseen clubs at `NEW_CLUB_ELO`), then
//!     `World::refresh_ratings` walks the played results with `ELO_K`;
//!   - Dixon-Coles (half-life 1460 days, as `fit_dc`) and pi-ratings are fit
//!     on every earlier result and frozen for the season, as production
//!     freezes them, and blended with `ensemble_weights()` (`ENSEMBLE_WEIGHTS`).
//!
//! A snapshot cuts on a date boundary: with `k` = a third (two thirds) of the
//! season's fixtures, everything dated before fixture `k`'s date is played.
//! Fixtures are taken in date order, so a postponed match counts when it was
//! actually played. The history has no rounds; the `World` numbers them by
//! cutting the date-ordered season into runs of clubs / 2 fixtures (a club
//! rests each round when the league is odd), and only `SEASON_DRIFT` reads
//! them, as the points its random-walk steps fall on. Kick-off order, date
//! first, is date order either way.
//!
//! Outcomes are the official final tables, ranked by `league::rank_table`.
//! Results awarded off the pitch count, as they did in the standings: the
//! 2022-23 earthquake walkovers (CSV rows that `history::load_history` drops
//! from the training data) and the 2024-25 Galatasaray v Adana Demirspor
//! award (`AWARDS_NOT_IN_CSV`). An award dated before a snapshot is a played
//! result there, as it stood in the table, and the in-season Elo walk takes
//! it like any other result, as production's `World` would take an awarded
//! score from the live calendar (four of the walkovers and the 2024-25
//! award fall before the two-thirds cut); one dated after is simulated, as
//! production would have simulated it. The training history behind the
//! preseason ratings, Dixon-Coles and pi leaves them out, as production's
//! does. Nothing could have foreseen the withdrawals, so 2022-23 scores
//! worse this way: the forecast plays out 29 fixtures that were never
//! contested. `SEASON_AWARDS=drop` scores played football only, with the
//! awarded fixtures in neither the calendar nor the table (as
//! `examples/arena.rs` compares played-fixture points); the title, top-four
//! and bottom-four clubs are the same either way.
//!
//! Relegation is the bottom `R` places, `R` being the season's relegation
//! places under its rules at kick-off. `R` comes from the data: the season's
//! clubs absent from the next season of `data/superlig_results.csv`
//! (`data::clubs()` after 2025-26) must be exactly the bottom `R` of the
//! table, except in the seasons of `RELEGATION_EXCEPTIONS`, where fewer went
//! down than the rules said.
//!
//! Scores, one row per club per season and snapshot, pooled as means:
//!   pos_rps       ranked probability score over finishing positions (0 is
//!                 perfect, 1 the worst possible)
//!   *_brier/_ll   Brier score and log-loss of the title, top-four and
//!                 relegation events; log-loss smooths a Monte Carlo share to
//!                 (hits + 0.5) / (n + 1) so an event never seen in n seasons
//!                 costs a finite amount
//!   *_bss         Brier skill against the base rate (places / clubs)
//!   p_champion    probability given to the eventual champion, per season
//!   pit_mean/sd   randomized PIT of final points (uniform: 0.500, 0.289);
//!                 the printout adds its share in each fifth (uniform: 20%)
//!   cov50/80_pct  central-interval coverage of final points, randomized
//!                 (PIT inside the central 50% / 80%), which a calibrated
//!                 forecast meets exactly; *_closed_pct counts the closed
//!                 integer interval between the simulated quantiles, which
//!                 over-covers by the probability at its two ends
//!   pts_sd_pred   forecast SD of final points, root mean square over clubs
//!   pts_rmse      root mean square error of the forecast's mean points
//!   pts_err_ratio pts_rmse / pts_sd_pred: 1 when the spread matches the
//!                 errors, above 1 when the forecast is too narrow
//!   pts_crps      continuous ranked probability score of final points, in
//!                 points: Σ_k (F(k) − 1{y ≤ k})² over integers k, F the
//!                 simulated CDF of the club's points and y what it got
//!                 (0 for a certain, right forecast; for a point forecast,
//!                 its absolute error). A proper score of the whole points
//!                 distribution, so it rewards spread only where the errors
//!                 need it, which coverage and PIT cannot tell
//!   table_sd_*    SD of final points across a table, simulated (mean over
//!                 trials) and actual: the spread `examples/arena.rs` tunes
//!                 `RATING_SIGMA` on
//! Pooled figures carry 95% season-cluster bootstrap intervals: whole seasons
//! are resampled, since clubs in one table are not independent. With the two
//! holdout seasons the interval only shows how far apart they are.
//!
//! Knobs:
//!   SEASON_SIMS=2000        simulated seasons per snapshot
//!   SEASON_SEED=12345       base seed (production's DEFAULT_SEED)
//!   SEASON_BOOT=2000        bootstrap resamples
//!   EVAL_HOLDOUT=1          also score 2024-25 and 2025-26 (as the backtest)
//!   EVAL_SEASONS=2017,2018  season end years to score [dev set]
//!   SEASON_AWARDS=drop      leave awarded results out of calendar and table
//!                           [count: the official tables]
//!   SEASON_DUMP=path.csv    write one row per club, season and snapshot
//!   SEASON_METRICS=path.json write the pooled point estimates as JSON
//!                           ({set}_{snapshot}_{metric}, sorted keys, each
//!                           value as printed; counts are integers)
//! Dispersion knobs (`sim::Dispersion`; the defaults are production's, and
//! with all four at their defaults the run is production's draw for draw):
//!   SEASON_SIGMA=75         SD of each club's per-season strength offset,
//!                           in Elo points [data::RATING_SIGMA]
//!   SEASON_SHOCK=blend|elo  where offsets act: on the blended, lifted λ
//!                           pair as an Elo gap acts on the Elo component,
//!                           mean-preserving (production), or added to the
//!                           Elo ratings [blend]
//!   SEASON_HOT_K=0          hot simulation: K of the Elo update after every
//!                           simulated result (production's rule; 0 is cold)
//!   SEASON_DRIFT=0          SD, in Elo points, of each club's random-walk
//!                           step before every round still to be played
//!
//! Run: cargo run --release --example season_eval
//! (seconds: dev and holdout at the defaults take about 5 s on 4 cores)

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use rayon::prelude::*;
use superlig_sim::data;
use superlig_sim::dixoncoles;
use superlig_sim::history::{self, HistoricalMatch, TeamIndex};
use superlig_sim::league::{self, TeamRecord};
use superlig_sim::piratings::PiRatings;
use superlig_sim::sim::{
    self, Dispersion, Ensemble, FixtureDate, LeagueFixture, SeasonResult, ShockOn, World,
};

const DEV: std::ops::RangeInclusive<i32> = 2017..=2024;
const HOLDOUT: [i32; 2] = [2025, 2026];

/// Snapshot labels and the share of a season's fixtures played by then.
const SNAPSHOTS: [(&str, f64); 3] = [("s0", 0.0), ("s33", 1.0 / 3.0), ("s67", 2.0 / 3.0)];

/// The Dixon-Coles half-life `fit_dc` ships with, in days.
const DC_HALF_LIFE: f64 = 1460.0;

const HISTORY_CSV: &str = include_str!("../data/superlig_results.csv");

/// Official results kept out of the CSV at source (`fetch_history.py`'s
/// `EXCLUDED_AWARDS`, `NOT_IN_CSV` in `src/history.rs`): not football, so not
/// training data, but they counted in the final table. (date, home, away,
/// home goals, away goals.)
const AWARDS_NOT_IN_CSV: &[(&str, &str, &str, u16, u16)] =
    &[("2025-02-09", "Galatasaray", "Ad. Demirspor", 3, 0)];

/// Seasons in which fewer clubs went down than the rules at kick-off set
/// relegation places for: (season end year, places, why). The forecast was
/// made under those rules, so the bottom `places` is still the event scored.
const RELEGATION_EXCEPTIONS: &[(i32, usize, &str)] = &[
    (
        2020,
        3,
        "relegation was cancelled over COVID-19 and the league grew to 21",
    ),
    (
        2023,
        4,
        "Hatayspor and Gaziantep, in the bottom four after withdrawing over \
         the 6 February earthquake, kept their places",
    ),
];

fn env_num<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .map(|v| {
            v.parse()
                .unwrap_or_else(|_| panic!("{key}={v} is not a number"))
        })
        .unwrap_or(default)
}

/// August boundary: keeps the pandemic-delayed July 2020 matches in 2019-20.
fn season_end_year(date: NaiveDate) -> i32 {
    if date.month() >= 8 {
        date.year() + 1
    } else {
        date.year()
    }
}

fn season_name(year: i32) -> String {
    format!("{}-{:02}", year - 1, year % 100)
}

// ---------------------------------------------------------------- scoring ---

/// Ranked probability score of a forecast over ordered outcomes (`probs`
/// sums to 1) when outcome `actual` happened, averaged over the `n - 1`
/// thresholds so it runs from 0 (certain and right) to 1.
fn rps(probs: &[f64], actual: usize) -> f64 {
    let n = probs.len();
    let (mut forecast, mut total) = (0.0, 0.0);
    for (k, p) in probs.iter().take(n - 1).enumerate() {
        forecast += p;
        let observed = if actual <= k { 1.0 } else { 0.0 };
        total += (forecast - observed) * (forecast - observed);
    }
    total / (n - 1) as f64
}

fn brier(p: f64, happened: bool) -> f64 {
    let y = if happened { 1.0 } else { 0.0 };
    (p - y) * (p - y)
}

/// Binary log-loss; `p` must be strictly between 0 and 1 (see `smoothed`).
fn log_loss(p: f64, happened: bool) -> f64 {
    -(if happened { p } else { 1.0 - p }).ln()
}

/// An event's probability from `hits` of `n` simulated seasons, pulled half
/// a season off 0 and 1 so log-loss stays finite.
fn smoothed(hits: usize, n: usize) -> f64 {
    (hits as f64 + 0.5) / (n as f64 + 1.0)
}

/// Randomized probability integral transform of an integer outcome `x`
/// against a forecast sample (`sorted` ascending): P(X < x) + v·P(X = x),
/// `v` uniform on [0, 1). Uniform on [0, 1] when the forecast is calibrated,
/// discrete or not.
fn randomized_pit(sorted: &[i64], x: i64, v: f64) -> f64 {
    let below = sorted.partition_point(|&p| p < x);
    let at_most = sorted.partition_point(|&p| p <= x);
    let n = sorted.len() as f64;
    (below as f64 + v * (at_most - below) as f64) / n
}

/// Whether a PIT value lies in the central `share` of the forecast: the
/// randomized central interval, which covers exactly `share` of outcomes
/// when the forecast is calibrated.
fn in_central(pit: f64, share: f64) -> bool {
    let tail = (1.0 - share) / 2.0;
    (tail..=1.0 - tail).contains(&pit)
}

/// Sample quantile, in the convention of the site's points thresholds
/// (`models.rs`): the value at rank round((n - 1)·q).
fn quantile(sorted: &[i64], q: f64) -> i64 {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

/// The closed integer interval between the sample's central-`share`
/// quantiles.
fn central_interval(sorted: &[i64], share: f64) -> (i64, i64) {
    let tail = (1.0 - share) / 2.0;
    (quantile(sorted, tail), quantile(sorted, 1.0 - tail))
}

/// Continuous ranked probability score of an integer outcome `y` against a
/// forecast sample (`sorted` ascending), in its own units:
/// Σ_k (F(k) − 1{y ≤ k})² over the integers, F the sample's CDF. Outside
/// the span of the sample and `y` every term is 0.
fn crps(sorted: &[i64], y: i64) -> f64 {
    let n = sorted.len() as f64;
    let (lo, hi) = (sorted[0].min(y), sorted[sorted.len() - 1].max(y));
    let mut at_most = 0;
    let mut total = 0.0;
    for k in lo..hi {
        while at_most < sorted.len() && sorted[at_most] <= k {
            at_most += 1;
        }
        let gap = at_most as f64 / n - if y <= k { 1.0 } else { 0.0 };
        total += gap * gap;
    }
    total
}

/// Mean and population SD.
fn mean_sd(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    (mean, var.sqrt())
}

/// Value at rank round((n - 1)·q) of an unsorted sample.
fn percentile(values: &mut [f64], q: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * q).round() as usize]
}

// ------------------------------------------------------------------- data ---

/// Every row of the embedded history, awards included, plus
/// `AWARDS_NOT_IN_CSV`, in date order: the record final tables are made of.
/// `history::load_history` drops the earthquake walkovers, which is right for
/// training and wrong for standings.
fn official_rows() -> Vec<HistoricalMatch> {
    #[derive(serde::Deserialize)]
    struct Row {
        date: String,
        home_team: String,
        away_team: String,
        home_score: u16,
        away_score: u16,
    }
    let date = |s: &str| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_else(|_| panic!("history date {s:?}"))
    };
    let mut rows: Vec<HistoricalMatch> = csv::Reader::from_reader(HISTORY_CSV.as_bytes())
        .deserialize::<Row>()
        .map(|r| {
            let r = r.expect("history row");
            HistoricalMatch {
                date: date(&r.date),
                home_team: r.home_team,
                away_team: r.away_team,
                home_score: r.home_score,
                away_score: r.away_score,
                neutral: false,
            }
        })
        .filter(|m| m.date.year() >= history::CUTOFF_YEAR)
        .collect();
    for &(d, home, away, hs, as_) in AWARDS_NOT_IN_CSV {
        rows.push(HistoricalMatch {
            date: date(d),
            home_team: home.to_string(),
            away_team: away.to_string(),
            home_score: hs,
            away_score: as_,
            neutral: false,
        });
    }
    rows.sort_by_key(|m| m.date);
    rows
}

/// The rows of `official` that the training history leaves out. Each must be
/// a 3-0 award; anything else means the two disagree about real results.
fn awards(official: &[HistoricalMatch], training: &[HistoricalMatch]) -> Vec<HistoricalMatch> {
    let key = |m: &HistoricalMatch| (m.date, m.home_team.clone(), m.away_team.clone());
    let trained: HashSet<_> = training.iter().map(key).collect();
    let extra: Vec<HistoricalMatch> = official
        .iter()
        .filter(|m| !trained.contains(&key(m)))
        .cloned()
        .collect();
    assert_eq!(
        official.len(),
        training.len() + extra.len(),
        "a training match is missing from the official record"
    );
    for m in &extra {
        assert!(
            matches!((m.home_score, m.away_score), (3, 0) | (0, 3)),
            "{} {} v {} {}-{} is left out of training but is no award",
            m.date,
            m.home_team,
            m.away_team,
            m.home_score,
            m.away_score
        );
    }
    extra
}

/// One finished season as the evaluator scores it.
struct Season {
    /// Season end year (2024 is 2023-24).
    year: i32,
    /// The results its table is made of, in date order.
    rows: Vec<HistoricalMatch>,
    /// Club names, sorted; indices into this are `World` indices.
    teams: Vec<String>,
    /// Final position by club index, 0 = champion.
    final_pos: Vec<usize>,
    final_pts: Vec<i64>,
    /// Relegation places under the rules at kick-off.
    relegation: usize,
    /// Clubs that actually went down.
    relegated: BTreeSet<String>,
}

fn clubs_of(rows: &[&HistoricalMatch]) -> BTreeSet<String> {
    rows.iter()
        .flat_map(|m| [m.home_team.clone(), m.away_team.clone()])
        .collect()
}

/// Final table of a set of results: records and finishing order.
fn final_table(teams: &[String], rows: &[HistoricalMatch]) -> (Vec<TeamRecord>, Vec<usize>) {
    let pos: HashMap<&str, usize> = teams
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), i))
        .collect();
    let mut records = vec![TeamRecord::default(); teams.len()];
    let mut results = HashMap::new();
    for m in rows {
        let (h, a) = (pos[m.home_team.as_str()], pos[m.away_team.as_str()]);
        let (hg, ag) = (i64::from(m.home_score), i64::from(m.away_score));
        league::apply_result(&mut records, h, a, hg, ag);
        let repeat = results.insert((h, a), (hg, ag));
        assert!(
            repeat.is_none(),
            "{} hosted {} twice",
            m.home_team,
            m.away_team
        );
    }
    // A tie no rule separates goes to a play-off; none has decided a final
    // table here, but keep the stand-in deterministic.
    let mut next = 0u64;
    let order = league::rank_table(&records, &results, &mut || {
        next += 1;
        next
    });
    (records, order)
}

/// Every season of `record` (`official_rows`, or the training history for
/// played football only), with its final table and relegation places (see
/// the file header for how those are derived and checked).
fn seasons(record: &[HistoricalMatch]) -> BTreeMap<i32, Season> {
    let mut by_year: BTreeMap<i32, Vec<&HistoricalMatch>> = BTreeMap::new();
    for m in record {
        by_year.entry(season_end_year(m.date)).or_default().push(m);
    }
    let current: BTreeSet<String> = data::clubs().iter().map(|c| c.to_string()).collect();
    let last = *by_year.keys().last().expect("history is not empty");
    let mut out = BTreeMap::new();
    for (&year, rows) in &by_year {
        let teams: Vec<String> = clubs_of(rows).into_iter().collect();
        let rows: Vec<HistoricalMatch> = rows.iter().map(|&m| m.clone()).collect();
        let (records, order) = final_table(&teams, &rows);
        let mut final_pos = vec![0; teams.len()];
        for (p, &c) in order.iter().enumerate() {
            final_pos[c] = p;
        }
        let next = match by_year.get(&(year + 1)) {
            Some(next_rows) => clubs_of(next_rows),
            // The record ends with the season before the current one, whose
            // clubs are `data::clubs()`.
            None if year == last => current.clone(),
            None => unreachable!("seasons are consecutive"),
        };
        let relegated: BTreeSet<String> = teams
            .iter()
            .filter(|t| !next.contains(*t))
            .cloned()
            .collect();
        let exception = RELEGATION_EXCEPTIONS.iter().find(|e| e.0 == year);
        let relegation = exception.map_or(relegated.len(), |e| e.1);
        let bottom: BTreeSet<String> = order[teams.len() - relegation..]
            .iter()
            .map(|&c| teams[c].clone())
            .collect();
        // Who went down must be the bottom of the table: anything else is a
        // name drifting between seasons, or a season needing an exception.
        if exception.is_some() {
            assert!(
                relegated.is_subset(&bottom) && relegated.len() < relegation,
                "{}: relegated {relegated:?}, bottom {relegation} {bottom:?}",
                season_name(year)
            );
        } else {
            assert_eq!(relegated, bottom, "{}: relegated clubs", season_name(year));
        }
        out.insert(
            year,
            Season {
                year,
                rows,
                final_pts: records.iter().map(|r| r.points).collect(),
                teams,
                final_pos,
                relegation,
                relegated,
            },
        );
    }
    out
}

/// The season's clubs plus the departed-club bucket, as production builds it.
fn index_for(teams: &[String]) -> TeamIndex {
    let mut idx_to_name = teams.to_vec();
    idx_to_name.push(history::OTHER_TEAM_NAME.to_string());
    TeamIndex {
        name_to_idx: idx_to_name
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect(),
        other_idx: idx_to_name.len() - 1,
        idx_to_name,
        league_names: teams.to_vec(),
    }
}

/// What production would hold at the start of `season`: preseason ratings
/// for its clubs and the ensemble fit on everything earlier.
fn preseason(season: &Season, training: &[HistoricalMatch]) -> (Vec<f64>, Ensemble) {
    let start = season.rows[0].date;
    let before: Vec<HistoricalMatch> = training
        .iter()
        .filter(|m| m.date < start)
        .cloned()
        .collect();
    let ratings = sim::preseason_ratings_with(&before, data::PRESEASON_K, data::NEW_CLUB_ELO);
    let elo = season
        .teams
        .iter()
        .map(|t| ratings.get(t).copied().unwrap_or(data::NEW_CLUB_ELO))
        .collect();
    let idx = index_for(&season.teams);
    let fits = history::prepare_fit_matches(&before, &idx, DC_HALF_LIFE, start);
    let dc = dixoncoles::fit(&fits, &idx, DC_HALF_LIFE, 300).expect("dixon-coles fit");
    let since = NaiveDate::from_ymd_opt(history::CUTOFF_YEAR, 1, 1).expect("valid date");
    let pi = PiRatings::compute(&before, &idx, since);
    let (w_elo, w_dc, w_pi) = superlig_sim::ensemble_weights();
    let ensemble = Ensemble {
        dc,
        pi,
        w_elo,
        w_dc,
        w_pi,
    };
    (elo, ensemble)
}

/// The first date not yet played when `share` of the season's fixtures
/// have been: everything dated earlier is played.
fn snapshot_cut(season: &Season, share: f64) -> NaiveDate {
    season.rows[(season.rows.len() as f64 * share) as usize].date
}

/// Production's `World` on `cut`: the season's calendar, results dated
/// before `cut`, ratings walked through them.
fn world_on(season: &Season, elo: &[f64], ensemble: Option<&Ensemble>, cut: NaiveDate) -> World {
    let idx: HashMap<String, usize> = season
        .teams
        .iter()
        .enumerate()
        .map(|(i, t)| (t.clone(), i))
        .collect();
    let mut fixtures = Vec::with_capacity(season.rows.len());
    let mut dates = Vec::with_capacity(season.rows.len());
    let mut played = HashMap::new();
    // No rounds in the history: runs of clubs / 2 fixtures in date order
    // stand in for them (see the file header). They never decrease in date
    // order, so `refresh_ratings`' walk is the date order it was.
    let per_round = (season.teams.len() / 2).max(1);
    for (k, m) in season.rows.iter().enumerate() {
        let (home, away) = (idx[&m.home_team], idx[&m.away_team]);
        fixtures.push(LeagueFixture {
            round: u8::try_from(k / per_round + 1).expect("fewer than 256 rounds"),
            home,
            away,
        });
        dates.push(FixtureDate {
            date: m.date.to_string(),
            kickoff: None,
        });
        if m.date < cut {
            played.insert((home, away), (m.home_score, m.away_score));
        }
    }
    let mut world = World {
        teams: season.teams.clone(),
        idx,
        elo: elo.to_vec(),
        elo_baseline: elo.to_vec(),
        fixtures,
        dates,
        played,
        ensemble: ensemble.cloned(),
        offset: vec![0.0; season.teams.len()],
    };
    world.refresh_ratings();
    world
}

// ------------------------------------------------------------- evaluation ---

/// A binary season event (title, top four, relegation) for one club.
struct Event {
    /// Monte Carlo share of simulated seasons.
    p: f64,
    /// `smoothed` share, for log-loss.
    p_ll: f64,
    happened: bool,
    /// Places / clubs: the no-skill reference for Brier skill.
    base_rate: f64,
}

impl Event {
    fn new(hits: usize, n_sims: usize, happened: bool, places: usize, clubs: usize) -> Self {
        Event {
            p: hits as f64 / n_sims as f64,
            p_ll: smoothed(hits, n_sims),
            happened,
            base_rate: places as f64 / clubs as f64,
        }
    }
}

/// One club's forecast at one snapshot, scored against its season.
struct Row {
    club: String,
    actual_pos: usize,
    actual_pts: i64,
    rps: f64,
    title: Event,
    top4: Event,
    rel: Event,
    pit: f64,
    closed50: bool,
    closed80: bool,
    mean_pts: f64,
    sd_pts: f64,
    /// `crps` of the club's final points.
    crps: f64,
}

/// One season at one snapshot.
struct Scored {
    year: i32,
    snapshot: usize,
    played: usize,
    relegation: usize,
    rows: Vec<Row>,
    /// SD of points across the table: mean over trials, and actual.
    table_sd_sim: f64,
    table_sd_actual: f64,
}

/// Production's seeding (`World::simulate`): trial `i` is seeded with
/// `seed + i·2654435761`.
fn simulate(world: &World, n_sims: usize, seed: u64, disp: &Dispersion) -> Vec<SeasonResult> {
    let nothing_pinned = HashMap::new();
    (0..n_sims)
        .into_par_iter()
        .map(|i| {
            let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(i as u64 * 2_654_435_761));
            world.simulate_one_dispersed(&mut rng, &nothing_pinned, disp)
        })
        .collect()
}

/// Score one snapshot's simulated seasons (`played` results were already
/// in) against how `season` ended.
fn score(
    season: &Season,
    snapshot: usize,
    played: usize,
    trials: &[SeasonResult],
    pit_seed: u64,
) -> Scored {
    let n = season.teams.len();
    let n_sims = trials.len();
    let mut positions = vec![vec![0usize; n]; n];
    let mut points: Vec<Vec<i64>> = vec![Vec::with_capacity(n_sims); n];
    let mut table_sd = 0.0;
    for t in trials {
        for (p, &c) in t.order.iter().enumerate() {
            positions[c][p] += 1;
        }
        for (c, r) in t.records.iter().enumerate() {
            points[c].push(r.points);
        }
        let pts: Vec<f64> = t.records.iter().map(|r| r.points as f64).collect();
        table_sd += mean_sd(&pts).1;
    }
    let actual: Vec<f64> = season.final_pts.iter().map(|&p| p as f64).collect();

    let mut v = SmallRng::seed_from_u64(pit_seed);
    let relegated_from = n - season.relegation;
    let rows = (0..n)
        .map(|c| {
            let counts = &positions[c];
            let probs: Vec<f64> = counts.iter().map(|&k| k as f64 / n_sims as f64).collect();
            let pos = season.final_pos[c];
            let x = season.final_pts[c];
            let pts = &mut points[c];
            pts.sort_unstable();
            let pit = randomized_pit(pts, x, v.gen());
            let covers = |share| {
                let (lo, hi) = central_interval(pts, share);
                (lo..=hi).contains(&x)
            };
            let as_f64: Vec<f64> = pts.iter().map(|&p| p as f64).collect();
            let (mean_pts, sd_pts) = mean_sd(&as_f64);
            let crps = crps(pts, x);
            Row {
                club: season.teams[c].clone(),
                actual_pos: pos,
                actual_pts: x,
                rps: rps(&probs, pos),
                title: Event::new(counts[0], n_sims, pos == 0, 1, n),
                top4: Event::new(
                    counts[..data::TOP_FOUR_PLACES].iter().sum(),
                    n_sims,
                    pos < data::TOP_FOUR_PLACES,
                    data::TOP_FOUR_PLACES,
                    n,
                ),
                rel: Event::new(
                    counts[relegated_from..].iter().sum(),
                    n_sims,
                    pos >= relegated_from,
                    season.relegation,
                    n,
                ),
                pit,
                closed50: covers(0.5),
                closed80: covers(0.8),
                mean_pts,
                sd_pts,
                crps,
            }
        })
        .collect();
    Scored {
        year: season.year,
        snapshot,
        played,
        relegation: season.relegation,
        rows,
        table_sd_sim: table_sd / n_sims as f64,
        table_sd_actual: mean_sd(&actual).1,
    }
}

/// Pooled metrics, in this order, with the digits they are printed to.
const METRICS: [(&str, usize); 23] = [
    ("pos_rps", 4),
    ("pts_crps", 3),
    ("title_brier", 4),
    ("title_ll", 4),
    ("title_bss", 3),
    ("p_champion", 3),
    ("top4_brier", 4),
    ("top4_ll", 4),
    ("top4_bss", 3),
    ("rel_brier", 4),
    ("rel_ll", 4),
    ("rel_bss", 3),
    ("pit_mean", 3),
    ("pit_sd", 3),
    ("cov50_pct", 1),
    ("cov80_pct", 1),
    ("cov50_closed_pct", 1),
    ("cov80_closed_pct", 1),
    ("pts_sd_pred", 2),
    ("pts_rmse", 2),
    ("pts_err_ratio", 3),
    ("table_sd_sim", 2),
    ("table_sd_actual", 2),
];

/// One value per entry of `METRICS`.
type Metrics = [f64; METRICS.len()];

/// The value of the metric named `key`.
fn get(m: &Metrics, key: &str) -> f64 {
    let i = METRICS.iter().position(|&(k, _)| k == key);
    m[i.unwrap_or_else(|| panic!("{key} is not in METRICS"))]
}

/// Every metric of `METRICS` over a set of scored seasons, pooled per club.
fn metrics(seasons: &[&Scored]) -> Metrics {
    let rows: Vec<&Row> = seasons.iter().flat_map(|s| &s.rows).collect();
    let n = rows.len() as f64;
    let mean = |f: &dyn Fn(&Row) -> f64| rows.iter().map(|r| f(r)).sum::<f64>() / n;
    let share = |f: &dyn Fn(&Row) -> bool| 100.0 * rows.iter().filter(|r| f(r)).count() as f64 / n;
    let event = |e: &dyn Fn(&Row) -> &Event| {
        let b: f64 = rows.iter().map(|r| brier(e(r).p, e(r).happened)).sum();
        let reference: f64 = rows
            .iter()
            .map(|r| brier(e(r).base_rate, e(r).happened))
            .sum();
        let ll: f64 = rows
            .iter()
            .map(|r| log_loss(e(r).p_ll, e(r).happened))
            .sum();
        (b / n, ll / n, 1.0 - b / reference)
    };
    let (title_b, title_ll, title_bss) = event(&|r| &r.title);
    let (top4_b, top4_ll, top4_bss) = event(&|r| &r.top4);
    let (rel_b, rel_ll, rel_bss) = event(&|r| &r.rel);
    let champions: Vec<f64> = rows
        .iter()
        .filter(|r| r.title.happened)
        .map(|r| r.title.p)
        .collect();
    let pits: Vec<f64> = rows.iter().map(|r| r.pit).collect();
    let (pit_mean, pit_sd) = mean_sd(&pits);
    let sd_pred = mean(&|r| r.sd_pts * r.sd_pts).sqrt();
    let rmse = mean(&|r| (r.actual_pts as f64 - r.mean_pts).powi(2)).sqrt();
    let k = seasons.len() as f64;
    [
        mean(&|r| r.rps),
        mean(&|r| r.crps),
        title_b,
        title_ll,
        title_bss,
        champions.iter().sum::<f64>() / champions.len() as f64,
        top4_b,
        top4_ll,
        top4_bss,
        rel_b,
        rel_ll,
        rel_bss,
        pit_mean,
        pit_sd,
        share(&|r| in_central(r.pit, 0.5)),
        share(&|r| in_central(r.pit, 0.8)),
        share(&|r| r.closed50),
        share(&|r| r.closed80),
        sd_pred,
        rmse,
        rmse / sd_pred,
        seasons.iter().map(|s| s.table_sd_sim).sum::<f64>() / k,
        seasons.iter().map(|s| s.table_sd_actual).sum::<f64>() / k,
    ]
}

/// PIT histogram in five equal bins, as percentages (uniform: 20 each).
fn pit_quintiles(seasons: &[&Scored]) -> [f64; 5] {
    let mut bins = [0usize; 5];
    let mut n = 0;
    for r in seasons.iter().flat_map(|s| &s.rows) {
        bins[((r.pit * 5.0) as usize).min(4)] += 1;
        n += 1;
    }
    bins.map(|b| 100.0 * b as f64 / n as f64)
}

/// 95% season-cluster bootstrap interval of every metric: draw as many
/// seasons as there are, with replacement, and pool their clubs.
fn bootstrap(seasons: &[&Scored], resamples: usize, seed: u64) -> Vec<(f64, f64)> {
    let mut rng = SmallRng::seed_from_u64(seed);
    let draws: Vec<Metrics> = (0..resamples)
        .map(|_| {
            let pick: Vec<&Scored> = (0..seasons.len())
                .map(|_| seasons[rng.gen_range(0..seasons.len())])
                .collect();
            metrics(&pick)
        })
        .collect();
    (0..METRICS.len())
        .map(|m| {
            let mut v: Vec<f64> = draws.iter().map(|d| d[m]).collect();
            (percentile(&mut v, 0.025), percentile(&mut v, 0.975))
        })
        .collect()
}

/// `x` for the metrics JSON, as printed to `digits` places: an integer at
/// 0 digits, a zero never signed, `NaN` as null.
fn json_number(x: f64, digits: usize) -> serde_json::Value {
    let printed = format!("{x:.digits$}");
    let printed = if printed.parse::<f64>() == Ok(0.0) {
        format!("{:.digits$}", 0.0)
    } else {
        printed
    };
    serde_json::from_str(&printed).unwrap_or(serde_json::Value::Null)
}

/// The `sim::Dispersion` the `SEASON_SIGMA`, `SEASON_SHOCK`,
/// `SEASON_HOT_K` and `SEASON_DRIFT` knobs ask for; production's where unset.
fn dispersion_from_env() -> Dispersion {
    let production = Dispersion::PRODUCTION;
    let shock_on = match std::env::var("SEASON_SHOCK").as_deref() {
        Err(_) => production.shock_on,
        Ok("elo") => ShockOn::Elo,
        Ok("blend") => ShockOn::Blend,
        Ok(v) => panic!("SEASON_SHOCK={v}: use elo or blend"),
    };
    let disp = Dispersion {
        sigma: env_num("SEASON_SIGMA", production.sigma),
        shock_on,
        hot_k: env_num("SEASON_HOT_K", production.hot_k),
        drift: env_num("SEASON_DRIFT", production.drift),
    };
    for (key, value) in [
        ("SEASON_SIGMA", disp.sigma),
        ("SEASON_HOT_K", disp.hot_k),
        ("SEASON_DRIFT", disp.drift),
    ] {
        assert!(
            value.is_finite() && value >= 0.0,
            "{key}={value} must be finite and non-negative"
        );
    }
    disp
}

/// The dispersion as the header prints it.
fn describe(d: &Dispersion) -> String {
    let shock = match d.shock_on {
        ShockOn::Elo => "elo",
        ShockOn::Blend => "blend",
    };
    format!(
        "sigma {} on {shock}, hot K {}, drift {}{}",
        d.sigma,
        d.hot_k,
        d.drift,
        if *d == Dispersion::PRODUCTION {
            " (production)"
        } else {
            ""
        }
    )
}

fn main() {
    let started = std::time::Instant::now();
    let n_sims: usize = env_num("SEASON_SIMS", 2000);
    let seed: u64 = env_num("SEASON_SEED", superlig_sim::forecast_cache::DEFAULT_SEED);
    let resamples: usize = env_num("SEASON_BOOT", 2000);
    assert!(
        n_sims > 0 && resamples > 0,
        "SEASON_SIMS and SEASON_BOOT must be positive"
    );
    let holdout = std::env::var("EVAL_HOLDOUT").is_ok_and(|v| v == "1");
    let scored_years: Vec<i32> = match std::env::var("EVAL_SEASONS") {
        Ok(s) => s
            .split(',')
            .map(|y| y.trim().parse().expect("season end year"))
            .collect(),
        Err(_) => DEV
            .chain(if holdout { HOLDOUT.to_vec() } else { vec![] })
            .collect(),
    };

    let count_awards = match std::env::var("SEASON_AWARDS").as_deref() {
        Err(_) | Ok("count") => true,
        Ok("drop") => false,
        Ok(v) => panic!("SEASON_AWARDS={v}: use count or drop"),
    };
    let disp = dispersion_from_env();

    let training = history::load_history_with_cutoff(history::CUTOFF_YEAR);
    let official = official_rows();
    let awarded = awards(&official, &training);
    let all = seasons(if count_awards { &official } else { &training });

    let (w_elo, w_dc, w_pi) = superlig_sim::ensemble_weights();
    println!(
        "season_eval: World::simulate_one_dispersed, {n_sims} seasons per snapshot, seed {seed}, {} threads",
        rayon::current_num_threads()
    );
    println!(
        "model: Elo K {} (preseason K {}, new club {}), weights elo/dc/pi {w_elo}/{w_dc}/{w_pi}, Dixon-Coles and pi fit before each season",
        data::ELO_K,
        data::PRESEASON_K,
        data::NEW_CLUB_ELO,
    );
    println!("dispersion: {}", describe(&disp));
    let mut awarded_by_season: BTreeMap<i32, usize> = BTreeMap::new();
    for m in &awarded {
        *awarded_by_season
            .entry(season_end_year(m.date))
            .or_default() += 1;
    }
    println!(
        "awarded results (never in training) {}: {}",
        if count_awards {
            "counted, as in the official tables"
        } else {
            "left out of calendar and table (SEASON_AWARDS=drop)"
        },
        awarded_by_season
            .iter()
            .map(|(y, n)| format!("{} {n}", season_name(*y)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("relegation places (clubs absent the next season; bottom of the table scored):");
    for &year in &scored_years {
        let s = all
            .get(&year)
            .unwrap_or_else(|| panic!("no season ends in {year}"));
        let note = RELEGATION_EXCEPTIONS
            .iter()
            .find(|e| e.0 == year)
            .map(|e| format!(", {} went down: {}", s.relegated.len(), e.2))
            .unwrap_or_default();
        println!(
            "  {} {} clubs, {} places{note}",
            season_name(year),
            s.teams.len(),
            s.relegation
        );
    }

    let mut scored: Vec<Scored> = Vec::new();
    for &year in &scored_years {
        let season = &all[&year];
        let (elo, ensemble) = preseason(season, &training);
        for (si, &(_, share)) in SNAPSHOTS.iter().enumerate() {
            let world = world_on(season, &elo, Some(&ensemble), snapshot_cut(season, share));
            let base = seed.wrapping_add(year as u64 * 10 + si as u64);
            let trials = simulate(&world, n_sims, base, &disp);
            scored.push(score(
                season,
                si,
                world.played.len(),
                &trials,
                base ^ 0x5049_5400,
            ));
        }
        eprintln!(
            "{} done ({:.1}s)",
            season_name(year),
            started.elapsed().as_secs_f64()
        );
    }

    for (si, &(label, share)) in SNAPSHOTS.iter().enumerate() {
        println!(
            "\n== {label}: {:.0}% of each season's fixtures played (cut on a date) ==",
            share * 100.0
        );
        println!(
            "{:<8} {:>5} {:>3} {:>8} {:>7} {:>6} {:>8} {:>7} {:>7} {:>7} {:>6} {:>6} {:>6} {:>7} {:>6} {:>13}",
            "season", "clubs", "rel", "played", "pos_rps", "crps", "title_br", "top4_br", "rel_br",
            "p_champ", "pit_sd", "cov50", "cov80", "sd_pred", "rmse", "table_sd s/a"
        );
        for s in scored.iter().filter(|s| s.snapshot == si) {
            let m = metrics(&[s]);
            println!(
                "{:<8} {:>5} {:>3} {:>8} {:>7.4} {:>6.3} {:>8.4} {:>7.4} {:>7.4} {:>7.3} {:>6.3} {:>6.1} {:>6.1} {:>7.2} {:>6.2} {:>13}",
                season_name(s.year),
                s.rows.len(),
                s.relegation,
                format!("{}/{}", s.played, all[&s.year].rows.len()),
                get(&m, "pos_rps"),
                get(&m, "pts_crps"),
                get(&m, "title_brier"),
                get(&m, "top4_brier"),
                get(&m, "rel_brier"),
                get(&m, "p_champion"),
                get(&m, "pit_sd"),
                get(&m, "cov50_pct"),
                get(&m, "cov80_pct"),
                get(&m, "pts_sd_pred"),
                get(&m, "pts_rmse"),
                format!(
                    "{:.1}/{:.1}",
                    get(&m, "table_sd_sim"),
                    get(&m, "table_sd_actual")
                ),
            );
        }
    }

    let mut json: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    json.insert("n_sims".into(), n_sims.into());
    json.insert("awards_counted".into(), u8::from(count_awards).into());
    for (set, years) in [
        (
            "dev",
            scored_years
                .iter()
                .filter(|y| !HOLDOUT.contains(y))
                .copied()
                .collect::<Vec<_>>(),
        ),
        (
            "hold",
            scored_years
                .iter()
                .filter(|y| HOLDOUT.contains(y))
                .copied()
                .collect(),
        ),
    ] {
        if years.is_empty() {
            continue;
        }
        let per_snapshot: Vec<Vec<&Scored>> = (0..SNAPSHOTS.len())
            .map(|si| {
                scored
                    .iter()
                    .filter(|s| s.snapshot == si && years.contains(&s.year))
                    .collect()
            })
            .collect();
        let clubs = per_snapshot[0].iter().map(|s| s.rows.len()).sum::<usize>();
        println!(
            "\n== {set} pooled: {}..{}, {} seasons, {clubs} club-seasons per snapshot ==",
            season_name(years[0]),
            season_name(years[years.len() - 1]),
            years.len()
        );
        println!("   point estimate [95% season-bootstrap interval, {resamples} resamples]");
        print!("{:<18}", "metric");
        for (label, _) in SNAPSHOTS {
            print!(" {label:<27}");
        }
        println!();
        let estimates: Vec<(Metrics, Vec<(f64, f64)>)> = per_snapshot
            .iter()
            .enumerate()
            .map(|(si, s)| {
                (
                    metrics(s),
                    bootstrap(s, resamples, seed ^ (0xB007 + si as u64)),
                )
            })
            .collect();
        for (m, &(key, digits)) in METRICS.iter().enumerate() {
            print!("{key:<18}");
            for (si, (point, ci)) in estimates.iter().enumerate() {
                let cell = format!(
                    "{:.digits$} [{:.digits$}, {:.digits$}]",
                    point[m], ci[m].0, ci[m].1
                );
                print!(" {cell:<27}");
                json.insert(
                    format!("{set}_{}_{key}", SNAPSHOTS[si].0),
                    json_number(point[m], digits),
                );
            }
            println!();
        }
        print!("{:<18}", "pit_quintiles_pct");
        for s in &per_snapshot {
            let q = pit_quintiles(s).map(|b| format!("{b:.0}"));
            print!(" {:<27}", q.join(" "));
        }
        println!();
        for (si, (point, _)) in estimates.iter().enumerate() {
            println!(
                "dispersion {}: forecast SD of final points {:.2} vs realized error {:.2} (ratio {:.3}); table spread {:.2} simulated vs {:.2} actual",
                SNAPSHOTS[si].0,
                get(point, "pts_sd_pred"),
                get(point, "pts_rmse"),
                get(point, "pts_err_ratio"),
                get(point, "table_sd_sim"),
                get(point, "table_sd_actual")
            );
        }
        json.insert(format!("{set}_n_seasons"), years.len().into());
        json.insert(format!("{set}_n_club_seasons"), clubs.into());
    }
    println!(
        "\n(uniform PIT: mean 0.500, sd 0.289, 20% per quintile; a calibrated forecast's randomized interval covers exactly its share)"
    );

    if let Ok(path) = std::env::var("SEASON_METRICS") {
        let mut s = serde_json::to_string_pretty(&json).expect("metrics serialize");
        s.push('\n');
        std::fs::write(&path, s).expect("write metrics");
        eprintln!("pooled metrics written to {path}");
    }
    if let Ok(path) = std::env::var("SEASON_DUMP") {
        let mut s = String::from(
            "season,snapshot,club,clubs,played,relegation,actual_pos,actual_pts,mean_pts,sd_pts,p_title,p_top4,p_rel,pit,rps,pts_crps\n",
        );
        for sc in &scored {
            for r in &sc.rows {
                s.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{:.3},{:.3},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5}\n",
                    season_name(sc.year),
                    SNAPSHOTS[sc.snapshot].0,
                    r.club,
                    sc.rows.len(),
                    sc.played,
                    sc.relegation,
                    r.actual_pos + 1,
                    r.actual_pts,
                    r.mean_pts,
                    r.sd_pts,
                    r.title.p,
                    r.top4.p,
                    r.rel.p,
                    r.pit,
                    r.rps,
                    r.crps
                ));
            }
        }
        std::fs::write(&path, s).expect("write dump");
        eprintln!("per-club rows written to {path}");
    }
    eprintln!(
        "elapsed {:.1}s on {} threads",
        started.elapsed().as_secs_f64(),
        rayon::current_num_threads()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rps_is_zero_for_a_perfect_forecast_and_one_for_the_worst() {
        assert_eq!(rps(&[0.0, 1.0, 0.0, 0.0], 1), 0.0);
        assert_eq!(rps(&[1.0, 0.0, 0.0], 0), 0.0);
        // All the weight on the far end from what happened.
        assert_eq!(rps(&[0.0, 0.0, 1.0], 0), 1.0);
        // Thresholds "first or better", "second or better": (0.5 - 1)^2 and
        // (1 - 1)^2, averaged over the two.
        assert!((rps(&[0.5, 0.5, 0.0], 0) - 0.125).abs() < 1e-12);
        // Nearer misses cost less than farther ones.
        assert!(rps(&[0.0, 1.0, 0.0], 0) < rps(&[0.0, 0.0, 1.0], 0));
    }

    #[test]
    fn brier_and_log_loss_on_toy_inputs() {
        assert!((brier(0.8, true) - 0.04).abs() < 1e-12);
        assert!((brier(0.8, false) - 0.64).abs() < 1e-12);
        assert_eq!(brier(1.0, true), 0.0);
        assert!((log_loss(0.8, true) + 0.8f64.ln()).abs() < 1e-12);
        assert!((log_loss(0.8, false) + 0.2f64.ln()).abs() < 1e-12);
        assert!((log_loss(0.5, true) - std::f64::consts::LN_2).abs() < 1e-12);
    }

    #[test]
    fn smoothing_keeps_log_loss_finite_and_a_half_a_half() {
        assert_eq!(smoothed(0, 1999), 0.5 / 2000.0);
        assert_eq!(smoothed(1999, 1999), 1999.5 / 2000.0);
        assert_eq!(smoothed(1000, 2000), 0.5);
        assert!(log_loss(smoothed(0, 2000), true).is_finite());
        assert!(log_loss(smoothed(2000, 2000), false).is_finite());
    }

    /// Points-like draws: 34 matches won, drawn or lost.
    fn season_points(rng: &mut SmallRng, p_win: f64, p_draw: f64) -> i64 {
        (0..34)
            .map(|_| {
                let u: f64 = rng.gen();
                if u < p_win {
                    3
                } else if u < p_win + p_draw {
                    1
                } else {
                    0
                }
            })
            .sum()
    }

    /// A forecast sample from the same distribution as the outcome must give
    /// uniform PITs, even though points are discrete; one that is too narrow
    /// must not.
    #[test]
    fn randomized_pit_is_uniform_for_calibrated_draws() {
        let mut rng = SmallRng::seed_from_u64(7);
        let mut forecast: Vec<i64> = (0..4000)
            .map(|_| season_points(&mut rng, 0.4, 0.3))
            .collect();
        forecast.sort_unstable();
        let pits: Vec<f64> = (0..4000)
            .map(|_| {
                let x = season_points(&mut rng, 0.4, 0.3);
                randomized_pit(&forecast, x, rng.gen())
            })
            .collect();
        let (mean, sd) = mean_sd(&pits);
        assert!((mean - 0.5).abs() < 0.02, "mean {mean}");
        assert!((sd - (1.0f64 / 12.0).sqrt()).abs() < 0.01, "sd {sd}");
        let scored = Scored {
            year: 0,
            snapshot: 0,
            played: 0,
            relegation: 0,
            rows: pits.iter().map(|&pit| row_with_pit(pit)).collect(),
            table_sd_sim: 0.0,
            table_sd_actual: 0.0,
        };
        for bin in pit_quintiles(&[&scored]) {
            assert!((bin - 20.0).abs() < 2.5, "quintile {bin}%");
        }
        let in80 = pits.iter().filter(|&&p| in_central(p, 0.8)).count() as f64 / 4000.0;
        assert!((in80 - 0.8).abs() < 0.02, "80% interval covered {in80}");

        // Outcomes far wider than the forecast pile into the tails.
        let narrow: Vec<f64> = (0..4000)
            .map(|_| {
                let x = season_points(&mut rng, 0.25, 0.3) + rng.gen_range(0..30);
                randomized_pit(&forecast, x, rng.gen())
            })
            .collect();
        assert!(mean_sd(&narrow).1 > 0.32);
    }

    fn row_with_pit(pit: f64) -> Row {
        let event = || Event::new(0, 1, false, 1, 18);
        Row {
            club: String::new(),
            actual_pos: 0,
            actual_pts: 0,
            rps: 0.0,
            title: event(),
            top4: event(),
            rel: event(),
            pit,
            closed50: false,
            closed80: false,
            mean_pts: 0.0,
            sd_pts: 0.0,
            crps: 0.0,
        }
    }

    #[test]
    fn crps_is_the_absolute_error_of_a_point_forecast() {
        assert_eq!(crps(&[50], 50), 0.0);
        assert_eq!(crps(&[50, 50, 50], 53), 3.0);
        assert_eq!(crps(&[50], 46), 4.0);
        // Half the weight on the outcome, half one point above it.
        assert_eq!(crps(&[40, 41], 40), 0.25);
        assert_eq!(crps(&[40, 41], 41), 0.25);
    }

    /// For an integer outcome the sum over integer thresholds is the
    /// sample's E|X − y| − E|X − X'| / 2, the energy form of the CRPS.
    #[test]
    fn crps_matches_its_energy_form() {
        let mut rng = SmallRng::seed_from_u64(3);
        let mut sample: Vec<i64> = (0..300)
            .map(|_| season_points(&mut rng, 0.4, 0.3))
            .collect();
        sample.sort_unstable();
        let n = sample.len() as f64;
        for y in [10, 38, 45, 52, 90] {
            let to_y = sample.iter().map(|&x| (x - y).abs() as f64).sum::<f64>() / n;
            let spread = sample
                .iter()
                .flat_map(|&a| sample.iter().map(move |&b| (a - b).abs() as f64))
                .sum::<f64>()
                / (n * n);
            let energy = to_y - spread / 2.0;
            assert!((crps(&sample, y) - energy).abs() < 1e-9, "y {y}");
        }
    }

    /// A proper score: over outcomes drawn from the forecast's own
    /// distribution, the forecast beats both a narrower and a wider one
    /// with the same centre.
    #[test]
    fn crps_prefers_the_spread_the_outcomes_have() {
        let mut rng = SmallRng::seed_from_u64(11);
        let mut sample: Vec<i64> = (0..2000)
            .map(|_| season_points(&mut rng, 0.4, 0.3))
            .collect();
        sample.sort_unstable();
        let centre = sample[sample.len() / 2];
        let scaled = |f: f64| {
            let mut v: Vec<i64> = sample
                .iter()
                .map(|&x| centre + ((x - centre) as f64 * f).round() as i64)
                .collect();
            v.sort_unstable();
            v
        };
        let (narrow, wide) = (scaled(0.5), scaled(1.8));
        let outcomes: Vec<i64> = (0..2000)
            .map(|_| season_points(&mut rng, 0.4, 0.3))
            .collect();
        let mean = |forecast: &[i64]| {
            outcomes.iter().map(|&y| crps(forecast, y)).sum::<f64>() / outcomes.len() as f64
        };
        let right = mean(&sample);
        assert!(mean(&narrow) > right * 1.05, "narrow {}", mean(&narrow));
        assert!(mean(&wide) > right * 1.05, "wide {}", mean(&wide));
    }

    #[test]
    fn pit_takes_the_whole_atom_at_the_extremes() {
        let sample = [40, 50, 50, 60];
        assert_eq!(randomized_pit(&sample, 30, 0.7), 0.0);
        assert_eq!(randomized_pit(&sample, 70, 0.7), 1.0);
        // P(X < 50) = 1/4, P(X = 50) = 1/2.
        assert_eq!(randomized_pit(&sample, 50, 0.0), 0.25);
        assert_eq!(randomized_pit(&sample, 50, 0.5), 0.5);
        // A value between atoms has no mass of its own.
        assert_eq!(randomized_pit(&sample, 45, 0.9), 0.25);
    }

    #[test]
    fn coverage_counts_the_central_interval() {
        let pits = [0.05, 0.3, 0.5, 0.85, 0.95];
        let count = |share| pits.iter().filter(|&&p| in_central(p, share)).count();
        assert_eq!(count(0.5), 2);
        assert_eq!(count(0.8), 3);
        assert!(in_central(0.25, 0.5) && in_central(0.75, 0.5));

        let sample: Vec<i64> = (1..=101).collect();
        assert_eq!(central_interval(&sample, 0.8), (11, 91));
        assert_eq!(central_interval(&sample, 0.5), (26, 76));
        // The site's threshold convention: rank round((n - 1)·q).
        assert_eq!(quantile(&[10, 20, 30, 40], 0.5), 30);
    }

    #[test]
    fn bootstrap_of_identical_seasons_has_no_width() {
        let make = || Scored {
            year: 0,
            snapshot: 0,
            played: 0,
            relegation: 3,
            rows: (0..18)
                .map(|i| Row {
                    rps: 0.01 * i as f64,
                    actual_pts: 40 + i,
                    mean_pts: 45.0,
                    sd_pts: 8.0,
                    ..row_with_pit(i as f64 / 18.0)
                })
                .collect(),
            table_sd_sim: 14.0,
            table_sd_actual: 16.0,
        };
        let (a, b) = (make(), make());
        let point = metrics(&[&a, &b]);
        for (m, (lo, hi)) in bootstrap(&[&a, &b], 50, 1).into_iter().enumerate() {
            if point[m].is_finite() {
                assert!((lo - point[m]).abs() < 1e-9 && (hi - point[m]).abs() < 1e-9);
            }
        }
        assert!((get(&point, "pos_rps") - 0.085).abs() < 1e-12);
        assert_eq!(get(&point, "pts_sd_pred"), 8.0);
    }

    /// Four hand-made trials of a five-club season with two relegation
    /// places: every event is counted from the season's own places, and the
    /// pooled scores match the arithmetic done by hand.
    #[test]
    fn score_counts_each_event_from_the_season_s_own_places() {
        let season = Season {
            year: 2000,
            rows: Vec::new(),
            teams: ["A", "B", "C", "D", "E"].map(String::from).to_vec(),
            final_pos: vec![0, 1, 2, 3, 4],
            final_pts: vec![10, 8, 6, 4, 2],
            relegation: 2,
            relegated: BTreeSet::new(),
        };
        let trial = |order: [usize; 5], points: [i64; 5]| SeasonResult {
            order: order.to_vec(),
            records: points
                .map(|points| TeamRecord {
                    points,
                    ..TeamRecord::default()
                })
                .to_vec(),
        };
        let trials = [
            trial([0, 1, 2, 3, 4], [10, 8, 6, 4, 2]),
            trial([0, 1, 2, 3, 4], [10, 8, 6, 4, 2]),
            trial([1, 0, 2, 4, 3], [8, 10, 6, 1, 3]),
            trial([0, 1, 3, 2, 4], [12, 8, 4, 5, 0]),
        ];
        let s = score(&season, 1, 7, &trials, 3);
        assert_eq!((s.played, s.relegation), (7, 2));
        let p = |e: fn(&Row) -> &Event| s.rows.iter().map(|r| e(r).p).collect::<Vec<_>>();
        assert_eq!(p(|r| &r.title), [0.75, 0.25, 0.0, 0.0, 0.0]);
        assert_eq!(p(|r| &r.top4), [1.0, 1.0, 1.0, 0.75, 0.25]);
        // Bottom two, not three: C is third-last in every trial but one.
        assert_eq!(p(|r| &r.rel), [0.0, 0.0, 0.25, 0.75, 1.0]);
        let happened: Vec<bool> = s.rows.iter().map(|r| r.rel.happened).collect();
        assert_eq!(happened, [false, false, false, true, true]);
        assert!(s.rows.iter().all(|r| r.rel.base_rate == 0.4));
        assert!(s.rows.iter().all(|r| r.top4.base_rate == 0.8));
        // A: P(1st) 0.75 when first; only "first or better" misses.
        assert!((s.rows[0].rps - 0.0625 / 4.0).abs() < 1e-12);
        // A's points: 8, 10, 10, 12 around the 10 it got.
        assert_eq!((s.rows[0].mean_pts, s.rows[0].sd_pts), (10.0, 2f64.sqrt()));
        assert!((0.25..=0.75).contains(&s.rows[0].pit) && s.rows[0].closed80);

        let m = metrics(&[&s]);
        // Relegation Brier: C 0.25², D 0.25², the rest exact; the base rate
        // 0.4 scores 3 × 0.4² + 2 × 0.6² = 1.2.
        assert!((get(&m, "rel_brier") - 0.125 / 5.0).abs() < 1e-12);
        assert!((get(&m, "rel_bss") - (1.0 - 0.125 / 1.2)).abs() < 1e-12);
        // Title: A 0.25², B 0.25²; the base rate 0.2 scores 0.8 in all.
        assert!((get(&m, "title_bss") - (1.0 - 0.125 / 0.8)).abs() < 1e-12);
        assert_eq!(get(&m, "p_champion"), 0.75);
    }

    #[test]
    fn metrics_json_keeps_the_printed_digits() {
        let map = BTreeMap::from([
            ("a", json_number(0.13221, 4)),
            ("b", json_number(2000.0, 0)),
            ("c", json_number(-0.0001, 3)),
            ("d", json_number(f64::NAN, 3)),
            ("e", json_number(70.44, 1)),
        ]);
        assert_eq!(
            serde_json::to_string(&map).expect("serialize"),
            r#"{"a":0.1322,"b":2000,"c":0.0,"d":null,"e":70.4}"#
        );
    }

    /// The official record is the training history plus the awards: the 29
    /// earthquake walkovers of 2022-23 and the one 2024-25 award.
    #[test]
    fn official_tables_count_the_awards() {
        let training = history::load_history_with_cutoff(history::CUTOFF_YEAR);
        let official = official_rows();
        let awarded = awards(&official, &training);
        let mut per_season: BTreeMap<i32, usize> = BTreeMap::new();
        for m in &awarded {
            *per_season.entry(season_end_year(m.date)).or_default() += 1;
        }
        assert_eq!(per_season, BTreeMap::from([(2023, 29), (2025, 1)]));

        let all = seasons(&official);
        let points = |year: i32, club: &str| {
            let s = &all[&year];
            s.final_pts[s.teams.iter().position(|t| t == club).expect(club)]
        };
        // The published final tables.
        assert_eq!(points(2025, "Galatasaray"), 95);
        assert_eq!(points(2023, "Galatasaray"), 88);
        for s in all.values() {
            let n = s.teams.len();
            assert_eq!(s.rows.len(), n * (n - 1), "{}", season_name(s.year));
        }

        // Played football only (SEASON_AWARDS=drop): the same relegation
        // places, and the 2024-25 table without the award.
        let played = seasons(&training);
        for (year, s) in &all {
            assert_eq!(played[year].relegation, s.relegation, "{year}");
        }
        assert_eq!(played[&2025].final_pts.iter().max(), Some(&92));
    }

    /// Champions and relegation places as the record has them. The places
    /// are derived from who was absent the next season, so a club name that
    /// drifts between seasons fails here (or in `seasons`' own check).
    #[test]
    fn final_tables_and_relegation_places_follow_the_record() {
        let all = seasons(&official_rows());
        fn champion(s: &Season) -> &str {
            let c = s
                .final_pos
                .iter()
                .position(|&p| p == 0)
                .expect("a champion");
            &s.teams[c]
        }
        let found: Vec<(i32, &str, usize)> = all
            .values()
            .filter(|s| s.year >= 2017)
            .map(|s| (s.year, champion(s), s.relegation))
            .collect();
        assert_eq!(
            found,
            [
                (2017, "Beşiktaş", 3),
                (2018, "Galatasaray", 3),
                (2019, "Galatasaray", 3),
                (2020, "Başakşehir", 3),
                (2021, "Beşiktaş", 4),
                (2022, "Trabzonspor", 4),
                (2023, "Galatasaray", 4),
                (2024, "Galatasaray", 4),
                (2025, "Galatasaray", 4),
                (2026, "Galatasaray", 3),
            ]
        );
        assert!(all[&2020].relegated.is_empty());
        assert_eq!(
            all[&2023].relegated,
            BTreeSet::from(["Giresunspor".to_string(), "Umraniyespor".to_string()])
        );
    }

    /// The snapshot world is production's: played results up to the cut,
    /// ratings walked through exactly those.
    #[test]
    fn snapshot_worlds_hold_the_results_before_the_cut() {
        let all = seasons(&official_rows());
        let season = &all[&2019];
        let elo = vec![1500.0; season.teams.len()];
        let start = world_on(season, &elo, None, snapshot_cut(season, 0.0));
        assert!(start.played.is_empty());
        assert_eq!(start.elo, elo);
        let cut = snapshot_cut(season, 1.0 / 3.0);
        let third = world_on(season, &elo, None, cut);
        let before = season.rows.iter().filter(|m| m.date < cut).count();
        assert_eq!(third.played.len(), before);
        assert!(before <= season.rows.len() / 3 && before > season.rows.len() / 4);
        assert_eq!(third.fixtures.len(), season.rows.len());

        // Production's in-season rule, walked by hand in date order.
        let mut walked = elo.clone();
        for m in season.rows.iter().filter(|m| m.date < cut) {
            let (h, a) = (third.idx[&m.home_team], third.idx[&m.away_team]);
            let d = sim::elo_delta(walked[h], walked[a], m.home_score, m.away_score);
            walked[h] += d;
            walked[a] -= d;
        }
        assert_ne!(third.elo, elo);
        for (got, want) in third.elo.iter().zip(&walked) {
            assert!((got - want).abs() < 1e-9, "{got} v {want}");
        }
    }
}
