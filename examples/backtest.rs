//! Walk-forward backtest of the production model.
//!
//! Every historical match is priced using only matches played on earlier
//! dates; all matches sharing a date are priced before any of their results
//! is revealed. With no `EVAL_*` overrides this runs production's own rules
//! (`sim::elo_delta`, `elo_lambdas`, `blend_lambdas`, `probs_from_lambdas`):
//! Elo updated after every result, Dixon-Coles and pi-ratings frozen at the
//! start of each season, blend 0.5/0.3/0.2.
//!
//! Each season starts from production's own preseason ratings
//! (`sim::preseason_ratings_with` over every earlier result, `PRESEASON_K`);
//! in-season updates use `data::ELO_K`.
//!
//! Development seasons 2016-17..2023-24 are always scored. The holdout
//! seasons 2024-25 and 2025-26 are scored only with `EVAL_HOLDOUT=1`.
//!
//! Overrides (probe knobs; production values in brackets):
//!   EVAL_SEASONS=2017,2018  season end years to score [dev set]
//!   EVAL_REFIT=season|week  when Dixon-Coles/pi see new results [season]
//!   EVAL_W=0.5,0.3,0.2      blend weights elo,dc,pi [0.5,0.3,0.2, the
//!                           `ENSEMBLE_WEIGHTS` default]: three non-negative
//!                           numbers, normalized on use, so 5,3,2 is the
//!                           same blend. Dixon-Coles and pi are fitted every
//!                           season whatever their weight; a weight of 0
//!                           only leaves their λs out. As in production, a
//!                           Dixon-Coles weight of 0 also sets ρ to 0 (no
//!                           low-score correction) unless EVAL_RHO is set
//!   EVAL_HL=1460            Dixon-Coles half-life in days
//!   EVAL_K=20               in-season Elo K [data::ELO_K]
//!   EVAL_K_PRE=20           preseason track K [data::PRESEASON_K]
//!   EVAL_HA=80              Elo home advantage (in-season update and λ;
//!                           the preseason track keeps data::HOME_ADV)
//!   EVAL_DDIV=1600          Elo→goals scale [data::D_DIV]
//!   EVAL_BASE=1.35          Elo→goals base rate [data::BASE]. It scales the
//!                           Elo component's λs only, before blending and
//!                           the lift; Dixon-Coles and pi λs are untouched,
//!                           so in the blend it moves goals by about w_elo
//!                           times as much as the same EVAL_GOALS would
//!   EVAL_GOALS=0.95         multiply both λs after the underdog lift, then
//!                           clamp them to [0.15, 5], as `blend_lambdas`
//!                           clamps [1: no scaling]
//!   EVAL_RHO=-0.1|fit       the Dixon-Coles ρ of the score table [the
//!                           season's fitted ρ while Dixon-Coles has weight,
//!                           else 0, as `sim::score_rho`]. A number is used
//!                           for every match whatever the weights, including
//!                           a Dixon-Coles weight of 0; `fit` uses the
//!                           season's fitted ρ even at a Dixon-Coles weight
//!                           of 0, which separates dropping Dixon-Coles' λs
//!                           from dropping its low-score correction. The
//!                           score table clamps ρ to ±0.4
//!   EVAL_PROMO=1450         Elo of a club never seen before [data::NEW_CLUB_ELO]
//!   EVAL_REGRESS=0          preseason pull towards the league mean, 0..1
//!   EVAL_PROMO_REL=-60      promoted clubs start at last season's league
//!                           mean plus this offset [off: new clubs at
//!                           EVAL_PROMO, returning clubs keep old rating]
//!   EVAL_PIVOT=0.9          underdog-lift pivot λ [data::UNDERDOG_PIVOT]
//!   EVAL_LIFT=0.7           share of the gap to the pivot a lower λ is
//!                           lifted [data::UNDERDOG_LIFT]; a fraction, not a
//!                           switch: 0 turns the lift, and with it the
//!                           favourite's follow, off
//!   EVAL_FOLLOW=0.5         favourite's share of the underdog's relative
//!                           lift [data::FAV_FOLLOW]
//!   EVAL_DUMP=path.csv      write one row per scored match
//!   EVAL_METRICS=path.json  also write the summary lines as one JSON object,
//!                           keys sorted, values at their printed precision
//!   EVAL_SEASON_SIM=2000    also replay each scored season from its start
//!                           with the production season simulator and
//!                           compare the spread of final points to reality.
//!                           Its `World` takes the season-start ratings and
//!                           ensemble, so EVAL_W, EVAL_HL, EVAL_K_PRE,
//!                           EVAL_PROMO, EVAL_PROMO_REL and EVAL_REGRESS act
//!                           on it (EVAL_K cannot: nothing is revealed in a
//!                           replay from the start), but it prices every
//!                           match with production's `elo_lambdas`,
//!                           `blend_lambdas` and `score_rho`, so it refuses
//!                           to run with a λ knob set: EVAL_HA, EVAL_DDIV,
//!                           EVAL_BASE, EVAL_GOALS, EVAL_RHO, EVAL_PIVOT,
//!                           EVAL_LIFT or EVAL_FOLLOW
//!
//! The model as it was before the October 2026 tuning is
//! `EVAL_K=20 EVAL_DDIV=1600 EVAL_LIFT=0`; its preseason track already ran at
//! K 20, today's `PRESEASON_K`. Compare two runs' dumps with
//! `cargo run --release --example compare -- A.csv B.csv`.
//!
//! The summary (`dev_*`, then `hold_*`) gives each aspect's mean loss. After
//! those lines come skill scores against a base-rate forecast (1 − the
//! model's loss / the base rate's, which `_base` gives), whose home/draw/away,
//! over-2.5 and both-score rates come from every season before the one being
//! scored, and calibration per outcome: mean forecast against observed rate,
//! and the slope `b` of `logit P(outcome) = a + b · logit(forecast)` fitted
//! by Newton's method with a free intercept (1 is calibrated; below 1, the
//! forecasts spread too wide), with its standard error.
//!
//! Last comes calibration in the large: `_goals_pred` is the mean of
//! λ_home + λ_away as priced (after blend, lift, EVAL_GOALS and the clamp;
//! the ρ correction leaves a score table's mean goals unchanged, so this is
//! the table's mean up to its 10-goal truncation) against `_goals_obs`, the
//! mean goals scored; `_ou25_pred`/`_btts_pred` are the mean forecast
//! P(3+ goals) and P(both score) against `_ou25_obs`/`_btts_obs`, how often
//! each happened.
//!
//! Each season's line also gives `dc_rho`, the ρ of its season-start
//! Dixon-Coles fit (what production's rule prices with while Dixon-Coles has
//! weight), and its mean λ total against the goals scored.
//!
//! Run: cargo run --release --example backtest

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use rand::{rngs::SmallRng, SeedableRng};
use superlig_sim::data;
use superlig_sim::dixoncoles::{self, MAX_GOALS};
use superlig_sim::history::{self, HistoricalMatch, TeamIndex};
use superlig_sim::piratings::PiRatings;
use superlig_sim::sim::{self, Ensemble, FixtureDate, LeagueFixture, World};

const DEV: std::ops::RangeInclusive<i32> = 2017..=2024;
const HOLDOUT: [i32; 2] = [2025, 2026];

/// The knobs that move a match's λs or score table, which `EVAL_SEASON_SIM`
/// cannot honour: its `World` prices with production's rules.
const SEASON_SIM_IGNORES: [&str; 8] = [
    "EVAL_HA",
    "EVAL_DDIV",
    "EVAL_BASE",
    "EVAL_GOALS",
    "EVAL_RHO",
    "EVAL_PIVOT",
    "EVAL_LIFT",
    "EVAL_FOLLOW",
];

/// A knob's value from `var`, or `default` when it is not set.
fn env_f(var: &impl Fn(&str) -> Option<String>, key: &str, default: f64) -> f64 {
    var(key)
        .map(|v| {
            v.parse()
                .unwrap_or_else(|_| panic!("{key}={v} is not a number"))
        })
        .unwrap_or(default)
}

/// The λ knobs `var` sets, which `EVAL_SEASON_SIM` would ignore.
fn season_sim_conflicts(var: &impl Fn(&str) -> Option<String>) -> Vec<&'static str> {
    SEASON_SIM_IGNORES
        .into_iter()
        .filter(|k| var(k).is_some())
        .collect()
}

