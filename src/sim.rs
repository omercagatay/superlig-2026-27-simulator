use std::collections::HashMap;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rand_distr::Poisson;
use rayon::prelude::*;

use crate::data;
use crate::league::{apply_result, rank_table, TeamRecord};

/// One scheduled fixture, with clubs resolved to `World` indices.
#[derive(Clone, Copy, Debug)]
pub struct LeagueFixture {
    pub round: u8,
    pub home: usize,
    pub away: usize,
}

/// Calendar metadata for a fixture, parallel to `World.fixtures` by index.
#[derive(Clone, Debug)]
pub struct FixtureDate {
    pub date: String,
    pub kickoff: Option<String>,
}

#[derive(Clone)]
pub struct World {
    pub teams: Vec<String>,
    pub idx: HashMap<String, usize>,
    pub elo: Vec<f64>,
    /// The official 306-fixture calendar, in round order.
    pub fixtures: Vec<LeagueFixture>,
    /// Kick-off dates, indexed alongside `fixtures`.
    pub dates: Vec<FixtureDate>,
    /// Real results, keyed `(home_idx, away_idx)`. Fixtures present here are
    /// never simulated.
    pub played: HashMap<(usize, usize), (u16, u16)>,
    /// Optional strength-model ensemble blended into expected goals; `None`
    /// means pure Elo (used by most unit tests).
    pub ensemble: Option<Ensemble>,
}

/// Weighted blend of the three strength models. Dixon-Coles and pi-rating
/// club indices coincide with `World` club indices because
/// `history::TeamIndex::league()` is built from the same `data::elo()` order.
#[derive(Clone)]
pub struct Ensemble {
    pub dc: crate::dixoncoles::DcParams,
    pub pi: crate::piratings::PiRatings,
    /// Blend weights (Elo, Dixon-Coles, pi-ratings); normalized on use.
    pub w_elo: f64,
    pub w_dc: f64,
    pub w_pi: f64,
}

impl Ensemble {
    /// Build the ensemble from data embedded in the binary: the offline
    /// Dixon-Coles fit (`data/dc_params.json`, refreshed via
    /// `cargo run --release --example fit_dc`) and pi-ratings computed in one
    /// pass over the historical Süper Lig results.
    pub fn from_embedded_data(w_elo: f64, w_dc: f64, w_pi: f64) -> Result<Self, String> {
        let dc: crate::dixoncoles::DcParams =
            serde_json::from_str(include_str!("../data/dc_params.json"))
                .map_err(|e| format!("dc_params.json: {e}"))?;
        let idx = crate::history::TeamIndex::league();
        if dc.n_teams != idx.idx_to_name.len() || dc.alpha.len() != dc.n_teams {
            return Err(format!(
                "dc_params.json club count {} does not match current club list {} — refit with `cargo run --release --example fit_dc`",
                dc.n_teams,
                idx.idx_to_name.len()
            ));
        }
        let history = crate::history::load_history_with_cutoff(crate::history::CUTOFF_YEAR);
        let since =
            chrono::NaiveDate::from_ymd_opt(crate::history::CUTOFF_YEAR, 1, 1).expect("valid date");
        let pi = crate::piratings::PiRatings::compute(&history, &idx, since);
        Ok(Ensemble {
            dc,
            pi,
            w_elo,
            w_dc,
            w_pi,
        })
    }

    /// Blended `(λ_home, λ_away)`, or `None` when the weights leave only Elo.
    fn lam_pair(&self, home: usize, away: usize) -> Option<(f64, f64)> {
        let total = self.w_dc + self.w_pi;
        if total <= 0.0 {
            return None;
        }
        // `neutral = false`: league fixtures always have a home side, so
        // Dixon-Coles applies its gamma home boost.
        let (dc_h, dc_a) = self.dc.lam(home, away, false);
        let (pi_h, pi_a) = self.pi.lambdas(home, away, true, false);
        Some((
            (self.w_dc * dc_h + self.w_pi * pi_h) / total,
            (self.w_dc * dc_a + self.w_pi * pi_a) / total,
        ))
    }
}

