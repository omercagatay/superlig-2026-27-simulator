//! Scoring the model against its own frozen predictions.
//!
//! `examples/log_forecast.rs` writes a prediction per fixture *before* it is
//! played and never revises it; this module joins those against real results
//! and reports how the model actually did. A forecast nobody scores is a
//! forecast nobody can trust.
//!
//! Each forecast carries the label of the model that made it
//! (`model_version`), and the report scores every label on its own as well
//! as pooled: a log written across model changes is several track records,
//! not one.

use std::collections::BTreeMap;
use std::sync::OnceLock;

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
    /// The model that made it (`model_version::LABEL` when it was logged).
    /// Absent only in logs written before forecasts were labelled; such an
    /// entry still parses, and still writes back byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
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
    /// The model that made the forecast; `None` for an unlabelled entry.
    pub model_version: Option<String>,
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
    /// `log_loss - baseline_log_loss` with its uncertainty.
    pub vs_baseline: VsBaseline,
    /// The label of the running model (`model_version::current()`), so a
    /// reader can tell its record from its predecessors'.
    pub current_model_version: String,
    /// The same scores for each model's forecasts alone: the running model
    /// first, then the older ones newest first, unlabelled entries last.
    /// Only models with a scored forecast appear.
    pub by_model: Vec<ModelScore>,
    /// Mean predicted probability vs realized frequency, in confidence bands
    /// — the honest way to spot over- or under-confidence.
    pub calibration: Vec<CalibrationBucket>,
    pub matches: Vec<ScoredMatch>,
}

/// The model's log-loss minus the base rates' for each of `n` matches,
/// summarized: the mean difference (below zero, the model did better) and
/// a 95% interval for it.
#[derive(Clone, Debug, Serialize)]
pub struct VsBaseline {
    pub n: usize,
    pub mean: f64,
    /// Sample standard deviation of the per-match differences over √n.
    /// `None` below two matches.
    pub std_error: Option<f64>,
    /// `[low, high]`: `mean` ± 1.96 standard errors. A normal approximation
    /// that treats matches as independent; an interval that straddles zero
    /// means the matches so far cannot tell the model from the base rates.
    pub ci95: Option<[f64; 2]>,
}

/// One model's scored forecasts, summarized as the pooled figures are.
#[derive(Clone, Debug, Serialize)]
pub struct ModelScore {
    pub model_version: Option<String>,
    pub scored: usize,
    pub hit_rate_pct: f64,
    pub log_loss: f64,
    pub baseline_log_loss: f64,
    pub vs_baseline: VsBaseline,
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

/// z for a two-sided 95% normal interval.
const Z95: f64 = 1.959_963_984_540_054;

/// The "did the model beat doing nothing?" reference: the league's home,
/// draw and away shares over every played match in the embedded history
/// before this season's first fixture. `history::load_history` has already
/// dropped the unplayed awards. Computed from the history rather than typed
/// in, so it cannot drift from it.
fn base_rates() -> [f64; 3] {
    static RATES: OnceLock<[f64; 3]> = OnceLock::new();
    *RATES.get_or_init(|| outcome_split(&crate::history::load_history(), season_start()))
}

/// The date of this season's first fixture.
fn season_start() -> chrono::NaiveDate {
    crate::data::fixtures()
        .iter()
        .filter_map(|f| chrono::NaiveDate::parse_from_str(&f.date, "%Y-%m-%d").ok())
        .min()
        .expect("the calendar has dated fixtures")
}

/// Home-win, draw and away-win shares of the matches before `before`.
fn outcome_split(
    history: &[crate::history::HistoricalMatch],
    before: chrono::NaiveDate,
) -> [f64; 3] {
    let mut counts = [0usize; 3];
    for m in history.iter().filter(|m| m.date < before) {
        counts[outcome_index(m.home_score, m.away_score)] += 1;
    }
    let n: usize = counts.iter().sum();
    assert!(n > 0, "no history before {before} to take base rates from");
    counts.map(|c| c as f64 / n as f64)
}

/// 0 for a home win, 1 for a draw, 2 for an away win.
fn outcome_index(home_goals: u16, away_goals: u16) -> usize {
    match home_goals.cmp(&away_goals) {
        std::cmp::Ordering::Greater => 0,
        std::cmp::Ordering::Equal => 1,
        std::cmp::Ordering::Less => 2,
    }
}

/// The per-match differences `diffs` (model minus base rates) summarized.
fn vs_baseline(diffs: &[f64]) -> VsBaseline {
    let n = diffs.len();
    let mean = if n > 0 {
        diffs.iter().sum::<f64>() / n as f64
    } else {
        0.0
    };
    let std_error = (n >= 2).then(|| {
        let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
        (var / n as f64).sqrt()
    });
    VsBaseline {
        n,
        mean,
        std_error,
        ci95: std_error.map(|se| [mean - Z95 * se, mean + Z95 * se]),
    }
}

/// Scored matches of one group, summed as they come.
#[derive(Default)]
struct Tally {
    hits: usize,
    loss: f64,
    baseline: f64,
    diffs: Vec<f64>,
}

impl Tally {
    fn add(&mut self, m: &ScoredMatch) {
        self.hits += usize::from(m.hit);
        self.loss += m.log_loss;
        self.baseline += m.baseline_log_loss;
        self.diffs.push(m.log_loss - m.baseline_log_loss);
    }