struct Cfg {
    seasons: Vec<i32>,
    weekly_refit: bool,
    w: (f64, f64, f64),
    half_life: f64,
    k: f64,
    k_pre: f64,
    home_adv: f64,
    d_div: f64,
    base: f64,
    promo: f64,
    promo_rel: Option<f64>,
    regress: f64,
    lift: Lift,
    rho: Rho,
    /// True when every Elo knob is at its production value, so the shared
    /// production functions are called rather than the local variants.
    production_elo: bool,
    /// `EVAL_SEASON_SIM`: simulated seasons per scored season, if any.
    season_sim: Option<usize>,
}

/// What `sim::blend_lambdas` does after blending: the underdog lift, then
/// (a backtest knob, 1 in production) a scale on both λs, then the clamp.
#[derive(Clone, Copy, PartialEq)]
struct Lift {
    pivot: f64,
    lift: f64,
    follow: f64,
    /// `EVAL_GOALS`: both λs are multiplied by this before the clamp.
    goals: f64,
}

impl Lift {
    const PRODUCTION: Lift = Lift {
        pivot: data::UNDERDOG_PIVOT,
        lift: data::UNDERDOG_LIFT,
        follow: data::FAV_FOLLOW,
        goals: 1.0,
    };

    /// Production's `sim::blend_lambdas` at production values, otherwise
    /// `blend_local`.
    fn blend(self, elo: (f64, f64), e: &Ensemble, home: usize, away: usize) -> (f64, f64) {
        if self == Lift::PRODUCTION {
            return sim::blend_lambdas(elo, Some(e), home, away);
        }
        self.blend_local(elo, e, home, away)
    }

    /// `sim::blend_lambdas`, operation for operation, with this lift and
    /// goal scale (multiplying by 1 is exact, so a scale of 1 changes nothing).
    fn blend_local(
        self,
        (lh_elo, la_elo): (f64, f64),
        e: &Ensemble,
        home: usize,
        away: usize,
    ) -> (f64, f64) {
        let w_models = e.w_dc + e.w_pi;
        let (lh, la) = if w_models > 0.0 {
            let (dc_h, dc_a) = e.dc.lam(home, away, false);
            let (pi_h, pi_a) = e.pi.lambdas(home, away, false);
            let eh = (e.w_dc * dc_h + e.w_pi * pi_h) / w_models;
            let ea = (e.w_dc * dc_a + e.w_pi * pi_a) / w_models;
            let total = e.w_elo + w_models;
            (
                (e.w_elo * lh_elo + w_models * eh) / total,
                (e.w_elo * la_elo + w_models * ea) / total,
            )
        } else {
            (lh_elo, la_elo)
        };
        let (lo, hi) = if lh < la { (lh, la) } else { (la, lh) };
        let lo_new = if lo < self.pivot {
            lo + self.lift * (self.pivot - lo)
        } else {
            lo
        };
        let hi_new = hi * (1.0 + self.follow * (lo_new / lo - 1.0));
        let (lh, la) = if lh < la {
            (lo_new, hi_new)
        } else {
            (hi_new, lo_new)
        };
        (
            (lh * self.goals).clamp(0.15, 5.0),
            (la * self.goals).clamp(0.15, 5.0),
        )
    }
}

/// The ρ of the score table (`EVAL_RHO`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rho {
    /// Production's `sim::score_rho`: the season's fitted ρ while
    /// Dixon-Coles has weight, else 0.
    Production,
    /// The season's fitted ρ whatever the weights (`EVAL_RHO=fit`).
    Fitted,
    /// This ρ for every match.
    Fixed(f64),
}

impl Rho {
    fn parse(v: Option<&str>) -> Self {
        match v.map(str::trim) {
            None => Rho::Production,
            Some("fit") => Rho::Fitted,
            Some(x) => {
                let r: f64 = x
                    .parse()
                    .unwrap_or_else(|_| panic!("EVAL_RHO={x} is neither a number nor `fit`"));
                assert!(r.is_finite(), "EVAL_RHO={x} is not finite");
                Rho::Fixed(r)
            }
        }
    }

    fn value(self, e: &Ensemble) -> f64 {
        match self {
            Rho::Production => sim::score_rho(Some(e)).unwrap_or(0.0),
            Rho::Fitted => e.dc.rho,
            Rho::Fixed(r) => r,
        }
    }
}

impl Cfg {
    fn from_env() -> Self {
        Self::from_vars(|key| std::env::var(key).ok())
    }

    /// The configuration the `EVAL_*` variables `var` returns describe.
    fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let holdout = var("EVAL_HOLDOUT").is_some_and(|v| v == "1");
        let seasons = match var("EVAL_SEASONS") {
            Some(s) => s
                .split(',')
                .map(|y| y.trim().parse().expect("season year"))
                .collect(),
            None => DEV
                .chain(if holdout { HOLDOUT.to_vec() } else { vec![] })
                .collect(),
        };
        let w: Vec<f64> = var("EVAL_W")
            .unwrap_or_else(|| "0.5,0.3,0.2".into())
            .split(',')
            .map(|x| x.trim().parse().expect("weight"))
            .collect();
        assert!(
            w.len() == 3
                && w.iter().all(|x| x.is_finite() && *x >= 0.0)
                && w.iter().sum::<f64>() > 0.0,
            "EVAL_W takes three non-negative weights elo,dc,pi with a positive sum, got {w:?}"
        );
        let goals = env_f(&var, "EVAL_GOALS", 1.0);
        assert!(
            goals.is_finite() && goals > 0.0,
            "EVAL_GOALS={goals} must be positive"
        );
        let refit = var("EVAL_REFIT").unwrap_or_else(|| "season".into());
        assert!(
            refit == "season" || refit == "week",
            "EVAL_REFIT must be season|week"
        );
        let season_sim = var("EVAL_SEASON_SIM").map(|n| n.parse().expect("EVAL_SEASON_SIM"));
        let ignored = season_sim_conflicts(&var);
        assert!(
            season_sim.is_none() || ignored.is_empty(),
            "EVAL_SEASON_SIM prices matches with production's own λ rules, which ignore {}; \
             unset {} or EVAL_SEASON_SIM",
            ignored.join(", "),
            if ignored.len() == 1 { "it" } else { "them" },
        );
        let (k, home_adv, d_div, base) = (
            env_f(&var, "EVAL_K", data::ELO_K),
            env_f(&var, "EVAL_HA", data::HOME_ADV),
            env_f(&var, "EVAL_DDIV", data::D_DIV),
            env_f(&var, "EVAL_BASE", data::BASE),
        );
        Cfg {
            seasons,
            weekly_refit: refit == "week",
            w: (w[0], w[1], w[2]),
            half_life: env_f(&var, "EVAL_HL", 1460.0),
            k,
            k_pre: env_f(&var, "EVAL_K_PRE", data::PRESEASON_K),
            home_adv,
            d_div,
            base,
            promo: env_f(&var, "EVAL_PROMO", data::NEW_CLUB_ELO),
            promo_rel: var("EVAL_PROMO_REL").map(|v| v.parse().expect("EVAL_PROMO_REL")),
            regress: env_f(&var, "EVAL_REGRESS", 0.0),
            lift: Lift {
                pivot: env_f(&var, "EVAL_PIVOT", data::UNDERDOG_PIVOT),
                lift: env_f(&var, "EVAL_LIFT", data::UNDERDOG_LIFT),
                follow: env_f(&var, "EVAL_FOLLOW", data::FAV_FOLLOW),
                goals,
            },
            rho: Rho::parse(var("EVAL_RHO").as_deref()),
            production_elo: k == data::ELO_K
                && home_adv == data::HOME_ADV
                && d_div == data::D_DIV
                && base == data::BASE,
            season_sim,
        }
    }

    fn elo_delta(&self, k: f64, rh: f64, ra: f64, hs: u16, as_: u16) -> f64 {
        if self.production_elo && k == data::ELO_K {
            return sim::elo_delta(rh, ra, hs, as_);
        }
        let dr = rh - ra + self.home_adv;
        let expected = 1.0 / (1.0 + 10f64.powf(-dr / data::ELO_DIV));
        let actual = match hs.cmp(&as_) {
            std::cmp::Ordering::Greater => 1.0,
            std::cmp::Ordering::Equal => 0.5,
            std::cmp::Ordering::Less => 0.0,
        };
        let margin = ((hs as f64 - as_ as f64).abs()).max(1.0).sqrt();
        k * margin * (actual - expected)
    }

    fn elo_lambdas(&self, rh: f64, ra: f64) -> (f64, f64) {
        if self.production_elo {
            return sim::elo_lambdas(rh, ra);
        }
        let dr = rh - ra + self.home_adv;
        (
            self.base * 10f64.powf(dr / self.d_div),
            self.base * 10f64.powf(-dr / self.d_div),
        )
    }
}

