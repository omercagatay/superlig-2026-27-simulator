//! Model arena: backtest candidate strength models against held-out seasons.
//!
//! Protocol (mirrors how the simulator is deployed — predict a whole season
//! ahead with ratings frozen at its start):
//!   - validation: fit on 2012-13..2023-24, predict 2024-25 (tunes
//!     hyperparameters and ensemble weights)
//!   - test:       fit on 2012-13..2024-25, predict 2025-26 (touched once,
//!     to produce the final leaderboard)
//!
//! Every model outputs P(home win), P(draw), P(away win) per match; scored on
//! log-loss (primary), ranked probability score, Brier, and accuracy.
//!
//! Run: cargo run --release --example arena

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};

use superlig_sim::dixoncoles::{self, DcParams};
use superlig_sim::history::{self, HistoricalMatch, TeamIndex};
use superlig_sim::piratings::PiRatings;

// ---------------------------------------------------------------- splits ---

struct Split {
    name: &'static str,
    train: Vec<HistoricalMatch>,
    eval: Vec<HistoricalMatch>,
    as_of: NaiveDate,
}

/// Seasons carry a nominal date of 1 Jan of their second year, so a season
/// is selected by that year.
fn split(eval_year: i32) -> Split {
    let all = history::load_history_with_cutoff(2012);
    let train: Vec<_> = all
        .iter()
        .filter(|m| m.date.year() < eval_year)
        .cloned()
        .collect();
    let eval: Vec<_> = all
        .iter()
        .filter(|m| m.date.year() == eval_year)
        .cloned()
        .collect();
    assert!(
        !train.is_empty() && !eval.is_empty(),
        "empty split for {eval_year}"
    );
    Split {
        name: if eval_year == 2026 {
            "test 2025-26"
        } else {
            "validation 2024-25"
        },
        train,
        eval,
        as_of: NaiveDate::from_ymd_opt(eval_year - 1, 7, 1).expect("valid date"),
    }
}

/// Index over the clubs of the evaluation season plus the departed-club
/// bucket, so DC/pi dimensions match what they must predict.
fn index_for(eval: &[HistoricalMatch]) -> TeamIndex {
    let mut names: Vec<String> = Vec::new();
    for m in eval {
        for t in [&m.home_team, &m.away_team] {
            if !names.contains(t) {
                names.push(t.clone());
            }
        }
    }
    names.sort();
    let mut idx_to_name = names.clone();
    idx_to_name.push(history::OTHER_TEAM_NAME.to_string());
    let other_idx = idx_to_name.len() - 1;
    let name_to_idx = idx_to_name
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();
    TeamIndex {
        name_to_idx,
        idx_to_name,
        league_names: names,
        other_idx,
    }
}

// --------------------------------------------------------------- metrics ---

#[derive(Clone, Copy, Default)]
struct Score {
    log_loss: f64,
    rps: f64,
    brier: f64,
    hits: usize,
    n: usize,
}

fn outcome(m: &HistoricalMatch) -> usize {
    match m.home_score.cmp(&m.away_score) {
        std::cmp::Ordering::Greater => 0,
        std::cmp::Ordering::Equal => 1,
        std::cmp::Ordering::Less => 2,
    }
}

fn evaluate(
    eval: &[HistoricalMatch],
    mut predict: impl FnMut(&HistoricalMatch) -> [f64; 3],
) -> Score {
    let mut s = Score::default();
    for m in eval {
        let p = predict(m);
        let o = outcome(m);
        s.log_loss += -(p[o].max(1e-9)).ln();
        // RPS over the ordered outcomes home < draw < away.
        let (mut cp, mut co, mut rps) = (0.0, 0.0, 0.0);
        for (k, &pk) in p.iter().enumerate().take(2) {
            cp += pk;
            co += if o == k { 1.0 } else { 0.0 };
            rps += (cp - co) * (cp - co);
        }
        s.rps += rps / 2.0;
        for (k, &pk) in p.iter().enumerate() {
            let ok = if o == k { 1.0 } else { 0.0 };
            s.brier += (pk - ok) * (pk - ok);
        }
        if p.iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0
            == o
        {
            s.hits += 1;
        }
        s.n += 1;
    }
    s
}

fn table_probs(lh: f64, la: f64, rho: f64) -> [f64; 3] {
    let (w, d, l) = dixoncoles::match_probs(lh, la, rho);
    let t = w + d + l;
    [w / t, d / t, l / t]
}

