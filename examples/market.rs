//! Offline benchmark of the shipped model against the betting market.
//!
//! Offline evaluation only: the odds this reads never reach the app, its
//! API or its UI (CLAUDE.md, "No licensed betting sources"). They are not
//! committed (`data/odds/` is ignored) and nothing in `src/` reads them.
//!
//! Inputs:
//!   - a backtest `EVAL_DUMP` written with `EVAL_HOLDOUT=1` (first argument):
//!     the model's walk-forward forecasts, each priced from earlier results
//!     only, for the dev seasons 2016-17..2023-24 and the holdout 2024-25
//!     and 2025-26;
//!   - the odds `scripts/fetch_odds.py` writes (`--odds`, default
//!     `data/odds/superlig_odds.csv`; `--live-odds`, default
//!     `data/odds/superlig_odds_2026_27.csv`): Bet365's and the best of
//!     ~17 books' 1X2 and over/under 2.5 prices, which the archive documents
//!     as pre-match (it has no closing prices), already reconciled with the
//!     history;
//!   - for the live season, `data/forecast_log.json` (read at run time: the
//!     frozen pre-kick-off forecasts and the model label of each) and the
//!     played results of the shipped calendar (`data::fixtures()`).
//!
//! Market probabilities. The primary market forecast, declared before any
//! result was looked at, is Bet365's 1X2 with the margin removed by Shin's
//! method: with π_i = 1/odds_i and book B = Σ π_i,
//! p_i = (√(z² + 4 (1 − z) π_i² / B) − z) / (2 (1 − z)), z in (0, 1) solved
//! by bisection so that Σ p_i = 1. z is Shin's share of insider money; the
//! method takes more margin off long shots than off favourites.
//! Proportional normalization (p_i = π_i / B) is the check, and the
//! best-price line gets both. A best-price book can come out at or below 1
//! (the best of many books, some of them stale), where Shin has no
//! solution in (0, 1): proportional stands in and is counted. Over/under
//! 2.5 takes the same two methods on its two prices.
//!
//! The join pairs dump and odds rows on season, date, home and away; the
//! scores must agree. A dump row without an odds row, or whose odds row
//! lacks any of the ten prices, is left out and counted; more than
//! `MAX_LEFT_OUT` (5%) of a set left out, an odds row of a scored season
//! with no dump row, a score that differs or a dump without holdout rows
//! stops the run. On 2026-10-19 the archive lacked 2024-25 matchdays 10-12
//! (27 of 647 holdout matches, 4.2%) and three 2017-18 Bet365 prices.
//!
//! Output, after the inputs and the conventions, per set (`dev`, `hold`):
//!   - the set line: matches joined, dates, seasons and what was left out;
//!   - the book: Bet365's mean 1X2 and over/under margin (B − 1), mean Shin
//!     z, and how many best-price books needed the proportional stand-in;
//!   - for each market line (`b365 shin`, the primary, then `b365 prop`,
//!     `max shin`, `max prop`) and aspect (1X2 log-loss, RPS, over/under
//!     2.5 log-loss): the model's mean loss, the line's, model − market, a
//!     95% date-block bootstrap interval for it (10,000 resamples, seed
//!     12345), the Diebold-Mariano-style t and a 95% season-cluster t
//!     interval with seasons − 1 degrees of freedom, all as
//!     `examples/compare.rs` defines them (the code is shared:
//!     `examples/common/stats.rs`). Positive model − market: the market
//!     has the lower loss;
//!   - calibration: the model's and the primary market's mean forecast of
//!     home, draw, away and over 2.5 against how often each happened;
//!   - the 1X2 gap by matchday band, 1-10, 11-22 and 23 to the season's end
//!     (2020-21..2024-25 ran 36 to 40 games a club), with each band's share
//!     of the set's summed gap. A match's matchday is 1 + the two clubs'
//!     mean league games before it (the dump's `gp_h`, `gp_a`, counted in
//!     date order), rounded down;
//!   - dev only, forecast encompassing: w in p = (1 − w) · market + w · model
//!     (1X2, primary market) fitted by maximum likelihood on [0, 1], with a
//!     95% season-cluster bootstrap interval (whole dev seasons resampled;
//!     with 8 seasons read it as rough), each dev season's own w, and the
//!     pool's in-sample log-loss;
//!   - holdout only: that pool at the dev-fitted w against the market alone,
//!     out of sample, with the same intervals as above (pool − market < 0:
//!     the model adds information the market lacks).
//!
//! Then the live season: the frozen forecasts of played 2026-27 fixtures
//! that have Bet365 odds: n, the model labels, the model's and the
//! market's mean 1X2 log-loss and model − market with a 95% normal
//! interval that treats matches as independent.
//!
//! `MARKET_METRICS=path.json` also writes the point estimates as one JSON
//! object, keys sorted, at their printed precision (as `EVAL_METRICS`
//! does); intervals are only printed. Keys: `{dev,hold}_n` and `_left_out`;
//! `_{model,market}_{1x2_ll,rps,ou25_ll}` and `_diff_*` (model − primary
//! market); `_{b365_prop,max_shin,max_prop}_{aspect}` for the check lines;
//! `_b365_margin`, `_b365_ou_margin`, `_shin_z`, `_shin_z_ou`,
//! `_max_book_le1`; `_cal_{home,draw,away,over}_{model,market,obs}`;
//! `_md{01_10,11_22,23_end}_{n,model_1x2_ll,market_1x2_ll,diff_1x2_ll}`
//! and `_gap_share` (the band's share of the set's summed model − market);
//! `dev_pool_w`, `dev_pool_1x2_ll`, `hold_pool_1x2_ll`,
//! `hold_pool_diff_1x2_ll`; `live_n`, `live_{model,market,diff}_1x2_ll`.
//! data/metrics/market.json is this file for the production dump and the
//! odds as pinned on 2026-10-19. CI cannot regenerate it: the odds are not
//! committed and fetching them needs the network. So CI does not diff it;
//! `scripts/check_metrics.py docs` checks the docs' `metric:market:`
//! markers against the committed copy.
//!
//! Run:
//!   EVAL_HOLDOUT=1 EVAL_DUMP=prod.csv cargo run --release --example backtest
//!   python3 scripts/fetch_odds.py
//!   MARKET_METRICS=data/metrics/market.json cargo run --release --example market -- prod.csv
//!     [--odds PATH] [--live-odds PATH] [--resamples 10000] [--seed 12345]

use std::collections::{BTreeMap, HashMap, HashSet};

use superlig_sim::accuracy::{LoggedForecast, FORECAST_LOG_PATH};
use superlig_sim::data;

#[path = "common/stats.rs"]
mod stats;

use stats::{
    blocks_by, bootstrap_ci, dm_t, interval_text, quantile, season_label, season_t_cis, SplitMix64,
};

/// The season end years `examples/backtest.rs` holds out.
const HOLDOUT: [i32; 2] = [2025, 2026];
const DEFAULT_ODDS: &str = "data/odds/superlig_odds.csv";
const DEFAULT_LIVE_ODDS: &str = "data/odds/superlig_odds_2026_27.csv";
const DEFAULT_RESAMPLES: usize = 10_000;
const DEFAULT_SEED: u64 = 12345;
/// The most of a set's dump rows the join may leave out.
const MAX_LEFT_OUT: f64 = 0.05;
const ASPECTS: [&str; 3] = ["1x2_ll", "rps", "ou25_ll"];
/// The market lines, the primary first, as printed and as metric keys.
const LINES: [(&str, &str); 4] = [
    ("b365 shin", "market"),
    ("b365 prop", "b365_prop"),
    ("max shin", "max_shin"),
    ("max prop", "max_prop"),
];
/// Matchday bands: last matchday of each, the last open-ended.
const BANDS: [(&str, &str, u32); 3] = [
    ("1-10", "md01_10", 10),
    ("11-22", "md11_22", 22),
    ("23-end", "md23_end", u32::MAX),
];
/// z for a two-sided 95% normal interval.
const Z95: f64 = 1.959_963_984_540_054;
const USAGE: &str = "usage: cargo run --release --example market -- DUMP.csv [--odds PATH] [--live-odds PATH] [--resamples N] [--seed N]";

