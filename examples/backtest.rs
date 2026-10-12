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
//!   EVAL_W=0.5,0.3,0.2      blend weights elo,dc,pi
//!   EVAL_HL=1460            Dixon-Coles half-life in days
//!   EVAL_K=20               in-season Elo K [data::ELO_K]
//!   EVAL_K_PRE=20           preseason track K [data::PRESEASON_K]
//!   EVAL_HA=80              Elo home advantage (update and λ)
//!   EVAL_DDIV=1600          Elo→goals scale
//!   EVAL_BASE=1.35          Elo→goals base rate
//!   EVAL_PROMO=1450         Elo of a club never seen before [data::NEW_CLUB_ELO]
//!   EVAL_REGRESS=0          preseason pull towards the league mean, 0..1
//!   EVAL_PROMO_REL=-60      promoted clubs start at last season's league
//!                           mean plus this offset [off: new clubs at
//!                           EVAL_PROMO, returning clubs keep old rating]
//!   EVAL_DUMP=path.csv      write one row per scored match
//!   EVAL_SEASON_SIM=2000    also replay each scored season from its start
//!                           with the production season simulator and
//!                           compare the spread of final points to reality
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

fn env_f(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .map(|v| {
            v.parse()
                .unwrap_or_else(|_| panic!("{key}={v} is not a number"))
        })
        .unwrap_or(default)
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
    /// True when every Elo knob is at its production value, so the shared
    /// production functions are called rather than the local variants.
    production_elo: bool,
}

