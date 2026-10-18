//! Scoring the model against its own frozen predictions.
//!
//! `examples/log_forecast.rs` writes a prediction per fixture *before* it is
//! played and never revises it; this module joins those against real results
//! and reports how the model actually did. A forecast nobody scores is a
//! forecast nobody can trust.

use serde::{Deserialize, Serialize};

pub const FORECAST_LOG_PATH: &str = "data/forecast_log.json";

/// One prediction, frozen at the moment it was logged.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoggedForecast {
    pub round: u8,
    pub date: String,
    pub home: String,
    pub away: String,
    pub home_pct: f64,
    pub draw_pct: f64,
    pub away_pct: f64,
    /// The day this prediction was written down.
    pub logged_at: String,
}

/// A scored prediction: what was said, and what happened.
#[derive(Clone, Debug, Serialize)]
pub struct ScoredMatch {
    pub round: u8,
    pub date: String,
    pub home: String,
    pub away: String,
    pub home_score: u16,
    pub away_score: u16,
    /// Probability the model gave the outcome that actually occurred.
    pub outcome_pct: f64,
    /// "1", "X" or "2".
    pub outcome: String,
    /// Whether the model's single most likely outcome was the right one.
    pub hit: bool,
    /// The frozen forecast, exactly as logged.
    pub home_pct: f64,
    pub draw_pct: f64,
    pub away_pct: f64,
    /// The day the forecast was written down.
    pub logged_at: String,
    /// This match's share of the aggregate: -ln p of the true outcome, under
    /// the same clamp. `AccuracyReport::log_loss` is the mean of these.
    pub log_loss: f64,
    /// The base-rate forecast's -ln p for the same outcome.
    pub baseline_log_loss: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccuracyReport {
    pub scored: usize,
    /// Share of matches where the likeliest predicted outcome happened.
    pub hit_rate_pct: f64,
    /// Mean negative log probability of the true outcome; lower is better.
    pub log_loss: f64,
    /// The same score for a naive "always the league's base rates" forecast.
    /// The model earns its keep only by beating this.
    pub baseline_log_loss: f64,
    /// Mean predicted probability vs realized frequency, in confidence bands
    /// — the honest way to spot over- or under-confidence.
    pub calibration: Vec<CalibrationBucket>,
    pub matches: Vec<ScoredMatch>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CalibrationBucket {
    /// Inclusive lower bound of the predicted-probability band, in percent.
    pub band_from_pct: f64,
    pub band_to_pct: f64,
    pub predictions: usize,
    pub mean_predicted_pct: f64,
    pub actual_pct: f64,
}

/// Historical Süper Lig outcome split, used as the "did the model beat doing
/// nothing?" reference. Matches the empirical rates in `tests/calibration.rs`.
const BASE_RATES: [f64; 3] = [0.454, 0.256, 0.290];

fn load_log() -> Vec<LoggedForecast> {
    serde_json::from_str(include_str!("../data/forecast_log.json")).unwrap_or_default()
}

/// Score every logged forecast whose fixture has since been played.
pub fn report(world: &crate::sim::World) -> AccuracyReport {
    report_with(world, load_log())
}

/// As `report`, against a supplied forecast log — the seam the tests use, so
/// the scoring maths is exercised even when the shipped log has nothing
/// scoreable in it yet.
pub fn report_with(world: &crate::sim::World, log: Vec<LoggedForecast>) -> AccuracyReport {
    let mut matches = Vec::new();
    let mut log_loss = 0.0;
    let mut baseline = 0.0;
    let mut hits = 0usize;
    // (predicted, hit) pairs for every outcome the model priced, for calibration.
    let mut points: Vec<(f64, bool)> = Vec::new();

    for f in log {
        let (Some(&h), Some(&a)) = (world.idx.get(&f.home), world.idx.get(&f.away)) else {
            continue;
        };
        let Some(&(hs, as_)) = world.played.get(&(h, a)) else {
            continue;
        };
        let idx = match hs.cmp(&as_) {
            std::cmp::Ordering::Greater => 0,
            std::cmp::Ordering::Equal => 1,
            std::cmp::Ordering::Less => 2,
        };
        let probs = [f.home_pct, f.draw_pct, f.away_pct];
        let p = (probs[idx] / 100.0).clamp(1e-6, 1.0);
        let match_loss = -p.ln();
        let match_baseline = -BASE_RATES[idx].ln();
        log_loss += match_loss;
        baseline += match_baseline;

        let best = probs
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.partial_cmp(y.1).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        if best == idx {
            hits += 1;
        }
        for (i, &pct) in probs.iter().enumerate() {
            points.push((pct, i == idx));
        }

        matches.push(ScoredMatch {
            round: f.round,
            date: f.date,
            home: f.home,
            away: f.away,
            home_score: hs,
            away_score: as_,
            outcome_pct: probs[idx],
            outcome: ["1", "X", "2"][idx].to_string(),
            hit: best == idx,
            home_pct: f.home_pct,
            draw_pct: f.draw_pct,
            away_pct: f.away_pct,
            logged_at: f.logged_at,
            log_loss: match_loss,
            baseline_log_loss: match_baseline,
        });
    }

    let n = matches.len();
    matches.sort_by(|a, b| (b.round, &b.date).cmp(&(a.round, &a.date)));
    AccuracyReport {
        scored: n,
        hit_rate_pct: if n > 0 {
            hits as f64 / n as f64 * 100.0
        } else {
            0.0
        },
        log_loss: if n > 0 { log_loss / n as f64 } else { 0.0 },
        baseline_log_loss: if n > 0 { baseline / n as f64 } else { 0.0 },
        calibration: calibrate(&points),
        matches,
    }
}

/// Bucket predictions into 20-point confidence bands and compare the mean
/// prediction against what actually happened in each.
fn calibrate(points: &[(f64, bool)]) -> Vec<CalibrationBucket> {
    let mut out = Vec::new();
    for band in 0..5 {
        let (from, to) = (band as f64 * 20.0, (band + 1) as f64 * 20.0);
        let in_band: Vec<&(f64, bool)> = points
            .iter()
            .filter(|(p, _)| *p >= from && (*p < to || (band == 4 && *p <= 100.0)))
            .collect();
        if in_band.is_empty() {
            continue;
        }
        let n = in_band.len() as f64;
        out.push(CalibrationBucket {
            band_from_pct: from,
            band_to_pct: to,
            predictions: in_band.len(),
            mean_predicted_pct: in_band.iter().map(|(p, _)| p).sum::<f64>() / n,
            actual_pct: in_band.iter().filter(|(_, hit)| *hit).count() as f64 / n * 100.0,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::World;

    #[test]
    fn report_scores_only_played_fixtures_and_stays_consistent() {
        let w = World::new();
        let r = report(&w);
        assert_eq!(r.scored, r.matches.len());
        for m in &r.matches {
            // Every scored match must be one the world knows the result of.
            let (h, a) = (w.idx[&m.home], w.idx[&m.away]);
            assert_eq!(w.played.get(&(h, a)), Some(&(m.home_score, m.away_score)));
            assert!(m.outcome_pct > 0.0 && m.outcome_pct <= 100.0);
        }
        if r.scored > 0 {
            assert!(r.log_loss > 0.0 && r.log_loss.is_finite());
            assert!((0.0..=100.0).contains(&r.hit_rate_pct));
        }
        for b in &r.calibration {
            assert!(b.predictions > 0);
            assert!((0.0..=100.0).contains(&b.actual_pct));
        }
    }

    /// The scoring maths itself: a confident correct call must score better
    /// than a confident wrong one, and better than the base-rate baseline.
    #[test]
    fn scoring_rewards_being_right_and_punishes_being_wrong() {
        // Nothing recorded but the two results below, whatever the shipped
        // calendar has reached.
        let mut w = World::with_played(0);
        let (gs, gaz) = (w.idx["Galatasaray"], w.idx["Gaziantep"]);
        let (fb, kon) = (w.idx["Fenerbahçe"], w.idx["Konyaspor"]);
        w.played.insert((gs, gaz), (3, 0)); // home win
        w.played.insert((fb, kon), (0, 2)); // away win

        let entry = |home: &str, away: &str, h: f64, d: f64, a: f64| LoggedForecast {
            round: 1,
            date: "2026-08-22".to_string(),
            home: home.to_string(),
            away: away.to_string(),
            home_pct: h,
            draw_pct: d,
            away_pct: a,
            logged_at: "2026-08-20".to_string(),
        };

        // Both calls confidently right.
        let sharp = report_with(
            &w,
            vec![
                entry("Galatasaray", "Gaziantep", 80.0, 12.0, 8.0),
                entry("Fenerbahçe", "Konyaspor", 10.0, 15.0, 75.0),
            ],
        );
        assert_eq!(sharp.scored, 2);
        assert_eq!(sharp.hit_rate_pct, 100.0);
        assert!(
            sharp.log_loss < sharp.baseline_log_loss,
            "must beat base rates"
        );

        // The same matches, called confidently wrong.
        let wrong = report_with(
            &w,
            vec![
                entry("Galatasaray", "Gaziantep", 8.0, 12.0, 80.0),
                entry("Fenerbahçe", "Konyaspor", 75.0, 15.0, 10.0),
            ],
        );
        assert_eq!(wrong.hit_rate_pct, 0.0);
        assert!(
            wrong.log_loss > wrong.baseline_log_loss,
            "must lose to base rates"
        );
        assert!(wrong.log_loss > sharp.log_loss);

        // Unplayed and unknown fixtures are simply not scored.
        let partial = report_with(
            &w,
            vec![
                entry("Galatasaray", "Gaziantep", 80.0, 12.0, 8.0),
                entry("Beşiktaş", "Trabzonspor", 40.0, 30.0, 30.0),
                entry("Atlantis FC", "Galatasaray", 40.0, 30.0, 30.0),
            ],
        );
        assert_eq!(partial.scored, 1);
    }

    /// The forecast history shows each match's own score next to its frozen
    /// forecast. The headline log-loss must be exactly the mean of those rows,
    /// and every row must carry the forecast as it was logged.
    #[test]
    fn per_match_scores_are_the_aggregate_broken_down() {
        let w = World::with_played(crate::sim::MID_SEASON);
        let mut log: Vec<LoggedForecast> = w
            .fixtures
            .iter()
            .enumerate()
            .map(|(k, f)| {
                let home_pct = 20.0 + (k * 7 % 50) as f64;
                let draw_pct = 15.0 + (k * 3 % 20) as f64;
                LoggedForecast {
                    round: f.round,
                    date: w.dates[k].date.clone(),
                    home: w.teams[f.home].clone(),
                    away: w.teams[f.away].clone(),
                    home_pct,
                    draw_pct,
                    away_pct: 100.0 - home_pct - draw_pct,
                    logged_at: format!("2026-08-{:02}", 1 + k % 28),
                }
            })
            .collect();
        let outcome_of = |f: &LoggedForecast| {
            let (hs, as_) = w.played[&(w.idx[&f.home], w.idx[&f.away])];
            match hs.cmp(&as_) {
                std::cmp::Ordering::Greater => 0,
                std::cmp::Ordering::Equal => 1,
                std::cmp::Ordering::Less => 2,
            }
        };
        // One played match called as impossible, so the clamp is exercised.
        let k = (0..log.len())
            .find(|&k| {
                w.played
                    .contains_key(&(w.fixtures[k].home, w.fixtures[k].away))
            })
            .expect("a played fixture");
        let mut probs = [50.0; 3];
        probs[outcome_of(&log[k])] = 0.0;
        log[k].home_pct = probs[0];
        log[k].draw_pct = probs[1];
        log[k].away_pct = probs[2];
        let impossible = (log[k].home.clone(), log[k].away.clone());

        let r = report_with(&w, log.clone());
        assert_eq!(r.scored, crate::sim::MID_SEASON);
        assert_eq!(r.matches.len(), r.scored);

        let n = r.matches.len() as f64;
        let mean = r.matches.iter().map(|m| m.log_loss).sum::<f64>() / n;
        let mean_baseline = r.matches.iter().map(|m| m.baseline_log_loss).sum::<f64>() / n;
        assert!((r.log_loss - mean).abs() < 1e-9, "{} vs {mean}", r.log_loss);
        assert!(
            (r.baseline_log_loss - mean_baseline).abs() < 1e-9,
            "{} vs {mean_baseline}",
            r.baseline_log_loss
        );

        for m in &r.matches {
            let f = log
                .iter()
                .find(|f| f.home == m.home && f.away == m.away)
                .expect("every scored match comes from the log");
            assert_eq!(
                (m.home_pct, m.draw_pct, m.away_pct),
                (f.home_pct, f.draw_pct, f.away_pct)
            );
            assert_eq!(m.logged_at, f.logged_at);
            assert_eq!(m.round, f.round);
            assert_eq!(m.date, f.date);
            let idx = outcome_of(f);
            assert_eq!(m.outcome, ["1", "X", "2"][idx]);
            assert_eq!(m.outcome_pct, [m.home_pct, m.draw_pct, m.away_pct][idx]);
            let expected = -(m.outcome_pct / 100.0).clamp(1e-6, 1.0).ln();
            assert_eq!(m.log_loss, expected);
            assert_eq!(m.baseline_log_loss, -BASE_RATES[idx].ln());
            assert!(m.log_loss.is_finite() && m.log_loss >= 0.0);
        }
        let clamped = r
            .matches
            .iter()
            .find(|m| (m.home.clone(), m.away.clone()) == impossible)
            .expect("the impossible call was scored");
        assert_eq!(clamped.outcome_pct, 0.0);
        assert_eq!(clamped.log_loss, -(1e-6f64).ln());

        // Newest matchday first, as the history lists them.
        assert!(r.matches.windows(2).all(|p| p[0].round >= p[1].round));
        assert!(r.matches.first().unwrap().round > r.matches.last().unwrap().round);
    }

    /// Calibration bands must report what was said against what happened.
    #[test]
    fn calibration_buckets_pair_claims_with_outcomes() {
        let mut w = World::with_played(0);
        let (gs, gaz) = (w.idx["Galatasaray"], w.idx["Gaziantep"]);
        w.played.insert((gs, gaz), (3, 0));
        let r = report_with(
            &w,
            vec![LoggedForecast {
                round: 1,
                date: "2026-08-22".to_string(),
                home: "Galatasaray".to_string(),
                away: "Gaziantep".to_string(),
                home_pct: 90.0,
                draw_pct: 6.0,
                away_pct: 4.0,
                logged_at: "2026-08-20".to_string(),
            }],
        );
        // Three outcomes priced: one in the 80-100 band that happened, two in
        // the 0-20 band that did not.
        let top = r
            .calibration
            .iter()
            .find(|b| b.band_from_pct == 80.0)
            .expect("a 90% claim lands in the top band");
        assert_eq!(top.predictions, 1);
        assert_eq!(top.actual_pct, 100.0);
        let low = r
            .calibration
            .iter()
            .find(|b| b.band_from_pct == 0.0)
            .expect("the two long shots land in the bottom band");
        assert_eq!(low.predictions, 2);
        assert_eq!(low.actual_pct, 0.0);
    }

    /// The log is only meaningful if predictions predate results — a forecast
    /// written for an already-played match would flatter the model.
    #[test]
    fn logged_forecasts_are_well_formed() {
        for f in load_log() {
            let sum = f.home_pct + f.draw_pct + f.away_pct;
            assert!((sum - 100.0).abs() < 1e-6, "{} v {}: {sum}", f.home, f.away);
            assert_eq!(f.date.len(), 10);
            assert_eq!(f.logged_at.len(), 10);
            assert!((1..=crate::data::N_ROUNDS as u8).contains(&f.round));
        }
    }
}
