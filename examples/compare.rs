//! Paired comparison of two walk-forward backtest runs.
//!
//! Reads two `EVAL_DUMP` files written by `examples/backtest.rs`, pairs their
//! rows by season, date, home and away (both runs must have scored exactly
//! the same matches, with the same scores), recomputes every match's losses
//! from the dumped probabilities and prints, for development and holdout
//! seasons separately and per aspect: each run's mean loss, the mean of
//! B − A, a 95% interval for it and a Diebold-Mariano-style t.
//!
//! The interval comes from a paired block bootstrap whose blocks are match
//! dates: every match of a resampled date comes along, because a date's
//! matches are priced from the same rating state and are not independent
//! draws. The t is the mean of the per-date summed differences over its
//! standard error across dates. Negative B − A and t mean B has the lower
//! loss. The dump keeps six decimals of each probability, so a mean can
//! differ from the backtest's own summary in its last printed place.
//!
//! Season mode (`--season`) reads two `SEASON_DUMP` files written by
//! `examples/season_eval.rs` instead, pairs their rows by season, snapshot
//! and club (both runs must have scored the same club-seasons, with the
//! same final positions and points), and prints, per snapshot and pooled
//! over the snapshots, each run's mean and the mean B − A of the position
//! RPS, the CRPS of final points and the title, top-four and relegation
//! Brier scores, with a 95% interval from a season-cluster bootstrap:
//! whole seasons are resampled, every snapshot of a resampled season coming
//! along when the snapshots are pooled, because a season's clubs share one
//! table and its snapshots one history. Negative B − A means B scores
//! better. The dump keeps five decimals of each probability and score.
//!
//! Run: cargo run --release --example compare -- A.csv B.csv [--resamples 10000] [--seed 12345]
//!      cargo run --release --example compare -- --season A.csv B.csv [--resamples 10000] [--seed 12345]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The season end years `examples/backtest.rs` holds out.
const HOLDOUT: [i32; 2] = [2025, 2026];
/// The aspects scored, named as in the backtest's summary.
const ASPECTS: [&str; 5] = ["1x2_ll", "rps", "ou25_ll", "btts_ll", "score_ll"];
const DEFAULT_RESAMPLES: usize = 10_000;
const DEFAULT_SEED: u64 = 12345;
const USAGE: &str =
    "usage: cargo run --release --example compare -- [--season] A.csv B.csv [--resamples N] [--seed N]";
/// The season-mode aspects, all losses (lower is better).
const SEASON_ASPECTS: [&str; 5] = [
    "pos_rps",
    "pts_crps",
    "title_brier",
    "top4_brier",
    "rel_brier",
];

/// One scored match of a dump, with its losses in `ASPECTS` order.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    season: i32,
    date: String,
    home: String,
    away: String,
    score: (u32, u32),
    loss: [f64; 5],
}

/// One match scored by both runs.
#[derive(Debug, Clone, PartialEq)]
struct Paired {
    season: i32,
    date: String,
    a: [f64; 5],
    b: [f64; 5],
}

/// Log-loss of a forecast that gave what happened probability `p`, floored
/// as the backtest floors it.
fn nll(p: f64) -> f64 {
    -p.max(1e-9).ln()
}

/// A match's losses from its dumped probabilities: home/draw/away `p3`,
/// over 2.5 goals, both teams scoring and the exact score.
fn losses((hs, as_): (u32, u32), p3: [f64; 3], pover: f64, pbtts: f64, pscore: f64) -> [f64; 5] {
    let o = match hs.cmp(&as_) {
        std::cmp::Ordering::Greater => 0,
        std::cmp::Ordering::Equal => 1,
        std::cmp::Ordering::Less => 2,
    };
    let c1 = p3[0] - if o == 0 { 1.0 } else { 0.0 };
    let c2 = p3[0] + p3[1] - if o <= 1 { 1.0 } else { 0.0 };
    let over = hs + as_ >= 3;
    let both = hs >= 1 && as_ >= 1;
    [
        nll(p3[o]),
        (c1 * c1 + c2 * c2) / 2.0,
        nll(if over { pover } else { 1.0 - pover }),
        nll(if both { pbtts } else { 1.0 - pbtts }),
        nll(pscore),
    ]
}