impl Cfg {
    fn from_env() -> Self {
        let holdout = std::env::var("EVAL_HOLDOUT").is_ok_and(|v| v == "1");
        let seasons = match std::env::var("EVAL_SEASONS") {
            Ok(s) => s
                .split(',')
                .map(|y| y.trim().parse().expect("season year"))
                .collect(),
            Err(_) => DEV
                .chain(if holdout { HOLDOUT.to_vec() } else { vec![] })
                .collect(),
        };
        let w: Vec<f64> = std::env::var("EVAL_W")
            .unwrap_or_else(|_| "0.5,0.3,0.2".into())
            .split(',')
            .map(|x| x.trim().parse().expect("weight"))
            .collect();
        let refit = std::env::var("EVAL_REFIT").unwrap_or_else(|_| "season".into());
        assert!(
            refit == "season" || refit == "week",
            "EVAL_REFIT must be season|week"
        );
        let (k, home_adv, d_div, base) = (
            env_f("EVAL_K", data::ELO_K),
            env_f("EVAL_HA", data::HOME_ADV),
            env_f("EVAL_DDIV", data::D_DIV),
            env_f("EVAL_BASE", data::BASE),
        );
        Cfg {
            seasons,
            weekly_refit: refit == "week",
            w: (w[0], w[1], w[2]),
            half_life: env_f("EVAL_HL", 1460.0),
            k,
            k_pre: env_f("EVAL_K_PRE", data::PRESEASON_K),
            home_adv,
            d_div,
            base,
            promo: env_f("EVAL_PROMO", data::NEW_CLUB_ELO),
            promo_rel: std::env::var("EVAL_PROMO_REL")
                .ok()
                .map(|v| v.parse().expect("EVAL_PROMO_REL")),
            regress: env_f("EVAL_REGRESS", 0.0),
            production_elo: k == data::ELO_K
                && home_adv == data::HOME_ADV
                && d_div == data::D_DIV
                && base == data::BASE,
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

#[derive(Default, Clone, Copy)]
struct Acc {
    n: usize,
    ll: f64,
    rps: f64,
    ou: f64,
    btts: f64,
    score: f64,
    hits: usize,
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
    }
    fn line(&self, prefix: &str) -> String {
        let n = self.n as f64;
        format!(
            "{prefix}_1x2_ll={:.5}\n{prefix}_rps={:.5}\n{prefix}_ou25_ll={:.5}\n{prefix}_btts_ll={:.5}\n{prefix}_score_ll={:.5}\n{prefix}_acc={:.4}\n{prefix}_n={}",
            self.ll / n,
            self.rps / n,
            self.ou / n,
            self.btts / n,
            self.score / n,
            self.hits as f64 / n,
            self.n
        )
    }
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
        if let (Some(e), Ok(n)) = (ens.as_ref(), std::env::var("EVAL_SEASON_SIM")) {
            season_sim(
                year,
                &season,
                &idx,
                &elo_in,
                e,
                n.parse().expect("EVAL_SEASON_SIM"),
            );
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
            if let Some(e) = ens.as_ref() {
                for m in &season[i..j] {
                    let (h, a) = (idx.canonical(&m.home_team), idx.canonical(&m.away_team));
                    let (rh, ra) = (elo_in[&m.home_team], elo_in[&m.away_team]);
                    let (lh, la) = sim::blend_lambdas(cfg.elo_lambdas(rh, ra), Some(e), h, a);
                    let rho = sim::score_rho(Some(e)).unwrap_or(0.0);
                    let p = sim::probs_from_lambdas(lh, la, rho);
                    let p3 = [
                        p.home_win_pct / 100.0,
                        p.draw_pct / 100.0,
                        p.away_win_pct / 100.0,
                    ];
                    let o = match m.home_score.cmp(&m.away_score) {
                        std::cmp::Ordering::Greater => 0,
                        std::cmp::Ordering::Equal => 1,
                        std::cmp::Ordering::Less => 2,
                    };
                    let table = dixoncoles::score_table(lh, la, rho);
                    let total: f64 = table.iter().flatten().sum();
                    let (hs, as_) = (m.home_score as usize, m.away_score as usize);
                    let pscore = if hs <= MAX_GOALS && as_ <= MAX_GOALS {
                        table[hs][as_] / total
                    } else {
                        1e-6
                    };
                    let pov = p.over25_pct / 100.0;
                    let pbtts = p.btts_pct / 100.0;
                    let over = hs + as_ >= 3;
                    let both = hs >= 1 && as_ >= 1;
                    let c1 = p3[0] - if o == 0 { 1.0 } else { 0.0 };
                    let c2 = p3[0] + p3[1] - if o <= 1 { 1.0 } else { 0.0 };
                    let pick = (0..3)
                        .max_by(|&x, &y| p3[x].total_cmp(&p3[y]))
                        .expect("three");
                    acc.add(&Acc {
                        n: 1,
                        ll: -p3[o].max(1e-9).ln(),
                        rps: (c1 * c1 + c2 * c2) / 2.0,
                        ou: -(if over { pov } else { 1.0 - pov }).max(1e-9).ln(),
                        btts: -(if both { pbtts } else { 1.0 - pbtts }).max(1e-9).ln(),
                        score: -pscore.max(1e-9).ln(),
                        hits: usize::from(pick == o),
                    });
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
                            p3[0],
                            p3[1],
                            p3[2],
                            pov,
                            pbtts,
                            pscore,
                            lh,
                            la,
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
                "season {}-{:02}: n={} 1x2_ll={:.4} rps={:.4} ou25_ll={:.4} btts_ll={:.4} score_ll={:.4} acc={:.3}",
                year - 1,
                year % 100,
                acc.n,
                acc.ll / acc.n as f64,
                acc.rps / acc.n as f64,
                acc.ou / acc.n as f64,
                acc.btts / acc.n as f64,
                acc.score / acc.n as f64,
                acc.hits as f64 / acc.n as f64
            );
            if HOLDOUT.contains(&year) {
                hold.add(&acc);
            } else {
                dev.add(&acc);
            }
        }
        prev_clubs = clubs;
    }

    if dev.n > 0 {
        println!("{}", dev.line("dev"));
    }
    if hold.n > 0 {
        println!("{}", hold.line("hold"));
    }
    if let Some((path, s)) = dump {
        std::fs::write(&path, s).expect("write dump");
        eprintln!("per-match rows written to {path}");
    }
}
