//! Calibration guard for the model that ships: the default blend (Elo 0.5,
//! Dixon-Coles 0.3, pi-ratings 0.2, built the way `main.rs` builds it) at
//! preseason, nothing played. It is measured two ways:
//!
//! - per fixture: the exact 1X2 split and expected goals that
//!   `World::fixture_probs` gives each of the 306 fixtures, which is what
//!   `/api/matches` and the forecast log carry, averaged over the calendar;
//! - per season: `World::simulate`, which also draws each season's ratings
//!   around their estimates (`RATING_SIGMA`). The wider spread of strengths
//!   means fewer draws and more goals than per fixture.
//!
//! Both are set against the embedded history: every played Süper Lig match
//! since 2012-13, the 2022-23 walkovers left out. Where the model stood when
//! these figures were pinned (`cargo test --test calibration -- --nocapture`
//! prints the full report):
//!
//! - home wins: 44.9% per fixture against 45.4% in the history, inside the
//!   history's own sampling error (0.7 points);
//! - draws are under-predicted: 24.8% per fixture and 24.4% in simulated
//!   seasons against 25.9%. The season shortfall, 1.4 points, is about two
//!   standard errors of the history (0.65). The missing draws turn up as
//!   away wins, 30.4% against 28.8%;
//! - goals are over-predicted: 2.88 expected a match per fixture and 2.91
//!   in simulated seasons against 2.79 scored, about three and four standard
//!   errors of the history apart. That error is 0.033 goals, not the 0.025
//!   independent matches would give: scoring moves from season to season
//!   (2.64 to 3.05 a match), so the seasons, not the matches, are the
//!   independent draws.
//!
//! Two guards, kept separate:
//!
//! 1. `calibration_is_pinned`: today's figures. A rate that moves more than
//!    a point (so any 2-point shift), or goals that move more than 0.05 a
//!    match, fail. The simulated figures' Monte Carlo error is under a tenth
//!    of that. A deliberate change (model constants, a DC refit, new
//!    history) re-measures and re-pins `LIMITS`, along with the docs that
//!    quote these numbers.
//! 2. `calibration_stays_within_its_band_of_the_history`: the gaps above may
//!    not grow past each figure's band. Home wins may sit 1.5 points either
//!    side of the history; the draw and goal bands end about half a point
//!    (0.06-0.07 goals) beyond where those gaps stand now, because they are
//!    known errors, not slack to spend.
//!
//! Mean points per club follow from the draw rate alone (each decisive match
//! hands out 3 points, a draw 2), so they are reported and checked against
//! that identity rather than pinned. A pure-Elo `World` is not checked:
//! `ENSEMBLE_WEIGHTS=1,0,0` is an opt-out, not what runs.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::Datelike;
use superlig_sim::history::{self, HistoricalMatch};
use superlig_sim::sim::{Ensemble, SimConfig, World};

/// `ENSEMBLE_WEIGHTS` when it is unset: the blend production runs.
const SHIPPED_WEIGHTS: (f64, f64, f64) = (0.5, 0.3, 0.2);

/// Simulated seasons, in batches so the spread of the batch means gives the
/// Monte Carlo standard error. 2,000 seasons take about a second in a debug
/// build.
const SEASON_BATCHES: usize = 10;
const SEASONS_PER_BATCH: usize = 200;
const SEED: u64 = 4242;

/// A pinned figure: today's value, how far it may move, and the band its
/// gap to the history must stay inside. Rates are percentages, goals are
/// per match.
struct Limits {
    name: &'static str,
    pinned: f64,
    pin_tol: f64,
    band: (f64, f64),
}

const LIMITS: [Limits; 5] = [
    Limits {
        name: "fixture home wins %",
        pinned: 44.86,
        pin_tol: 1.0,
        band: (-1.5, 1.5),
    },
    Limits {
        name: "fixture draws %",
        pinned: 24.78,
        pin_tol: 1.0,
        band: (-1.5, 1.5),
    },
    Limits {
        name: "fixture goals",
        pinned: 2.878,
        pin_tol: 0.05,
        band: (-0.15, 0.15),
    },
    Limits {
        name: "season draws %",
        pinned: 24.40,
        pin_tol: 1.0,
        band: (-2.0, 2.0),
    },
    Limits {
        name: "season goals",
        pinned: 2.915,
        pin_tol: 0.05,
        band: (-0.2, 0.2),
    },
];

