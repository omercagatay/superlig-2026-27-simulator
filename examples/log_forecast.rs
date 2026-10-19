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
//! 34 that no one would have relied on. A refresh that would change nothing
//! but `logged_at` is skipped, and a run with nothing new or refreshed leaves
//! the file untouched, so a rerun with no new results commits nothing.
//! serde_json parses floats exactly (its `float_roundtrip` feature), so an
//! entry kept through a rewrite, frozen or unchanged, keeps every digit.
//!
//! Each new or refreshed entry is labelled with the model that made it
//! (`model_version::current()`); a kept entry keeps its label as it keeps
//! `logged_at`. The label names the shipped model, so the run refuses to
//! log under anything else: other `ENSEMBLE_WEIGHTS`, pure Elo after a
//! failed ensemble load, or inputs that no longer match the label's
//! fingerprint.
//!
//! Run weekly, before each matchday: `cargo run --release --example log_forecast`

use std::collections::HashMap;

use superlig_sim::accuracy::{LoggedForecast, FORECAST_LOG_PATH};
use superlig_sim::model_version;
use superlig_sim::sim::World;

fn main() {
    let label = model_version::current();
    assert_eq!(
        model_version::fingerprint(),
        model_version::FINGERPRINT,
        "model inputs no longer match the label {label:?}; bump it in src/model_version.rs \
         before logging forecasts"
    );
    let weights = superlig_sim::ensemble_weights();
    assert_eq!(
        weights,
        model_version::DEFAULT_ENSEMBLE_WEIGHTS,
        "ENSEMBLE_WEIGHTS sets other weights than the model {label:?} names; \
         unset it to log forecasts"
    );
    let mut world = World::new();
    let (w_elo, w_dc, w_pi) = weights;
    match superlig_sim::sim::Ensemble::from_embedded_data(w_elo, w_dc, w_pi) {
        Ok(e) => world.ensemble = Some(e),
        Err(e) => panic!("the ensemble did not load ({e}); pure Elo is not the model {label:?}"),
    }

    // Only a missing log starts afresh. One that cannot be read or parsed
    // stops the run: treating it as empty would drop every frozen entry.
    let existing: Vec<LoggedForecast> = match std::fs::read_to_string(FORECAST_LOG_PATH) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            panic!("{FORECAST_LOG_PATH} does not parse ({e}); not touching it")
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => panic!("read {FORECAST_LOG_PATH}: {e}"),
    };
    let mut by_key: HashMap<(String, String), LoggedForecast> = existing
        .into_iter()
        .map(|f| ((f.home.clone(), f.away.clone()), f))
        .collect();
    let before = by_key.len();

    let today = chrono::Utc::now().date_naive().to_string();
    let (mut refreshed, mut unchanged) = (0usize, 0usize);
    for (i, f) in world.fixtures.iter().enumerate() {
        if world.played.contains_key(&(f.home, f.away)) {
            continue;
        }
        // Only ever touch fixtures still ahead of us. A match played today
        // whose result has not been scraped yet must not be re-forecast.
        if world.dates[i].date.as_str() <= today.as_str() {
            continue;
        }
        let p = world.fixture_probs(f.home, f.away);
        let fresh = LoggedForecast {
            round: f.round,
            date: world.dates[i].date.clone(),
            home: world.teams[f.home].clone(),
            away: world.teams[f.away].clone(),
            home_pct: logged_pct(p.home_win_pct),
            draw_pct: logged_pct(p.draw_pct),
            away_pct: logged_pct(p.away_win_pct),
            logged_at: today.clone(),
            model_version: Some(label.to_string()),
        };
        match merge(&mut by_key, fresh) {
            Merge::Added => {}
            Merge::Refreshed => refreshed += 1,
            Merge::Unchanged => unchanged += 1,
        }
    }

    let added = by_key.len() - before;
    let counts = format!("{added} new, {refreshed} refreshed, {unchanged} unchanged");
    // Nothing new to say: leave the file exactly as it is. Rewriting it would
    // still re-serialize older full-precision entries a last digit off.
    if added == 0 && refreshed == 0 {
        println!("forecast log: {before} entries ({counts}), {FORECAST_LOG_PATH} left as is");
        return;
    }
    let mut out: Vec<LoggedForecast> = by_key.into_values().collect();
    out.sort_by(|a, b| (a.round, &a.date, &a.home).cmp(&(b.round, &b.date, &b.home)));
    let json = serde_json::to_string_pretty(&out).expect("serialize forecasts");
    std::fs::write(FORECAST_LOG_PATH, json).expect("write forecast log");
    println!(
        "forecast log: {} entries ({counts}, model {label}), written to {FORECAST_LOG_PATH}",
        out.len()
    );
}

/// What `merge` did with a fresh forecast.
#[derive(Debug, PartialEq)]
enum Merge {
    Added,
    Refreshed,
    Unchanged,
}

/// File `fresh` under its fixture: a new entry, a replacement for an entry
/// that said something else, or nothing at all when the logged entry
/// already says it. A kept entry keeps its `logged_at` and its
/// `model_version`: it is still the forecast that model made that day.
fn merge(by_key: &mut HashMap<(String, String), LoggedForecast>, fresh: LoggedForecast) -> Merge {
    let key = (fresh.home.clone(), fresh.away.clone());
    let outcome = match by_key.get(&key) {
        Some(old) if same_forecast(old, &fresh) => return Merge::Unchanged,
        Some(_) => Merge::Refreshed,
        None => Merge::Added,
    };
    by_key.insert(key, fresh);
    outcome
}