// ------------------------------------------------------------ components ---

/// Elo computed from the match history itself (ClubElo's own method), which
/// is what makes the Elo family backtestable at all: historical ClubElo
/// snapshots are not obtainable here, but the results that generate them are.
struct SelfElo {
    ratings: HashMap<String, f64>,
    mean_goals: f64,
}

impl SelfElo {
    const K: f64 = 20.0;
    const HOME_FIELD: f64 = 65.0;

    fn walk(train: &[HistoricalMatch]) -> Self {
        let mut ordered: Vec<&HistoricalMatch> = train.iter().collect();
        ordered.sort_by_key(|m| m.date);
        let mut ratings: HashMap<String, f64> = HashMap::new();
        let mut goals = 0u64;
        for m in &ordered {
            let rh = *ratings.entry(m.home_team.clone()).or_insert(1500.0);
            let ra = *ratings.entry(m.away_team.clone()).or_insert(1500.0);
            let e = 1.0 / (1.0 + 10f64.powf(-(rh - ra + Self::HOME_FIELD) / 400.0));
            let s = match m.home_score.cmp(&m.away_score) {
                std::cmp::Ordering::Greater => 1.0,
                std::cmp::Ordering::Equal => 0.5,
                std::cmp::Ordering::Less => 0.0,
            };
            let gd = (m.home_score as f64 - m.away_score as f64).abs().max(1.0);
            let delta = Self::K * gd.sqrt() * (s - e);
            *ratings.get_mut(&m.home_team).expect("inserted") += delta;
            *ratings.get_mut(&m.away_team).expect("inserted") -= delta;
            goals += (m.home_score + m.away_score) as u64;
        }
        SelfElo {
            ratings,
            mean_goals: goals as f64 / ordered.len() as f64,
        }
    }

    fn rating(&self, team: &str) -> f64 {
        // Unseen (newly promoted) clubs enter below par, like real promotees.
        self.ratings.get(team).copied().unwrap_or(1450.0)
    }

    /// (λ_home, λ_away) via the production Elo→goals mapping, with tunable
    /// scale `d_div` and home advantage `home_adv` in rating points.
    fn lam(&self, m: &HistoricalMatch, d_div: f64, home_adv: f64) -> (f64, f64) {
        let base = self.mean_goals / 2.0;
        let dr = self.rating(&m.home_team) - self.rating(&m.away_team) + home_adv;
        let lh = base * 10f64.powf(dr / d_div);
        let la = base * 10f64.powf(-dr / d_div);
        (lh.clamp(0.15, 5.0), la.clamp(0.15, 5.0))
    }
}

struct Fitted {
    idx: TeamIndex,
    dc: DcParams,
    dc_short: DcParams,
    pi: PiRatings,
    elo: SelfElo,
    base_rates: [f64; 3],
    mean_hg: f64,
    mean_ag: f64,
}

fn fit_all(s: &Split) -> Fitted {
    let idx = index_for(&s.eval);

    let fits = history::prepare_fit_matches(&s.train, &idx, 1460.0, s.as_of);
    let dc = dixoncoles::fit(&fits, &idx, 1460.0, 300).expect("dc fit");
    let fits_short = history::prepare_fit_matches(&s.train, &idx, 550.0, s.as_of);
    let dc_short = dixoncoles::fit(&fits_short, &idx, 550.0, 300).expect("dc short fit");

    let since = NaiveDate::from_ymd_opt(2012, 1, 1).expect("valid date");
    let pi = PiRatings::compute(&s.train, &idx, since);
    let elo = SelfElo::walk(&s.train);

    let n = s.train.len() as f64;
    let counts = s.train.iter().fold([0usize; 3], |mut acc, m| {
        acc[outcome(m)] += 1;
        acc
    });
    let base_rates = [
        counts[0] as f64 / n,
        counts[1] as f64 / n,
        counts[2] as f64 / n,
    ];
    let mean_hg = s.train.iter().map(|m| m.home_score as f64).sum::<f64>() / n;
    let mean_ag = s.train.iter().map(|m| m.away_score as f64).sum::<f64>() / n;

    Fitted {
        idx,
        dc,
        dc_short,
        pi,
        elo,
        base_rates,
        mean_hg,
        mean_ag,
    }
}

