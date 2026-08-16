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

    // --- the ten contenders ------------------------------------------------
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

    for (split, fitted) in [(&val, &fv), (&test, &ft)] {
        println!("=== {} ({} matches) ===", split.name, split.eval.len());
        println!(
            "{:<22} {:>9} {:>8} {:>8} {:>7}",
            "model", "log-loss", "RPS", "Brier", "acc"
        );
        let mut rows: Vec<(&str, Score)> = Vec::new();
        for (name, mut predict) in models(fitted, elo_cfg, fitted_w) {
            rows.push((name, evaluate(&split.eval, &mut predict)));
        }
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