/// The rows of an `EVAL_DUMP` file, columns found by their header names.
fn parse_dump(text: &str) -> Result<Vec<Row>, String> {
    let mut reader = csv::Reader::from_reader(text.as_bytes());
    let header = reader
        .headers()
        .map_err(|e| format!("header: {e}"))?
        .clone();
    let column = |name: &str| {
        header
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| format!("no `{name}` column"))
    };
    let [season, date, home, away, hs, as_, p1, px, p2, pover, pbtts, pscore] = [
        "season", "date", "home", "away", "hs", "as", "p1", "px", "p2", "pover25", "pbtts",
        "pscore",
    ]
    .map(column);
    let (season, date, home, away) = (season?, date?, home?, away?);
    let (hs, as_, p1, px, p2) = (hs?, as_?, p1?, px?, p2?);
    let (pover, pbtts, pscore) = (pover?, pbtts?, pscore?);
    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|e| format!("row {}: {e}", i + 2))?;
        let field = |c: usize| record.get(c).unwrap_or("");
        let num = |c: usize| {
            field(c)
                .parse::<f64>()
                .map_err(|_| format!("row {}: `{}` is not a number", i + 2, field(c)))
        };
        let goals = |c: usize| {
            field(c)
                .parse::<u32>()
                .map_err(|_| format!("row {}: `{}` is not a score", i + 2, field(c)))
        };
        let score = (goals(hs)?, goals(as_)?);
        rows.push(Row {
            season: field(season)
                .parse()
                .map_err(|_| format!("row {}: bad season", i + 2))?,
            date: field(date).to_string(),
            home: field(home).to_string(),
            away: field(away).to_string(),
            score,
            loss: losses(
                score,
                [num(p1)?, num(px)?, num(p2)?],
                num(pover)?,
                num(pbtts)?,
                num(pscore)?,
            ),
        });
    }
    Ok(rows)
}

/// Pair two runs' rows match by match, in A's order. Both must hold the
/// same matches once each, with the same scores.
fn pair(a: &[Row], b: &[Row]) -> Result<Vec<Paired>, String> {
    let key = |r: &Row| format!("{} {} {} v {}", r.season, r.date, r.home, r.away);
    let mut by_key: HashMap<String, &Row> = HashMap::new();
    for r in b {
        if by_key.insert(key(r), r).is_some() {
            return Err(format!("B scores {} twice", key(r)));
        }
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(a.len());
    for r in a {
        let k = key(r);
        if !seen.insert(k.clone()) {
            return Err(format!("A scores {k} twice"));
        }
        let other = by_key
            .get(&k)
            .ok_or_else(|| format!("B has no row for {k}"))?;
        if other.score != r.score {
            return Err(format!(
                "{k}: A has {}-{}, B {}-{}; the runs read different histories",
                r.score.0, r.score.1, other.score.0, other.score.1
            ));
        }
        out.push(Paired {
            season: r.season,
            date: r.date.clone(),
            a: r.loss,
            b: other.loss,
        });
    }
    if let Some(extra) = b.iter().find(|r| !seen.contains(&key(r))) {
        return Err(format!(
            "B scores {} matches A does not, e.g. {}",
            b.len() - a.len(),
            key(extra)
        ));
    }
    Ok(out)
}

/// Per match date, in date order: the summed B − A of each aspect and the
/// number of matches.
fn date_blocks(pairs: &[&Paired]) -> Vec<([f64; 5], usize)> {
    let mut by_date: BTreeMap<&str, ([f64; 5], usize)> = BTreeMap::new();
    for p in pairs {
        let block = by_date.entry(p.date.as_str()).or_default();
        for (k, sum) in block.0.iter_mut().enumerate() {
            *sum += p.b[k] - p.a[k];
        }
        block.1 += 1;
    }
    by_date.into_values().collect()
}

/// SplitMix64: a small fixed generator, so a seed gives the same intervals
/// on every platform and with every `rand` release.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform on `0..n` by multiply-shift; the bias is far below anything a
    /// few thousand dates could show.
    fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next()) * n as u128) >> 64) as usize
    }
}

/// Linear interpolation between order statistics of `sorted`.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let h = (sorted.len() - 1) as f64 * q;
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
}

/// 95% percentile intervals for each aspect's mean B − A. Each of
/// `resamples` draws picks as many dates as there are, with replacement,
/// and takes the mean over all the matches of the dates it picked.
fn bootstrap_ci(blocks: &[([f64; 5], usize)], resamples: usize, seed: u64) -> [(f64, f64); 5] {
    let mut rng = SplitMix64(seed);
    let mut draws: [Vec<f64>; 5] = Default::default();
    for _ in 0..resamples {
        let (mut sums, mut count) = ([0.0; 5], 0usize);
        for _ in 0..blocks.len() {
            let (block, n) = &blocks[rng.below(blocks.len())];
            for k in 0..5 {
                sums[k] += block[k];
            }
            count += n;
        }
        for k in 0..5 {
            draws[k].push(sums[k] / count as f64);
        }
    }
    draws.map(|mut d| {
        d.sort_by(f64::total_cmp);
        (quantile(&d, 0.025), quantile(&d, 0.975))
    })
}

/// Diebold-Mariano-style t: the mean of the per-date summed differences
/// over its standard error across dates.
fn dm_t(sums: &[f64]) -> f64 {
    let d = sums.len() as f64;
    let mean = sums.iter().sum::<f64>() / d;
    let var = sums.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (d - 1.0);
    mean / (var / d).sqrt()
}

fn season_label(year: i32) -> String {
    format!("{}-{:02}", year - 1, year % 100)
}

