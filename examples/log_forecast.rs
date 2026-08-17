//! Freeze the current model probabilities for every unplayed fixture.
//!
//! A forecast is only testable if it was written down before the match, so
//! this writes today's predictions to `data/forecast_log.json`, which is
//! committed — the git history is the audit trail.
//!
//! An entry is refreshed only while its fixture is still in the future and
//! unplayed. Once a match is played (or its date has arrived), its prediction
//! is frozen forever. That gives the honest thing to score: the model's most
//! recent view *before* kick-off, rather than a preseason guess about matchday
//! 34 that no one would have relied on.
//!
//! Run weekly, before each matchday: `cargo run --release --example log_forecast`

use std::collections::HashMap;

use superlig_sim::accuracy::{LoggedForecast, FORECAST_LOG_PATH};
use superlig_sim::sim::World;

fn main() {
    let mut world = World::new();
    let (w_elo, w_dc, w_pi) = superlig_sim::ensemble_weights();
    match superlig_sim::sim::Ensemble::from_embedded_data(w_elo, w_dc, w_pi) {
        Ok(e) => world.ensemble = Some(e),
        Err(e) => eprintln!("warning: running on pure Elo ({e})"),
    }

    let existing: Vec<LoggedForecast> = std::fs::read_to_string(FORECAST_LOG_PATH)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut by_key: HashMap<(String, String), LoggedForecast> = existing
        .into_iter()
        .map(|f| ((f.home.clone(), f.away.clone()), f))
        .collect();
    let before = by_key.len();

    let today = chrono::Utc::now().date_naive().to_string();
    let mut refreshed = 0usize;
    for (i, f) in world.fixtures.iter().enumerate() {
        if world.played.contains_key(&(f.home, f.away)) {
            continue;
        }
        // Only ever touch fixtures still ahead of us. A match played today
        // whose result has not been scraped yet must not be re-forecast.
        if world.dates[i].date.as_str() <= today.as_str() {
            continue;
        }
        let key = (world.teams[f.home].clone(), world.teams[f.away].clone());
        if by_key.contains_key(&key) {
            refreshed += 1;
        }
        let p = world.fixture_probs(f.home, f.away);
        by_key.insert(
            key.clone(),
            LoggedForecast {
                round: f.round,
                date: world.dates[i].date.clone(),
                home: key.0,
                away: key.1,
                home_pct: p.home_win_pct,
                draw_pct: p.draw_pct,
                away_pct: p.away_win_pct,
                logged_at: today.clone(),
            },
        );
    }

    let mut out: Vec<LoggedForecast> = by_key.into_values().collect();
    out.sort_by(|a, b| (a.round, &a.date, &a.home).cmp(&(b.round, &b.date, &b.home)));
    let json = serde_json::to_string_pretty(&out).expect("serialize forecasts");
    std::fs::write(FORECAST_LOG_PATH, json).expect("write forecast log");
    println!(
        "forecast log: {} entries ({} new, {} refreshed), written to {}",
        out.len(),
        out.len() - before,
        refreshed,
        FORECAST_LOG_PATH
    );
}