/// Log-loss of a forecast that gave what happened probability `p`, floored
/// as the backtest floors it.
fn nll(p: f64) -> f64 {
    -p.max(1e-9).ln()
}

/// Ranked probability score of a home/draw/away forecast.
fn rps(p3: [f64; 3], o: usize) -> f64 {
    let c1 = p3[0] - if o == 0 { 1.0 } else { 0.0 };
    let c2 = p3[0] + p3[1] - if o <= 1 { 1.0 } else { 0.0 };
    (c1 * c1 + c2 * c2) / 2.0
}

/// 0 home win, 1 draw, 2 away win.
fn outcome((hs, as_): (u32, u32)) -> usize {
    match hs.cmp(&as_) {
        std::cmp::Ordering::Greater => 0,
        std::cmp::Ordering::Equal => 1,
        std::cmp::Ordering::Less => 2,
    }
}

// --------------------------------------------------------------- de-vig ---

/// Proportional normalization: each 1/odds over the book.
fn proportional<const N: usize>(odds: [f64; N]) -> [f64; N] {
    let book: f64 = odds.iter().map(|o| 1.0 / o).sum();
    odds.map(|o| 1.0 / o / book)
}

/// Shin's probabilities for the insider share `z` (they sum to 1 only at
/// the right `z`).
fn shin_at<const N: usize>(odds: [f64; N], z: f64) -> [f64; N] {
    let book: f64 = odds.iter().map(|o| 1.0 / o).sum();
    odds.map(|o| {
        let pi = 1.0 / o;
        ((z * z + 4.0 * (1.0 - z) * pi * pi / book).sqrt() - z) / (2.0 * (1.0 - z))
    })
}

/// Shin's method: the probabilities and the `z` in (0, 1) at which they sum
/// to 1, by bisection (the sum falls from √B at z = 0 to Σ π_i² / B < 1 as
/// z nears 1). `None` for a book at or below 1, which has no such `z`.
fn shin<const N: usize>(odds: [f64; N]) -> Option<([f64; N], f64)> {
    let book: f64 = odds.iter().map(|o| 1.0 / o).sum();
    if !book.is_finite() || book <= 1.0 || odds.iter().any(|&o| !o.is_finite() || o <= 1.0) {
        return None;
    }
    let excess = |z: f64| shin_at(odds, z).iter().sum::<f64>() - 1.0;
    let (mut lo, mut hi) = (0.0, 1.0 - 1e-9);
    if excess(hi) >= 0.0 {
        return None;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if excess(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-15 {
            break;
        }
    }
    let z = 0.5 * (lo + hi);
    Some((shin_at(odds, z), z))
}

/// Shin's probabilities, or proportional ones where Shin has no solution;
/// the second value is Shin's z, `None` for the stand-in.
fn shin_or_proportional<const N: usize>(odds: [f64; N]) -> ([f64; N], Option<f64>) {
    match shin(odds) {
        Some((p, z)) => (p, Some(z)),
        None => (proportional(odds), None),
    }
}

// ---------------------------------------------------------------- inputs ---

/// One match of the backtest dump.
#[derive(Debug, Clone, PartialEq)]
struct DumpRow {
    /// Season end year (2017 is 2016-17).
    season: i32,
    date: String,
    home: String,
    away: String,
    score: (u32, u32),
    p3: [f64; 3],
    p_over: f64,
    /// The two clubs' league games before this match.
    games: (u32, u32),
}

/// One match's prices; a market is `None` unless all of its prices are
/// there.
#[derive(Debug, Clone, PartialEq)]
struct OddsRow {
    season: i32,
    date: String,
    home: String,
    away: String,
    score: (u32, u32),
    b365: Option<[f64; 3]>,
    max: Option<[f64; 3]>,
    b365_ou: Option<[f64; 2]>,
    max_ou: Option<[f64; 2]>,
}

impl OddsRow {
    fn complete(&self) -> bool {
        self.b365.is_some() && self.max.is_some() && self.b365_ou.is_some() && self.max_ou.is_some()
    }
}

/// Season end year of a `2016-17` label.
fn season_end(label: &str) -> Option<i32> {
    let (start, end) = label.split_once('-')?;
    let start: i32 = start.parse().ok()?;
    let end: i32 = end.parse().ok()?;
    (end == (start + 1) % 100).then_some(start + 1)
}

/// A CSV's header and records, columns looked up by name.
struct Table {
    header: csv::StringRecord,
    records: Vec<csv::StringRecord>,
}

impl Table {
    fn parse(text: &str) -> Result<Table, String> {
        let mut reader = csv::Reader::from_reader(text.as_bytes());
        let header = reader
            .headers()
            .map_err(|e| format!("header: {e}"))?
            .clone();
        let records = reader
            .records()
            .enumerate()
            .map(|(i, r)| r.map_err(|e| format!("row {}: {e}", i + 2)))
            .collect::<Result<_, _>>()?;
        Ok(Table { header, records })
    }

    fn column(&self, name: &str) -> Result<usize, String> {
        self.header
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| format!("no `{name}` column"))
    }
}

fn parse_num<T: std::str::FromStr>(text: &str, row: usize) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("row {}: `{text}` is not a number", row + 2))
}

/// The rows of a backtest `EVAL_DUMP` file.
fn parse_dump(text: &str) -> Result<Vec<DumpRow>, String> {
    let t = Table::parse(text)?;
    let [season, date, home, away, hs, as_, p1, px, p2, pover, gp_h, gp_a] = [
        "season", "date", "home", "away", "hs", "as", "p1", "px", "p2", "pover25", "gp_h", "gp_a",
    ]
    .map(|c| t.column(c));
    let (season, date, home, away) = (season?, date?, home?, away?);
    let (hs, as_, p1, px, p2, pover) = (hs?, as_?, p1?, px?, p2?, pover?);
    let (gp_h, gp_a) = (gp_h?, gp_a?);
    t.records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let f = |c: usize| r.get(c).unwrap_or("");
            Ok(DumpRow {
                season: parse_num(f(season), i)?,
                date: f(date).to_string(),
                home: f(home).to_string(),
                away: f(away).to_string(),
                score: (parse_num(f(hs), i)?, parse_num(f(as_), i)?),
                p3: [
                    parse_num(f(p1), i)?,
                    parse_num(f(px), i)?,
                    parse_num(f(p2), i)?,
                ],
                p_over: parse_num(f(pover), i)?,
                games: (parse_num(f(gp_h), i)?, parse_num(f(gp_a), i)?),
            })
        })
        .collect()
}