/// One figure as the model gives it and as the league played it.
struct Figure {
    name: &'static str,
    model: f64,
    /// Monte Carlo standard error; 0 for an exact sum.
    model_se: f64,
    history: f64,
    history_se: f64,
}

impl Figure {
    fn gap(&self) -> f64 {
        self.model - self.history
    }

    fn limits(&self) -> Option<&'static Limits> {
        LIMITS.iter().find(|l| l.name == self.name)
    }
}

struct Measured {
    figures: Vec<Figure>,
    /// The printed table, repeated in every failure message.
    report: String,
    /// Mean simulated points per club, and `17 * (3 - draw rate)` from the
    /// same seasons' draws.
    season_points: f64,
    season_points_from_draws: f64,
}

/// The shipped model on its own: preseason ratings and nothing played.
/// Results the shipped calendar has recorded would fix part of the season
/// and make these checks depend on how far it has got.
fn shipped_preseason_world() -> World {
    let mut w = World::new();
    w.played.clear();
    w.refresh_ratings();
    let (elo, dc, pi) = SHIPPED_WEIGHTS;
    w.ensemble = Some(Ensemble::from_embedded_data(elo, dc, pi).expect("embedded ensemble data"));
    w
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn standard_error(xs: &[f64]) -> f64 {
    let mu = mean(xs);
    let var = xs.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / (xs.len() - 1) as f64;
    (var / xs.len() as f64).sqrt()
}

/// Seasons start in August, which keeps the July 2020 matches in 2019-20.
fn season_of(m: &HistoricalMatch) -> i32 {
    if m.date.month() >= 8 {
        m.date.year()
    } else {
        m.date.year() - 1
    }
}

/// Measured once and shared: the tests read the same figures.
fn measured() -> &'static Measured {
    static MEASURED: OnceLock<Measured> = OnceLock::new();
    MEASURED.get_or_init(measure)
}

/// A per-match quantity's mean over the history and its standard error: the
/// larger of the independent-match error and the error clustered by season.
/// The matches of one season share its clubs and its scoring, so they are
/// not independent draws; for goals that matters, the seasons' means scatter
/// enough to make the clustered error a third larger (0.033 against 0.025).
/// With only 14 seasons the clustered error is itself rough, and for the
/// outcome rates it comes out a little smaller, so the larger one is kept.
fn history_mean(
    seasons: &BTreeMap<i32, Vec<&HistoricalMatch>>,
    x: impl Fn(&HistoricalMatch) -> f64,
) -> (f64, f64) {
    let all: Vec<f64> = seasons.values().flatten().map(|m| x(m)).collect();
    let (n, mu) = (all.len() as f64, mean(&all));
    let k = seasons.len() as f64;
    let spread: f64 = seasons
        .values()
        .map(|s| s.iter().map(|m| x(m) - mu).sum::<f64>().powi(2))
        .sum();
    let clustered = (k / (k - 1.0) * spread).sqrt() / n;
    (mu, standard_error(&all).max(clustered))
}