impl Fitted {
    fn dc_lam(&self, m: &HistoricalMatch) -> (f64, f64) {
        let h = self.idx.canonical(&m.home_team);
        let a = self.idx.canonical(&m.away_team);
        self.dc.lam(h, a, false)
    }

    fn pi_lam(&self, m: &HistoricalMatch) -> (f64, f64) {
        let h = self.idx.canonical(&m.home_team);
        let a = self.idx.canonical(&m.away_team);
        self.pi.lambdas(h, a, true, false)
    }

    /// The λ blend production uses, with arbitrary weights (sum 1).
    fn blend_probs(
        &self,
        m: &HistoricalMatch,
        we: f64,
        wd: f64,
        wp: f64,
        elo_cfg: (f64, f64),
    ) -> [f64; 3] {
        let (eh, ea) = self.elo.lam(m, elo_cfg.0, elo_cfg.1);
        let (dh, da) = self.dc_lam(m);
        let (ph, pa) = self.pi_lam(m);
        let lh = we * eh + wd * dh + wp * ph;
        let la = we * ea + wd * da + wp * pa;
        let rho = if wd > 0.0 { self.dc.rho } else { 0.0 };
        table_probs(lh.clamp(0.15, 5.0), la.clamp(0.15, 5.0), rho)
    }
}

// ------------------------------------------------- learned combiner (ML) ---

/// Feature vector for one match, built only from models fitted on seasons
/// strictly before the match's season — walk-forward, no leakage. The Elo
/// term uses fixed production-style scaling; the learner owns the weights.
fn features(f: &Fitted, m: &HistoricalMatch) -> [f64; 7] {
    let h = f.idx.canonical(&m.home_team);
    let a = f.idx.canonical(&m.away_team);
    let (dh, da) = f.dc.lam(h, a, false);
    let (sh, sa) = f.dc_short.lam(h, a, false);
    let (ph, pa) = f.pi.lambdas(h, a, true, false);
    let dr = f.elo.rating(&m.home_team) - f.elo.rating(&m.away_team);
    [
        dh.ln(),
        da.ln(),
        sh.ln(),
        sa.ln(),
        dr / 400.0,
        ph.ln(),
        pa.ln(),
    ]
}

/// One row of the walk-forward meta-dataset.
struct MetaRow {
    year: i32,
    x: [f64; 7],
    y: usize,
}

/// Multinomial logistic regression (softmax), full-batch gradient descent.
/// Deterministic: zero init, fixed schedule — no RNG, so the arena stays
/// exactly reproducible.
struct Softmax {
    w: [[f64; 8]; 3],
    mean: [f64; 7],
    sd: [f64; 7],
}

impl Softmax {
    fn train(rows: &[&MetaRow], l2: f64, lr: f64, iters: usize) -> Self {
        let n = rows.len() as f64;
        let mut mean = [0.0; 7];
        let mut sd = [0.0; 7];
        for r in rows {
            for (k, v) in r.x.iter().enumerate() {
                mean[k] += v / n;
            }
        }
        for r in rows {
            for (k, v) in r.x.iter().enumerate() {
                sd[k] += (v - mean[k]) * (v - mean[k]) / n;
            }
        }
        for v in &mut sd {
            *v = v.sqrt().max(1e-9);
        }
        let norm = |x: &[f64; 7]| -> [f64; 8] {
            let mut out = [1.0; 8];
            for k in 0..7 {
                out[k] = (x[k] - mean[k]) / sd[k];
            }
            out
        };

        let mut w = [[0.0f64; 8]; 3];
        for _ in 0..iters {
            let mut grad = [[0.0f64; 8]; 3];
            for r in rows {
                let z = norm(&r.x);
                let p = Self::soft(&w, &z);
                for c in 0..3 {
                    let err = p[c] - if r.y == c { 1.0 } else { 0.0 };
                    for k in 0..8 {
                        grad[c][k] += err * z[k] / n;
                    }
                }
            }
            for c in 0..3 {
                for k in 0..8 {
                    w[c][k] -= lr * (grad[c][k] + l2 * w[c][k]);
                }
            }
        }
        Softmax { w, mean, sd }
    }