/// A percentage as the log stores it: to 12 decimal places, far past any
/// meaningful precision. Comparing at this precision is what lets a
/// recomputed forecast match the entry already logged for it.
fn logged_pct(pct: f64) -> f64 {
    (pct * 1e12).round() / 1e12
}

/// Whether the logged `old` already says what `fresh` does: same round, date
/// and probabilities to the precision the log stores. Such an entry is kept,
/// `logged_at` included; rewriting only that date would make every run
/// commit (and redeploy) with nothing new to say.
fn same_forecast(old: &LoggedForecast, fresh: &LoggedForecast) -> bool {
    let pcts = |f: &LoggedForecast| [f.home_pct, f.draw_pct, f.away_pct].map(logged_pct);
    old.round == fresh.round && old.date == fresh.date && pcts(old) == pcts(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PCTS: [f64; 3] = [42.64993508207272, 26.14833699248445, 31.20172792544283];

    fn entry(pcts: [f64; 3], date: &str, logged_at: &str) -> LoggedForecast {
        LoggedForecast {
            round: 9,
            date: date.to_string(),
            home: "Galatasaray".to_string(),
            away: "Fenerbahçe".to_string(),
            home_pct: pcts[0],
            draw_pct: pcts[1],
            away_pct: pcts[2],
            logged_at: logged_at.to_string(),
            model_version: Some(model_version::current().to_string()),
        }
    }

    /// `entries` as the next run reads them back from the file.
    fn through_log(entries: &[LoggedForecast]) -> Vec<LoggedForecast> {
        serde_json::from_str(&serde_json::to_string_pretty(entries).unwrap()).unwrap()
    }

    #[test]
    fn logged_percentages_read_back_exactly() {
        // A spread of percentages in [0, 100). Unrounded, about one in ten
        // would read back a last bit off and fail this.
        let mut x = 0.123_456_789_f64;
        for _ in 0..30_000 {
            x = (x * 7.3 + 0.618).fract();
            let pct = logged_pct(100.0 * x);
            let back: f64 = serde_json::from_str(&serde_json::to_string(&pct).unwrap()).unwrap();
            assert_eq!(back, pct);
        }
    }

    #[test]
    fn the_shipped_log_rewrites_byte_for_byte() {
        // Frozen entries are the audit trail: reading the log and writing it
        // back must not move a digit, full-precision entries from before
        // `logged_pct` included.
        let text = include_str!("../data/forecast_log.json");
        let entries: Vec<LoggedForecast> = serde_json::from_str(text).unwrap();
        assert_eq!(serde_json::to_string_pretty(&entries).unwrap(), text);
    }

    #[test]
    fn a_forecast_that_has_not_moved_keeps_its_entry() {
        let fresh = entry(PCTS.map(logged_pct), "2026-10-24", "2026-10-16");
        // As an earlier run logged it, and as one did before the rounding.
        for pcts in [PCTS.map(logged_pct), PCTS] {
            let old = &through_log(&[entry(pcts, "2026-10-24", "2026-10-15")])[0];
            assert!(same_forecast(old, &fresh), "{pcts:?}");
        }
    }

    #[test]
    fn a_moved_probability_or_date_is_a_refresh() {
        let old = &through_log(&[entry(PCTS.map(logged_pct), "2026-10-24", "2026-10-15")])[0];
        let [h, d, a] = PCTS;
        let moved = entry(
            [h + 1e-9, d, a - 1e-9].map(logged_pct),
            "2026-10-24",
            "2026-10-16",
        );
        let redated = entry(PCTS.map(logged_pct), "2026-10-25", "2026-10-16");
        assert!(!same_forecast(old, &moved));
        assert!(!same_forecast(old, &redated));
    }

    /// A refreshed forecast is filed under the model that made it; one that
    /// has not moved stays the older model's, logged on its own day.
    #[test]
    fn a_kept_entry_keeps_its_model_and_a_refresh_takes_the_current_one() {
        let mut old = entry(PCTS.map(logged_pct), "2026-10-24", "2026-10-16");
        old.model_version = Some("2026-10a".to_string());
        let key = (old.home.clone(), old.away.clone());
        let mut by_key = HashMap::from([(key.clone(), through_log(&[old])[0].clone())]);

        let same = entry(PCTS.map(logged_pct), "2026-10-24", "2026-10-22");
        assert_eq!(merge(&mut by_key, same), Merge::Unchanged);
        assert_eq!(by_key[&key].model_version.as_deref(), Some("2026-10a"));
        assert_eq!(by_key[&key].logged_at, "2026-10-16");

        let [h, d, a] = PCTS;
        let moved = entry(
            [h + 0.5, d, a - 0.5].map(logged_pct),
            "2026-10-24",
            "2026-10-22",
        );
        assert_eq!(merge(&mut by_key, moved), Merge::Refreshed);
        assert_eq!(
            by_key[&key].model_version.as_deref(),
            Some(model_version::current())
        );
        assert_eq!(by_key[&key].logged_at, "2026-10-22");

        let mut other = entry(PCTS.map(logged_pct), "2026-10-25", "2026-10-22");
        other.home = "Beşiktaş".to_string();
        assert_eq!(merge(&mut by_key, other), Merge::Added);
        assert_eq!(by_key.len(), 2);
    }
}