fn measure() -> Measured {
    let ms = history::load_history();
    assert!(!ms.is_empty());
    let mut seasons: BTreeMap<i32, Vec<&HistoricalMatch>> = BTreeMap::new();
    for m in &ms {
        seasons.entry(season_of(m)).or_default().push(m);
    }
    let pct = |hit: fn(&HistoricalMatch) -> bool| {
        history_mean(&seasons, |m| if hit(m) { 100.0 } else { 0.0 })
    };
    let goals = |side: fn(&HistoricalMatch) -> u16| history_mean(&seasons, |m| f64::from(side(m)));
    let (h_home, h_home_se) = pct(|m| m.home_score > m.away_score);
    let (h_draw, h_draw_se) = pct(|m| m.home_score == m.away_score);
    let (h_away, h_away_se) = pct(|m| m.home_score < m.away_score);
    let (h_home_goals, h_home_goals_se) = goals(|m| m.home_score);
    let (h_away_goals, h_away_goals_se) = goals(|m| m.away_score);
    let (h_goals, h_goals_se) = goals(|m| m.home_score + m.away_score);

    let draw_rates: Vec<(i32, f64)> = seasons
        .iter()
        .map(|(&s, games)| {
            let draws = games
                .iter()
                .filter(|m| m.home_score == m.away_score)
                .count();
            (s, draws as f64 / games.len() as f64 * 100.0)
        })
        .collect();
    let low = draw_rates.iter().min_by(|a, b| a.1.total_cmp(&b.1));
    let high = draw_rates.iter().max_by(|a, b| a.1.total_cmp(&b.1));
    let (Some(&(low_season, low)), Some(&(high_season, high))) = (low, high) else {
        panic!("no seasons in the history");
    };
    let label = |s: i32| format!("{s}-{:02}", (s + 1) % 100);
    let mut report = vec![format!(
        "history: {} played matches over {} seasons, {}..{}; season draw rates \
         {low:.1}% ({}) to {high:.1}% ({}); history se: the larger of per-match and per-season",
        ms.len(),
        seasons.len(),
        label(*seasons.keys().next().expect("seasons")),
        label(*seasons.keys().next_back().expect("seasons")),
        label(low_season),
        label(high_season),
    )];

    let w = shipped_preseason_world();

    // Per fixture: exact sums over each fixture's scoreline table.
    let (mut home, mut draw, mut away, mut xg_home, mut xg_away) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for f in &w.fixtures {
        let p = w.fixture_probs(f.home, f.away);
        home += p.home_win_pct;
        draw += p.draw_pct;
        away += p.away_win_pct;
        xg_home += p.home_xg;
        xg_away += p.away_xg;
    }
    let k = w.fixtures.len() as f64;
    let (home, draw, away, xg_home, xg_away) =
        (home / k, draw / k, away / k, xg_home / k, xg_away / k);
    assert!((home + draw + away - 100.0).abs() < 1e-6);

    // Per season: the production simulator, batch by batch.
    let matches = (SEASONS_PER_BATCH * w.fixtures.len()) as f64;
    let clubs = w.teams.len() as f64;
    let (mut draws, mut goals_sim, mut points) = (vec![], vec![], vec![]);
    for b in 0..SEASON_BATCHES {
        let r = w.simulate(&SimConfig {
            n_sims: SEASONS_PER_BATCH,
            seed: SEED + b as u64,
            ..SimConfig::default()
        });
        // Every draw is in both clubs' records, every goal in one club's.
        draws.push(r.drawn_sum.iter().sum::<f64>() / 2.0 / matches * 100.0);
        goals_sim.push(r.gf_sum.iter().sum::<f64>() / matches);
        points.push(r.points_sum.iter().sum::<f64>() / SEASONS_PER_BATCH as f64 / clubs);
    }
    // Total points are 3 per match less one per draw; per club over a
    // 34-game season that is 34 * (3 - d) / 2 = 17 * (3 - d).
    let per_club = |draw_pct: f64| 17.0 * (3.0 - draw_pct / 100.0);

    let figures = vec![
        Figure {
            name: "fixture home wins %",
            model: home,
            model_se: 0.0,
            history: h_home,
            history_se: h_home_se,
        },
        Figure {
            name: "fixture draws %",
            model: draw,
            model_se: 0.0,
            history: h_draw,
            history_se: h_draw_se,
        },
        Figure {
            name: "fixture away wins %",
            model: away,
            model_se: 0.0,
            history: h_away,
            history_se: h_away_se,
        },
        Figure {
            name: "fixture home goals",
            model: xg_home,
            model_se: 0.0,
            history: h_home_goals,
            history_se: h_home_goals_se,
        },
        Figure {
            name: "fixture away goals",
            model: xg_away,
            model_se: 0.0,
            history: h_away_goals,
            history_se: h_away_goals_se,
        },
        Figure {
            name: "fixture goals",
            model: xg_home + xg_away,
            model_se: 0.0,
            history: h_goals,
            history_se: h_goals_se,
        },
        Figure {
            name: "season draws %",
            model: mean(&draws),
            model_se: standard_error(&draws),
            history: h_draw,
            history_se: h_draw_se,
        },
        Figure {
            name: "season goals",
            model: mean(&goals_sim),
            model_se: standard_error(&goals_sim),
            history: h_goals,
            history_se: h_goals_se,
        },
        Figure {
            name: "season points per club",
            model: mean(&points),
            model_se: standard_error(&points),
            history: per_club(h_draw),
            history_se: 17.0 * h_draw_se / 100.0,
        },
    ];

    let (elo, dc, pi) = SHIPPED_WEIGHTS;
    report.push(format!(
        "model: elo {elo} / dc {dc} / pi {pi} at preseason; fixture figures are exact \
         means over the {} fixtures, season figures {} x {} simulated seasons (seed {SEED})",
        w.fixtures.len(),
        SEASON_BATCHES,
        SEASONS_PER_BATCH,
    ));
    report.push(format!(
        "{:<24} {:>8} {:>7} {:>8} {:>7} {:>7} {:>6}   {:<16} band",
        "", "model", "MC se", "history", "se", "gap", "z", "pinned"
    ));
    for f in &figures {
        let mc = if f.model_se > 0.0 {
            format!("{:.3}", f.model_se)
        } else {
            "exact".to_string()
        };
        let (pinned, band) = match f.limits() {
            Some(l) => (
                format!("{} ± {}", l.pinned, l.pin_tol),
                format!("[{:+}, {:+}]", l.band.0, l.band.1),
            ),
            None => ("report only".to_string(), String::new()),
        };
        let row = format!(
            "{:<24} {:>8.3} {:>7} {:>8.3} {:>7.3} {:>+7.3} {:>+6.1}   {:<16} {band}",
            f.name,
            f.model,
            mc,
            f.history,
            f.history_se,
            f.gap(),
            f.gap() / f.model_se.hypot(f.history_se),
            pinned,
        );
        report.push(row.trim_end().to_string());
    }

    let season_points = mean(&points);
    let season_points_from_draws = per_club(mean(&draws));
    report.push(format!(
        "season points per club {season_points:.3} = 17 x (3 - {:.4}) = \
         {season_points_from_draws:.3}",
        mean(&draws) / 100.0
    ));
    let report = report.join("\n");
    eprintln!("{report}");
    Measured {
        figures,
        report,
        season_points,
        season_points_from_draws,
    }
}