/// The rows of an odds file written by `scripts/fetch_odds.py`.
fn parse_odds(text: &str) -> Result<Vec<OddsRow>, String> {
    let t = Table::parse(text)?;
    let [season, date, home, away, hs, as_] =
        ["season", "date", "home", "away", "hs", "as"].map(|c| t.column(c));
    let (season, date, home, away, hs, as_) = (season?, date?, home?, away?, hs?, as_?);
    let b365 = ["b365_h", "b365_d", "b365_a"].map(|c| t.column(c));
    let max = ["max_h", "max_d", "max_a"].map(|c| t.column(c));
    let b365_ou = ["b365_over25", "b365_under25"].map(|c| t.column(c));
    let max_ou = ["max_over25", "max_under25"].map(|c| t.column(c));
    let b365 = b365.into_iter().collect::<Result<Vec<_>, _>>()?;
    let max = max.into_iter().collect::<Result<Vec<_>, _>>()?;
    let b365_ou = b365_ou.into_iter().collect::<Result<Vec<_>, _>>()?;
    let max_ou = max_ou.into_iter().collect::<Result<Vec<_>, _>>()?;
    t.records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let f = |c: usize| r.get(c).unwrap_or("");
            // All of a market's prices, or None if any is blank.
            let prices = |cols: &[usize]| -> Result<Option<Vec<f64>>, String> {
                if cols.iter().any(|&c| f(c).is_empty()) {
                    return Ok(None);
                }
                let p: Vec<f64> = cols
                    .iter()
                    .map(|&c| parse_num(f(c), i))
                    .collect::<Result<_, _>>()?;
                if p.iter().any(|&o| !o.is_finite() || o <= 1.0) {
                    return Err(format!("row {}: odds {p:?} are not prices above 1", i + 2));
                }
                Ok(Some(p))
            };
            let three = |v: Option<Vec<f64>>| v.map(|p| [p[0], p[1], p[2]]);
            let two = |v: Option<Vec<f64>>| v.map(|p| [p[0], p[1]]);
            // Bet365's books carry its margin; one at or below 1 is not a
            // bookmaker's price, and Shin could not take a margin off it.
            let (b365, b365_ou) = (prices(&b365)?, prices(&b365_ou)?);
            for p in [&b365, &b365_ou].into_iter().flatten() {
                if p.iter().map(|o| 1.0 / o).sum::<f64>() <= 1.0 {
                    return Err(format!("row {}: Bet365 odds {p:?} carry no margin", i + 2));
                }
            }
            Ok(OddsRow {
                season: season_end(f(season))
                    .ok_or_else(|| format!("row {}: bad season `{}`", i + 2, f(season)))?,
                date: f(date).to_string(),
                home: f(home).to_string(),
                away: f(away).to_string(),
                score: (parse_num(f(hs), i)?, parse_num(f(as_), i)?),
                b365: three(b365),
                max: three(prices(&max)?),
                b365_ou: two(b365_ou),
                max_ou: two(prices(&max_ou)?),
            })
        })
        .collect()
}

// ------------------------------------------------------------------ join ---

/// A home/draw/away and over-2.5 forecast.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Forecast {
    p3: [f64; 3],
    p_over: f64,
}

impl Forecast {
    /// 1X2 log-loss, RPS and over/under 2.5 log-loss against the result.
    fn losses(&self, o: usize, over: bool) -> [f64; 3] {
        [
            nll(self.p3[o]),
            rps(self.p3, o),
            nll(if over { self.p_over } else { 1.0 - self.p_over }),
        ]
    }
}

/// One match both the model and the market priced.
#[derive(Debug, Clone, PartialEq)]
struct Joined {
    season: i32,
    date: String,
    matchday: u32,
    outcome: usize,
    over: bool,
    model: Forecast,
    /// In `LINES` order.
    lines: [Forecast; 4],
    /// Bet365's 1X2 and over/under books and Shin z's.
    book: (f64, f64),
    z: (f64, f64),
    /// Best-price books (1X2, over/under) Shin could not solve.
    max_le1: usize,
}

/// What the join left out of one set, by reason.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct LeftOut {
    no_row: usize,
    incomplete: usize,
}

impl LeftOut {
    fn total(&self) -> usize {
        self.no_row + self.incomplete
    }
}

/// A match's matchday: 1 + the two clubs' mean games before it, rounded
/// down.
fn matchday((gh, ga): (u32, u32)) -> u32 {
    1 + (gh + ga) / 2
}

/// Index into `BANDS` of a matchday.
fn band(md: u32) -> usize {
    BANDS
        .iter()
        .position(|b| md <= b.2)
        .expect("the last band is open")
}

fn joined(row: &DumpRow, odds: &OddsRow) -> Joined {
    let b365 = odds.b365.expect("complete");
    let max = odds.max.expect("complete");
    let b365_ou = odds.b365_ou.expect("complete");
    let max_ou = odds.max_ou.expect("complete");
    let (p_b365, z) = shin(b365).expect("parse_odds refuses Bet365 books at or below 1");
    let (p_b365_ou, z_ou) = shin(b365_ou).expect("parse_odds refuses Bet365 books at or below 1");
    let (p_max, z_max) = shin_or_proportional(max);
    let (p_max_ou, z_max_ou) = shin_or_proportional(max_ou);
    let book = |o: &[f64]| o.iter().map(|x| 1.0 / x).sum::<f64>();
    let f = |p3: [f64; 3], ou: [f64; 2]| Forecast { p3, p_over: ou[0] };
    Joined {
        season: row.season,
        date: row.date.clone(),
        matchday: matchday(row.games),
        outcome: outcome(row.score),
        over: row.score.0 + row.score.1 >= 3,
        model: Forecast {
            p3: row.p3,
            p_over: row.p_over,
        },
        lines: [
            f(p_b365, p_b365_ou),
            f(proportional(b365), proportional(b365_ou)),
            f(p_max, p_max_ou),
            f(proportional(max), proportional(max_ou)),
        ],
        book: (book(&b365), book(&b365_ou)),
        z: (z, z_ou),
        max_le1: usize::from(z_max.is_none()) + usize::from(z_max_ou.is_none()),
    }
}

/// Pair every dump row with its odds row. Rows without complete odds are
/// left out and counted per set (dev, hold); a differing score, an odds row
/// of a scored season that no dump row takes, a duplicate, a dump without
/// holdout rows or more than `MAX_LEFT_OUT` of a set left out is an error.
fn join(dump: &[DumpRow], odds: &[OddsRow]) -> Result<(Vec<Joined>, [LeftOut; 2]), String> {
    let key = |s: i32, d: &str, h: &str, a: &str| format!("{} {d} {h} v {a}", season_label(s));
    let mut by_key: HashMap<String, &OddsRow> = HashMap::new();
    for o in odds {
        if by_key
            .insert(key(o.season, &o.date, &o.home, &o.away), o)
            .is_some()
        {
            return Err(format!(
                "odds file has {} twice",
                key(o.season, &o.date, &o.home, &o.away)
            ));
        }
    }
    let mut used: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(dump.len());
    let mut left = [LeftOut::default(); 2];
    let mut rows = [0usize; 2];
    for r in dump {
        let k = key(r.season, &r.date, &r.home, &r.away);
        if !used.insert(k.clone()) {
            return Err(format!("dump has {k} twice"));
        }
        let set = usize::from(HOLDOUT.contains(&r.season));
        rows[set] += 1;
        match by_key.get(&k) {
            None => left[set].no_row += 1,
            Some(o) if o.score != r.score => {
                return Err(format!(
                    "{k}: dump {}-{}, odds {}-{}; the files come from different histories",
                    r.score.0, r.score.1, o.score.0, o.score.1
                ))
            }
            Some(o) if !o.complete() => left[set].incomplete += 1,
            Some(o) => out.push(joined(r, o)),
        }
    }
    let scored: HashSet<i32> = dump.iter().map(|r| r.season).collect();
    let unused: Vec<&String> = by_key
        .iter()
        .filter(|(k, o)| scored.contains(&o.season) && !used.contains(*k))
        .map(|(k, _)| k)
        .collect();
    if let Some(first) = unused.iter().min() {
        return Err(format!(
            "{} odds rows of scored seasons match no dump row, e.g. {first}; \
             the files come from different histories",
            unused.len()
        ));
    }
    if rows[1] == 0 {
        return Err("the dump has no holdout rows; run the backtest with EVAL_HOLDOUT=1".into());
    }
    for (set, name) in [(0, "dev"), (1, "hold")] {
        let share = left[set].total() as f64 / rows[set].max(1) as f64;
        if share > MAX_LEFT_OUT {
            return Err(format!(
                "the join leaves out {} of {} {name} matches ({:.1}%, at most {:.0}% allowed): \
                 {} without an odds row, {} with incomplete odds",
                left[set].total(),
                rows[set],
                100.0 * share,
                100.0 * MAX_LEFT_OUT,
                left[set].no_row,
                left[set].incomplete
            ));
        }
    }
    Ok((out, left))
}