    fn soft(w: &[[f64; 8]; 3], z: &[f64; 8]) -> [f64; 3] {
        let mut logits = [0.0; 3];
        for c in 0..3 {
            logits[c] = w[c].iter().zip(z.iter()).map(|(wk, zk)| wk * zk).sum();
        }
        let m = logits.iter().cloned().fold(f64::MIN, f64::max);
        let mut e = [0.0; 3];
        let mut sum = 0.0;
        for c in 0..3 {
            e[c] = (logits[c] - m).exp();
            sum += e[c];
        }
        [e[0] / sum, e[1] / sum, e[2] / sum]
    }

    fn predict(&self, x: &[f64; 7]) -> [f64; 3] {
        let mut z = [1.0; 8];
        for k in 0..7 {
            z[k] = (x[k] - self.mean[k]) / self.sd[k];
        }
        Self::soft(&self.w, &z)
    }
}

/// Sharpen or soften a probability triple: p_c ∝ p_c^(1/T).
fn temper(p: [f64; 3], t: f64) -> [f64; 3] {
    let q = [p[0].powf(1.0 / t), p[1].powf(1.0 / t), p[2].powf(1.0 / t)];
    let s = q[0] + q[1] + q[2];
    [q[0] / s, q[1] / s, q[2] / s]
}

// ----------------------------------------------------------------- arena ---

