//! Pi-ratings (Constantinou & Fenton, 2013): sequential team ratings that
//! map directly to expected goal difference. Each team keeps separate
//! home/away ratings updated from every result, with cross-learning between
//! the two. Computed in one fast pass over the historical results at
//! startup — no optimization step.

use chrono::NaiveDate;

use crate::history::{HistoricalMatch, TeamIndex};

/// Rating-to-goal-difference base (b) and divisor (c) from the paper.
const B: f64 = 10.0;
const C: f64 = 3.0;
/// Learning rate for the directly involved rating.
const LAMBDA: f64 = 0.035;
/// Cross-learning rate home<->away.
const GAMMA: f64 = 0.7;

#[derive(Clone, Debug)]
pub struct PiRatings {
    /// Home-ground rating per team, indexed like `TeamIndex::idx_to_name`.
    pub home: Vec<f64>,
    /// Away-ground rating per team.
    pub away: Vec<f64>,
    /// Mean total goals per match in the fit window; used to split an
    /// expected goal difference into a (λ_a, λ_b) pair.
    pub avg_goals: f64,
    pub n_matches: usize,
}

/// Expected goal difference contribution of a rating.
fn expected_gd(rating: f64) -> f64 {
    let mag = B.powf(rating.abs() / C) - 1.0;
    if rating < 0.0 {
        -mag
    } else {
        mag
    }
}

/// Error weighting: large surprises move ratings sub-linearly.
fn psi(error: f64) -> f64 {
    C * (1.0 + error.abs()).log10()
}

impl PiRatings {
    /// One sequential pass over `history` (matches after `since` only),
    /// in date order.
    pub fn compute(history: &[HistoricalMatch], idx: &TeamIndex, since: NaiveDate) -> Self {
        let n = idx.idx_to_name.len();
        let mut ratings = PiRatings {
            home: vec![0.0; n],
            away: vec![0.0; n],
            avg_goals: 0.0,
            n_matches: 0,
        };

        let mut ordered: Vec<&HistoricalMatch> =
            history.iter().filter(|m| m.date >= since).collect();
        ordered.sort_by_key(|m| m.date);

        let mut total_goals = 0u64;
        let mut start = 0;
        while start < ordered.len() {
            let day = ordered[start].date;
            let end = ordered[start..]
                .iter()
                .position(|m| m.date != day)
                .map_or(ordered.len(), |offset| start + offset);

            // All fixtures on a date use the ratings entering that date.
            // This makes the result independent of arbitrary CSV ordering —
            // especially important because historical clubs share one bucket.
            let updates: Vec<(usize, usize, f64)> = ordered[start..end]
                .iter()
                .map(|m| {
                    let h = idx.canonical(&m.home_team);
                    let a = idx.canonical(&m.away_team);
                    let step = ratings.update_step(h, a, m.home_score as f64 - m.away_score as f64);
                    (h, a, step)
                })
                .collect();
            for (home, away, step) in updates {
                ratings.apply_step(home, away, step);
            }
            total_goals += ordered[start..end]
                .iter()
                .map(|m| (m.home_score + m.away_score) as u64)
                .sum::<u64>();
            start = end;
        }
        ratings.n_matches = ordered.len();
        ratings.avg_goals = if ordered.is_empty() {
            2.6
        } else {
            total_goals as f64 / ordered.len() as f64
        };
        tracing::info!(
            "Pi-ratings computed over {} matches (avg {:.2} goals/match)",
            ratings.n_matches,
            ratings.avg_goals
        );
        ratings
    }

    fn update_step(&self, home: usize, away: usize, observed_gd: f64) -> f64 {
        let predicted = expected_gd(self.home[home]) - expected_gd(self.away[away]);
        let error = observed_gd - predicted;
        psi(error) * LAMBDA * error.signum()
    }

    fn apply_step(&mut self, home: usize, away: usize, step: f64) {
        self.home[home] += step;
        self.away[home] += step * GAMMA;

        self.away[away] -= step;
        self.home[away] -= step * GAMMA;
    }

    /// Rating used for a neutral-venue match (mean of home/away form).
    fn neutral_rating(&self, team: usize) -> f64 {
        (self.home[team] + self.away[team]) / 2.0
    }