/// August boundary: keeps the pandemic-delayed July 2020 matches in 2019-20.
fn season_end_year(date: NaiveDate) -> i32 {
    if date.month() >= 8 {
        date.year() + 1
    } else {
        date.year()
    }
}

/// The season's clubs plus the departed-club bucket, as production builds it.
fn index_for(matches: &[&HistoricalMatch]) -> TeamIndex {
    let mut names: Vec<String> = matches
        .iter()
        .flat_map(|m| [m.home_team.clone(), m.away_team.clone()])
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    names.sort();
    let mut idx_to_name = names.clone();
    idx_to_name.push(history::OTHER_TEAM_NAME.to_string());
    TeamIndex {
        name_to_idx: idx_to_name
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect(),
        other_idx: idx_to_name.len() - 1,
        idx_to_name,
        league_names: names,
    }
}

fn fit_ensemble(
    cfg: &Cfg,
    before: &[HistoricalMatch],
    idx: &TeamIndex,
    as_of: NaiveDate,
) -> Ensemble {
    let fits = history::prepare_fit_matches(before, idx, cfg.half_life, as_of);
    let dc = dixoncoles::fit(&fits, idx, cfg.half_life, 300).expect("dixon-coles fit");
    let since = NaiveDate::from_ymd_opt(history::CUTOFF_YEAR, 1, 1).expect("valid date");
    let pi = PiRatings::compute(before, idx, since);
    Ensemble {
        dc,
        pi,
        w_elo: cfg.w.0,
        w_dc: cfg.w.1,
        w_pi: cfg.w.2,
    }
}

/// 0 home win, 1 draw, 2 away win.
fn outcome(home_goals: u16, away_goals: u16) -> usize {
    match home_goals.cmp(&away_goals) {
        std::cmp::Ordering::Greater => 0,
        std::cmp::Ordering::Equal => 1,
        std::cmp::Ordering::Less => 2,
    }
}

/// Log-loss of a forecast that gave what happened probability `p`.
fn nll(p: f64) -> f64 {
    -p.max(1e-9).ln()
}

/// Ranked probability score of a home/draw/away forecast.
fn rps(p3: [f64; 3], o: usize) -> f64 {
    let c1 = p3[0] - if o == 0 { 1.0 } else { 0.0 };
    let c2 = p3[0] + p3[1] - if o <= 1 { 1.0 } else { 0.0 };
    (c1 * c1 + c2 * c2) / 2.0
}

/// The forecast that knows nothing about the two clubs: league-wide
/// home/draw/away, over-2.5 and both-score rates.
struct BaseRates {
    p3: [f64; 3],
    over: f64,
    btts: f64,
}

impl BaseRates {
    fn from_matches(matches: &[HistoricalMatch]) -> Self {
        assert!(
            !matches.is_empty(),
            "no earlier match to take base rates from"
        );
        let (mut counts, mut over, mut btts) = ([0usize; 3], 0usize, 0usize);
        for m in matches {
            counts[outcome(m.home_score, m.away_score)] += 1;
            over += usize::from(m.home_score + m.away_score >= 3);
            btts += usize::from(m.home_score >= 1 && m.away_score >= 1);
        }
        let n = matches.len() as f64;
        BaseRates {
            p3: counts.map(|c| c as f64 / n),
            over: over as f64 / n,
            btts: btts as f64 / n,
        }
    }
}

/// Summed losses over scored matches, with the base rates' alongside.
#[derive(Default, Clone)]
struct Acc {
    n: usize,
    ll: f64,
    rps: f64,
    ou: f64,
    btts: f64,
    score: f64,
    hits: usize,
    base_ll: f64,
    base_rps: f64,
    base_ou: f64,
    base_btts: f64,
    /// Every match's home/draw/away forecast and outcome, for calibration.
    outcomes: Vec<([f64; 3], usize)>,
    /// Calibration in the large: summed λ_home + λ_away and goals scored,
    /// summed P(3+ goals) and matches with 3+, summed P(both score) and
    /// matches where both did.
    lam: f64,
    goals: usize,
    p_over: f64,
    over: usize,
    p_both: f64,
    both: usize,
}

impl Acc {
    fn add(&mut self, o: &Acc) {
        self.n += o.n;
        self.ll += o.ll;
        self.rps += o.rps;
        self.ou += o.ou;
        self.btts += o.btts;
        self.score += o.score;
        self.hits += o.hits;
        self.base_ll += o.base_ll;
        self.base_rps += o.base_rps;
        self.base_ou += o.base_ou;
        self.base_btts += o.base_btts;
        self.outcomes.extend_from_slice(&o.outcomes);
        self.lam += o.lam;
        self.goals += o.goals;
        self.p_over += o.p_over;
        self.over += o.over;
        self.p_both += o.p_both;
        self.both += o.both;
    }

    /// The loss lines, `(key, value)` as printed.
    fn summary(&self, prefix: &str) -> Vec<(String, String)> {
        let n = self.n as f64;
        vec![
            (format!("{prefix}_1x2_ll"), format!("{:.5}", self.ll / n)),
            (format!("{prefix}_rps"), format!("{:.5}", self.rps / n)),
            (format!("{prefix}_ou25_ll"), format!("{:.5}", self.ou / n)),
            (format!("{prefix}_btts_ll"), format!("{:.5}", self.btts / n)),
            (
                format!("{prefix}_score_ll"),
                format!("{:.5}", self.score / n),
            ),
            (
                format!("{prefix}_acc"),
                format!("{:.4}", self.hits as f64 / n),
            ),
            (format!("{prefix}_n"), self.n.to_string()),
        ]
    }

    /// Skill against the base rates, then calibration per outcome.
    fn evidence(&self, prefix: &str) -> Vec<(String, String)> {
        let n = self.n as f64;
        let mut out = Vec::new();
        for (base_key, skill_key, model, base) in [
            ("1x2_ll_base", "1x2_skill", self.ll, self.base_ll),
            ("rps_base", "rpss", self.rps, self.base_rps),
            ("ou25_ll_base", "ou25_skill", self.ou, self.base_ou),
            ("btts_ll_base", "btts_skill", self.btts, self.base_btts),
        ] {
            out.push((format!("{prefix}_{base_key}"), format!("{:.5}", base / n)));
            out.push((
                format!("{prefix}_{skill_key}"),
                format!("{:.5}", 1.0 - model / base),
            ));
        }
        for (k, name) in ["home", "draw", "away"].into_iter().enumerate() {
            let forecasts: Vec<(f64, bool)> =
                self.outcomes.iter().map(|&(p, o)| (p[k], o == k)).collect();
            let pred = forecasts.iter().map(|f| f.0).sum::<f64>() / n;
            let obs = forecasts.iter().filter(|f| f.1).count() as f64 / n;
            let (slope, se) = calibration_slope(&forecasts).unwrap_or((f64::NAN, f64::NAN));
            out.push((format!("{prefix}_cal_{name}_pred"), format!("{pred:.4}")));
            out.push((format!("{prefix}_cal_{name}_obs"), format!("{obs:.4}")));
            out.push((format!("{prefix}_cal_{name}_slope"), format!("{slope:.3}")));
            out.push((format!("{prefix}_cal_{name}_slope_se"), format!("{se:.3}")));
        }
        out
    }

    /// Calibration in the large: mean forecast against mean outcome for
    /// goals, 3+ goals and both scoring.
    fn in_the_large(&self, prefix: &str) -> Vec<(String, String)> {
        let n = self.n as f64;
        [
            ("goals", self.lam, self.goals),
            ("ou25", self.p_over, self.over),
            ("btts", self.p_both, self.both),
        ]
        .into_iter()
        .flat_map(|(name, pred, obs)| {
            [
                (format!("{prefix}_{name}_pred"), format!("{:.4}", pred / n)),
                (
                    format!("{prefix}_{name}_obs"),
                    format!("{:.4}", obs as f64 / n),
                ),
            ]
        })
        .collect()
    }
}