/// The comparison of one set of seasons, as printed.
fn report(name: &str, pairs: &[&Paired], resamples: usize, seed: u64) -> String {
    let n = pairs.len() as f64;
    let blocks = date_blocks(pairs);
    let ci = bootstrap_ci(&blocks, resamples, seed);
    let first = pairs.iter().map(|p| p.season).min().expect("non-empty");
    let last = pairs.iter().map(|p| p.season).max().expect("non-empty");
    let mut out = format!(
        "{name} seasons {}..{}: {} matches on {} dates\n  {:<9} {:>9} {:>9} {:>9}   {:<22} {:>7}\n",
        season_label(first),
        season_label(last),
        pairs.len(),
        blocks.len(),
        "aspect",
        "mean A",
        "mean B",
        "B-A",
        "95% interval",
        "DM t"
    );
    for (k, aspect) in ASPECTS.iter().enumerate() {
        let mean_a = pairs.iter().map(|p| p.a[k]).sum::<f64>() / n;
        let mean_b = pairs.iter().map(|p| p.b[k]).sum::<f64>() / n;
        let diff = pairs.iter().map(|p| p.b[k] - p.a[k]).sum::<f64>() / n;
        let sums: Vec<f64> = blocks.iter().map(|b| b.0[k]).collect();
        let interval = format!("[{:+.5}, {:+.5}]", ci[k].0, ci[k].1);
        out.push_str(&format!(
            "  {aspect:<9} {mean_a:>9.5} {mean_b:>9.5} {diff:>+9.5}   {interval:<22} {:>+7.2}\n",
            dm_t(&sums)
        ));
    }
    out
}

// ------------------------------------------------------------ season mode ---

/// One club's forecast at one snapshot of a `SEASON_DUMP`, with its losses
/// in `SEASON_ASPECTS` order.
#[derive(Debug, Clone, PartialEq)]
struct SeasonRow {
    /// Season end year (2017 is 2016-17).
    season: i32,
    snapshot: String,
    club: String,
    /// Final position (1 = champion) and points.
    outcome: (u32, i64),
    loss: [f64; 5],
}

/// One club-season-snapshot scored by both runs.
#[derive(Debug, Clone, PartialEq)]
struct SeasonPaired {
    season: i32,
    snapshot: String,
    a: [f64; 5],
    b: [f64; 5],
}

/// Season end year of a dump's `2016-17` label.
fn season_end(label: &str) -> Option<i32> {
    let (start, end) = label.split_once('-')?;
    let start: i32 = start.parse().ok()?;
    let end: i32 = end.parse().ok()?;
    (end == (start + 1) % 100).then_some(start + 1)
}

fn brier(p: f64, happened: bool) -> f64 {
    let y = if happened { 1.0 } else { 0.0 };
    (p - y) * (p - y)
}

/// The rows of a `SEASON_DUMP` file, columns found by their header names.
fn parse_season_dump(text: &str) -> Result<Vec<SeasonRow>, String> {
    let mut reader = csv::Reader::from_reader(text.as_bytes());
    let header = reader
        .headers()
        .map_err(|e| format!("header: {e}"))?
        .clone();
    let column = |name: &str| {
        header
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| format!("no `{name}` column"))
    };
    let [season, snapshot, club, clubs, relegation, pos, pts, p_title, p_top4, p_rel, rps, crps] =
        [
            "season",
            "snapshot",
            "club",
            "clubs",
            "relegation",
            "actual_pos",
            "actual_pts",
            "p_title",
            "p_top4",
            "p_rel",
            "rps",
            "pts_crps",
        ]
        .map(column);
    let (season, snapshot, club, clubs, relegation) =
        (season?, snapshot?, club?, clubs?, relegation?);
    let (pos, pts, p_title, p_top4, p_rel, rps, crps) =
        (pos?, pts?, p_title?, p_top4?, p_rel?, rps?, crps?);
    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|e| format!("row {}: {e}", i + 2))?;
        let field = |c: usize| record.get(c).unwrap_or("");
        let bad = |c: usize| format!("row {}: `{}` is not a number", i + 2, field(c));
        let num = |c: usize| field(c).parse::<f64>().map_err(|_| bad(c));
        let count = |c: usize| field(c).parse::<u32>().map_err(|_| bad(c));
        let (n_clubs, places, position) = (count(clubs)?, count(relegation)?, count(pos)?);
        if position == 0 || position > n_clubs || places >= n_clubs {
            return Err(format!(
                "row {}: position {position} of {n_clubs} clubs, {places} relegated",
                i + 2
            ));
        }
        let top4 = position as usize <= superlig_sim::data::TOP_FOUR_PLACES;
        rows.push(SeasonRow {
            season: season_end(field(season))
                .ok_or_else(|| format!("row {}: bad season `{}`", i + 2, field(season)))?,
            snapshot: field(snapshot).to_string(),
            club: field(club).to_string(),
            outcome: (position, field(pts).parse().map_err(|_| bad(pts))?),
            loss: [
                num(rps)?,
                num(crps)?,
                brier(num(p_title)?, position == 1),
                brier(num(p_top4)?, top4),
                brier(num(p_rel)?, position > n_clubs - places),
            ],
        });
    }
    Ok(rows)
}