/// Exact per-fixture outcome and market probabilities, as percentages.
#[derive(Clone, Copy, Debug)]
pub struct FixtureProbs {
    pub home_win_pct: f64,
    pub draw_pct: f64,
    pub away_win_pct: f64,
    /// P(total goals >= 3).
    pub over25_pct: f64,
    /// P(both sides score).
    pub btts_pct: f64,
    /// Expected goals for the home side (the model's λ).
    pub home_xg: f64,
    /// Expected goals for the away side.
    pub away_xg: f64,
    /// The three most probable exact scorelines, `(home, away, pct)`,
    /// most likely first.
    pub top_scores: [(u8, u8, f64); 3],
}

/// One simulated season: finishing order plus each club's final record.
#[derive(Clone, Debug)]
pub struct SeasonResult {
    /// Club indices in finishing order; `order[0]` is the champion.
    pub order: Vec<usize>,
    /// Indexed by club, not by position.
    pub records: Vec<TeamRecord>,
}

#[derive(Clone, Debug)]
pub struct SimResults {
    pub n_sims: usize,
    /// `position_counts[club][position]`, position 0 = champion.
    pub position_counts: Vec<Vec<usize>>,
    pub title_counts: Vec<usize>,
    pub ucl_counts: Vec<usize>,
    pub uel_counts: Vec<usize>,
    pub uecl_counts: Vec<usize>,
    pub europe_counts: Vec<usize>,
    pub relegation_counts: Vec<usize>,
    pub points_sum: Vec<f64>,
    pub gd_sum: Vec<f64>,
    pub won_sum: Vec<f64>,
    pub drawn_sum: Vec<f64>,
    pub gf_sum: Vec<f64>,
    pub ga_sum: Vec<f64>,
    /// `pairwise_above[a * n + b]` = trials where `a` finished above `b`.
    pub pairwise_above: Vec<usize>,
    pub representative: SeasonResult,
}