// ------------------------------------------------------------ statistics ---

/// A paired row: season, date, the market's losses and the model's.
type LossRow<'a, const N: usize> = (i32, &'a str, [f64; N], [f64; N]);

/// One paired comparison, model − market, of `N` aspects.
struct PairedLine<const N: usize> {
    model: [f64; N],
    market: [f64; N],
    diff: [f64; N],
    boot: [(f64, f64); N],
    dm: [f64; N],
    season_t: [Option<(f64, f64)>; N],
    dates: usize,
    seasons: usize,
}

/// Paired statistics of `(season, date, market loss, model loss)` rows: the
/// date-block bootstrap, the DM t over dates and the season-cluster t.
fn paired<const N: usize>(rows: &[LossRow<N>], resamples: usize, seed: u64) -> PairedLine<N> {
    let n = rows.len() as f64;
    let dates = blocks_by(rows.iter().map(|r| (r.1, r.2, r.3)));
    let seasons = blocks_by(rows.iter().map(|r| (r.0, r.2, r.3)));
    let mean = |pick: &dyn Fn(&LossRow<N>) -> [f64; N]| -> [f64; N] {
        let mut sum = [0.0; N];
        for r in rows {
            for (k, v) in pick(r).into_iter().enumerate() {
                sum[k] += v;
            }
        }
        sum.map(|s| s / n)
    };
    PairedLine {
        model: mean(&|r| r.3),
        market: mean(&|r| r.2),
        diff: mean(&|r| std::array::from_fn(|k| r.3[k] - r.2[k])),
        boot: bootstrap_ci(&dates, resamples, seed),
        dm: std::array::from_fn(|k| dm_t(&dates.iter().map(|b| b.0[k]).collect::<Vec<_>>())),
        season_t: season_t_cis(&seasons),
        dates: dates.len(),
        seasons: seasons.len(),
    }
}

/// d/dw of Σ ln((1 − w) m + w f) over `(m, f)`, the market's and the
/// model's probability of what happened. It falls as w grows.
fn pool_slope(pairs: &[(f64, f64)], w: f64) -> f64 {
    pairs
        .iter()
        .map(|&(m, f)| (f - m) / ((1.0 - w) * m + w * f).max(1e-12))
        .sum()
}