    /// Expected goals `(λ_home, λ_away)`, splitting the expected total around
    /// the predicted goal difference. League fixtures use the two venue-
    /// specific ratings; a genuinely neutral match averages each club's home
    /// and away ratings.
    pub fn lambdas(&self, home: usize, away: usize, neutral: bool) -> (f64, f64) {
        let (home_rating, away_rating) = if neutral {
            (self.neutral_rating(home), self.neutral_rating(away))
        } else {
            (self.home[home], self.away[away])
        };
        let gd = expected_gd(home_rating) - expected_gd(away_rating);
        let la = (self.avg_goals + gd) / 2.0;
        let lb = (self.avg_goals - gd) / 2.0;
        (la.clamp(0.15, 5.0), lb.clamp(0.15, 5.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx_fixture() -> TeamIndex {
        TeamIndex::league()
    }

    fn m(date: (i32, u32, u32), h: &str, a: &str, hs: u16, as_: u16) -> HistoricalMatch {
        HistoricalMatch {
            date: NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
            home_team: h.to_string(),
            away_team: a.to_string(),
            home_score: hs,
            away_score: as_,
            neutral: false,
        }
    }

    #[test]
    fn winning_club_gains_rating_and_loser_drops() {
        let idx = idx_fixture();
        let since = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let history = vec![
            m((2021, 1, 1), "Galatasaray", "Gaziantep", 4, 0),
            m((2021, 2, 1), "Galatasaray", "Gaziantep", 3, 0),
        ];
        let pi = PiRatings::compute(&history, &idx, since);
        let (gs, gaz) = (idx.canonical("Galatasaray"), idx.canonical("Gaziantep"));
        assert!(pi.home[gs] > 0.0);
        assert!(pi.away[gs] > 0.0, "cross-learning should lift away rating");
        assert!(pi.away[gaz] < 0.0);
        assert!(pi.home[gaz] < 0.0);
    }

    #[test]
    fn lambdas_favor_stronger_club_and_stay_positive() {
        let idx = idx_fixture();
        let since = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let mut history = Vec::new();
        for i in 0..30 {
            history.push(m((2021, 1, 1 + (i % 27)), "Galatasaray", "Gaziantep", 3, 0));
        }
        let pi = PiRatings::compute(&history, &idx, since);
        let (gs, gaz) = (idx.canonical("Galatasaray"), idx.canonical("Gaziantep"));
        let (lgs, lgaz) = pi.lambdas(gs, gaz, true);
        assert!(lgs > lgaz, "Galatasaray should have higher expected goals");
        assert!(lgaz >= 0.15);
        // Order symmetry.
        let (lgaz2, lgs2) = pi.lambdas(gaz, gs, true);
        assert!((lgs - lgs2).abs() < 1e-12 && (lgaz - lgaz2).abs() < 1e-12);
    }

    #[test]
    fn home_rating_boosts_expected_goals() {
        let idx = idx_fixture();
        let since = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        // Trabzonspor strong at home, weaker away.
        let history = vec![
            m((2021, 1, 1), "Trabzonspor", "Konyaspor", 3, 0),
            m((2021, 2, 1), "Konyaspor", "Trabzonspor", 2, 0),
            m((2021, 3, 1), "Trabzonspor", "Konyaspor", 2, 0),
        ];
        let pi = PiRatings::compute(&history, &idx, since);
        let tra = idx.canonical("Trabzonspor");
        let kon = idx.canonical("Konyaspor");
        let (host_lam, _) = pi.lambdas(tra, kon, false);
        let (neutral_lam, _) = pi.lambdas(tra, kon, true);
        assert!(
            host_lam > neutral_lam,
            "playing at home should use the stronger home rating: {host_lam} vs {neutral_lam}"
        );
    }

    #[test]
    fn league_fixture_uses_the_away_clubs_away_rating() {
        let idx = idx_fixture();
        let gs = idx.canonical("Galatasaray");
        let gaz = idx.canonical("Gaziantep");
        let mut pi = PiRatings {
            home: vec![0.0; idx.idx_to_name.len()],
            away: vec![0.0; idx.idx_to_name.len()],
            avg_goals: 2.6,
            n_matches: 0,
        };
        // Deliberately make Gaziantep's venue ratings disagree. A league away
        // fixture must use -1.0, while a neutral fixture uses their mean 1.0.
        pi.home[gaz] = 3.0;
        pi.away[gaz] = -1.0;

        let (_, league_away) = pi.lambdas(gs, gaz, false);
        let (_, neutral_away) = pi.lambdas(gs, gaz, true);
        assert!(
            league_away < neutral_away,
            "away venue rating was ignored: {league_away} vs {neutral_away}"
        );
    }

    #[test]
    fn same_day_results_do_not_depend_on_csv_row_order() {
        let idx = idx_fixture();
        let since = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        // Both unknown historical clubs map to Other Club. Sequentially
        // updating that bucket would make the second row depend on the first.
        let a = m((2021, 1, 1), "Historical A", "Galatasaray", 0, 3);
        let b = m((2021, 1, 1), "Historical B", "Gaziantep", 2, 0);
        let forward = PiRatings::compute(&[a.clone(), b.clone()], &idx, since);
        let reversed = PiRatings::compute(&[b, a], &idx, since);
        assert_eq!(forward.home, reversed.home);
        assert_eq!(forward.away, reversed.away);
    }

    #[test]
    fn compute_on_real_history_produces_sane_ratings() {
        let idx = idx_fixture();
        let history = crate::history::load_history_with_cutoff(2018);
        let since = NaiveDate::from_ymd_opt(2018, 1, 1).unwrap();
        let pi = PiRatings::compute(&history, &idx, since);
        assert!(pi.n_matches > 1000);
        assert!(pi.avg_goals > 1.5 && pi.avg_goals < 4.0);
        // Top sides should out-rate minnows on aggregate rating.
        let strong = idx.canonical("Galatasaray");
        let weak = idx.canonical("Gaziantep");
        let s = (pi.home[strong] + pi.away[strong]) / 2.0;
        let w = (pi.home[weak] + pi.away[weak]) / 2.0;
        assert!(s > w, "Galatasaray {s} should out-rate Gaziantep {w}");
    }
}