#[derive(Clone, Debug)]
pub struct SimConfig {
    pub n_sims: usize,
    pub seed: u64,
    pub elo_overrides: HashMap<String, f64>,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            n_sims: 50000,
            seed: 12345,
            elo_overrides: HashMap::new(),
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        let teams: Vec<String> = data::elo().iter().map(|(t, _)| t.to_string()).collect();
        let idx: HashMap<String, usize> = teams
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i))
            .collect();
        let elo: Vec<f64> = data::elo().iter().map(|(_, e)| *e).collect();

        let mut fixtures = Vec::with_capacity(data::N_FIXTURES);
        let mut dates = Vec::with_capacity(data::N_FIXTURES);
        let mut played = HashMap::new();
        for f in data::fixtures() {
            let home = idx[&f.home];
            let away = idx[&f.away];
            fixtures.push(LeagueFixture {
                round: f.round,
                home,
                away,
            });
            dates.push(FixtureDate {
                date: f.date.clone(),
                kickoff: f.kickoff.clone(),
            });
            if let (Some(hs), Some(as_)) = (f.home_score, f.away_score) {
                played.insert((home, away), (hs, as_));
            }
        }

        World {
            teams,
            idx,
            elo,
            fixtures,
            dates,
            played,
            ensemble: None,
        }
    }

    /// Overlay scraped results onto the recorded set. Results only ever get
    /// added or corrected — an empty or partial scrape never erases the
    /// baseline, so a bad fetch degrades to "no new information".
    ///
    /// Elo ratings are no longer scraped: club ratings come from `data::elo()`
    /// and are adjusted only through scenario overrides.
    pub fn update_from_live(&mut self, live: &crate::scraper::LiveData) -> usize {
        let mut applied = 0;
        for m in &live.played_matches {
            let (Some(&home), Some(&away)) = (self.idx.get(&m.home), self.idx.get(&m.away)) else {
                tracing::debug!("live scrape: unknown club in {} v {}", m.home, m.away);
                continue;
            };
            if !self
                .fixtures
                .iter()
                .any(|f| f.home == home && f.away == away)
            {
                tracing::debug!(
                    "live scrape: {} v {} is not a 2026-27 fixture",
                    m.home,
                    m.away
                );
                continue;
            }
            self.played
                .insert((home, away), (m.home_score, m.away_score));
            applied += 1;
        }
        applied
    }

    pub fn apply_overrides(&mut self, overrides: &HashMap<String, f64>) {
        for (team, rating) in overrides {
            if let Some(&i) = self.idx.get(team) {
                self.elo[i] = *rating;
            }
        }
    }

    /// Expected goals `(λ_home, λ_away)` for a fixture. The home-ground
    /// boost goes to `home`, not to a per-team host flag.
    fn lam_pair(&self, home: usize, away: usize) -> (f64, f64) {
        let dr = self.elo[home] - self.elo[away] + data::HOME_ADV;
        let lh_elo = data::BASE * (10.0_f64).powf(dr / data::D_DIV);
        let la_elo = data::BASE * (10.0_f64).powf(-dr / data::D_DIV);

        let ensemble_lam = self
            .ensemble
            .as_ref()
            .and_then(|e| e.lam_pair(home, away).map(|l| (e, l)));
        let (lh, la) = match ensemble_lam {
            None => (lh_elo, la_elo),
            Some((e, (eh, ea))) => {
                let w_models = e.w_dc + e.w_pi;
                let total = e.w_elo + w_models;
                (
                    (e.w_elo * lh_elo + w_models * eh) / total,
                    (e.w_elo * la_elo + w_models * ea) / total,
                )
            }
        };
        (lh.clamp(0.15, 5.0), la.clamp(0.15, 5.0))
    }

    fn sample_poisson(rng: &mut SmallRng, lambda: f64) -> i64 {
        let dist = Poisson::new(lambda).unwrap();
        rng.sample(dist) as i64
    }

    /// ρ for joint-scoreline sampling; `Some` only while the Dixon-Coles
    /// component carries weight, so `ENSEMBLE_WEIGHTS=1,0,0` (and plain
    /// `World::new()`) keep the independent-Poisson behavior.
    fn score_rho(&self) -> Option<f64> {
        self.ensemble
            .as_ref()
            .filter(|e| e.w_dc > 0.0)
            .map(|e| e.dc.rho)
    }

    /// Draw a 90-minute scoreline: from the Dixon-Coles joint distribution
    /// (low-score cells corrected by ρ) when DC is active, otherwise two
    /// independent Poissons.
    fn sample_match_score(&self, la: f64, lb: f64, rng: &mut SmallRng) -> (i64, i64) {
        match self.score_rho() {
            Some(rho) => {
                let (ga, gb) = crate::dixoncoles::sample_score(la, lb, rho, rng.gen::<f64>());
                (ga as i64, gb as i64)
            }
            None => (Self::sample_poisson(rng, la), Self::sample_poisson(rng, lb)),
        }
    }

    /// Monte Carlo outcome probabilities for a single league fixture:
    /// `(home_win_pct, draw_pct, away_win_pct)`. Unlike a knockout tie, a
    /// league match can end level, so the draw is a first-class outcome.
    pub fn match_win_probs(&self, home: usize, away: usize) -> (f64, f64, f64) {
        let p = self.fixture_probs(home, away);
        (p.home_win_pct, p.draw_pct, p.away_win_pct)
    }

    /// Exact outcome and market probabilities for one fixture, summed from
    /// the joint scoreline table rather than Monte Carlo sampled — no noise,
    /// no seed, and cheap enough to run for all 306 fixtures per request.
    /// When the ensemble is off (pure Elo) the table is used with ρ = 0,
    /// which reduces to independent Poissons.
    pub fn fixture_probs(&self, home: usize, away: usize) -> FixtureProbs {
        let (lh, la) = self.lam_pair(home, away);
        let rho = self.score_rho().unwrap_or(0.0);
        let t = crate::dixoncoles::score_table(lh, la, rho);

        let (mut hw, mut dr, mut aw, mut over25, mut btts) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (x, row) in t.iter().enumerate() {
            for (y, &p) in row.iter().enumerate() {
                match x.cmp(&y) {
                    std::cmp::Ordering::Greater => hw += p,
                    std::cmp::Ordering::Equal => dr += p,
                    std::cmp::Ordering::Less => aw += p,
                }
                if x + y >= 3 {
                    over25 += p;
                }
                if x >= 1 && y >= 1 {
                    btts += p;
                }
            }
        }
        // The three most probable exact scorelines.
        let mut top = [(0u8, 0u8, 0.0f64); 3];
        for (x, row) in t.iter().enumerate() {
            for (y, &p) in row.iter().enumerate() {
                if p > top[2].2 {
                    top[2] = (x as u8, y as u8, p);
                    top.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
                }
            }
        }

        // The table truncates at MAX_GOALS per side; normalize so the 1X2
        // triple sums to exactly 100 even for extreme-mismatch lambdas.
        let total = hw + dr + aw;
        for score in &mut top {
            score.2 = score.2 / total * 100.0;
        }
        FixtureProbs {
            home_win_pct: hw / total * 100.0,
            draw_pct: dr / total * 100.0,
            away_win_pct: aw / total * 100.0,
            over25_pct: over25 / total * 100.0,
            btts_pct: btts / total * 100.0,
            home_xg: lh,
            away_xg: la,
            top_scores: top,
        }
    }

    /// Unplayed fixtures of the earliest round that still has any — the
    /// league's equivalent of "the next matchday".
    pub fn upcoming_matches(&self) -> Vec<LeagueFixture> {
        let next = self
            .fixtures
            .iter()
            .filter(|f| !self.played.contains_key(&(f.home, f.away)))
            .map(|f| f.round)
            .min();
        match next {
            None => Vec::new(),
            Some(round) => self
                .fixtures
                .iter()
                .filter(|f| f.round == round && !self.played.contains_key(&(f.home, f.away)))
                .copied()
                .collect(),
        }
    }

    pub fn simulate_one(&self, rng: &mut SmallRng) -> SeasonResult {
        let n = self.teams.len();
        let mut records = vec![TeamRecord::default(); n];
        let mut results: HashMap<(usize, usize), (i64, i64)> =
            HashMap::with_capacity(self.fixtures.len());

        for f in &self.fixtures {
            let (hg, ag) = match self.played.get(&(f.home, f.away)) {
                Some(&(hs, as_)) => (hs as i64, as_ as i64),
                None => {
                    let (lh, la) = self.lam_pair(f.home, f.away);
                    self.sample_match_score(lh, la, rng)
                }
            };
            apply_result(&mut records, f.home, f.away, hg, ag);
            results.insert((f.home, f.away), (hg, ag));
        }

        let order = rank_table(&records, &results, &mut || rng.gen::<u64>());
        SeasonResult { order, records }
    }

    pub fn simulate(&self, config: &SimConfig) -> SimResults {
        let n_sims = config.n_sims;
        let n = self.teams.len();
        let seasons: Vec<SeasonResult> = (0..n_sims)
            .into_par_iter()
            .map(|i| {
                let mut rng =
                    SmallRng::seed_from_u64(config.seed.wrapping_add(i as u64 * 2654435761));
                self.simulate_one(&mut rng)
            })
            .collect();

        let mut position_counts = vec![vec![0usize; n]; n];
        let mut title_counts = vec![0usize; n];
        let mut ucl_counts = vec![0usize; n];
        let mut uel_counts = vec![0usize; n];
        let mut uecl_counts = vec![0usize; n];
        let mut europe_counts = vec![0usize; n];
        let mut relegation_counts = vec![0usize; n];
        let mut points_sum = vec![0.0f64; n];
        let mut gd_sum = vec![0.0f64; n];
        let mut won_sum = vec![0.0f64; n];
        let mut drawn_sum = vec![0.0f64; n];
        let mut gf_sum = vec![0.0f64; n];
        let mut ga_sum = vec![0.0f64; n];
        let mut pairwise_above = vec![0usize; n * n];

        let relegation_from = n - data::RELEGATION_SPOTS;
        for s in &seasons {
            for (pos, &club) in s.order.iter().enumerate() {
                position_counts[club][pos] += 1;
                if pos == 0 {
                    title_counts[club] += 1;
                }
                if pos < data::UCL_SPOTS {
                    ucl_counts[club] += 1;
                } else if pos < data::UCL_SPOTS + data::UEL_SPOTS {
                    uel_counts[club] += 1;
                } else if pos < data::EUROPE_SPOTS {
                    uecl_counts[club] += 1;
                }
                if pos < data::EUROPE_SPOTS {
                    europe_counts[club] += 1;
                }
                if pos >= relegation_from {
                    relegation_counts[club] += 1;
                }
                for &below in &s.order[pos + 1..] {
                    pairwise_above[club * n + below] += 1;
                }
            }
            for club in 0..n {
                points_sum[club] += s.records[club].points as f64;
                gd_sum[club] += s.records[club].gd() as f64;
                won_sum[club] += s.records[club].won as f64;
                drawn_sum[club] += s.records[club].drawn as f64;
                gf_sum[club] += s.records[club].gf as f64;
                ga_sum[club] += s.records[club].ga as f64;
            }
        }

        // Representative season: the trial whose finishing order best matches
        // the per-position modal club. Replaces the old representative bracket.
        let mode_at: Vec<usize> = (0..n)
            .map(|pos| {
                (0..n)
                    .max_by_key(|&club| position_counts[club][pos])
                    .unwrap_or(0)
            })
            .collect();
        let score = |s: &SeasonResult| -> usize {
            s.order
                .iter()
                .enumerate()
                .filter(|&(pos, &club)| mode_at[pos] == club)
                .count()
        };
        let representative = seasons
            .iter()
            .max_by_key(|s| score(s))
            .cloned()
            .expect("at least one trial");

        SimResults {
            n_sims,
            position_counts,
            title_counts,
            ucl_counts,
            uel_counts,
            uecl_counts,
            europe_counts,
            relegation_counts,
            points_sum,
            gd_sum,
            won_sum,
            drawn_sum,
            gf_sum,
            ga_sum,
            pairwise_above,
            representative,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_new_has_eighteen_clubs_and_the_full_calendar() {
        let w = World::new();
        assert_eq!(w.teams.len(), data::N_TEAMS);
        assert_eq!(w.fixtures.len(), data::N_FIXTURES);
        assert_eq!(w.idx["Galatasaray"], 0);
        for f in &w.fixtures {
            assert!(f.home < data::N_TEAMS && f.away < data::N_TEAMS);
            assert_ne!(f.home, f.away);
        }
    }

    #[test]
    fn recorded_results_are_loaded_from_the_calendar() {
        let w = World::new();
        for f in data::fixtures() {
            if let (Some(hs), Some(as_)) = (f.home_score, f.away_score) {
                let key = (w.idx[&f.home], w.idx[&f.away]);
                assert_eq!(w.played.get(&key), Some(&(hs, as_)));
            }
        }
    }

    #[test]
    fn home_advantage_applies_to_the_fixture_home_side_not_a_team_flag() {
        let w = World::new();
        let (a, b) = (w.idx["Galatasaray"], w.idx["Gaziantep"]);

        let (la_home, lb_away) = w.lam_pair(a, b);
        let (lb_home, la_away) = w.lam_pair(b, a);

        // The same club scores more at home than away against the same opponent.
        assert!(la_home > la_away, "home side must get the boost");
        assert!(
            lb_home > lb_away,
            "and so must the other club at its own ground"
        );
        // The stronger club still outscores the weaker one on neutral comparison.
        assert!(la_home > lb_home);
        assert!(la_away > lb_away);
        for l in [la_home, lb_away, lb_home, la_away] {
            assert!((0.15..=5.0).contains(&l), "lambda {l} out of clamp range");
        }
    }

    #[test]
    fn elo_override_changes_ratings() {
        let mut w = World::new();
        let before = w.elo[w.idx["Galatasaray"]];
        let mut o = HashMap::new();
        o.insert("Galatasaray".to_string(), before - 200.0);
        w.apply_overrides(&o);
        assert_eq!(w.elo[w.idx["Galatasaray"]], before - 200.0);
    }

    #[test]
    fn simulate_one_produces_a_complete_valid_season() {
        let w = World::new();
        let mut rng = SmallRng::seed_from_u64(42);
        let r = w.simulate_one(&mut rng);

        assert_eq!(r.order.len(), data::N_TEAMS);
        let mut seen = r.order.clone();
        seen.sort_unstable();
        assert_eq!(
            seen,
            (0..data::N_TEAMS).collect::<Vec<_>>(),
            "order is a permutation"
        );

        for rec in &r.records {
            assert_eq!(rec.played(), 34, "every club plays 34 matches");
            assert_eq!(
                rec.points,
                rec.won as i64 * 3 + rec.drawn as i64,
                "points must follow from W/D"
            );
        }

        // A closed league: total goals for equals total goals against, and the
        // 306 fixtures produce 306 results' worth of points.
        let gf: i64 = r.records.iter().map(|x| x.gf).sum();
        let ga: i64 = r.records.iter().map(|x| x.ga).sum();
        assert_eq!(gf, ga);
        let draws: u16 = r.records.iter().map(|x| x.drawn).sum();
        let wins: u16 = r.records.iter().map(|x| x.won).sum();
        assert_eq!(wins as usize + draws as usize / 2, data::N_FIXTURES);
    }

    #[test]
    fn simulate_one_is_deterministic() {
        let w = World::new();
        let a = w.simulate_one(&mut SmallRng::seed_from_u64(7));
        let b = w.simulate_one(&mut SmallRng::seed_from_u64(7));
        assert_eq!(a.order, b.order);
        assert_eq!(a.records, b.records);
    }

    #[test]
    fn recorded_results_are_used_and_never_resampled() {
        let mut w = World::new();
        let (gs, gaz) = (w.idx["Galatasaray"], w.idx["Gaziantep"]);
        // Force an extreme, unmistakable result onto a real fixture.
        let fixture = w
            .fixtures
            .iter()
            .find(|f| f.home == gs && f.away == gaz)
            .copied()
            .expect("Galatasaray host Gaziantep once");
        w.played.insert((fixture.home, fixture.away), (9, 0));

        for seed in 0..20u64 {
            let r = w.simulate_one(&mut SmallRng::seed_from_u64(seed));
            assert!(r.records[gs].gf >= 9, "recorded 9 goals must always appear");
            assert!(r.records[gaz].ga >= 9);
        }
    }

    #[test]
    fn simulate_is_deterministic_for_same_seed() {
        let w = World::new();
        let cfg = SimConfig {
            n_sims: 500,
            seed: 999,
            elo_overrides: HashMap::new(),
        };
        let a = w.simulate(&cfg);
        let b = w.simulate(&cfg);
        assert_eq!(a.position_counts, b.position_counts);
        assert_eq!(a.title_counts, b.title_counts);
        assert_eq!(a.relegation_counts, b.relegation_counts);
        assert_eq!(a.representative.order, b.representative.order);
    }

    #[test]
    fn every_club_position_distribution_sums_to_n_sims() {
        let w = World::new();
        let cfg = SimConfig {
            n_sims: 400,
            seed: 3,
            elo_overrides: HashMap::new(),
        };
        let r = w.simulate(&cfg);
        assert_eq!(r.position_counts.len(), data::N_TEAMS);
        for (club, dist) in r.position_counts.iter().enumerate() {
            assert_eq!(dist.len(), data::N_TEAMS);
            assert_eq!(dist.iter().sum::<usize>(), cfg.n_sims, "club {club}");
        }
        // Each position is filled exactly once per trial.
        for pos in 0..data::N_TEAMS {
            let total: usize = r.position_counts.iter().map(|d| d[pos]).sum();
            assert_eq!(total, cfg.n_sims, "position {pos}");
        }
    }

    #[test]
    fn spot_counts_agree_with_the_position_distribution() {
        let w = World::new();
        let cfg = SimConfig {
            n_sims: 400,
            seed: 11,
            elo_overrides: HashMap::new(),
        };
        let r = w.simulate(&cfg);
        for club in 0..data::N_TEAMS {
            let d = &r.position_counts[club];
            assert_eq!(r.title_counts[club], d[0]);
            assert_eq!(r.ucl_counts[club], d[0] + d[1]);
            assert_eq!(r.uel_counts[club], d[2]);
            assert_eq!(r.uecl_counts[club], d[3]);
            assert_eq!(
                r.europe_counts[club],
                d[..data::EUROPE_SPOTS].iter().sum::<usize>()
            );
            let rel: usize = d[data::N_TEAMS - data::RELEGATION_SPOTS..].iter().sum();
            assert_eq!(
                r.relegation_counts[club],
                rel,
                "bottom {} relegated",
                data::RELEGATION_SPOTS
            );
        }
        assert_eq!(r.title_counts.iter().sum::<usize>(), cfg.n_sims);
        assert_eq!(
            r.relegation_counts.iter().sum::<usize>(),
            cfg.n_sims * data::RELEGATION_SPOTS
        );
    }

    #[test]
    fn pairwise_above_is_complementary() {
        let w = World::new();
        let cfg = SimConfig {
            n_sims: 300,
            seed: 5,
            elo_overrides: HashMap::new(),
        };
        let r = w.simulate(&cfg);
        let n = data::N_TEAMS;
        for a in 0..n {
            for b in 0..n {
                if a == b {
                    continue;
                }
                assert_eq!(
                    r.pairwise_above[a * n + b] + r.pairwise_above[b * n + a],
                    cfg.n_sims,
                    "{a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn stronger_clubs_win_the_title_more_often() {
        let w = World::new();
        let cfg = SimConfig {
            n_sims: 2000,
            seed: 1,
            elo_overrides: HashMap::new(),
        };
        let r = w.simulate(&cfg);
        let gs = w.idx["Galatasaray"];
        let gaz = w.idx["Gaziantep"];
        assert!(
            r.title_counts[gs] > r.title_counts[gaz],
            "the strongest club must out-title the weakest"
        );
        assert!(r.relegation_counts[gaz] > r.relegation_counts[gs]);
    }

    #[test]
    fn match_win_probs_sum_to_100_and_favor_the_stronger_side() {
        let w = World::new();
        let (gs, gaz) = (w.idx["Galatasaray"], w.idx["Gaziantep"]);
        let (h, d, a) = w.match_win_probs(gs, gaz);
        assert!((h + d + a - 100.0).abs() < 1e-9, "{h} + {d} + {a}");
        assert!(h > a, "the stronger home side must be favored");
        assert!(d > 5.0 && d < 40.0, "draw probability {d} is implausible");

        let p = w.fixture_probs(gs, gaz);
        assert!(p.over25_pct > 0.0 && p.over25_pct < 100.0);
        assert!(p.btts_pct > 0.0 && p.btts_pct < 100.0);
        // A mismatch this size should still clear typical market baselines.
        assert!(p.over25_pct > 30.0, "over 2.5 {} too low", p.over25_pct);
        assert!(p.home_xg > p.away_xg, "stronger home side out-xGs the away");
        assert!(p.top_scores[0].2 >= p.top_scores[1].2);
        assert!(p.top_scores[1].2 >= p.top_scores[2].2);
        assert!(p.top_scores[0].2 > 0.0 && p.top_scores[0].2 < 100.0);
    }

    #[test]
    fn home_advantage_shows_up_in_match_probabilities() {
        let w = World::new();
        let (gs, gaz) = (w.idx["Galatasaray"], w.idx["Gaziantep"]);
        let (gs_at_home, _, _) = w.match_win_probs(gs, gaz);
        let (_, _, gs_away) = w.match_win_probs(gaz, gs);
        assert!(gs_at_home > gs_away, "same club, better odds at home");
    }

    #[test]
    fn upcoming_returns_the_earliest_round_with_unplayed_fixtures() {
        let w = World::new();
        let up = w.upcoming_matches();
        assert!(!up.is_empty(), "the 2026-27 season is not finished");
        let round = up[0].round;
        assert!(up.iter().all(|f| f.round == round), "one round at a time");
        for f in &up {
            assert!(!w.played.contains_key(&(f.home, f.away)));
        }
        // Nothing earlier may remain unplayed.
        for f in &w.fixtures {
            if f.round < round {
                assert!(
                    w.played.contains_key(&(f.home, f.away)),
                    "round {} incomplete",
                    f.round
                );
            }
        }
    }

    #[test]
    fn upcoming_is_empty_for_a_fully_played_season() {
        let mut w = World::new();
        for f in w.fixtures.clone() {
            w.played.insert((f.home, f.away), (1, 1));
        }
        assert!(w.upcoming_matches().is_empty());
    }

    #[test]
    fn update_from_live_records_scraped_results() {
        let mut w = World::new();
        let f = w
            .fixtures
            .iter()
            .find(|f| !w.played.contains_key(&(f.home, f.away)))
            .copied()
            .expect("an unplayed fixture exists");
        let live = crate::scraper::LiveData {
            played_matches: vec![crate::scraper::ScrapedMatch {
                round: f.round,
                home: w.teams[f.home].clone(),
                home_score: 3,
                away: w.teams[f.away].clone(),
                away_score: 1,
            }],
            fetched_at: "2026-08-16T00:00:00Z".to_string(),
        };
        assert_eq!(w.update_from_live(&live), 1);
        assert_eq!(w.played.get(&(f.home, f.away)), Some(&(3, 1)));
    }

    #[test]
    fn update_from_live_drops_unrecognized_names_without_panicking() {
        let mut w = World::new();
        let before = w.played.len();
        let live = crate::scraper::LiveData {
            played_matches: vec![crate::scraper::ScrapedMatch {
                round: 1,
                home: "Atlantis FC".to_string(),
                home_score: 1,
                away: "Galatasaray".to_string(),
                away_score: 0,
            }],
            fetched_at: "2026-08-16T00:00:00Z".to_string(),
        };
        assert_eq!(w.update_from_live(&live), 0);
        assert_eq!(w.played.len(), before);
    }

    #[test]
    fn update_from_live_with_an_empty_scrape_keeps_the_baseline() {
        let mut w = World::new();
        let before = w.played.clone();
        let live = crate::scraper::LiveData {
            played_matches: Vec::new(),
            fetched_at: "2026-08-16T00:00:00Z".to_string(),
        };
        assert_eq!(w.update_from_live(&live), 0);
        assert_eq!(w.played, before, "an empty scrape must not erase results");
    }

    #[test]
    fn ensemble_builds_from_embedded_data_and_blends_lambdas() {
        let mut world = World::new();
        let gs = world.idx["Galatasaray"];
        let gaz = world.idx["Gaziantep"];
        let (elo_h, elo_a) = world.lam_pair(gs, gaz);

        let ens = Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("embedded ensemble");
        assert_eq!(ens.dc.n_teams, world.teams.len() + 1); // + Other Club bucket
        world.ensemble = Some(ens);

        let (mix_h, mix_a) = world.lam_pair(gs, gaz);
        // Strong side still favored, lambdas clamped and changed by the blend.
        assert!(mix_h > mix_a);
        assert!((0.15..=5.0).contains(&mix_h) && (0.15..=5.0).contains(&mix_a));
        assert!(
            (mix_h - elo_h).abs() > 1e-9 || (mix_a - elo_a).abs() > 1e-9,
            "blend should differ from pure Elo"
        );
    }

    #[test]
    fn elo_only_weights_reproduce_pure_elo_lambdas() {
        let mut world = World::new();
        let gs = world.idx["Galatasaray"];
        let fb = world.idx["Fenerbahçe"];
        let pure = world.lam_pair(gs, fb);
        world.ensemble = Some(Ensemble::from_embedded_data(1.0, 0.0, 0.0).expect("ensemble"));
        let weighted = world.lam_pair(gs, fb);
        assert!((pure.0 - weighted.0).abs() < 1e-12);
        assert!((pure.1 - weighted.1).abs() < 1e-12);
    }

    #[test]
    fn scoreline_sampling_uses_dc_joint_only_when_dc_weight_is_active() {
        let mut world = World::new();
        assert!(world.score_rho().is_none(), "pure Elo world: Poisson path");

        world.ensemble = Some(Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("ensemble"));
        let rho = world.score_rho().expect("DC active");
        assert!(rho < 0.0, "fitted rho should be negative: {rho}");

        // With DC active, sampled draw frequency of a tight match must track
        // the joint table (which inflates low-score draws for rho < 0).
        let gs = world.idx["Galatasaray"];
        let fb = world.idx["Fenerbahçe"];
        let (la, lb) = world.lam_pair(gs, fb);
        let table = crate::dixoncoles::score_table(la, lb, rho);
        let expected_p00 = table[0][0];
        let mut rng = SmallRng::seed_from_u64(3);
        let n = 100_000;
        let mut c00 = 0;
        for _ in 0..n {
            if world.sample_match_score(la, lb, &mut rng) == (0, 0) {
                c00 += 1;
            }
        }
        let p00 = c00 as f64 / n as f64;
        assert!(
            (p00 - expected_p00).abs() < 0.005,
            "sampled p00 {p00} vs table {expected_p00}"
        );

        // Zero DC weight switches back to the independent-Poisson path.
        world.ensemble = Some(Ensemble::from_embedded_data(0.8, 0.0, 0.2).expect("ensemble"));
        assert!(world.score_rho().is_none());
    }

    #[test]
    fn simulate_with_ensemble_is_deterministic() {
        let mut world = World::new();
        world.ensemble = Some(Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("ensemble"));
        let config = SimConfig {
            n_sims: 500,
            seed: 42,
            elo_overrides: HashMap::new(),
        };
        let r1 = world.simulate(&config);
        let r2 = world.simulate(&config);
        assert_eq!(r1.title_counts, r2.title_counts);
        assert_eq!(r1.position_counts, r2.position_counts);
        assert_eq!(r1.representative.order, r2.representative.order);
    }
}