/// One match as the backtest prices it: the λs and forecasts the dump
/// writes, and the record `Acc` sums for it.
struct Priced {
    lam: (f64, f64),
    p3: [f64; 3],
    /// P(3+ goals) and P(both score).
    p_over: f64,
    p_both: f64,
    /// Probability of the exact final score.
    p_score: f64,
    acc: Acc,
}

/// Price a match under `cfg`, home and away rated `(rh, ra)` on Elo and at
/// `(h, a)` in the ensemble's index, then score it against its final score
/// and the base rates.
fn price(
    cfg: &Cfg,
    e: &Ensemble,
    base: &BaseRates,
    (rh, ra): (f64, f64),
    (h, a): (usize, usize),
    (home_score, away_score): (u16, u16),
) -> Priced {
    let (lh, la) = cfg.lift.blend(cfg.elo_lambdas(rh, ra), e, h, a);
    let rho = cfg.rho.value(e);
    let p = sim::probs_from_lambdas(lh, la, rho);
    let p3 = [
        p.home_win_pct / 100.0,
        p.draw_pct / 100.0,
        p.away_win_pct / 100.0,
    ];
    let o = outcome(home_score, away_score);
    let table = dixoncoles::score_table(lh, la, rho);
    let total: f64 = table.iter().flatten().sum();
    let (hs, as_) = (home_score as usize, away_score as usize);
    let pscore = if hs <= MAX_GOALS && as_ <= MAX_GOALS {
        table[hs][as_] / total
    } else {
        1e-6
    };
    let pov = p.over25_pct / 100.0;
    let pbtts = p.btts_pct / 100.0;
    let over = hs + as_ >= 3;
    let both = hs >= 1 && as_ >= 1;
    let pick = (0..3)
        .max_by(|&x, &y| p3[x].total_cmp(&p3[y]))
        .expect("three");
    Priced {
        lam: (lh, la),
        p3,
        p_over: pov,
        p_both: pbtts,
        p_score: pscore,
        acc: Acc {
            n: 1,
            ll: nll(p3[o]),
            rps: rps(p3, o),
            ou: nll(if over { pov } else { 1.0 - pov }),
            btts: nll(if both { pbtts } else { 1.0 - pbtts }),
            score: nll(pscore),
            hits: usize::from(pick == o),
            base_ll: nll(base.p3[o]),
            base_rps: rps(base.p3, o),
            base_ou: nll(if over { base.over } else { 1.0 - base.over }),
            base_btts: nll(if both { base.btts } else { 1.0 - base.btts }),
            outcomes: vec![(p3, o)],
            lam: lh + la,
            goals: hs + as_,
            p_over: pov,
            over: usize::from(over),
            p_both: pbtts,
            both: usize::from(both),
        },
    }
}

/// Logistic recalibration `logit P(event) = a + b · logit(p)` over
/// `(forecast p, happened)` pairs, by Newton's method from the calibrated
/// a = 0, b = 1. Returns `b` and its standard error (from the inverse
/// Hessian), or `None` if the fit is degenerate or does not converge.
fn calibration_slope(forecasts: &[(f64, bool)]) -> Option<(f64, f64)> {
    let logit = |p: f64| {
        let p = p.clamp(1e-9, 1.0 - 1e-9);
        (p / (1.0 - p)).ln()
    };
    let (mut a, mut b) = (0.0, 1.0);
    for _ in 0..100 {
        let (mut g0, mut g1, mut h00, mut h01, mut h11) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &(p, happened) in forecasts {
            let x = logit(p);
            let mu = 1.0 / (1.0 + (-(a + b * x)).exp());
            let r = if happened { 1.0 } else { 0.0 } - mu;
            let w = mu * (1.0 - mu);
            g0 += r;
            g1 += r * x;
            h00 += w;
            h01 += w * x;
            h11 += w * x * x;
        }
        // A forecast that never varies leaves the slope undetermined.
        let det = h00 * h11 - h01 * h01;
        if det.is_nan() || det <= 1e-12 * h00 * h11 {
            return None;
        }
        let (da, db) = ((h11 * g0 - h01 * g1) / det, (h00 * g1 - h01 * g0) / det);
        a += da;
        b += db;
        if !(a.is_finite() && b.is_finite()) {
            return None;
        }
        if da.abs() + db.abs() < 1e-10 {
            return Some((b, (h00 / det).sqrt()));
        }
    }
    None
}

