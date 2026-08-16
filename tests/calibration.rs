//! Calibration guard: the simulator's scoring and outcome rates must match
//! the real Süper Lig. These bounds are deliberately loose enough to survive
//! ordinary model changes and tight enough to catch a mis-scaled Elo term.

use superlig_sim::history;
use superlig_sim::sim::{SimConfig, World};

/// Empirical home/away goal means and outcome split over the historical set.
fn empirical() -> (f64, f64, f64, f64) {
    let ms = history::load_history();
    assert!(!ms.is_empty());
    let n = ms.len() as f64;
    let hg: f64 = ms.iter().map(|m| m.home_score as f64).sum::<f64>() / n;
    let ag: f64 = ms.iter().map(|m| m.away_score as f64).sum::<f64>() / n;
    let home_wins = ms.iter().filter(|m| m.home_score > m.away_score).count() as f64 / n;
    let draws = ms.iter().filter(|m| m.home_score == m.away_score).count() as f64 / n;
    (hg, ag, home_wins * 100.0, draws * 100.0)
}

#[test]
fn simulated_scoring_matches_the_real_league() {
    let (emp_hg, emp_ag, emp_home, emp_draw) = empirical();
    eprintln!(
        "empirical: home {emp_hg:.3} away {emp_ag:.3} goals; \
         home wins {emp_home:.1}% draws {emp_draw:.1}%"
    );

    // A clean World has no ensemble (pure Elo), which is exactly the
    // component being calibrated here.
    let w = World::new();
    let cfg = SimConfig {
        n_sims: 2000,
        seed: 4242,
        ..SimConfig::default()
    };
    let r = w.simulate(&cfg);

    let n_teams = w.teams.len() as f64;
    let total_goals: f64 = r.gd_sum.iter().map(|g| g.abs()).sum::<f64>();
    let mean_points = r.points_sum.iter().sum::<f64>() / cfg.n_sims as f64 / n_teams;
    eprintln!("simulated mean points per club: {mean_points:.2} (gd spread {total_goals:.0})");

    // Mean points per club over a 34-game season is fixed by the draw rate:
    // 34 * (3 - draw_rate). A draw rate of 22-30% implies 44.9-47.6 points.
    assert!(
        (44.0..=48.5).contains(&mean_points),
        "mean points per club {mean_points:.2} implies an implausible draw rate; \
         retune BASE/D_DIV/HOME_ADV in src/data.rs"
    );
}

#[test]
fn simulated_match_outcomes_match_the_real_league() {
    let (_, _, emp_home, emp_draw) = empirical();
    let w = World::new();

    // Average the exact outcome split over every fixture in the calendar, so
    // the comparison covers the real distribution of mismatches.
    let (mut home, mut draw, mut away, mut n) = (0.0, 0.0, 0.0, 0.0);
    for f in &w.fixtures {
        let (h, d, a) = w.match_win_probs(f.home, f.away);
        home += h;
        draw += d;
        away += a;
        n += 1.0;
    }
    let (home, draw, away) = (home / n, draw / n, away / n);
    eprintln!("simulated: home {home:.1}% draw {draw:.1}% away {away:.1}%");

    assert!(
        (home - emp_home).abs() < 7.0,
        "home win rate {home:.1}% vs empirical {emp_home:.1}% — adjust HOME_ADV"
    );
    assert!(
        (draw - emp_draw).abs() < 7.0,
        "draw rate {draw:.1}% vs empirical {emp_draw:.1}% — adjust BASE/D_DIV"
    );
    assert!((home + draw + away - 100.0).abs() < 1e-6);
}