/// The w in [0, 1] where a falling `slope` crosses zero: the maximum of a
/// concave log-likelihood, at a bound when the slope keeps one sign.
fn fit_weight(slope: impl Fn(f64) -> f64) -> f64 {
    if slope(0.0) <= 0.0 {
        return 0.0;
    }
    if slope(1.0) >= 0.0 {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if slope(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// The maximum-likelihood pool weight of the model over `(m, f)` pairs.
fn fit_pool(pairs: &[(f64, f64)]) -> f64 {
    fit_weight(|w| pool_slope(pairs, w))
}

/// 95% season-cluster bootstrap interval of the pool weight: each draw
/// takes as many seasons as there are, with replacement, and refits.
fn pool_bootstrap(seasons: &[Vec<(f64, f64)>], resamples: usize, seed: u64) -> (f64, f64) {
    let mut rng = SplitMix64(seed);
    let mut draws = Vec::with_capacity(resamples);
    let mut counts = vec![0usize; seasons.len()];
    for _ in 0..resamples {
        counts.iter_mut().for_each(|c| *c = 0);
        for _ in 0..seasons.len() {
            counts[rng.below(seasons.len())] += 1;
        }
        draws.push(fit_weight(|w| {
            seasons
                .iter()
                .zip(&counts)
                .filter(|(_, &c)| c > 0)
                .map(|(s, &c)| c as f64 * pool_slope(s, w))
                .sum()
        }));
    }
    draws.sort_by(f64::total_cmp);
    (quantile(&draws, 0.025), quantile(&draws, 0.975))
}

/// The pooled 1X2 forecast's probabilities.
fn pool(w: f64, market: [f64; 3], model: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|k| (1.0 - w) * market[k] + w * model[k])
}

// ------------------------------------------------------------- reporting ---

/// The summary as one JSON object for `MARKET_METRICS`: keys sorted, each
/// value the number as printed.
fn metrics_json(summary: &[(String, String)]) -> String {
    let map: BTreeMap<&str, serde_json::Value> = summary
        .iter()
        .map(|(k, v)| {
            let value = match v.as_str() {
                "NaN" | "inf" | "-inf" => serde_json::Value::Null,
                _ => serde_json::from_str::<serde_json::Number>(v)
                    .unwrap_or_else(|_| panic!("summary {k}={v} is not a number"))
                    .into(),
            };
            (k.as_str(), value)
        })
        .collect();
    assert_eq!(map.len(), summary.len(), "duplicate summary key");
    serde_json::to_string_pretty(&map).expect("serialize metrics") + "\n"
}

/// Collects printed lines and the metrics behind them.
struct Report {
    out: String,
    summary: Vec<(String, String)>,
    resamples: usize,
    seed: u64,
}

impl Report {
    fn line(&mut self, text: impl AsRef<str>) {
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    fn metric(&mut self, key: String, value: String) {
        self.summary.push((key, value));
    }

    /// A paired line's aspect as printed, `label` in the first column.
    fn paired_row<const N: usize>(
        &mut self,
        label: &str,
        aspect: &str,
        l: &PairedLine<N>,
        k: usize,
    ) {
        let interval = format!("[{:+.5}, {:+.5}]", l.boot[k].0, l.boot[k].1);
        self.line(format!(
            "  {label:<10} {aspect:<8} {:>8.5} {:>8.5} {:>+9.5}   {interval:<22} {:>+6.2}   {}",
            l.model[k],
            l.market[k],
            l.diff[k],
            l.dm[k],
            interval_text(l.season_t[k])
        ));
    }

    fn set(&mut self, name: &str, matches: &[&Joined], left: LeftOut, rows: usize) {
        let (resamples, seed) = (self.resamples, self.seed);
        let first = matches.iter().map(|m| m.season).min().expect("non-empty");
        let last = matches.iter().map(|m| m.season).max().expect("non-empty");
        let n = matches.len() as f64;
        let mean = |f: &dyn Fn(&Joined) -> f64| matches.iter().map(|m| f(m)).sum::<f64>() / n;

        let per_line: Vec<PairedLine<3>> = (0..LINES.len())
            .map(|l| {
                let rows: Vec<LossRow<3>> = matches
                    .iter()
                    .map(|m| {
                        (
                            m.season,
                            m.date.as_str(),
                            m.lines[l].losses(m.outcome, m.over),
                            m.model.losses(m.outcome, m.over),
                        )
                    })
                    .collect();
                paired(&rows, resamples, seed)
            })
            .collect();
        self.line(format!(
            "\n{name} seasons {}..{}: {} of {rows} matches joined on {} dates, {} seasons; left out {} ({:.1}%): {} without an odds row, {} with incomplete odds",
            season_label(first),
            season_label(last),
            matches.len(),
            per_line[0].dates,
            per_line[0].seasons,
            left.total(),
            100.0 * left.total() as f64 / rows as f64,
            left.no_row,
            left.incomplete
        ));
        self.metric(format!("{name}_n"), matches.len().to_string());
        self.metric(format!("{name}_left_out"), left.total().to_string());

        let margin = mean(&|m| m.book.0) - 1.0;
        let margin_ou = mean(&|m| m.book.1) - 1.0;
        let (z, z_ou) = (mean(&|m| m.z.0), mean(&|m| m.z.1));
        let le1: usize = matches.iter().map(|m| m.max_le1).sum();
        self.line(format!(
            "  Bet365 margin: 1X2 {:.2}%, over/under {:.2}%; mean Shin z: 1X2 {z:.4}, over/under {z_ou:.4}; best-price books at or below 1 (proportional stand-in): {le1}",
            100.0 * margin,
            100.0 * margin_ou
        ));
        self.metric(format!("{name}_b365_margin"), format!("{margin:.4}"));
        self.metric(format!("{name}_b365_ou_margin"), format!("{margin_ou:.4}"));
        self.metric(format!("{name}_shin_z"), format!("{z:.4}"));
        self.metric(format!("{name}_shin_z_ou"), format!("{z_ou:.4}"));
        self.metric(format!("{name}_max_book_le1"), le1.to_string());

        self.line(format!(
            "  {:<10} {:<8} {:>8} {:>8} {:>9}   {:<22} {:>6}   {}",
            "market",
            "aspect",
            "model",
            "market",
            "model-mkt",
            "95% interval",
            "DM t",
            "95% season t"
        ));
        for (l, (label, key)) in LINES.iter().enumerate() {
            for (k, aspect) in ASPECTS.iter().enumerate() {
                let shown = if k == 0 { *label } else { "" };
                self.paired_row(shown, aspect, &per_line[l], k);
                let line = &per_line[l];
                if l == 0 {
                    self.metric(
                        format!("{name}_model_{aspect}"),
                        format!("{:.5}", line.model[k]),
                    );
                    self.metric(
                        format!("{name}_diff_{aspect}"),
                        format!("{:.5}", line.diff[k]),
                    );
                }
                self.metric(
                    format!("{name}_{key}_{aspect}"),
                    format!("{:.5}", line.market[k]),
                );
            }
        }

        self.line("  calibration, mean forecast against observed (primary market):");
        self.line(format!(
            "    {:<9} {:>7} {:>7} {:>9}",
            "outcome", "model", "market", "observed"
        ));
        for (k, outcome) in ["home", "draw", "away", "over"].iter().enumerate() {
            let pick = |f: &Forecast| if k < 3 { f.p3[k] } else { f.p_over };
            let model = mean(&|m| pick(&m.model));
            let market = mean(&|m| pick(&m.lines[0]));
            let observed = mean(&|m| {
                let happened = if k < 3 { m.outcome == k } else { m.over };
                f64::from(u8::from(happened))
            });
            self.line(format!(
                "    {:<9} {model:>7.4} {market:>7.4} {observed:>9.4}",
                if k < 3 { *outcome } else { "over 2.5" }
            ));
            self.metric(format!("{name}_cal_{outcome}_model"), format!("{model:.4}"));
            self.metric(
                format!("{name}_cal_{outcome}_market"),
                format!("{market:.4}"),
            );
            self.metric(
                format!("{name}_cal_{outcome}_obs"),
                format!("{observed:.4}"),
            );
        }

        self.line(
            "  1X2 log-loss by matchday band (matchday = 1 + the clubs' mean games before it; share: of the summed model-mkt):",
        );
        self.line(format!(
            "    {:<8} {:>5} {:>8} {:>8} {:>9} {:>6}   {:<22} {:>6}   {}",
            "band",
            "n",
            "model",
            "market",
            "model-mkt",
            "share",
            "95% interval",
            "DM t",
            "95% season t"
        ));
        let gap = per_line[0].diff[0] * n;
        for (b, (label, key, _)) in BANDS.iter().enumerate() {
            let rows: Vec<LossRow<1>> = matches
                .iter()
                .filter(|m| band(m.matchday) == b)
                .map(|m| {
                    (
                        m.season,
                        m.date.as_str(),
                        [m.lines[0].losses(m.outcome, m.over)[0]],
                        [m.model.losses(m.outcome, m.over)[0]],
                    )
                })
                .collect();
            if rows.is_empty() {
                continue;
            }
            let l = paired(&rows, resamples, seed);
            let interval = format!("[{:+.5}, {:+.5}]", l.boot[0].0, l.boot[0].1);
            let share = l.diff[0] * rows.len() as f64 / gap;
            self.line(format!(
                "    {label:<8} {:>5} {:>8.5} {:>8.5} {:>+9.5} {:>5.1}%   {interval:<22} {:>+6.2}   {}",
                rows.len(),
                l.model[0],
                l.market[0],
                l.diff[0],
                100.0 * share,
                l.dm[0],
                interval_text(l.season_t[0])
            ));
            self.metric(format!("{name}_{key}_gap_share"), format!("{share:.3}"));
            self.metric(format!("{name}_{key}_n"), rows.len().to_string());
            self.metric(
                format!("{name}_{key}_model_1x2_ll"),
                format!("{:.5}", l.model[0]),
            );
            self.metric(
                format!("{name}_{key}_market_1x2_ll"),
                format!("{:.5}", l.market[0]),
            );
            self.metric(
                format!("{name}_{key}_diff_1x2_ll"),
                format!("{:.5}", l.diff[0]),
            );
        }
    }

    /// Forecast encompassing: the pool weight fitted on dev, then the pool
    /// at that weight against the market alone on the holdout.
    fn encompassing(&mut self, dev: &[&Joined], hold: &[&Joined]) {
        let (resamples, seed) = (self.resamples, self.seed);
        let observed = |m: &Joined| (m.lines[0].p3[m.outcome], m.model.p3[m.outcome]);
        let pairs: Vec<(f64, f64)> = dev.iter().map(|m| observed(m)).collect();
        let w = fit_pool(&pairs);
        let mut by_season: BTreeMap<i32, Vec<(f64, f64)>> = BTreeMap::new();
        for m in dev {
            by_season.entry(m.season).or_default().push(observed(m));
        }
        let seasons: Vec<Vec<(f64, f64)>> = by_season.values().cloned().collect();
        let (lo, hi) = pool_bootstrap(&seasons, resamples, seed);
        let ll = |w: f64, set: &[&Joined]| {
            set.iter()
                .map(|m| nll(pool(w, m.lines[0].p3, m.model.p3)[m.outcome]))
                .sum::<f64>()
                / set.len() as f64
        };
        let (pool_ll, market_ll) = (ll(w, dev), ll(0.0, dev));
        self.line(
            "\nforecast encompassing, 1X2: p = (1 - w) x market + w x model, w fitted by maximum likelihood on dev",
        );
        self.line(format!(
            "  dev w = {w:.3}, 95% season-cluster bootstrap [{lo:.3}, {hi:.3}] ({} seasons resampled); in-sample log-loss pool {pool_ll:.5}, market {market_ll:.5}",
            seasons.len()
        ));
        let each: Vec<String> = by_season
            .iter()
            .map(|(s, p)| format!("{} {:.3}", season_label(*s), fit_pool(p)))
            .collect();
        self.line(format!("  each dev season's own w: {}", each.join(", ")));
        self.metric("dev_pool_w".into(), format!("{w:.3}"));
        self.metric("dev_pool_1x2_ll".into(), format!("{pool_ll:.5}"));

        let rows: Vec<LossRow<1>> = hold
            .iter()
            .map(|m| {
                (
                    m.season,
                    m.date.as_str(),
                    [nll(m.lines[0].p3[m.outcome])],
                    [nll(pool(w, m.lines[0].p3, m.model.p3)[m.outcome])],
                )
            })
            .collect();
        let l = paired(&rows, resamples, seed);
        self.line(format!(
            "  hold, out of sample at the dev w: pool {:.5}, market {:.5}, pool-mkt {:+.5}, 95% interval [{:+.5}, {:+.5}], DM t {:+.2}, 95% season t {}",
            l.model[0],
            l.market[0],
            l.diff[0],
            l.boot[0].0,
            l.boot[0].1,
            l.dm[0],
            interval_text(l.season_t[0])
        ));
        self.metric("hold_pool_1x2_ll".into(), format!("{:.5}", l.model[0]));
        self.metric("hold_pool_diff_1x2_ll".into(), format!("{:.5}", l.diff[0]));
    }
}

/// One live match: the frozen forecast, the primary market's, the outcome
/// and the label of the model that made the forecast.
#[derive(Debug, Clone, PartialEq)]
struct LiveMatch {
    model: [f64; 3],
    market: [f64; 3],
    outcome: usize,
    label: String,
}

/// Join the forecast log to played fixtures and live odds rows by ordered
/// pair. Returns the matches with all three, and the number of scored log
/// entries without Bet365 odds. A score that disagrees with the calendar or
/// a pair logged or priced twice is an error.
fn join_live(
    log: &[LoggedForecast],
    fixtures: &[data::Fixture],
    odds: &[OddsRow],
) -> Result<(Vec<LiveMatch>, usize), String> {
    let played: HashMap<(&str, &str), (u32, u32)> = fixtures
        .iter()
        .filter_map(|f| {
            let score = (u32::from(f.home_score?), u32::from(f.away_score?));
            Some(((f.home.as_str(), f.away.as_str()), score))
        })
        .collect();
    let mut priced: HashMap<(&str, &str), &OddsRow> = HashMap::new();
    for o in odds {
        let pair = (o.home.as_str(), o.away.as_str());
        if priced.insert(pair, o).is_some() {
            return Err(format!("live odds have {} v {} twice", o.home, o.away));
        }
        match played.get(&pair) {
            Some(&score) if score == o.score => {}
            Some(&(h, a)) => {
                return Err(format!(
                    "{} v {}: live odds {}-{}, calendar {h}-{a}",
                    o.home, o.away, o.score.0, o.score.1
                ))
            }
            None => {
                return Err(format!(
                    "live odds for {} v {}, which the calendar has not played",
                    o.home, o.away
                ))
            }
        }
    }
    let mut seen: HashSet<(&str, &str)> = HashSet::new();
    let (mut out, mut no_odds) = (Vec::new(), 0);
    for f in log {
        let pair = (f.home.as_str(), f.away.as_str());
        if !seen.insert(pair) {
            return Err(format!(
                "the forecast log has {} v {} twice",
                f.home, f.away
            ));
        }
        let Some(&score) = played.get(&pair) else {
            continue;
        };
        match priced.get(&pair).and_then(|o| o.b365) {
            Some(b365) => out.push(LiveMatch {
                model: [f.home_pct, f.draw_pct, f.away_pct].map(|p| p / 100.0),
                market: shin(b365)
                    .expect("parse_odds refuses Bet365 books at or below 1")
                    .0,
                outcome: outcome(score),
                label: f
                    .model_version
                    .clone()
                    .unwrap_or_else(|| "unlabelled".into()),
            }),
            None => no_odds += 1,
        }
    }
    Ok((out, no_odds))
}

impl Report {
    fn live(&mut self, matches: &[LiveMatch], no_odds: usize, last_odds: Option<&str>) {
        let n = matches.len();
        self.line(format!(
            "\nlive 2026-27: {n} played fixtures with a frozen forecast and Bet365 odds ({no_odds} scored forecasts have no odds row; the archive's 2026-27 rows end on {})",
            last_odds.unwrap_or("-")
        ));
        self.metric("live_n".into(), n.to_string());
        if n < 2 {
            return;
        }
        let mut labels: BTreeMap<&str, usize> = BTreeMap::new();
        for m in matches {
            *labels.entry(m.label.as_str()).or_default() += 1;
        }
        let labels: Vec<String> = labels.iter().map(|(l, c)| format!("{l} {c}")).collect();
        let diffs: Vec<f64> = matches
            .iter()
            .map(|m| nll(m.model[m.outcome]) - nll(m.market[m.outcome]))
            .collect();
        let model = matches.iter().map(|m| nll(m.model[m.outcome])).sum::<f64>() / n as f64;
        let market = matches
            .iter()
            .map(|m| nll(m.market[m.outcome]))
            .sum::<f64>()
            / n as f64;
        let mean = diffs.iter().sum::<f64>() / n as f64;
        let sd = (diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0)).sqrt();
        let half = Z95 * sd / (n as f64).sqrt();
        self.line(format!("  model labels: {}", labels.join(", ")));
        self.line(format!(
            "  1X2 log-loss: model {model:.5}, market {market:.5}, model-mkt {mean:+.5}, 95% normal interval [{:+.5}, {:+.5}] (matches independent; n = {n} is far too few to separate them)",
            mean - half,
            mean + half
        ));
        self.metric("live_model_1x2_ll".into(), format!("{model:.5}"));
        self.metric("live_market_1x2_ll".into(), format!("{market:.5}"));
        self.metric("live_diff_1x2_ll".into(), format!("{mean:.5}"));
    }
}

// ------------------------------------------------------------------ main ---

struct Options {
    dump: String,
    odds: String,
    live_odds: String,
    resamples: usize,
    seed: u64,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut files = Vec::new();
    let (mut odds, mut live_odds) = (DEFAULT_ODDS.to_string(), DEFAULT_LIVE_ODDS.to_string());
    let (mut resamples, mut seed) = (DEFAULT_RESAMPLES, DEFAULT_SEED);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        let number = |flag: &str, v: String| {
            v.parse::<u64>()
                .map_err(|_| format!("{flag} needs a whole number"))
        };
        match arg.as_str() {
            "--odds" => odds = value("--odds")?,
            "--live-odds" => live_odds = value("--live-odds")?,
            "--resamples" => resamples = number("--resamples", value("--resamples")?)? as usize,
            "--seed" => seed = number("--seed", value("--seed")?)?,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            file => files.push(file.to_string()),
        }
    }
    if resamples == 0 {
        return Err("--resamples must be at least 1".into());
    }
    match <[String; 1]>::try_from(files) {
        Ok([dump]) => Ok(Options {
            dump,
            odds,
            live_odds,
            resamples,
            seed,
        }),
        Err(_) => Err("give exactly one backtest dump".into()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fail = |e: String| -> ! {
        eprintln!("market: {e}");
        std::process::exit(1)
    };
    let opts = parse_args(&args).unwrap_or_else(|e| {
        eprintln!("market: {e}\n{USAGE}");
        std::process::exit(2)
    });
    let read = |path: &str| {
        std::fs::read_to_string(path).unwrap_or_else(|e| {
            fail(format!(
                "{path}: {e} (the odds come from python3 scripts/fetch_odds.py)"
            ))
        })
    };
    let dump =
        parse_dump(&read(&opts.dump)).unwrap_or_else(|e| fail(format!("{}: {e}", opts.dump)));
    let odds =
        parse_odds(&read(&opts.odds)).unwrap_or_else(|e| fail(format!("{}: {e}", opts.odds)));
    let live_odds = parse_odds(&read(&opts.live_odds))
        .unwrap_or_else(|e| fail(format!("{}: {e}", opts.live_odds)));
    let (matches, left) = join(&dump, &odds).unwrap_or_else(|e| fail(e));
    let log: Vec<LoggedForecast> = serde_json::from_str(&read(FORECAST_LOG_PATH))
        .unwrap_or_else(|e| fail(format!("{FORECAST_LOG_PATH}: {e}")));
    let (live, no_odds) =
        join_live(&log, &data::fixtures(), &live_odds).unwrap_or_else(|e| fail(e));

    let mut report = Report {
        out: String::new(),
        summary: Vec::new(),
        resamples: opts.resamples,
        seed: opts.seed,
    };
    report.line(format!(
        "model = {} (backtest dump); odds = {}, live {}",
        opts.dump, opts.odds, opts.live_odds
    ));
    report.line(
        "odds: Bet365 and best of ~17 books, pre-match (the archive has no closing prices); offline evaluation only",
    );
    report.line("primary market: Bet365 1X2 and over/under 2.5, margin removed by Shin's method (declared in advance); checks: proportional, and the best-price line");
    report.line(format!(
        "paired block bootstrap over match dates: {} resamples, seed {}; model-mkt > 0: the market has the lower loss",
        opts.resamples, opts.seed
    ));
    report.line("season t: mean +/- t(G-1) x cluster-robust SE over the G seasons, small-sample factor G/(G-1)");
    let rows = |holdout: bool| {
        dump.iter()
            .filter(|r| HOLDOUT.contains(&r.season) == holdout)
            .count()
    };
    let dev: Vec<&Joined> = matches
        .iter()
        .filter(|m| !HOLDOUT.contains(&m.season))
        .collect();
    let hold: Vec<&Joined> = matches
        .iter()
        .filter(|m| HOLDOUT.contains(&m.season))
        .collect();
    report.set("dev", &dev, left[0], rows(false));
    report.set("hold", &hold, left[1], rows(true));
    report.encompassing(&dev, &hold);
    let last_odds = live_odds.iter().map(|o| o.date.as_str()).max();
    report.live(&live, no_odds, last_odds);
    print!("{}", report.out);

    if let Ok(path) = std::env::var("MARKET_METRICS") {
        std::fs::write(&path, metrics_json(&report.summary)).expect("write metrics");
        eprintln!("market metrics written to {path}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    /// Two equal prices of 1.90: book 2/1.9 = 1.0526. By symmetry Shin
    /// gives 1/2 each, and from Σ p_i = 1 with p_i = 1/2,
    /// √(z² + 4 (1 − z) π² / B) = 1, so z = B − 1 = 1/19.
    #[test]
    fn shin_on_an_even_two_way_book() {
        let (p, z) = shin([1.9, 1.9]).expect("a book above 1");
        assert!(close(p[0], 0.5, 1e-12) && close(p[1], 0.5, 1e-12), "{p:?}");
        assert!(close(z, 1.0 / 19.0, 1e-12), "{z}");
        assert_eq!(proportional([1.9, 1.9]), [0.5, 0.5]);
    }

    /// Prices built from p = (0.5, 0.3, 0.2) and z = 0.03 by inverting
    /// Shin's formula: π_i² / B = (1 − z) p_i² + z p_i, so √B = Σ
    /// √((1 − z) p_i² + z p_i) = 1.029427 and π = (0.522378, 0.319454,
    /// 0.217889), odds (1.91433, 3.13034, 4.58950). Shin must give back p
    /// and z; proportional (π / B) gives (0.49294, 0.30145, 0.20561): it
    /// leaves the long shot more of the margin.
    #[test]
    fn shin_recovers_a_book_built_from_known_probabilities() {
        let (p, z) = ([0.5, 0.3, 0.2], 0.03);
        let root: [f64; 3] = p.map(|q: f64| ((1.0 - z) * q * q + z * q).sqrt());
        let sqrt_book: f64 = root.iter().sum();
        assert!(close(sqrt_book, 1.029427, 1e-6), "{sqrt_book}");
        let odds = root.map(|r| 1.0 / (sqrt_book * r));
        for (o, expected) in odds.iter().zip([1.91433, 3.13034, 4.58950]) {
            assert!(close(*o, expected, 1e-5), "{odds:?}");
        }
        let (got, got_z) = shin(odds).expect("a book above 1");
        for k in 0..3 {
            assert!(close(got[k], p[k], 1e-10), "{got:?}");
        }
        assert!(close(got_z, z, 1e-10), "{got_z}");
        let prop = proportional(odds);
        for (k, expected) in [0.49294, 0.30145, 0.20561].iter().enumerate() {
            assert!(close(prop[k], *expected, 1e-5), "{prop:?}");
        }
        assert!(got[0] > prop[0] && got[2] < prop[2]);
    }

    /// 1.8, 3.6, 3.6: π = (5/9, 5/18, 5/18), book 10/9, so proportional
    /// gives (1/2, 1/4, 1/4).
    #[test]
    fn proportional_by_hand() {
        let p = proportional([1.8, 3.6, 3.6]);
        for (k, expected) in [0.5, 0.25, 0.25].iter().enumerate() {
            assert!(close(p[k], *expected, 1e-12), "{p:?}");
        }
        assert!(close(p.iter().sum::<f64>(), 1.0, 1e-12));
    }

    #[test]
    fn shin_has_no_answer_for_a_book_at_or_below_one() {
        assert_eq!(shin([2.0, 2.0]), None);
        assert_eq!(shin([2.1, 3.6, 4.5]), None);
        let (p, z) = shin_or_proportional([2.1, 3.6, 4.5]);
        assert_eq!((p, z), (proportional([2.1, 3.6, 4.5]), None));
        // Every real Bet365 book in the archive lies between these.
        for odds in [[1.25, 6.0, 11.0], [2.6, 3.1, 2.8], [1.05, 13.0, 34.0]] {
            let (p, z) = shin(odds).expect("a book above 1");
            assert!(close(p.iter().sum::<f64>(), 1.0, 1e-12) && z > 0.0 && z < 0.1);
        }
    }

    /// Ten matches where market (0.6, 0.2, 0.2) and model (0.2, 0.2, 0.6)
    /// disagree, and the outcomes split 4/2/4: the likelihood
    /// 4 ln(0.6 − 0.4 w) + 4 ln(0.2 + 0.4 w) peaks where the two terms
    /// balance, w = 0.5.
    #[test]
    fn the_pool_weight_on_a_synthetic_example() {
        let (market, model) = ([0.6, 0.2, 0.2], [0.2, 0.2, 0.6]);
        let outcomes = |h: usize, d: usize, a: usize| -> Vec<(f64, f64)> {
            [(0, h), (1, d), (2, a)]
                .iter()
                .flat_map(|&(o, n)| std::iter::repeat_n((market[o], model[o]), n))
                .collect()
        };
        assert!(close(fit_pool(&outcomes(4, 2, 4)), 0.5, 1e-12));
        // Outcomes as the market says: the model adds nothing.
        assert!(close(fit_pool(&outcomes(6, 2, 2)), 0.0, 1e-12));
        // As the model says: the slope at w = 1 is 2 (−0.4/0.2) + 6
        // (0.4/0.6) = 0, so the maximum sits on the bound.
        assert!(close(fit_pool(&outcomes(2, 2, 6)), 1.0, 1e-12));
        assert_eq!(fit_pool(&outcomes(1, 2, 7)), 1.0);
        // Resampling seasons that each hold the 4/2/4 split cannot move it.
        let seasons = vec![outcomes(4, 2, 4); 5];
        let (lo, hi) = pool_bootstrap(&seasons, 200, 3);
        assert!(
            close(lo, 0.5, 1e-12) && close(hi, 0.5, 1e-12),
            "[{lo}, {hi}]"
        );
        let p = pool(0.25, market, model);
        assert!(
            (0..3).all(|k| close(p[k], [0.5, 0.2, 0.3][k], 1e-12)),
            "{p:?}"
        );
    }

    #[test]
    fn matchday_bands() {
        assert_eq!(matchday((0, 0)), 1);
        assert_eq!(matchday((9, 8)), 9);
        assert_eq!(matchday((9, 10)), 10);
        let bands: Vec<usize> = [1, 10, 11, 22, 23, 34, 40].map(band).to_vec();
        assert_eq!(bands, vec![0, 0, 1, 1, 2, 2, 2]);
    }

    const DUMP_HEADER: &str = "season,date,home,away,hs,as,p1,px,p2,pover25,pbtts,pscore,lh,la,elo_h,elo_a,gp_h,gp_a,promo_h,promo_a\n";
    const ODDS_HEADER: &str = "season,date,home,away,hs,as,b365_h,b365_d,b365_a,max_h,max_d,max_a,b365_over25,b365_under25,max_over25,max_under25\n";

    /// A dump of `n` matches a season in 2016-17 and 2024-25, every one
    /// 1-0 at home on its own date.
    fn dump(n: usize) -> String {
        let mut s = String::from(DUMP_HEADER);
        for (year, start) in [(2017, "2016"), (2025, "2024")] {
            for i in 0..n {
                s.push_str(&format!(
                    "{year},{start}-09-{:02},H{i},A{i},1,0,0.5,0.3,0.2,0.4,0.5,0.1,1.5,1.0,1500.0,1500.0,{i},{i},0,0\n",
                    i + 1
                ));
            }
        }
        s
    }

    /// The odds rows for `dump(n)`, `edit` applied to each line.
    fn odds(n: usize, edit: impl Fn(String) -> String) -> String {
        let mut s = String::from(ODDS_HEADER);
        for (label, start) in [("2016-17", "2016"), ("2024-25", "2024")] {
            for i in 0..n {
                s.push_str(&edit(format!(
                    "{label},{start}-09-{:02},H{i},A{i},1,0,1.9,3.4,4.5,2.0,3.6,4.8,1.9,1.9,2.0,1.95\n",
                    i + 1
                )));
            }
        }
        s
    }

    fn try_join(dump_text: &str, odds_text: &str) -> Result<(Vec<Joined>, [LeftOut; 2]), String> {
        join(
            &parse_dump(dump_text).expect("dump parses"),
            &parse_odds(odds_text).expect("odds parse"),
        )
    }

    #[test]
    fn the_join_pairs_every_match() {
        let (joined, left) = try_join(&dump(20), &odds(20, |l| l)).expect("same matches");
        assert_eq!((joined.len(), left), (40, [LeftOut::default(); 2]));
        let m = &joined[3];
        assert_eq!(
            (m.season, m.date.as_str(), m.matchday, m.outcome),
            (2017, "2016-09-04", 4, 0)
        );
        assert!(!m.over && m.model.p3 == [0.5, 0.3, 0.2]);
        assert_eq!(m.lines[1].p3, proportional([1.9, 3.4, 4.5]));
        assert_eq!(
            m.lines[0].p3,
            shin([1.9, 3.4, 4.5]).expect("book above 1").0
        );
        assert!(close(m.lines[0].p_over, 0.5, 1e-12));
        // One holdout match without odds of twenty is 5%, still allowed.
        let one_short = odds(20, |l| {
            if l.contains("2024-09-01") {
                String::new()
            } else {
                l
            }
        });
        let (joined, left) = try_join(&dump(20), &one_short).expect("5% left out");
        assert_eq!(joined.len(), 39);
        assert_eq!(
            left[1],
            LeftOut {
                no_row: 1,
                incomplete: 0
            }
        );
    }

    #[test]
    fn the_join_fails_on_a_mismatched_file() {
        // Another score: the files come from different histories.
        let rescored = odds(20, |l| l.replacen(",1,0,1.9", ",2,0,1.9", 1));
        assert!(try_join(&dump(20), &rescored)
            .unwrap_err()
            .contains("different histories"));
        // Other spellings: nothing pairs, every odds row is left unused.
        let renamed = odds(20, |l| l.replace(",H", ",Home "));
        assert!(try_join(&dump(20), &renamed)
            .unwrap_err()
            .contains("match no dump row"));
        // Two holdout matches of twenty without a price: 10% left out.
        let blank = odds(20, |l| {
            if l.contains("2024-09-01") || l.contains("2024-09-02") {
                l.replacen(",1.9,3.4,", ",,3.4,", 1)
            } else {
                l
            }
        });
        let err = try_join(&dump(20), &blank).unwrap_err();
        assert!(err.contains("leaves out 2 of 20 hold matches"), "{err}");
        // A dump without holdout rows.
        let dev_only: String = dump(20)
            .lines()
            .filter(|l| !l.starts_with("2025"))
            .map(|l| format!("{l}\n"))
            .collect();
        let dev_odds: String = odds(20, |l| l)
            .lines()
            .filter(|l| !l.starts_with("2024-25"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(try_join(&dev_only, &dev_odds)
            .unwrap_err()
            .contains("EVAL_HOLDOUT=1"));
        // A Bet365 book with no margin is not a bookmaker's price.
        let fair = odds(1, |l| l.replacen(",1.9,3.4,4.5,", ",2.0,4.0,4.0,", 1));
        assert!(parse_odds(&fair).unwrap_err().contains("carry no margin"));
        // A missing column.
        assert_eq!(
            parse_odds(&odds(1, |l| l).replacen("max_under25", "max_u25", 1)).unwrap_err(),
            "no `max_under25` column"
        );
    }

    #[test]
    fn the_live_join_needs_the_calendar_score() {
        let fixture = |home: &str, away: &str, score: Option<(u16, u16)>| data::Fixture {
            round: 1,
            home: home.into(),
            away: away.into(),
            home_score: score.map(|s| s.0),
            away_score: score.map(|s| s.1),
            date: "2026-08-15".into(),
            kickoff: None,
        };
        let logged = |home: &str, away: &str| LoggedForecast {
            round: 1,
            date: "2026-08-15".into(),
            home: home.into(),
            away: away.into(),
            home_pct: 50.0,
            draw_pct: 30.0,
            away_pct: 20.0,
            logged_at: "2026-08-10".into(),
            model_version: Some("2026-08".into()),
        };
        let fixtures = [
            fixture("A", "B", Some((0, 0))),
            fixture("C", "D", Some((2, 1))),
            fixture("E", "F", None),
        ];
        let log = [logged("A", "B"), logged("C", "D"), logged("E", "F")];
        let text = format!(
            "{ODDS_HEADER}2026-27,2026-08-15,A,B,0,0,1.9,3.4,4.5,2.0,3.6,4.8,1.9,1.9,2.0,1.95\n"
        );
        let (live, no_odds) =
            join_live(&log, &fixtures, &parse_odds(&text).expect("parses")).expect("joins");
        assert_eq!((live.len(), no_odds), (1, 1));
        assert_eq!((live[0].outcome, live[0].label.as_str()), (1, "2026-08"));
        assert_eq!(live[0].model, [0.5, 0.3, 0.2]);
        let wrong = parse_odds(&text.replace(",0,0,1.9", ",1,0,1.9")).expect("parses");
        assert!(join_live(&log, &fixtures, &wrong)
            .unwrap_err()
            .contains("calendar 0-0"));
    }

    #[test]
    fn metrics_json_sorts_keys_and_keeps_printed_numbers() {
        let summary = [("b", "0.97564"), ("a", "19"), ("c", "-0.00120")]
            .map(|(k, v)| (k.to_string(), v.to_string()));
        assert_eq!(
            metrics_json(&summary),
            "{\n  \"a\": 19,\n  \"b\": 0.97564,\n  \"c\": -0.0012\n}\n"
        );
    }

    #[test]
    fn arguments() {
        let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        let o = parse_args(&args("prod.csv")).expect("one dump");
        assert_eq!(
            (o.dump.as_str(), o.odds.as_str()),
            ("prod.csv", DEFAULT_ODDS)
        );
        assert_eq!((o.resamples, o.seed), (DEFAULT_RESAMPLES, DEFAULT_SEED));
        let o =
            parse_args(&args("--odds o.csv d.csv --live-odds l.csv --seed 4")).expect("options");
        assert_eq!(
            (o.odds.as_str(), o.live_odds.as_str(), o.seed),
            ("o.csv", "l.csv", 4)
        );
        assert!(parse_args(&args("")).is_err());
        assert!(parse_args(&args("a.csv b.csv")).is_err());
        assert!(parse_args(&args("a.csv --resamples 0")).is_err());
        assert!(parse_args(&args("a.csv --odds")).is_err());
    }
}
