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
//! Run: cargo run --release --example compare -- A.csv B.csv [--resamples 10000] [--seed 12345]

use std::collections::{BTreeMap, HashMap, HashSet};

/// The season end years `examples/backtest.rs` holds out.
const HOLDOUT: [i32; 2] = [2025, 2026];
/// The aspects scored, named as in the backtest's summary.
const ASPECTS: [&str; 5] = ["1x2_ll", "rps", "ou25_ll", "btts_ll", "score_ll"];
const DEFAULT_RESAMPLES: usize = 10_000;
const DEFAULT_SEED: u64 = 12345;
const USAGE: &str =
    "usage: cargo run --release --example compare -- A.csv B.csv [--resamples N] [--seed N]";

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

struct Options {
    a: String,
    b: String,
    resamples: usize,
    seed: u64,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let (mut files, mut resamples, mut seed) = (Vec::new(), DEFAULT_RESAMPLES, DEFAULT_SEED);
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
    let load = |path: &str| {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(format!("{path}: {e}")));
        parse_dump(&text).unwrap_or_else(|e| fail(format!("{path}: {e}")))
    };
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

    #[test]
    fn a_constant_difference_has_a_zero_width_interval() {
        let dates: Vec<String> = (0..30).map(date).collect();
        let diffs: Vec<(&str, f64)> = dates.iter().map(|d| (d.as_str(), 0.25)).collect();
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
    }
}