/// Pair two season runs row by row, in A's order. Both must hold the same
/// club-seasons and snapshots once each, with the same final tables.
fn pair_seasons(a: &[SeasonRow], b: &[SeasonRow]) -> Result<Vec<SeasonPaired>, String> {
    let key = |r: &SeasonRow| format!("{} {} {}", season_label(r.season), r.snapshot, r.club);
    let mut by_key: HashMap<String, &SeasonRow> = HashMap::new();
    for r in b {
        if by_key.insert(key(r), r).is_some() {
            return Err(format!("B scores {} twice", key(r)));
        }
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(a.len());
    for r in a {
        let k = key(r);
        if !seen.insert(k.clone()) {
            return Err(format!("A scores {k} twice"));
        }
        let other = by_key
            .get(&k)
            .ok_or_else(|| format!("B has no row for {k}"))?;
        if other.outcome != r.outcome {
            return Err(format!(
                "{k}: A has position {} on {} points, B {} on {}; the runs read different histories",
                r.outcome.0, r.outcome.1, other.outcome.0, other.outcome.1
            ));
        }
        out.push(SeasonPaired {
            season: r.season,
            snapshot: r.snapshot.clone(),
            a: r.loss,
            b: other.loss,
        });
    }
    if let Some(extra) = b.iter().find(|r| !seen.contains(&key(r))) {
        return Err(format!(
            "B scores {} rows A does not, e.g. {}",
            b.len() - a.len(),
            key(extra)
        ));
    }
    Ok(out)
}

/// Per season, in season order: the summed B − A of each aspect and the
/// number of rows. The blocks `bootstrap_ci` resamples.
fn season_blocks(pairs: &[&SeasonPaired]) -> Vec<([f64; 5], usize)> {
    let mut by_season: BTreeMap<i32, ([f64; 5], usize)> = BTreeMap::new();
    for p in pairs {
        let block = by_season.entry(p.season).or_default();
        for (k, sum) in block.0.iter_mut().enumerate() {
            *sum += p.b[k] - p.a[k];
        }
        block.1 += 1;
    }
    by_season.into_values().collect()
}

/// One aspect of a season comparison: mean A, mean B, mean B − A and its
/// 95% season-cluster bootstrap interval.
type SeasonLine = (f64, f64, f64, (f64, f64));

/// Every aspect's `SeasonLine` over a set of paired rows.
fn season_summary(pairs: &[&SeasonPaired], resamples: usize, seed: u64) -> [SeasonLine; 5] {
    let n = pairs.len() as f64;
    let ci = bootstrap_ci(&season_blocks(pairs), resamples, seed);
    std::array::from_fn(|k| {
        let mean_a = pairs.iter().map(|p| p.a[k]).sum::<f64>() / n;
        let mean_b = pairs.iter().map(|p| p.b[k]).sum::<f64>() / n;
        let diff = pairs.iter().map(|p| p.b[k] - p.a[k]).sum::<f64>() / n;
        (mean_a, mean_b, diff, ci[k])
    })
}

/// The season comparison of one set of seasons, as printed: every snapshot
/// in the order the dump has them, then all of them pooled.
fn season_report(name: &str, pairs: &[&SeasonPaired], resamples: usize, seed: u64) -> String {
    let seasons: BTreeSet<i32> = pairs.iter().map(|p| p.season).collect();
    let mut snapshots: Vec<&str> = Vec::new();
    for p in pairs {
        if !snapshots.contains(&p.snapshot.as_str()) {
            snapshots.push(&p.snapshot);
        }
    }
    let mut out = format!(
        "{name} seasons {}..{}: {} seasons, {} snapshots, {} club rows\n",
        season_label(*seasons.first().expect("non-empty")),
        season_label(*seasons.last().expect("non-empty")),
        seasons.len(),
        snapshots.len(),
        pairs.len()
    );
    let groups = snapshots
        .iter()
        .map(|&s| {
            let set: Vec<&SeasonPaired> =
                pairs.iter().copied().filter(|p| p.snapshot == s).collect();
            (s.to_string(), set)
        })
        .chain(std::iter::once(("pooled".to_string(), pairs.to_vec())));
    for (label, set) in groups {
        out.push_str(&format!(
            "  {label} ({} rows)\n    {:<11} {:>9} {:>9} {:>9}   {:<22}\n",
            set.len(),
            "aspect",
            "mean A",
            "mean B",
            "B-A",
            "95% interval"
        ));
        for (k, (mean_a, mean_b, diff, (lo, hi))) in season_summary(&set, resamples, seed)
            .into_iter()
            .enumerate()
        {
            let interval = format!("[{lo:+.5}, {hi:+.5}]");
            out.push_str(&format!(
                "    {:<11} {mean_a:>9.5} {mean_b:>9.5} {diff:>+9.5}   {interval:<22}\n",
                SEASON_ASPECTS[k]
            ));
        }
    }
    out
}

struct Options {
    a: String,
    b: String,
    resamples: usize,
    seed: u64,
    /// Compare `examples/season_eval.rs` dumps instead of backtest dumps.
    season: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let (mut files, mut resamples, mut seed) = (Vec::new(), DEFAULT_RESAMPLES, DEFAULT_SEED);
    let mut season = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| format!("{flag} needs a whole number"))
        };
        match arg.as_str() {
            "--resamples" => resamples = value("--resamples")? as usize,
            "--seed" => seed = value("--seed")?,
            "--season" => season = true,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            file => files.push(file.to_string()),
        }
    }
    if resamples == 0 {
        return Err("--resamples must be at least 1".into());
    }
    match <[String; 2]>::try_from(files) {
        Ok([a, b]) => Ok(Options {
            a,
            b,
            resamples,
            seed,
            season,
        }),
        Err(_) => Err("give exactly two dump files".into()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fail = |e: String| -> ! {
        eprintln!("compare: {e}");
        std::process::exit(1)
    };
    let opts = parse_args(&args).unwrap_or_else(|e| {
        eprintln!("compare: {e}\n{USAGE}");
        std::process::exit(2)
    });
    let read =
        |path: &str| std::fs::read_to_string(path).unwrap_or_else(|e| fail(format!("{path}: {e}")));
    if opts.season {
        let load = |path: &str| {
            parse_season_dump(&read(path)).unwrap_or_else(|e| fail(format!("{path}: {e}")))
        };
        let (a, b) = (load(&opts.a), load(&opts.b));
        let pairs = pair_seasons(&a, &b).unwrap_or_else(|e| fail(e));
        println!("A = {}\nB = {}", opts.a, opts.b);
        println!(
            "paired season-cluster bootstrap: {} resamples, seed {}; B-A < 0: B scores better",
            opts.resamples, opts.seed
        );
        for (name, holdout) in [("dev", false), ("hold", true)] {
            let set: Vec<&SeasonPaired> = pairs
                .iter()
                .filter(|p| HOLDOUT.contains(&p.season) == holdout)
                .collect();
            if !set.is_empty() {
                print!("\n{}", season_report(name, &set, opts.resamples, opts.seed));
            }
        }
        return;
    }
    let load =
        |path: &str| parse_dump(&read(path)).unwrap_or_else(|e| fail(format!("{path}: {e}")));
    let (a, b) = (load(&opts.a), load(&opts.b));
    let pairs = pair(&a, &b).unwrap_or_else(|e| fail(e));
    println!("A = {}\nB = {}", opts.a, opts.b);
    println!(
        "paired block bootstrap over match dates: {} resamples, seed {}; B-A < 0: B has the lower loss",
        opts.resamples, opts.seed
    );
    for (name, holdout) in [("dev", false), ("hold", true)] {
        let set: Vec<&Paired> = pairs
            .iter()
            .filter(|p| HOLDOUT.contains(&p.season) == holdout)
            .collect();
        if !set.is_empty() {
            print!("\n{}", report(name, &set, opts.resamples, opts.seed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "season,date,home,away,hs,as,p1,px,p2,pover25,pbtts,pscore,lh,la,elo_h,elo_a,gp_h,gp_a,promo_h,promo_a\n";

    fn row(date: &str, home: &str, score: (u32, u32), p: f64) -> String {
        format!(
            "2017,{date},{home},Away,{},{},{p},0.3,{},0.6,0.55,0.1,1.4,1.1,1500.0,1500.0,0,0,0,0\n",
            score.0,
            score.1,
            0.7 - p
        )
    }

    /// Pairs whose B − A is `diffs[i]` in every aspect, on the dates given.
    fn paired(diffs: &[(&str, f64)]) -> Vec<Paired> {
        diffs
            .iter()
            .map(|&(date, d)| Paired {
                season: 2017,
                date: date.to_string(),
                a: [1.0; 5],
                b: [1.0 + d; 5],
            })
            .collect()
    }

    /// A distinct date key per `i`, in order.
    fn date(i: usize) -> String {
        format!("day-{i:04}")
    }

    #[test]
    fn losses_by_hand() {
        let l = losses((2, 1), [0.5, 0.3, 0.2], 0.6, 0.55, 0.1);
        let expected = [
            2f64.ln(),
            0.145,
            -(0.6f64.ln()),
            -(0.55f64.ln()),
            10f64.ln(),
        ];
        for k in 0..5 {
            assert!((l[k] - expected[k]).abs() < 1e-12, "{}", ASPECTS[k]);
        }
        // A goalless draw: under, nobody scored, draw probability counts.
        let l = losses((0, 0), [0.5, 0.3, 0.2], 0.6, 0.55, 0.1);
        assert!((l[0] + 0.3f64.ln()).abs() < 1e-12);
        assert!((l[2] + 0.4f64.ln()).abs() < 1e-12);
        assert!((l[3] + 0.45f64.ln()).abs() < 1e-12);
    }

    #[test]
    fn parse_dump_reads_the_backtest_columns_by_name() {
        let text = format!("{HEADER}{}", row("2016-08-19", "Home", (2, 1), 0.5));
        let rows = parse_dump(&text).expect("parses");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].season, rows[0].date.as_str()),
            (2017, "2016-08-19")
        );
        assert_eq!(rows[0].score, (2, 1));
        assert!((rows[0].loss[0] - 2f64.ln()).abs() < 1e-12);
        let missing = text.replacen("pbtts", "p_btts", 1);
        assert_eq!(parse_dump(&missing).unwrap_err(), "no `pbtts` column");
    }

    #[test]
    fn pairing_needs_the_same_matches_with_the_same_scores() {
        let rows = |text: String| parse_dump(&format!("{HEADER}{text}")).expect("parses");
        let a = rows(row("2016-08-19", "X", (1, 0), 0.5) + &row("2016-08-20", "Y", (0, 0), 0.4));
        // Same matches in another order, other forecasts: pairs fine.
        let b = rows(row("2016-08-20", "Y", (0, 0), 0.2) + &row("2016-08-19", "X", (1, 0), 0.6));
        let p = pair(&a, &b).expect("same matches");
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].date, "2016-08-19");
        assert!((p[0].b[0] - p[0].a[0] - (0.5f64 / 0.6).ln()).abs() < 1e-12);

        let fewer = rows(row("2016-08-19", "X", (1, 0), 0.5));
        assert!(pair(&a, &fewer).unwrap_err().starts_with("B has no row"));
        assert!(pair(&fewer, &a)
            .unwrap_err()
            .starts_with("B scores 1 matches A does not"));
        let twice = rows(row("2016-08-19", "X", (1, 0), 0.5).repeat(2));
        assert!(pair(&twice, &twice).unwrap_err().contains("twice"));
        let rescored =
            rows(row("2016-08-19", "X", (2, 0), 0.5) + &row("2016-08-20", "Y", (0, 0), 0.4));
        assert!(pair(&a, &rescored)
            .unwrap_err()
            .contains("different histories"));
    }

    #[test]
    fn bootstrap_is_reproducible_for_a_seed() {
        let dates: Vec<String> = (0..60).map(date).collect();
        let diffs: Vec<(&str, f64)> = dates
            .iter()
            .enumerate()
            .map(|(i, d)| (d.as_str(), ((i * 37) % 11) as f64 / 10.0 - 0.4))
            .collect();
        let p = paired(&diffs);
        let blocks = date_blocks(&p.iter().collect::<Vec<_>>());
        let first = bootstrap_ci(&blocks, 2000, 7);
        assert_eq!(first, bootstrap_ci(&blocks, 2000, 7));
        assert_ne!(first, bootstrap_ci(&blocks, 2000, 8));
    }

    /// Dates hold one to four matches, so a resample's mean must be taken
    /// over its matches, not its dates, for every draw to come out at 0.25.
    #[test]
    fn a_constant_difference_has_a_zero_width_interval() {
        let dates: Vec<String> = (0..30).map(date).collect();
        let diffs: Vec<(&str, f64)> = dates
            .iter()
            .enumerate()
            .flat_map(|(i, d)| std::iter::repeat_n((d.as_str(), 0.25), 1 + i % 4))
            .collect();
        let p = paired(&diffs);
        let blocks = date_blocks(&p.iter().collect::<Vec<_>>());
        for (lo, hi) in bootstrap_ci(&blocks, 1000, 1) {
            assert!((lo - 0.25).abs() < 1e-12 && (hi - 0.25).abs() < 1e-12);
        }
    }

    /// Two matches a date, +1 and −1: every date sums to zero, so resampling
    /// whole dates can never move the mean, though single matches vary.
    #[test]
    fn whole_dates_are_resampled_together() {
        let dates: Vec<String> = (0..40).map(date).collect();
        let diffs: Vec<(&str, f64)> = dates
            .iter()
            .flat_map(|d| [(d.as_str(), 1.0), (d.as_str(), -1.0)])
            .collect();
        let p = paired(&diffs);
        let blocks = date_blocks(&p.iter().collect::<Vec<_>>());
        assert_eq!(blocks.len(), 40);
        assert!(blocks.iter().all(|b| b.1 == 2));
        for (lo, hi) in bootstrap_ci(&blocks, 1000, 3) {
            assert_eq!((lo, hi), (0.0, 0.0));
        }
    }

    /// Differences of 0 or 1 a date, half each: the interval sits around the
    /// mean 0.5, close to ±1.96 · 0.5 / √n, and halves with four times the
    /// dates.
    #[test]
    fn the_interval_brackets_the_mean_and_shrinks_with_more_dates() {
        let width = |n: usize| {
            let dates: Vec<String> = (0..n).map(date).collect();
            let diffs: Vec<(&str, f64)> = dates
                .iter()
                .enumerate()
                .map(|(i, d)| (d.as_str(), (i % 2) as f64))
                .collect();
            let p = paired(&diffs);
            let blocks = date_blocks(&p.iter().collect::<Vec<_>>());
            let (lo, hi) = bootstrap_ci(&blocks, 10_000, 11)[0];
            assert!(lo < 0.5 && 0.5 < hi, "[{lo}, {hi}]");
            let normal = 2.0 * 1.96 * 0.5 / (n as f64).sqrt();
            assert!(
                (hi - lo) / normal > 0.85 && (hi - lo) / normal < 1.15,
                "{n}: {}",
                hi - lo
            );
            hi - lo
        };
        let ratio = width(100) / width(400);
        assert!(ratio > 1.7 && ratio < 2.3, "ratio {ratio}");
    }

    #[test]
    fn dm_t_has_the_sign_of_the_worse_model() {
        // Per-date sums 1, 2, 3: mean 2, sd 1, so t = 2 / (1 / √3).
        assert!((dm_t(&[1.0, 2.0, 3.0]) - 2.0 * 3f64.sqrt()).abs() < 1e-12);
        let worse_b = [0.3, -0.1, 0.2, 0.4, 0.0, 0.1];
        let better_b = worse_b.map(|d: f64| -d);
        assert!(dm_t(&worse_b) > 0.0);
        assert_eq!(dm_t(&better_b), -dm_t(&worse_b));
    }

    #[test]
    fn arguments() {
        let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        let o = parse_args(&args("a.csv b.csv")).expect("two files");
        assert_eq!((o.a.as_str(), o.b.as_str()), ("a.csv", "b.csv"));
        assert_eq!((o.resamples, o.seed), (DEFAULT_RESAMPLES, DEFAULT_SEED));
        let o = parse_args(&args("--seed 9 a.csv --resamples 500 b.csv")).expect("options");
        assert_eq!((o.resamples, o.seed), (500, 9));
        assert!(parse_args(&args("a.csv")).is_err());
        assert!(parse_args(&args("a.csv b.csv c.csv")).is_err());
        assert!(parse_args(&args("a.csv b.csv --seed")).is_err());
        assert!(parse_args(&args("a.csv b.csv --resamples 0")).is_err());
        assert!(parse_args(&args("a.csv b.csv --fast")).is_err());
        assert!(!parse_args(&args("a.csv b.csv")).expect("two files").season);
        let o = parse_args(&args("--season a.csv b.csv --seed 3")).expect("season mode");
        assert!(o.season);
        assert_eq!((o.a.as_str(), o.b.as_str(), o.seed), ("a.csv", "b.csv", 3));
    }

    const SEASON_HEADER: &str = "season,snapshot,club,clubs,played,relegation,actual_pos,actual_pts,mean_pts,sd_pts,p_title,p_top4,p_rel,pit,rps,pts_crps\n";

    /// A season dump: `seasons` seasons of three snapshots of an 18-club
    /// table with three relegated and made-up forecasts, every `rps` and
    /// `pts_crps` raised by `shift`.
    fn season_dump(seasons: usize, shift: f64) -> String {
        let mut s = String::from(SEASON_HEADER);
        for y in 0..seasons {
            let label = format!("{}-{:02}", 2016 + y, (17 + y) % 100);
            for snapshot in ["s0", "s33", "s67"] {
                for c in 0..18usize {
                    let pos = (c + y) % 18 + 1;
                    let p = ((c * 7 + y * 3) % 10) as f64 / 10.0;
                    s.push_str(&format!(
                        "{label},{snapshot},Club {c},18,0,3,{pos},{},50.0,8.0,{p},{},{},0.5,{},{}\n",
                        70 - 3 * pos,
                        1.0 - p,
                        p / 2.0,
                        0.01 * c as f64 + 0.05 + shift,
                        4.0 + (c % 5) as f64 + shift,
                    ));
                }
            }
        }
        s
    }

    #[test]
    fn season_dumps_are_read_by_column_name_and_scored_by_hand() {
        let text = format!(
            "{SEASON_HEADER}2016-17,s33,Galatasaray,18,102,3,1,75,70.5,7.1,0.6,0.9,0.0,0.4,0.03,4.25\n\
             2016-17,s33,Adanaspor,18,102,3,17,25,30.0,7.1,0.0,0.01,0.7,0.4,0.05,3.5\n"
        );
        let rows = parse_season_dump(&text).expect("parses");
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].season, rows[0].snapshot.as_str()), (2017, "s33"));
        assert_eq!(rows[0].outcome, (1, 75));
        let expected = [0.03, 4.25, 0.16, 0.01, 0.0];
        for k in 0..5 {
            assert!(
                (rows[0].loss[k] - expected[k]).abs() < 1e-12,
                "{}",
                SEASON_ASPECTS[k]
            );
        }
        // Seventeenth of eighteen with three down: relegated, outside the
        // top four.
        let expected = [0.05, 3.5, 0.0, 0.0001, 0.09];
        for k in 0..5 {
            assert!(
                (rows[1].loss[k] - expected[k]).abs() < 1e-12,
                "{}",
                SEASON_ASPECTS[k]
            );
        }
        assert_eq!(
            parse_season_dump(&text.replacen("pts_crps", "crps", 1)).unwrap_err(),
            "no `pts_crps` column"
        );
        assert_eq!(season_end("2019-20"), Some(2020));
        assert_eq!(season_end("1999-00"), Some(2000));
        assert_eq!(season_end("2019-21"), None);
    }

    #[test]
    fn identical_season_dumps_differ_by_zero_with_no_width() {
        let rows = parse_season_dump(&season_dump(8, 0.0)).expect("parses");
        let pairs = pair_seasons(&rows, &rows).expect("same rows");
        assert_eq!(pairs.len(), 8 * 3 * 18);
        let all: Vec<&SeasonPaired> = pairs.iter().collect();
        for snapshot in ["s0", "s33", "s67"] {
            let set: Vec<&SeasonPaired> = pairs.iter().filter(|p| p.snapshot == snapshot).collect();
            for (_, _, diff, ci) in season_summary(&set, 1000, 1) {
                assert_eq!((diff, ci), (0.0, (0.0, 0.0)));
            }
        }
        for (mean_a, mean_b, diff, ci) in season_summary(&all, 1000, 1) {
            assert_eq!(mean_a, mean_b);
            assert_eq!((diff, ci), (0.0, (0.0, 0.0)));
        }
        let report = season_report("dev", &all, 200, 1);
        assert!(
            report.contains("8 seasons, 3 snapshots, 432 club rows"),
            "{report}"
        );
        assert!(report.contains("  pooled (432 rows)"), "{report}");
    }

    #[test]
    fn a_constant_season_shift_is_recovered_exactly() {
        let a = parse_season_dump(&season_dump(6, 0.0)).expect("parses");
        let b = parse_season_dump(&season_dump(6, 0.25)).expect("parses");
        let pairs = pair_seasons(&a, &b).expect("same club-seasons");
        let all: Vec<&SeasonPaired> = pairs.iter().collect();
        let summary = season_summary(&all, 2000, 9);
        for (k, &(mean_a, mean_b, diff, (lo, hi))) in summary.iter().enumerate().take(2) {
            assert!(
                (mean_b - mean_a - 0.25).abs() < 1e-9,
                "{}",
                SEASON_ASPECTS[k]
            );
            for x in [diff, lo, hi] {
                assert!((x - 0.25).abs() < 1e-9, "{}: {x}", SEASON_ASPECTS[k]);
            }
        }
        // The forecasts of events were not touched.
        for (_, _, diff, ci) in &summary[2..] {
            assert_eq!((*diff, *ci), (0.0, (0.0, 0.0)));
        }

        // Every aspect, seasons of different sizes: the mean is over rows.
        let shifted: Vec<SeasonPaired> = (0..40)
            .map(|i| SeasonPaired {
                season: 2017 + (i % 5) as i32,
                snapshot: ["s0", "s33"][i % 2].to_string(),
                a: [0.1 * i as f64; 5],
                b: [0.1 * i as f64 - 0.03; 5],
            })
            .chain((0..3).map(|_| SeasonPaired {
                season: 2030,
                snapshot: "s0".to_string(),
                a: [1.0; 5],
                b: [0.97; 5],
            }))
            .collect();
        let refs: Vec<&SeasonPaired> = shifted.iter().collect();
        for (_, _, diff, (lo, hi)) in season_summary(&refs, 500, 4) {
            for x in [diff, lo, hi] {
                assert!((x + 0.03).abs() < 1e-9, "{x}");
            }
        }
    }

    /// A season's own rows move together: two seasons, one +1 and one −1
    /// throughout, give an interval from −1 to +1, not one around 0 that
    /// treats every club as independent.
    #[test]
    fn whole_seasons_are_resampled_together() {
        let pairs: Vec<SeasonPaired> = (0..2)
            .flat_map(|s| {
                (0..54).map(move |i| SeasonPaired {
                    season: 2017 + s,
                    snapshot: ["s0", "s33", "s67"][i % 3].to_string(),
                    a: [0.0; 5],
                    b: [if s == 0 { 1.0 } else { -1.0 }; 5],
                })
            })
            .collect();
        let all: Vec<&SeasonPaired> = pairs.iter().collect();
        assert_eq!(season_blocks(&all).len(), 2);
        let (_, _, diff, (lo, hi)) = season_summary(&all, 2000, 2)[0];
        assert_eq!(diff, 0.0);
        assert_eq!((lo, hi), (-1.0, 1.0));
    }

    #[test]
    fn season_pairing_needs_the_same_rows_with_the_same_tables() {
        let a = parse_season_dump(&season_dump(2, 0.0)).expect("parses");
        let fewer = &a[..a.len() - 1];
        assert!(pair_seasons(&a, fewer)
            .unwrap_err()
            .starts_with("B has no row"));
        assert!(pair_seasons(fewer, &a)
            .unwrap_err()
            .starts_with("B scores 1 rows A does not"));
        let mut moved = a.clone();
        moved[5].outcome.1 += 1;
        assert!(pair_seasons(&a, &moved)
            .unwrap_err()
            .contains("different histories"));
        let mut twice = a.clone();
        twice.push(a[0].clone());
        assert!(pair_seasons(&twice, &twice).unwrap_err().contains("twice"));
    }
}