    fn n(&self) -> usize {
        self.diffs.len()
    }

    /// `total` over the matches tallied; 0 with none.
    fn mean(&self, total: f64) -> f64 {
        if self.n() > 0 {
            total / self.n() as f64
        } else {
            0.0
        }
    }
}

/// The shipped log. It is embedded at build time, so a log that does not
/// parse is a broken build, not an empty history: failing here keeps
/// `/api/accuracy` from quietly reporting nothing scored.
/// `logged_forecasts_are_well_formed` catches it before it ships.
fn load_log() -> Vec<LoggedForecast> {
    serde_json::from_str(include_str!("../data/forecast_log.json")).expect(
        "data/forecast_log.json is embedded at build time and must parse; \
         restore it from git rather than shipping an empty accuracy report",
    )
}

/// Score every logged forecast whose fixture has since been played.
pub fn report(world: &crate::sim::World) -> AccuracyReport {
    report_with(world, load_log())
}

/// As `report`, against a supplied forecast log — the seam the tests use, so
/// the scoring maths is exercised even when the shipped log has nothing
/// scoreable in it yet.
pub fn report_with(world: &crate::sim::World, log: Vec<LoggedForecast>) -> AccuracyReport {
    let base = base_rates();
    let mut matches = Vec::new();
    // (predicted, hit) pairs for every outcome the model priced, for calibration.
    let mut points: Vec<(f64, bool)> = Vec::new();

    for f in log {
        let (Some(&h), Some(&a)) = (world.idx.get(&f.home), world.idx.get(&f.away)) else {
            continue;
        };
        let Some(&(hs, as_)) = world.played.get(&(h, a)) else {
            continue;
        };
        let idx = outcome_index(hs, as_);
        let probs = [f.home_pct, f.draw_pct, f.away_pct];
        let p = (probs[idx] / 100.0).clamp(1e-6, 1.0);

        let best = probs
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.partial_cmp(y.1).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
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
            model_version: f.model_version,
            log_loss: -p.ln(),
            baseline_log_loss: -base[idx].ln(),
        });
    }

    let mut all = Tally::default();
    let mut per_model: BTreeMap<Option<String>, Tally> = BTreeMap::new();
    for m in &matches {
        all.add(m);
        per_model.entry(m.model_version.clone()).or_default().add(m);
    }
    let current = crate::model_version::current();
    let mut by_model: Vec<ModelScore> = per_model
        .into_iter()
        .map(|(model_version, t)| ModelScore {
            model_version,
            scored: t.n(),
            hit_rate_pct: t.mean(t.hits as f64) * 100.0,
            log_loss: t.mean(t.loss),
            baseline_log_loss: t.mean(t.baseline),
            vs_baseline: vs_baseline(&t.diffs),
        })
        .collect();
    // The running model, then the other labels newest first (labels sort in
    // the order they shipped), then unlabelled entries.
    by_model.sort_by(|a, b| {
        let rank = |s: &ModelScore| {
            let v = s.model_version.as_deref();
            (v != Some(current), v.is_none())
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| b.model_version.cmp(&a.model_version))
    });

    matches.sort_by(|a, b| (b.round, &b.date).cmp(&(a.round, &a.date)));
    AccuracyReport {
        scored: all.n(),
        hit_rate_pct: all.mean(all.hits as f64) * 100.0,
        log_loss: all.mean(all.loss),
        baseline_log_loss: all.mean(all.baseline),
        vs_baseline: vs_baseline(&all.diffs),
        current_model_version: current.to_string(),
        by_model,
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
            model_version: Some("2026-08".to_string()),
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
                    model_version: Some(["2026-08", "2026-10a"][k % 2].to_string()),
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
            assert_eq!(m.baseline_log_loss, -base_rates()[idx].ln());
            assert_eq!(m.model_version, f.model_version);
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
                model_version: None,
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

    /// The shipped log must parse and hold entries: it has had predictions
    /// since before the first matchday, so an empty one is a lost log, not an
    /// early season. Parsed here with an explicit unwrap rather than through
    /// `load_log`, so a failure names the JSON error. The log is only
    /// meaningful if predictions predate results — a forecast written after
    /// its match date would flatter the model.
    #[test]
    fn logged_forecasts_are_well_formed() {
        let log: Vec<LoggedForecast> =
            serde_json::from_str(include_str!("../data/forecast_log.json")).unwrap();
        assert!(!log.is_empty(), "data/forecast_log.json has no entries");
        assert_eq!(load_log().len(), log.len());
        for f in &log {
            let sum = f.home_pct + f.draw_pct + f.away_pct;
            assert!((sum - 100.0).abs() < 1e-6, "{} v {}: {sum}", f.home, f.away);
            assert_eq!(f.date.len(), 10);
            assert_eq!(f.logged_at.len(), 10);
            assert!(
                f.logged_at <= f.date,
                "{} v {} logged {} for {}",
                f.home,
                f.away,
                f.logged_at,
                f.date
            );
            assert!((1..=crate::data::N_ROUNDS as u8).contains(&f.round));
        }
    }

    /// Every entry names a known model, and the labels backfilled when
    /// labels were introduced follow the weekly runs that wrote them: the
    /// August runs on the August model, the 2026-10-12 and 2026-10-16 runs on
    /// the October tuning before the history was corrected. Later entries
    /// were written by a labelling run, so they name the model labels came
    /// in with or a newer one, and labels never go backwards in time.
    #[test]
    fn every_logged_forecast_names_the_model_that_made_it() {
        use crate::model_version::LABELS;
        let log = load_log();
        let mut by_day: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for f in &log {
            let at = format!("{} v {} logged {}", f.home, f.away, f.logged_at);
            let v = f
                .model_version
                .as_deref()
                .unwrap_or_else(|| panic!("{at}: no model_version"));
            let rank = LABELS
                .iter()
                .position(|l| *l == v)
                .unwrap_or_else(|| panic!("{at}: unknown model_version {v:?}"));
            let expected = match f.logged_at.as_str() {
                "2026-08-17" | "2026-08-28" => Some("2026-08"),
                "2026-10-12" | "2026-10-16" => Some("2026-10a"),
                day => {
                    assert!(day >= "2026-10-19", "{at}: no weekly run logged that day");
                    // Labels sort in the order they shipped.
                    assert!(
                        v >= "2026-10b",
                        "{at}: written since labels came in, as {v:?}"
                    );
                    None
                }
            };
            if let Some(expected) = expected {
                assert_eq!(v, expected, "{at}");
            }
            let span = by_day.entry(f.logged_at.as_str()).or_insert((rank, rank));
            span.0 = span.0.min(rank);
            span.1 = span.1.max(rank);
        }
        let days: Vec<_> = by_day.into_iter().collect();
        for pair in days.windows(2) {
            let ((d0, (_, newest)), (d1, (oldest, _))) = (pair[0], pair[1]);
            assert!(
                newest <= oldest,
                "{d1} logged {} after {d0} logged {}",
                LABELS[oldest],
                LABELS[newest]
            );
        }
    }

    fn forecast(home: &str, away: &str, pcts: [f64; 3], version: Option<&str>) -> LoggedForecast {
        LoggedForecast {
            round: 1,
            date: "2026-08-22".to_string(),
            home: home.to_string(),
            away: away.to_string(),
            home_pct: pcts[0],
            draw_pct: pcts[1],
            away_pct: pcts[2],
            logged_at: "2026-08-20".to_string(),
            model_version: version.map(str::to_string),
        }
    }

    /// Each model's forecasts are scored on their own, the running model
    /// first, and the pooled figures are all of them together.
    #[test]
    fn the_report_scores_each_model_on_its_own() {
        let mut w = World::with_played(0);
        let results = [
            ("Galatasaray", "Gaziantep", (3, 0)),
            ("Fenerbahçe", "Konyaspor", (0, 2)),
            ("Beşiktaş", "Trabzonspor", (1, 1)),
            ("Samsunspor", "Göztepe", (2, 1)),
            ("Rizespor", "Kasımpaşa", (0, 0)),
        ];
        for (h, a, score) in results {
            w.played.insert((w.idx[h], w.idx[a]), score);
        }
        let current = crate::model_version::current();
        let log = vec![
            forecast(
                "Galatasaray",
                "Gaziantep",
                [70.0, 20.0, 10.0],
                Some("2026-08"),
            ),
            forecast(
                "Fenerbahçe",
                "Konyaspor",
                [60.0, 25.0, 15.0],
                Some("2026-08"),
            ),
            forecast("Beşiktaş", "Trabzonspor", [45.0, 30.0, 25.0], Some(current)),
            forecast(
                "Samsunspor",
                "Göztepe",
                [40.0, 30.0, 30.0],
                Some("2026-10a"),
            ),
            forecast("Rizespor", "Kasımpaşa", [35.0, 30.0, 35.0], None),
            // Not played: in no group.
            forecast("Alanyaspor", "Eyüpspor", [40.0, 30.0, 30.0], Some(current)),
        ];
        let r = report_with(&w, log);
        assert_eq!(r.scored, 5);
        assert_eq!(r.current_model_version, current);
        let order: Vec<Option<&str>> = r
            .by_model
            .iter()
            .map(|s| s.model_version.as_deref())
            .collect();
        assert_eq!(
            order,
            [Some(current), Some("2026-10a"), Some("2026-08"), None]
        );
        assert_eq!(
            r.by_model.iter().map(|s| s.scored).collect::<Vec<_>>(),
            [1, 1, 2, 1]
        );
        assert_eq!(r.by_model.iter().map(|s| s.scored).sum::<usize>(), r.scored);

        for s in &r.by_model {
            let mine: Vec<&ScoredMatch> = r
                .matches
                .iter()
                .filter(|m| m.model_version == s.model_version)
                .collect();
            let n = mine.len() as f64;
            let mean =
                |pick: fn(&ScoredMatch) -> f64| mine.iter().map(|m| pick(m)).sum::<f64>() / n;
            assert!((s.log_loss - mean(|m| m.log_loss)).abs() < 1e-12);
            assert!((s.baseline_log_loss - mean(|m| m.baseline_log_loss)).abs() < 1e-12);
            let hits = mine.iter().filter(|m| m.hit).count() as f64;
            assert_eq!(s.hit_rate_pct, hits / n * 100.0);
            assert_eq!(s.vs_baseline.n, s.scored);
            assert!((s.vs_baseline.mean - (s.log_loss - s.baseline_log_loss)).abs() < 1e-12);
        }
        // The 2026-08 pair: one hit (the home win) and one miss.
        let august = &r.by_model[2];
        assert_eq!(august.hit_rate_pct, 50.0);
        assert!(august.vs_baseline.ci95.is_some());
        // One match is a point estimate, not an interval.
        assert!(r.by_model[0].vs_baseline.ci95.is_none());

        assert_eq!(r.vs_baseline.n, 5);
        assert!((r.vs_baseline.mean - (r.log_loss - r.baseline_log_loss)).abs() < 1e-12);
        let weighted = r
            .by_model
            .iter()
            .map(|s| s.log_loss * s.scored as f64)
            .sum::<f64>()
            / r.scored as f64;
        assert!((r.log_loss - weighted).abs() < 1e-12);
    }

    /// The interval on a toy sample worked by hand: differences 1, 2, 3, 4
    /// have mean 2.5 and sample variance 5/3, so the standard error is
    /// √(5/12) and the 95% interval 2.5 ± 1.96 √(5/12).
    #[test]
    fn the_interval_is_a_normal_approximation_on_per_match_differences() {
        let v = vs_baseline(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(v.n, 4);
        assert_eq!(v.mean, 2.5);
        let se = (5.0f64 / 12.0).sqrt();
        assert!((v.std_error.unwrap() - se).abs() < 1e-12);
        let [lo, hi] = v.ci95.unwrap();
        assert!((lo - (2.5 - 1.959_963_984_540_054 * se)).abs() < 1e-12);
        assert!((hi - (2.5 + 1.959_963_984_540_054 * se)).abs() < 1e-12);
        assert!((lo - 1.234_8).abs() < 1e-4 && (hi - 3.765_2).abs() < 1e-4);

        // Identical differences: no spread, a zero-width interval.
        let flat = vs_baseline(&[-0.25, -0.25, -0.25]);
        assert_eq!(flat.std_error, Some(0.0));
        let [lo, hi] = flat.ci95.unwrap();
        assert_eq!([lo, hi], [-0.25, -0.25]);

        let one = vs_baseline(&[0.3]);
        assert_eq!(
            (one.n, one.mean, one.std_error, one.ci95),
            (1, 0.3, None, None)
        );
        let none = vs_baseline(&[]);
        assert_eq!((none.n, none.mean, none.ci95), (0, 0.0, None));

        // The report's pooled interval is this one, over its own matches.
        let w = World::with_played(crate::sim::MID_SEASON);
        let log: Vec<LoggedForecast> = w
            .fixtures
            .iter()
            .enumerate()
            .map(|(k, f)| {
                let home = 25.0 + (k * 11 % 45) as f64;
                let mut e = forecast(
                    &w.teams[f.home],
                    &w.teams[f.away],
                    [home, 27.0, 73.0 - home],
                    None,
                );
                e.round = f.round;
                e
            })
            .collect();
        let r = report_with(&w, log);
        let diffs: Vec<f64> = r
            .matches
            .iter()
            .map(|m| m.log_loss - m.baseline_log_loss)
            .collect();
        let expected = vs_baseline(&diffs);
        assert_eq!(r.vs_baseline.n, crate::sim::MID_SEASON);
        assert!((r.vs_baseline.mean - expected.mean).abs() < 1e-12);
        assert!((r.vs_baseline.std_error.unwrap() - expected.std_error.unwrap()).abs() < 1e-12);
    }

    /// The base rates are the outcome split of the embedded history: played
    /// matches only, and nothing from on or after this season's first day.
    #[test]
    fn base_rates_are_the_league_history_before_this_season() {
        use crate::history::HistoricalMatch;
        let day = |y, m, d| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let m = |date, hs, as_| HistoricalMatch {
            date,
            home_team: "A".to_string(),
            away_team: "B".to_string(),
            home_score: hs,
            away_score: as_,
            neutral: false,
        };
        let toy = [
            m(day(2025, 9, 1), 2, 0),
            m(day(2025, 9, 8), 1, 0),
            m(day(2025, 9, 15), 1, 1),
            m(day(2026, 3, 1), 0, 3),
            // This season: left out.
            m(day(2026, 8, 20), 0, 0),
            m(day(2026, 8, 21), 0, 0),
        ];
        assert_eq!(outcome_split(&toy, day(2026, 8, 14)), [0.5, 0.25, 0.25]);

        let start = season_start();
        assert!(start.format("%Y-%m-%d").to_string().as_str() >= "2026-07-01");
        let history = crate::history::load_history();
        assert!(
            history.iter().all(|m| m.date < start),
            "a 2026-27 row in the history"
        );
        let rates = base_rates();
        assert_eq!(rates, outcome_split(&history, start));
        assert!((rates.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        let home_wins = history
            .iter()
            .filter(|m| m.home_score > m.away_score)
            .count();
        assert_eq!(rates[0], home_wins as f64 / history.len() as f64);
        // A home edge and draws near a quarter, as in any recent Süper Lig
        // season: a sanity check on the source, not a pin.
        assert!(
            rates[0] > rates[2] && (0.2..0.3).contains(&rates[1]),
            "{rates:?}"
        );
    }

    /// An entry from before labels existed still parses, scores as
    /// unlabelled, and writes back without gaining a field.
    #[test]
    fn an_unlabelled_entry_still_parses_and_round_trips() {
        let text = r#"{"round":1,"date":"2026-08-22","home":"Galatasaray","away":"Gaziantep","home_pct":70.0,"draw_pct":20.0,"away_pct":10.0,"logged_at":"2026-08-20"}"#;
        let f: LoggedForecast = serde_json::from_str(text).unwrap();
        assert_eq!(f.model_version, None);
        assert_eq!(serde_json::to_string(&f).unwrap(), text);
        let labelled = LoggedForecast {
            model_version: Some("2026-08".to_string()),
            ..f
        };
        assert!(serde_json::to_string(&labelled)
            .unwrap()
            .ends_with(r#""logged_at":"2026-08-20","model_version":"2026-08"}"#));
    }
}