/// The summary as one JSON object for `EVAL_METRICS`: keys sorted, each
/// value the number as printed (a failed fit's `NaN` as null). Any other
/// value that is not a number is a bug, not a null.
fn metrics_json(summary: &[(String, String)]) -> String {
    let map: std::collections::BTreeMap<&str, serde_json::Value> = summary
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

/// Replay a season from its start `n` times with the production simulator
/// and print the spread of final points against what actually happened.
fn season_sim(
    year: i32,
    season: &[&HistoricalMatch],
    idx: &TeamIndex,
    elo: &HashMap<String, f64>,
    ens: &Ensemble,
    n: usize,
) {
    let teams = idx.league_names.clone();
    let pos: HashMap<String, usize> = teams
        .iter()
        .enumerate()
        .map(|(i, t)| (t.clone(), i))
        .collect();
    let fixtures = season
        .iter()
        .map(|m| LeagueFixture {
            round: 1,
            home: pos[&m.home_team],
            away: pos[&m.away_team],
        })
        .collect();
    let dates = season
        .iter()
        .map(|m| FixtureDate {
            date: m.date.to_string(),
            kickoff: None,
        })
        .collect();
    let ratings: Vec<f64> = teams.iter().map(|t| elo[t]).collect();
    let world = World {
        idx: pos.clone(),
        teams: teams.clone(),
        elo: ratings.clone(),
        elo_baseline: ratings,
        fixtures,
        dates,
        played: HashMap::new(),
        ensemble: Some(ens.clone()),
        offset: vec![0.0; teams.len()],
    };
    let points = |pts: &[f64]| {
        let mean = pts.iter().sum::<f64>() / pts.len() as f64;
        let sd = (pts.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / pts.len() as f64).sqrt();
        (sd, pts.iter().cloned().fold(f64::MIN, f64::max))
    };
    let mut actual = vec![0.0; teams.len()];
    for m in season {
        let (h, a) = (pos[&m.home_team], pos[&m.away_team]);
        match m.home_score.cmp(&m.away_score) {
            std::cmp::Ordering::Greater => actual[h] += 3.0,
            std::cmp::Ordering::Equal => {
                actual[h] += 1.0;
                actual[a] += 1.0;
            }
            std::cmp::Ordering::Less => actual[a] += 3.0,
        }
    }
    let (act_sd, act_max) = points(&actual);
    let mut rng = SmallRng::seed_from_u64(year as u64);
    let (mut sds, mut maxes) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for _ in 0..n {
        let r = world.simulate_one(&mut rng);
        let pts: Vec<f64> = r.records.iter().map(|t| t.points as f64).collect();
        let (sd, max) = points(&pts);
        sds.push(sd);
        maxes.push(max);
    }
    sds.sort_by(f64::total_cmp);
    maxes.sort_by(f64::total_cmp);
    let q = |v: &[f64], f: f64| v[((v.len() - 1) as f64 * f) as usize];
    let p_ge = maxes.iter().filter(|&&m| m >= act_max).count() as f64 / n as f64;
    println!(
        "season-sim {}-{:02}: points sd actual {:.1} | simulated median {:.1} (p10 {:.1}, p90 {:.1}) | champion actual {:.0} | simulated median {:.0}, P(>=actual) {:.1}%",
        year - 1,
        year % 100,
        act_sd,
        q(&sds, 0.5),
        q(&sds, 0.1),
        q(&sds, 0.9),
        act_max,
        q(&maxes, 0.5),
        100.0 * p_ge
    );
}

fn main() {
    let cfg = Cfg::from_env();
    let mut all = history::load_history_with_cutoff(history::CUTOFF_YEAR);
    all.sort_by_key(|m| m.date);
    let last_season = *cfg.seasons.iter().max().expect("at least one season");

    let mut dump = std::env::var("EVAL_DUMP").ok().map(|p| {
        let mut s = String::from(
            "season,date,home,away,hs,as,p1,px,p2,pover25,pbtts,pscore,lh,la,elo_h,elo_a,gp_h,gp_a,promo_h,promo_a\n",
        );
        s.reserve(1 << 20);
        (p, s)
    });

    let mut prev_clubs: HashSet<String> = HashSet::new();
    let (mut dev, mut hold) = (Acc::default(), Acc::default());
    let first_season = season_end_year(all[0].date);

    for year in first_season..=last_season {
        let season: Vec<&HistoricalMatch> = all
            .iter()
            .filter(|m| season_end_year(m.date) == year)
            .collect();
        if season.is_empty() {
            continue;
        }
        let clubs: HashSet<String> = season
            .iter()
            .flat_map(|m| [m.home_team.clone(), m.away_team.clone()])
            .collect();
        let season_start = season[0].date;
        let before_season: Vec<HistoricalMatch> = all
            .iter()
            .filter(|m| m.date < season_start)
            .cloned()
            .collect();
        // Production's preseason rule, over everything before this season.
        let mut elo = sim::preseason_ratings_with(&before_season, cfg.k_pre, cfg.promo);
        let prev_mean = (!prev_clubs.is_empty())
            .then(|| prev_clubs.iter().map(|c| elo[c]).sum::<f64>() / prev_clubs.len() as f64);
        for c in &clubs {
            match (cfg.promo_rel, prev_mean) {
                (Some(rel), Some(mean)) if !prev_clubs.contains(c) => {
                    elo.insert(c.clone(), mean + rel);
                }
                _ => {
                    elo.entry(c.clone()).or_insert(cfg.promo);
                }
            }
        }
        if cfg.regress != 0.0 {
            let mean = clubs.iter().map(|c| elo[c]).sum::<f64>() / clubs.len() as f64;
            for c in &clubs {
                let r = elo.get_mut(c).expect("inserted above");
                *r -= cfg.regress * (*r - mean);
            }
        }

        let mut elo_in = elo;
        let scored = cfg.seasons.contains(&year);
        let idx = index_for(&season);
        let mut ens = scored.then(|| fit_ensemble(&cfg, &before_season, &idx, season_start));
        let season_rho = ens.as_ref().map_or(f64::NAN, |e| e.dc.rho);
        let base = scored.then(|| BaseRates::from_matches(&before_season));
        if let (Some(e), Some(n)) = (ens.as_ref(), cfg.season_sim) {
            season_sim(year, &season, &idx, &elo_in, e, n);
        }
        let mut last_fit = season_start;
        let mut games: HashMap<&str, u32> = HashMap::new();
        let mut acc = Acc::default();

        let mut i = 0;
        while i < season.len() {
            let date = season[i].date;
            let mut j = i;
            while j < season.len() && season[j].date == date {
                j += 1;
            }
            if scored && cfg.weekly_refit && (date - last_fit).num_days() >= 7 {
                let before: Vec<HistoricalMatch> =
                    all.iter().filter(|m| m.date < date).cloned().collect();
                ens = Some(fit_ensemble(&cfg, &before, &idx, date));
                last_fit = date;
            }
            // Price every match of the day before revealing any result.
            if let (Some(e), Some(base)) = (ens.as_ref(), base.as_ref()) {
                for m in &season[i..j] {
                    let clubs = (idx.canonical(&m.home_team), idx.canonical(&m.away_team));
                    let (rh, ra) = (elo_in[&m.home_team], elo_in[&m.away_team]);
                    let score = (m.home_score, m.away_score);
                    let priced = price(&cfg, e, base, (rh, ra), clubs, score);
                    acc.add(&priced.acc);
                    if let Some((_, s)) = dump.as_mut() {
                        let gh = games.get(m.home_team.as_str()).copied().unwrap_or(0);
                        let ga = games.get(m.away_team.as_str()).copied().unwrap_or(0);
                        s.push_str(&format!(
                            "{year},{},{},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.4},{:.4},{:.1},{:.1},{gh},{ga},{},{}\n",
                            m.date,
                            m.home_team,
                            m.away_team,
                            m.home_score,
                            m.away_score,
                            priced.p3[0],
                            priced.p3[1],
                            priced.p3[2],
                            priced.p_over,
                            priced.p_both,
                            priced.p_score,
                            priced.lam.0,
                            priced.lam.1,
                            rh,
                            ra,
                            u8::from(!prev_clubs.is_empty() && !prev_clubs.contains(&m.home_team)),
                            u8::from(!prev_clubs.is_empty() && !prev_clubs.contains(&m.away_team)),
                        ));
                    }
                }
            }
            // Then reveal the day's results.
            for m in &season[i..j] {
                let (h, a) = (&m.home_team, &m.away_team);
                let d = cfg.elo_delta(cfg.k, elo_in[h], elo_in[a], m.home_score, m.away_score);
                *elo_in.get_mut(h).expect("club") += d;
                *elo_in.get_mut(a).expect("club") -= d;
                *games.entry(m.home_team.as_str()).or_default() += 1;
                *games.entry(m.away_team.as_str()).or_default() += 1;
            }
            i = j;
        }

        if scored {
            println!(
                "season {}-{:02}: n={} 1x2_ll={:.4} rps={:.4} ou25_ll={:.4} btts_ll={:.4} score_ll={:.4} acc={:.3} dc_rho={:.4} goals={:.3}/{:.3}",
                year - 1,
                year % 100,
                acc.n,
                acc.ll / acc.n as f64,
                acc.rps / acc.n as f64,
                acc.ou / acc.n as f64,
                acc.btts / acc.n as f64,
                acc.score / acc.n as f64,
                acc.hits as f64 / acc.n as f64,
                season_rho,
                acc.lam / acc.n as f64,
                acc.goals as f64 / acc.n as f64,
            );
            if HOLDOUT.contains(&year) {
                hold.add(&acc);
            } else {
                dev.add(&acc);
            }
        }
        prev_clubs = clubs;
    }

    // The loss lines first, in their long-standing order, then the rest.
    let sets = [(&dev, "dev"), (&hold, "hold")];
    let mut summary: Vec<(String, String)> = Vec::new();
    for (acc, prefix) in sets.iter().filter(|(acc, _)| acc.n > 0) {
        summary.extend(acc.summary(prefix));
    }
    for (acc, prefix) in sets.iter().filter(|(acc, _)| acc.n > 0) {
        summary.extend(acc.evidence(prefix));
    }
    for (acc, prefix) in sets.iter().filter(|(acc, _)| acc.n > 0) {
        summary.extend(acc.in_the_large(prefix));
    }
    for (key, value) in &summary {
        println!("{key}={value}");
    }
    if let Ok(path) = std::env::var("EVAL_METRICS") {
        std::fs::write(&path, metrics_json(&summary)).expect("write metrics");
        eprintln!("summary metrics written to {path}");
    }
    if let Some((path, s)) = dump {
        std::fs::write(&path, s).expect("write dump");
        eprintln!("per-match rows written to {path}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The local blend is production's, operation for operation, so setting
    /// a lift knob changes nothing but the lift.
    #[test]
    fn local_blend_is_production_at_production_values() {
        for (w_elo, w_dc, w_pi) in [(0.5, 0.3, 0.2), (1.0, 0.0, 0.0), (0.2, 0.0, 0.8)] {
            let e = Ensemble::from_embedded_data(w_elo, w_dc, w_pi).expect("embedded ensemble");
            for (home, away) in [(0, 1), (3, 17), (17, 3), (5, 18)] {
                for elo in [
                    (0.3, 2.9),
                    (1.35, 1.35),
                    (2.4, 0.5),
                    (0.05, 6.0),
                    (0.9, 0.95),
                ] {
                    assert_eq!(
                        Lift::PRODUCTION.blend_local(elo, &e, home, away),
                        sim::blend_lambdas(elo, Some(&e), home, away),
                        "weights {w_elo}/{w_dc}/{w_pi}, clubs {home} v {away}, Elo λs {elo:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_zero_lift_turns_the_lift_and_the_follow_off() {
        let elo_only = Ensemble::from_embedded_data(1.0, 0.0, 0.0).expect("embedded ensemble");
        let off = Lift {
            lift: 0.0,
            ..Lift::PRODUCTION
        };
        for elo in [(0.3, 2.9), (2.4, 0.5), (1.2, 1.1)] {
            assert_eq!(off.blend(elo, &elo_only, 0, 1), elo);
        }
        let (lh, la) = Lift::PRODUCTION.blend((0.3, 2.9), &elo_only, 0, 1);
        assert!(lh > 0.3 && la > 2.9, "production lifts both: {lh}, {la}");
    }

    /// With no variable set the backtest is production: its weights, its Elo
    /// functions, no goal scale and production's ρ rule.
    #[test]
    fn no_knob_is_production() {
        let cfg = Cfg::from_vars(|_| None);
        assert_eq!(cfg.w, superlig_sim::model_version::DEFAULT_ENSEMBLE_WEIGHTS);
        assert!(cfg.production_elo);
        assert!(cfg.lift == Lift::PRODUCTION);
        assert_eq!(cfg.rho, Rho::Production);
        assert_eq!(cfg.seasons, DEV.collect::<Vec<_>>());
    }

    /// `EVAL_BASE` scales the Elo λs; at its production value it keeps
    /// production's functions, and the local ones it switches to are
    /// production's at production values.
    #[test]
    fn eval_base_scales_the_elo_lambdas_only() {
        let one = |key: &'static str, value: String| {
            Cfg::from_vars(move |k| (k == key).then(|| value.clone()))
        };
        assert!(one("EVAL_BASE", data::BASE.to_string()).production_elo);
        let local = Cfg {
            production_elo: false,
            ..Cfg::from_vars(|_| None)
        };
        let lower = one("EVAL_BASE", "1.3".into());
        assert!(!lower.production_elo);
        for (rh, ra) in [(1500.0, 1500.0), (1700.0, 1400.0), (1350.0, 1820.5)] {
            assert_eq!(local.elo_lambdas(rh, ra), sim::elo_lambdas(rh, ra));
            let (ph, pa) = sim::elo_lambdas(rh, ra);
            let (lh, la) = lower.elo_lambdas(rh, ra);
            assert!((lh / ph - 1.3 / data::BASE).abs() < 1e-12);
            assert!((la / pa - 1.3 / data::BASE).abs() < 1e-12);
            for (hs, as_) in [(0, 0), (3, 1), (0, 4)] {
                let d = sim::elo_delta(rh, ra, hs, as_);
                assert_eq!(local.elo_delta(data::ELO_K, rh, ra, hs, as_), d);
                assert_eq!(lower.elo_delta(data::ELO_K, rh, ra, hs, as_), d);
            }
        }
    }

    /// `EVAL_W` weights are normalized on use.
    #[test]
    fn eval_w_is_normalized() {
        let cfg = Cfg::from_vars(|k| (k == "EVAL_W").then(|| " 5, 3 ,2".to_string()));
        assert_eq!(cfg.w, (5.0, 3.0, 2.0));
        let scaled = Ensemble::from_embedded_data(5.0, 3.0, 2.0).expect("embedded ensemble");
        let shipped = Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("embedded ensemble");
        for elo in [(0.3, 2.9), (1.35, 1.35), (2.4, 0.5)] {
            let (a, b) = (
                Lift::PRODUCTION.blend(elo, &scaled, 3, 17),
                Lift::PRODUCTION.blend(elo, &shipped, 3, 17),
            );
            assert!((a.0 - b.0).abs() < 1e-12 && (a.1 - b.1).abs() < 1e-12);
        }
    }

    #[test]
    #[should_panic(expected = "three non-negative weights")]
    fn eval_w_needs_three_weights() {
        Cfg::from_vars(|k| (k == "EVAL_W").then(|| "0.6,0.4".to_string()));
    }

    #[test]
    fn the_goal_scale_multiplies_both_lambdas_before_the_clamp() {
        let elo_only = Ensemble::from_embedded_data(1.0, 0.0, 0.0).expect("embedded ensemble");
        let shipped = Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("embedded ensemble");
        let scaled = Lift {
            goals: 0.9,
            ..Lift::PRODUCTION
        };
        for e in [&elo_only, &shipped] {
            for elo in [(0.3, 2.9), (2.4, 0.5), (1.2, 1.1)] {
                let (ph, pa) = Lift::PRODUCTION.blend(elo, e, 3, 17);
                assert_eq!(scaled.blend(elo, e, 3, 17), (ph * 0.9, pa * 0.9));
            }
        }
        // Scaled first, then clamped: 5.4 · 0.95 is clamped to 5, where
        // clamping first would give 4.75, and 0.15 · 0.95 back up to 0.15.
        let no_lift = Lift {
            lift: 0.0,
            goals: 0.95,
            ..Lift::PRODUCTION
        };
        assert_eq!(no_lift.blend((0.15, 5.4), &elo_only, 0, 1), (0.15, 5.0));
    }

    #[test]
    fn rho_follows_production_unless_set() {
        let with_dc = Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("embedded ensemble");
        let without_dc = Ensemble::from_embedded_data(0.7, 0.0, 0.3).expect("embedded ensemble");
        let fitted = with_dc.dc.rho;
        assert_ne!(fitted, 0.0);
        assert_eq!(Rho::parse(None), Rho::Production);
        assert_eq!(Rho::Production.value(&with_dc), fitted);
        assert_eq!(Rho::Production.value(&without_dc), 0.0);
        assert_eq!(Rho::parse(Some("fit")), Rho::Fitted);
        assert_eq!(Rho::Fitted.value(&without_dc), fitted);
        assert_eq!(Rho::parse(Some("-0.09")).value(&without_dc), -0.09);
        assert_eq!(Rho::parse(Some(" 0 ")).value(&with_dc), 0.0);
    }

    #[test]
    #[should_panic(expected = "neither a number nor `fit`")]
    fn a_rho_that_is_not_a_number_is_refused() {
        Rho::parse(Some("dc"));
    }

    /// The configuration these variables, and no others, describe.
    fn cfg_of(vars: &[(&str, &str)]) -> Cfg {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|&(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Cfg::from_vars(move |key| vars.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
    }

    fn shipped() -> Ensemble {
        Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("embedded ensemble")
    }

    /// Base rates whose 3+, both-score and complementary rates all differ,
    /// so a loss taken from the wrong one shows.
    const RATES: BaseRates = BaseRates {
        p3: [0.45, 0.26, 0.29],
        over: 0.47,
        btts: 0.55,
    };

    /// Each knob reaches its own field and leaves the others at production.
    #[test]
    fn knobs_reach_the_configuration() {
        let goals = cfg_of(&[("EVAL_GOALS", "0.9")]);
        assert!(
            goals.lift
                == Lift {
                    goals: 0.9,
                    ..Lift::PRODUCTION
                }
        );
        assert_eq!(goals.rho, Rho::Production);
        assert!(goals.production_elo);

        let fixed = cfg_of(&[("EVAL_RHO", "-0.1")]);
        assert_eq!(fixed.rho, Rho::Fixed(-0.1));
        assert!(fixed.lift == Lift::PRODUCTION && fixed.production_elo);
        assert_eq!(cfg_of(&[("EVAL_RHO", "fit")]).rho, Rho::Fitted);

        // A Dixon-Coles weight of 0 leaves ρ at 0, as production does,
        // unless EVAL_RHO asks for the fitted one.
        let no_dc = cfg_of(&[("EVAL_W", "0.7,0,0.3")]);
        assert_eq!(no_dc.w, (0.7, 0.0, 0.3));
        assert_eq!(no_dc.rho, Rho::Production);
        let e = Ensemble::from_embedded_data(no_dc.w.0, no_dc.w.1, no_dc.w.2)
            .expect("embedded ensemble");
        assert_ne!(e.dc.rho, 0.0);
        assert_eq!(no_dc.rho.value(&e), 0.0);
        let fitted = cfg_of(&[("EVAL_W", "0.7,0,0.3"), ("EVAL_RHO", "fit")]);
        assert_eq!(fitted.rho.value(&e), e.dc.rho);

        // EVAL_BASE scales the Elo component's λs before the blend: with
        // the lift off, each blended λ moves by w_elo / Σw = 0.5 of the
        // Elo λ's change, and Dixon-Coles and pi's share stays put.
        let base = cfg_of(&[("EVAL_BASE", "1.2"), ("EVAL_LIFT", "0")]);
        let unscaled = cfg_of(&[("EVAL_LIFT", "0")]);
        assert_eq!(base.base, 1.2);
        assert!(!base.production_elo && unscaled.production_elo);
        assert!(base.lift == unscaled.lift && base.rho == Rho::Production);
        let e = shipped();
        for (rh, ra) in [(1500.0, 1500.0), (1700.0, 1400.0), (1350.0, 1820.5)] {
            let (eh, ea) = sim::elo_lambdas(rh, ra);
            let (sh, sa) = base.lift.blend(base.elo_lambdas(rh, ra), &e, 3, 17);
            let (ph, pa) = unscaled.lift.blend(unscaled.elo_lambdas(rh, ra), &e, 3, 17);
            let change = 1.2 / data::BASE - 1.0;
            assert!((sh - ph - 0.5 * eh * change).abs() < 1e-12, "{rh} v {ra}");
            assert!((sa - pa - 0.5 * ea * change).abs() < 1e-12, "{rh} v {ra}");
        }
    }

    /// A hand-built match priced as `main` prices it: production's λs, the
    /// 3+ and both-score forecasts of `probs_from_lambdas` (and of the score
    /// table itself), and each loss in its own accumulator.
    #[test]
    fn a_match_is_priced_and_scored_by_hand() {
        let cfg = Cfg::from_vars(|_| None);
        let e = shipped();
        let (elo, clubs) = ((1750.0, 1400.0), (3, 17));
        let (lh, la) = sim::blend_lambdas(sim::elo_lambdas(elo.0, elo.1), Some(&e), 3, 17);
        let p = sim::probs_from_lambdas(lh, la, e.dc.rho);
        let table = dixoncoles::score_table(lh, la, e.dc.rho);
        let total: f64 = table.iter().flatten().sum();
        let mass = |event: fn(usize, usize) -> bool| {
            let mut sum = 0.0;
            for (x, row) in table.iter().enumerate() {
                for (y, &q) in row.iter().enumerate() {
                    if event(x, y) {
                        sum += q;
                    }
                }
            }
            sum / total
        };

        // 3-0: three goals, one side scoring, the favourite winning.
        let won = price(&cfg, &e, &RATES, elo, clubs, (3, 0));
        assert_eq!(won.lam, (lh, la));
        assert_eq!(
            won.p3,
            [
                p.home_win_pct / 100.0,
                p.draw_pct / 100.0,
                p.away_win_pct / 100.0
            ]
        );
        assert_eq!(
            (won.p_over, won.p_both),
            (p.over25_pct / 100.0, p.btts_pct / 100.0)
        );
        assert!((won.p_over - mass(|x, y| x + y >= 3)).abs() < 1e-12);
        assert!((won.p_both - mass(|x, y| x >= 1 && y >= 1)).abs() < 1e-12);
        assert!(
            (won.p_over - won.p_both).abs() > 0.05,
            "3+ {} and both score {} must differ for a swap to show",
            won.p_over,
            won.p_both
        );
        assert_eq!(won.p_score, table[3][0] / total);
        let a = &won.acc;
        assert_eq!((a.n, a.goals, a.over, a.both, a.hits), (1, 3, 1, 0, 1));
        assert_eq!(a.lam, lh + la);
        assert_eq!((a.p_over, a.p_both), (won.p_over, won.p_both));
        assert_eq!(a.ou, nll(won.p_over));
        assert_eq!(a.btts, nll(1.0 - won.p_both));
        assert_eq!(a.base_ou, nll(RATES.over));
        assert_eq!(a.base_btts, nll(1.0 - RATES.btts));
        assert_eq!((a.ll, a.rps), (nll(won.p3[0]), rps(won.p3, 0)));
        assert_eq!((a.base_ll, a.base_rps), (nll(0.45), rps(RATES.p3, 0)));
        assert_eq!(a.score, nll(won.p_score));
        assert_eq!(a.outcomes, vec![(won.p3, 0)]);

        // 1-1: two goals, both scoring, a draw the favourite's pick missed.
        let drawn = price(&cfg, &e, &RATES, elo, clubs, (1, 1));
        assert_eq!((drawn.p_over, drawn.p_both), (won.p_over, won.p_both));
        let d = &drawn.acc;
        assert_eq!((d.goals, d.over, d.both, d.hits), (2, 0, 1, 0));
        assert_eq!(d.ou, nll(1.0 - won.p_over));
        assert_eq!(d.btts, nll(won.p_both));
        assert_eq!(d.base_ou, nll(1.0 - RATES.over));
        assert_eq!(d.base_btts, nll(RATES.btts));
        assert_eq!(d.ll, nll(won.p3[1]));
        assert_eq!(drawn.p_score, table[1][1] / total);

        // A score past the table's edge, and the goal scale's reach.
        assert_eq!(price(&cfg, &e, &RATES, elo, clubs, (11, 0)).p_score, 1e-6);
        let scaled = price(
            &cfg_of(&[("EVAL_GOALS", "0.9")]),
            &e,
            &RATES,
            elo,
            clubs,
            (3, 0),
        );
        assert_eq!(scaled.lam, (lh * 0.9, la * 0.9));
    }

    /// EVAL_RHO=-0.1 prices with that ρ, at any Dixon-Coles weight, and
    /// moves the score table, not the λs.
    #[test]
    fn a_fixed_rho_reaches_the_score_table() {
        let (elo, clubs) = ((1600.0, 1550.0), (3, 17));
        for (weights, w) in [
            ("0.5,0.3,0.2", (0.5, 0.3, 0.2)),
            ("0.7,0,0.3", (0.7, 0.0, 0.3)),
        ] {
            let e = Ensemble::from_embedded_data(w.0, w.1, w.2).expect("embedded ensemble");
            assert_ne!(e.dc.rho, -0.1);
            let fixed = cfg_of(&[("EVAL_W", weights), ("EVAL_RHO", "-0.1")]);
            let default = cfg_of(&[("EVAL_W", weights)]);
            let p = price(&fixed, &e, &RATES, elo, clubs, (1, 1));
            let q = price(&default, &e, &RATES, elo, clubs, (1, 1));
            assert_eq!(p.lam, q.lam);
            let (lh, la) = p.lam;
            let table = dixoncoles::score_table(lh, la, -0.1);
            let total: f64 = table.iter().flatten().sum();
            assert_eq!(p.p_score, table[1][1] / total);
            assert_ne!(p.p_score, q.p_score);
            let probs = sim::probs_from_lambdas(lh, la, -0.1);
            assert_eq!(p.p3[1], probs.draw_pct / 100.0);
            assert_eq!(p.p_both, probs.btts_pct / 100.0);
            // Unset, ρ is the fitted one while Dixon-Coles has weight, else 0.
            let rho = if w.1 > 0.0 { e.dc.rho } else { 0.0 };
            let unset = dixoncoles::score_table(lh, la, rho);
            assert_eq!(q.p_score, unset[1][1] / unset.iter().flatten().sum::<f64>());
        }
    }

    #[test]
    fn the_season_sim_takes_the_knobs_it_honours() {
        assert_eq!(Cfg::from_vars(|_| None).season_sim, None);
        let cfg = cfg_of(&[
            ("EVAL_SEASON_SIM", "200"),
            ("EVAL_W", "0.6,0.2,0.2"),
            ("EVAL_HL", "1000"),
            ("EVAL_K", "40"),
            ("EVAL_K_PRE", "30"),
            ("EVAL_PROMO", "1420"),
            ("EVAL_PROMO_REL", "-60"),
            ("EVAL_REGRESS", "0.1"),
            ("EVAL_REFIT", "week"),
        ]);
        assert_eq!(cfg.season_sim, Some(200));
        // Every λ knob is caught by being set, whatever its value.
        for knob in SEASON_SIM_IGNORES {
            let set = |k: &str| (k == knob).then(|| "1".to_string());
            assert_eq!(season_sim_conflicts(&set), vec![knob]);
        }
        // Without EVAL_SEASON_SIM they are knobs like any other.
        let lambda = cfg_of(&[("EVAL_GOALS", "0.9"), ("EVAL_RHO", "fit")]);
        assert_eq!(lambda.season_sim, None);
    }

    #[test]
    #[should_panic(
        expected = "EVAL_SEASON_SIM prices matches with production's own λ rules, which ignore EVAL_GOALS, EVAL_RHO; unset them or EVAL_SEASON_SIM"
    )]
    fn the_season_sim_refuses_lambda_knobs() {
        cfg_of(&[
            ("EVAL_RHO", "-0.1"),
            ("EVAL_SEASON_SIM", "200"),
            ("EVAL_GOALS", "0.9"),
        ]);
    }

    /// Mean forecast against mean outcome, summed across `add`.
    #[test]
    fn calibration_in_the_large_by_hand() {
        let mut acc = Acc::default();
        acc.add(&Acc {
            n: 3,
            lam: 8.0,
            goals: 9,
            p_over: 1.6,
            over: 2,
            p_both: 1.2,
            both: 1,
            ..Acc::default()
        });
        acc.add(&Acc {
            n: 1,
            lam: 3.0,
            goals: 1,
            p_over: 0.6,
            over: 1,
            p_both: 0.6,
            both: 0,
            ..Acc::default()
        });
        let expected: Vec<(String, String)> = [
            ("dev_goals_pred", "2.7500"),
            ("dev_goals_obs", "2.5000"),
            ("dev_ou25_pred", "0.5500"),
            ("dev_ou25_obs", "0.7500"),
            ("dev_btts_pred", "0.4500"),
            ("dev_btts_obs", "0.2500"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(acc.in_the_large("dev"), expected);
    }

    #[test]
    fn losses_by_hand() {
        assert_eq!(outcome(2, 1), 0);
        assert_eq!(outcome(1, 1), 1);
        assert_eq!(outcome(0, 3), 2);
        assert!((nll(0.5) - 2f64.ln()).abs() < 1e-15);
        assert_eq!(nll(0.0), nll(1e-9));
        // ((0.5 - 1)² + (0.8 - 1)²) / 2, ((0.5 - 0)² + (0.8 - 1)²) / 2 and
        // (0.5² + 0.8²) / 2.
        assert!((rps([0.5, 0.3, 0.2], 0) - 0.145).abs() < 1e-12);
        assert!((rps([0.5, 0.3, 0.2], 1) - 0.145).abs() < 1e-12);
        assert!((rps([0.5, 0.3, 0.2], 2) - 0.445).abs() < 1e-12);
    }

    #[test]
    fn base_rates_count_results_goals_and_both_scoring() {
        let day = NaiveDate::from_ymd_opt(2020, 1, 1).expect("valid date");
        let m = |home_score, away_score| HistoricalMatch {
            date: day,
            home_team: "A".into(),
            away_team: "B".into(),
            home_score,
            away_score,
            neutral: false,
        };
        let b = BaseRates::from_matches(&[m(2, 1), m(1, 0), m(0, 0), m(0, 3)]);
        assert_eq!(b.p3, [0.5, 0.25, 0.25]);
        assert_eq!(b.over, 0.5);
        assert_eq!(b.btts, 0.25);
    }

    /// `(forecast, times it happened, times it was made)` as pairs.
    fn grouped(rates: &[(f64, usize, usize)]) -> Vec<(f64, bool)> {
        rates
            .iter()
            .flat_map(|&(p, hits, n)| (0..n).map(move |i| (p, i < hits)))
            .collect()
    }

    #[test]
    fn calibrated_forecasts_have_slope_one() {
        let f = grouped(&[(0.2, 2, 10), (0.5, 5, 10), (0.8, 8, 10)]);
        let (b, se) = calibration_slope(&f).expect("fit");
        assert!((b - 1.0).abs() < 1e-9, "slope {b}");
        // At a = 0, b = 1 the logits ±ln 4 cancel in the cross term, so
        // Var(b) = 1 / Σ p(1 − p) logit(p)² = 1 / (20 · 0.16 · ln² 4).
        let expected = 1.0 / (3.2f64.sqrt() * 4f64.ln());
        assert!((se - expected).abs() < 1e-9, "se {se}, expected {expected}");
    }

    #[test]
    fn overconfident_forecasts_have_slope_below_one() {
        // 20% and 80% forecasts come true 35% and 65% of the time. The
        // observed rates lie on a line through the origin in logit space,
        // so the fit passes through all three: b = logit(0.65) / logit(0.8).
        let expected = (0.65f64 / 0.35).ln() / 4f64.ln();
        let f = grouped(&[(0.2, 7, 20), (0.5, 10, 20), (0.8, 13, 20)]);
        let (b, se) = calibration_slope(&f).expect("fit");
        assert!(
            (b - expected).abs() < 1e-9,
            "slope {b}, expected {expected}"
        );
        // Four times the matches at the same rates: same slope, half the error.
        let f4 = grouped(&[(0.2, 28, 80), (0.5, 40, 80), (0.8, 52, 80)]);
        let (b4, se4) = calibration_slope(&f4).expect("fit");
        assert!((b4 - expected).abs() < 1e-9, "slope {b4}");
        assert!((se / se4 - 2.0).abs() < 1e-6, "se {se} vs {se4}");
    }

    /// The evidence lines, in order: skill against the base rates, then
    /// calibration per outcome. Their keys are what `EVAL_METRICS` writes.
    #[test]
    fn evidence_is_skill_then_calibration() {
        let acc = Acc {
            n: 4,
            ll: 3.0,
            rps: 0.6,
            ou: 2.0,
            btts: 3.0,
            base_ll: 4.0,
            base_rps: 0.8,
            base_ou: 2.5,
            base_btts: 2.0,
            outcomes: vec![
                ([0.5, 0.3, 0.2], 0),
                ([0.4, 0.3, 0.3], 1),
                ([0.6, 0.2, 0.2], 2),
                ([0.3, 0.3, 0.4], 2),
            ],
            ..Acc::default()
        };
        let lines = acc.evidence("dev");
        let mut keys: Vec<String> = [
            "1x2_ll_base",
            "1x2_skill",
            "rps_base",
            "rpss",
            "ou25_ll_base",
            "ou25_skill",
            "btts_ll_base",
            "btts_skill",
        ]
        .iter()
        .map(|k| format!("dev_{k}"))
        .collect();
        for name in ["home", "draw", "away"] {
            for part in ["pred", "obs", "slope", "slope_se"] {
                keys.push(format!("dev_cal_{name}_{part}"));
            }
        }
        assert_eq!(
            lines.iter().map(|l| &l.0).collect::<Vec<_>>(),
            keys.iter().collect::<Vec<_>>()
        );
        let value = |key: &str| {
            lines
                .iter()
                .find(|l| l.0 == key)
                .map(|l| l.1.as_str())
                .expect("key")
        };
        // Base losses per match, and 1 − model / base.
        assert_eq!(value("dev_1x2_ll_base"), "1.00000");
        assert_eq!(value("dev_1x2_skill"), "0.25000");
        assert_eq!(value("dev_rps_base"), "0.20000");
        assert_eq!(value("dev_rpss"), "0.25000");
        assert_eq!(value("dev_ou25_skill"), "0.20000");
        assert_eq!(value("dev_btts_skill"), "-0.50000");
        // Mean forecast, then how often it happened: home 1, draw 1, away 2 of 4.
        assert_eq!(value("dev_cal_home_pred"), "0.4500");
        assert_eq!(value("dev_cal_home_obs"), "0.2500");
        assert_eq!(value("dev_cal_draw_pred"), "0.2750");
        assert_eq!(value("dev_cal_draw_obs"), "0.2500");
        assert_eq!(value("dev_cal_away_pred"), "0.2750");
        assert_eq!(value("dev_cal_away_obs"), "0.5000");
    }

    #[test]
    fn a_forecast_that_never_varies_has_no_slope() {
        assert_eq!(calibration_slope(&grouped(&[(0.3, 3, 10)])), None);
        assert_eq!(calibration_slope(&[]), None);
    }

    #[test]
    fn metrics_json_sorts_keys_and_keeps_printed_numbers() {
        let summary: Vec<(String, String)> = [
            ("dev_n", "2717".to_string()),
            ("dev_acc", "0.5410".to_string()),
            ("dev_1x2_ll", "0.99907".to_string()),
            ("dev_cal_draw_slope", format!("{:.3}", f64::NAN)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        assert_eq!(
            metrics_json(&summary),
            "{\n  \"dev_1x2_ll\": 0.99907,\n  \"dev_acc\": 0.541,\n  \"dev_cal_draw_slope\": null,\n  \"dev_n\": 2717\n}\n"
        );
    }
}