/// Every pinned name is measured, so a renamed figure cannot drop out of
/// both guards unnoticed.
fn pinned_figures() -> Vec<(&'static Figure, &'static Limits)> {
    let m = measured();
    LIMITS
        .iter()
        .map(|l| {
            let f = m
                .figures
                .iter()
                .find(|f| f.name == l.name)
                .unwrap_or_else(|| panic!("no measured figure is called {:?}", l.name));
            (f, l)
        })
        .collect()
}

#[test]
fn calibration_is_pinned() {
    let moved: Vec<String> = pinned_figures()
        .into_iter()
        .filter(|(f, l)| (f.model - l.pinned).abs() > l.pin_tol)
        .map(|(f, l)| {
            format!(
                "{}: {:.3}, pinned at {} ± {}",
                f.name, f.model, l.pinned, l.pin_tol
            )
        })
        .collect();
    assert!(
        moved.is_empty(),
        "the shipped model's calibration moved:\n  {}\nIf the change is intended, re-measure \
         with `cargo test --test calibration -- --nocapture`, re-pin LIMITS in \
         tests/calibration.rs and update the docs that quote these figures.\n\n{}",
        moved.join("\n  "),
        measured().report
    );
}

#[test]
fn calibration_stays_within_its_band_of_the_history() {
    let outside: Vec<String> = pinned_figures()
        .into_iter()
        .filter(|(f, l)| !(l.band.0..=l.band.1).contains(&f.gap()))
        .map(|(f, l)| {
            format!(
                "{}: model {:.3} vs history {:.3}, gap {:+.3} outside [{:+}, {:+}]",
                f.name,
                f.model,
                f.history,
                f.gap(),
                l.band.0,
                l.band.1
            )
        })
        .collect();
    assert!(
        outside.is_empty(),
        "the shipped model has drifted from the real league:\n  {}\nThe bands are \
         documented in tests/calibration.rs; widening one hides a miscalibration.\n\n{}",
        outside.join("\n  "),
        measured().report
    );
}

#[test]
fn mean_points_follow_from_the_draw_rate() {
    let m = measured();
    assert!(
        (m.season_points - m.season_points_from_draws).abs() < 1e-9,
        "mean points per club {:.6} != 17 * (3 - draw rate) = {:.6}",
        m.season_points,
        m.season_points_from_draws
    );
}

#[test]
fn the_calibrated_blend_is_the_shipped_default() {
    if std::env::var_os("ENSEMBLE_WEIGHTS").is_some() {
        eprintln!("ENSEMBLE_WEIGHTS is set, so the default blend cannot be read; skipped");
        return;
    }
    assert_eq!(
        superlig_sim::ensemble_weights(),
        SHIPPED_WEIGHTS,
        "the default ENSEMBLE_WEIGHTS changed: set SHIPPED_WEIGHTS to it and re-pin LIMITS"
    );
}