fn main() {
    let val = split(2025);
    let test = split(2026);
    println!(
        "train(val)={} eval(val)={} | train(test)={} eval(test)={}\n",
        val.train.len(),
        val.eval.len(),
        test.train.len(),
        test.eval.len()
    );

    let fv = fit_all(&val);
    let ft = fit_all(&test);

    // --- hyperparameters tuned on validation only --------------------------
    // Elo scale + home advantage for the self-computed ratings.
    let mut elo_cfg = (1600.0, 80.0);
    let mut best = f64::INFINITY;
    for d_div in [600.0, 800.0, 1000.0, 1200.0, 1600.0, 2200.0] {
        for home_adv in [40.0, 80.0, 120.0] {
            let s = evaluate(&val.eval, |m| {
                let (lh, la) = fv.elo.lam(m, d_div, home_adv);
                table_probs(lh, la, 0.0)
            });
            if s.log_loss < best {
                best = s.log_loss;
                elo_cfg = (d_div, home_adv);
            }
        }
    }
    println!(
        "elo-poisson tuned on validation: D_DIV={} HOME_ADV={}\n",
        elo_cfg.0, elo_cfg.1
    );

    // Ensemble weights, 0.1 grid, tuned on validation.
    let mut fitted_w = (0.5, 0.3, 0.2);
    let mut best = f64::INFINITY;
    for e in 0..=10 {
        for d in 0..=(10 - e) {
            let p = 10 - e - d;
            let (we, wd, wp) = (e as f64 / 10.0, d as f64 / 10.0, p as f64 / 10.0);
            let s = evaluate(&val.eval, |m| fv.blend_probs(m, we, wd, wp, elo_cfg));
            if s.log_loss < best {
                best = s.log_loss;
                fitted_w = (we, wd, wp);
            }
        }
    }
    println!(
        "ensemble weights tuned on validation: elo={} dc={} pi={}\n",
        fitted_w.0, fitted_w.1, fitted_w.2
    );

    // --- walk-forward meta-dataset for the learned models ------------------
    // For every season since 2015-16, features come from models fitted only
    // on the seasons before it. Optionally dumped for outside experiments.
    let meta: Vec<MetaRow> = (2016..=2026)
        .flat_map(|year| {
            let sp = split(year);
            let f = fit_all(&sp);
            sp.eval
                .iter()
                .map(|m| MetaRow {
                    year,
                    x: features(&f, m),
                    y: outcome(m),
                })
                .collect::<Vec<_>>()
        })
        .collect();
    if let Ok(dir) = std::env::var("ARENA_DUMP_DIR") {
        let mut csv = String::from("year,dc_lh,dc_la,dcs_lh,dcs_la,elo_dr,pi_lh,pi_la,outcome\n");
        for r in &meta {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                r.year, r.x[0], r.x[1], r.x[2], r.x[3], r.x[4], r.x[5], r.x[6], r.y
            ));
        }
        std::fs::write(format!("{dir}/arena_meta.csv"), csv).expect("dump meta csv");
        println!("meta-dataset dumped ({} rows)\n", meta.len());
    }

    // Stacker for the validation leaderboard trains on seasons < 2025; the
    // test one also gets 2024-25. Same protocol as every other contender.
    let train_rows =
        |max_year: i32| -> Vec<&MetaRow> { meta.iter().filter(|r| r.year <= max_year).collect() };
    let stack_v = Softmax::train(&train_rows(2024), 1e-3, 0.2, 4000);
    let stack_t = Softmax::train(&train_rows(2025), 1e-3, 0.2, 4000);

    // Temperature for the production-weight ensemble, tuned on validation.
    let mut temp = 1.0;
    let mut best = f64::INFINITY;
    for t10 in 6..=16 {
        let t = t10 as f64 / 10.0;
        let sc = evaluate(&val.eval, |m| {
            temper(fv.blend_probs(m, 0.5, 0.3, 0.2, elo_cfg), t)
        });
        if sc.log_loss < best {
            best = sc.log_loss;
            temp = t;
        }
    }
    println!("ensemble temperature tuned on validation: T={temp}\n");

    // --- the contenders ----------------------------------------------------
    type Model<'a> = (
        &'static str,
        Box<dyn FnMut(&HistoricalMatch) -> [f64; 3] + 'a>,
    );
    fn models<'a>(f: &'a Fitted, elo_cfg: (f64, f64), fitted_w: (f64, f64, f64)) -> Vec<Model<'a>> {
        let base = f.base_rates;
        vec![
            ("uniform", Box::new(|_| [1.0 / 3.0; 3])),
            ("base-rates", Box::new(move |_| base)),
            ("home-poisson", {
                let (hg, ag) = (f.mean_hg, f.mean_ag);
                Box::new(move |_| table_probs(hg, ag, 0.0))
            }),
            (
                "elo-poisson",
                Box::new(move |m| {
                    let (lh, la) = f.elo.lam(m, elo_cfg.0, elo_cfg.1);
                    table_probs(lh, la, 0.0)
                }),
            ),
            (
                "maher (dc, rho=0)",
                Box::new(move |m| {
                    let (lh, la) = f.dc_lam(m);
                    table_probs(lh, la, 0.0)
                }),
            ),
            (
                "dixon-coles 4y",
                Box::new(move |m| {
                    let (lh, la) = f.dc_lam(m);
                    table_probs(lh, la, f.dc.rho)
                }),
            ),
            (
                "dixon-coles 1.5y",
                Box::new(move |m| {
                    let h = f.idx.canonical(&m.home_team);
                    let a = f.idx.canonical(&m.away_team);
                    let (lh, la) = f.dc_short.lam(h, a, false);
                    table_probs(lh, la, f.dc_short.rho)
                }),
            ),
            (
                "pi-ratings",
                Box::new(move |m| {
                    let (lh, la) = f.pi_lam(m);
                    table_probs(lh, la, 0.0)
                }),
            ),
            (
                "ensemble 0.5/0.3/0.2",
                Box::new(move |m| f.blend_probs(m, 0.5, 0.3, 0.2, elo_cfg)),
            ),
            (
                "ensemble fitted",
                Box::new(move |m| f.blend_probs(m, fitted_w.0, fitted_w.1, fitted_w.2, elo_cfg)),
            ),
        ]
    }

    for (split, fitted, stack) in [(&val, &fv, &stack_v), (&test, &ft, &stack_t)] {
        println!("=== {} ({} matches) ===", split.name, split.eval.len());
        println!(
            "{:<22} {:>9} {:>8} {:>8} {:>7}",
            "model", "log-loss", "RPS", "Brier", "acc"
        );
        let mut rows: Vec<(&str, Score)> = Vec::new();
        for (name, mut predict) in models(fitted, elo_cfg, fitted_w) {
            rows.push((name, evaluate(&split.eval, &mut predict)));
        }
        rows.push((
            "logit-stack (ML)",
            evaluate(&split.eval, |m| stack.predict(&features(fitted, m))),
        ));
        rows.push((
            "ensemble + temp",
            evaluate(&split.eval, |m| {
                temper(fitted.blend_probs(m, 0.5, 0.3, 0.2, elo_cfg), temp)
            }),
        ));
        rows.sort_by(|a, b| a.1.log_loss.partial_cmp(&b.1.log_loss).unwrap());
        for (name, s) in rows {
            let n = s.n as f64;
            println!(
                "{:<22} {:>9.4} {:>8.4} {:>8.4} {:>6.1}%",
                name,
                s.log_loss / n,
                s.rps / n,
                s.brier / n,
                s.hits as f64 / n * 100.0
            );
        }
        println!();
    }
}
