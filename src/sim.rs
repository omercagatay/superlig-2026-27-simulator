use std::collections::{BTreeMap, HashMap};

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

/// Today's date in Istanbul, ISO `YYYY-MM-DD`: the calendar's own time zone
/// (UTC+3 all year since Turkey dropped daylight saving in 2016), so the
/// next matchday turns over at local midnight rather than UTC's.
pub fn today_istanbul() -> String {
    let istanbul = chrono::FixedOffset::east_opt(3 * 3600).expect("UTC+3 is a valid offset");
    chrono::Utc::now()
        .with_timezone(&istanbul)
        .date_naive()
        .to_string()
}

#[derive(Clone)]
pub struct World {
    pub teams: Vec<String>,
    pub idx: HashMap<String, usize>,
    /// Current club ratings: the preseason baseline walked forward through
    /// every result played so far. Never mutate directly — set
    /// `elo_baseline` or `played` and call `refresh_ratings`.
    pub elo: Vec<f64>,
    /// Preseason ratings from `preseason_ratings()`, kept so in-season updates can be
    /// recomputed from scratch instead of compounding on themselves.
    pub elo_baseline: Vec<f64>,
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
/// `history::TeamIndex::league()` is built from the same `data::clubs()` order.
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
        let (pi_h, pi_a) = self.pi.lambdas(home, away, false);
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

// The model's per-match rules as free functions, so `World` and the
// walk-forward backtest (`examples/backtest.rs`) run the identical code.

/// Home side's in-season Elo change after one result; the away side gets the
/// negation.
pub fn elo_delta(home_elo: f64, away_elo: f64, home_goals: u16, away_goals: u16) -> f64 {
    elo_delta_k(data::ELO_K, home_elo, away_elo, home_goals, away_goals)
}

/// Preseason ratings: every result in `history` replayed in date order with
/// `k`, clubs entering at `new_club`. Production uses the embedded history
/// with `data::PRESEASON_K` (`preseason_ratings`); the backtest calls this
/// with the matches before each season it replays.
pub fn preseason_ratings_with(
    history: &[crate::history::HistoricalMatch],
    k: f64,
    new_club: f64,
) -> HashMap<String, f64> {
    let mut ordered: Vec<&crate::history::HistoricalMatch> = history.iter().collect();
    ordered.sort_by_key(|m| m.date);
    let mut ratings: HashMap<String, f64> = HashMap::new();
    for m in ordered {
        let rh = *ratings.entry(m.home_team.clone()).or_insert(new_club);
        let ra = *ratings.entry(m.away_team.clone()).or_insert(new_club);
        let d = elo_delta_k(k, rh, ra, m.home_score, m.away_score);
        *ratings.get_mut(&m.home_team).expect("inserted") += d;
        *ratings.get_mut(&m.away_team).expect("inserted") -= d;
    }
    ratings
}

/// This season's starting ratings for `data::clubs()`, in that order, from
/// the embedded results history.
pub fn preseason_ratings() -> Vec<f64> {
    static RATINGS: std::sync::OnceLock<Vec<f64>> = std::sync::OnceLock::new();
    RATINGS
        .get_or_init(|| {
            let history = crate::history::load_history_with_cutoff(crate::history::CUTOFF_YEAR);
            let r = preseason_ratings_with(&history, data::PRESEASON_K, data::NEW_CLUB_ELO);
            data::clubs()
                .iter()
                .map(|c| r.get(*c).copied().unwrap_or(data::NEW_CLUB_ELO))
                .collect()
        })
        .clone()
}

/// `elo_delta` with an explicit update rate.
pub fn elo_delta_k(k: f64, home_elo: f64, away_elo: f64, home_goals: u16, away_goals: u16) -> f64 {
    let dr = home_elo - away_elo + data::HOME_ADV;
    let expected = 1.0 / (1.0 + 10f64.powf(-dr / data::ELO_DIV));
    let actual = match home_goals.cmp(&away_goals) {
        std::cmp::Ordering::Greater => 1.0,
        std::cmp::Ordering::Equal => 0.5,
        std::cmp::Ordering::Less => 0.0,
    };
    // Margin of victory matters, with diminishing returns — a 4-0 is
    // stronger evidence than a 1-0, but not four times stronger.
    let margin = ((home_goals as f64 - away_goals as f64).abs())
        .max(1.0)
        .sqrt();
    k * margin * (actual - expected)
}

/// Expected goals `(λ_home, λ_away)` from the two clubs' Elo ratings.
pub fn elo_lambdas(home_elo: f64, away_elo: f64) -> (f64, f64) {
    let dr = home_elo - away_elo + data::HOME_ADV;
    (
        data::BASE * (10.0_f64).powf(dr / data::D_DIV),
        data::BASE * (10.0_f64).powf(-dr / data::D_DIV),
    )
}

/// Blend Elo λs with the ensemble's Dixon-Coles/pi λs (club indices in the
/// ensemble's own index), clamped to the range the score table supports.
pub fn blend_lambdas(
    (lh_elo, la_elo): (f64, f64),
    ensemble: Option<&Ensemble>,
    home: usize,
    away: usize,
) -> (f64, f64) {
    let ensemble_lam = ensemble.and_then(|e| e.lam_pair(home, away).map(|l| (e, l)));
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
    // Heavy underdogs score more than the blend predicts (backtest: 0.66
    // expected, 0.88 scored), so lift low λs part of the way to the pivot.
    let lift = |l: f64| {
        if l < data::UNDERDOG_PIVOT {
            l + data::UNDERDOG_LIFT * (data::UNDERDOG_PIVOT - l)
        } else {
            l
        }
    };
    // The favourite's λ follows the underdog's relative lift by FAV_FOLLOW,
    // so extra goals in mismatches need not all become upsets.
    let (lo, hi) = if lh < la { (lh, la) } else { (la, lh) };
    let lo_new = lift(lo);
    let hi_new = hi * (1.0 + data::FAV_FOLLOW * (lo_new / lo - 1.0));
    let (lh, la) = if lh < la {
        (lo_new, hi_new)
    } else {
        (hi_new, lo_new)
    };
    (lh.clamp(0.15, 5.0), la.clamp(0.15, 5.0))
}

/// ρ for the joint score table; `Some` only while Dixon-Coles carries weight.
pub fn score_rho(ensemble: Option<&Ensemble>) -> Option<f64> {
    ensemble.filter(|e| e.w_dc > 0.0).map(|e| e.dc.rho)
}

/// Exact outcome and market probabilities from λs, summed over the joint
/// scoreline table.
pub fn probs_from_lambdas(lh: f64, la: f64, rho: f64) -> FixtureProbs {
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
    pub top_two_counts: Vec<usize>,
    pub third_counts: Vec<usize>,
    pub fourth_counts: Vec<usize>,
    pub top_four_counts: Vec<usize>,
    pub relegation_counts: Vec<usize>,
    pub points_sum: Vec<f64>,
    pub gd_sum: Vec<f64>,
    pub won_sum: Vec<f64>,
    pub drawn_sum: Vec<f64>,
    pub gf_sum: Vec<f64>,
    pub ga_sum: Vec<f64>,
    /// `pairwise_above[a * n + b]` = trials where `a` finished above `b`.
    pub pairwise_above: Vec<usize>,
    /// `cutoff_point_counts[position][points]` is the number of trials where
    /// that points total occupied the position. A fixed-size histogram avoids
    /// retaining one points value per simulated season.
    pub cutoff_point_counts: Vec<Vec<usize>>,
    /// A deterministic sample season retained for diagnostics and tests.
    pub representative: SeasonResult,
}

/// Streaming reduction state. Keeping one of these per Rayon worker is a few
/// kilobytes; retaining every `SeasonResult` for a 200k-trial request was
/// roughly 150 MB and multiplied that cost across concurrent requests.
struct SimAccumulator {
    position_counts: Vec<Vec<usize>>,
    title_counts: Vec<usize>,
    top_two_counts: Vec<usize>,
    third_counts: Vec<usize>,
    fourth_counts: Vec<usize>,
    top_four_counts: Vec<usize>,
    relegation_counts: Vec<usize>,
    points_sum: Vec<f64>,
    gd_sum: Vec<f64>,
    won_sum: Vec<f64>,
    drawn_sum: Vec<f64>,
    gf_sum: Vec<f64>,
    ga_sum: Vec<f64>,
    pairwise_above: Vec<usize>,
    cutoff_point_counts: Vec<Vec<usize>>,
}

impl SimAccumulator {
    fn new(n: usize) -> Self {
        Self {
            position_counts: vec![vec![0; n]; n],
            title_counts: vec![0; n],
            top_two_counts: vec![0; n],
            third_counts: vec![0; n],
            fourth_counts: vec![0; n],
            top_four_counts: vec![0; n],
            relegation_counts: vec![0; n],
            points_sum: vec![0.0; n],
            gd_sum: vec![0.0; n],
            won_sum: vec![0.0; n],
            drawn_sum: vec![0.0; n],
            gf_sum: vec![0.0; n],
            ga_sum: vec![0.0; n],
            pairwise_above: vec![0; n * n],
            cutoff_point_counts: vec![vec![0; data::N_ROUNDS * 3 + 1]; n],
        }
    }

    fn record(&mut self, season: &SeasonResult) {
        let n = season.order.len();
        let relegation_from = n - data::RELEGATION_SPOTS;
        for (position, &club) in season.order.iter().enumerate() {
            self.position_counts[club][position] += 1;
            let points = usize::try_from(season.records[club].points)
                .expect("simulated league points are non-negative");
            self.cutoff_point_counts[position][points] += 1;
            if position == 0 {
                self.title_counts[club] += 1;
            }
            if position < data::TOP_TWO_PLACES {
                self.top_two_counts[club] += 1;
            }
            if position == 2 {
                self.third_counts[club] += 1;
            }
            if position == 3 {
                self.fourth_counts[club] += 1;
            }
            if position < data::TOP_FOUR_PLACES {
                self.top_four_counts[club] += 1;
            }
            if position >= relegation_from {
                self.relegation_counts[club] += 1;
            }
            for &below in &season.order[position + 1..] {
                self.pairwise_above[club * n + below] += 1;
            }
        }
        for club in 0..n {
            self.points_sum[club] += season.records[club].points as f64;
            self.gd_sum[club] += season.records[club].gd() as f64;
            self.won_sum[club] += season.records[club].won as f64;
            self.drawn_sum[club] += season.records[club].drawn as f64;
            self.gf_sum[club] += season.records[club].gf as f64;
            self.ga_sum[club] += season.records[club].ga as f64;
        }
    }

    fn merge(mut self, other: Self) -> Self {
        for (dst, src) in self.position_counts.iter_mut().zip(&other.position_counts) {
            for (dst, src) in dst.iter_mut().zip(src) {
                *dst += *src;
            }
        }
        for (dst, src) in self
            .cutoff_point_counts
            .iter_mut()
            .zip(&other.cutoff_point_counts)
        {
            for (dst, src) in dst.iter_mut().zip(src) {
                *dst += *src;
            }
        }
        macro_rules! add_vec {
            ($field:ident) => {
                for (dst, src) in self.$field.iter_mut().zip(&other.$field) {
                    *dst += *src;
                }
            };
        }
        add_vec!(title_counts);
        add_vec!(top_two_counts);
        add_vec!(third_counts);
        add_vec!(fourth_counts);
        add_vec!(top_four_counts);
        add_vec!(relegation_counts);
        add_vec!(points_sum);
        add_vec!(gd_sum);
        add_vec!(won_sum);
        add_vec!(drawn_sum);
        add_vec!(gf_sum);
        add_vec!(ga_sum);
        add_vec!(pairwise_above);
        self
    }
}

/// A result the user has pinned for an unplayed fixture. The *outcome* is
/// fixed, not the scoreline — the model still decides how the win happens,
/// which keeps goal difference honest instead of dictating 1-0 every time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForcedOutcome {
    Home,
    Draw,
    Away,
}

#[derive(Clone, Debug)]
pub struct SimConfig {
    pub n_sims: usize,
    pub seed: u64,
    pub elo_overrides: HashMap<String, f64>,
    /// "Suppose this happens": fixtures whose outcome is pinned, keyed
    /// `(home_idx, away_idx)`.
    pub forced: HashMap<(usize, usize), ForcedOutcome>,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            n_sims: 50000,
            seed: 12345,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
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
        let teams: Vec<String> = data::clubs().iter().map(|t| t.to_string()).collect();
        let idx: HashMap<String, usize> = teams
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i))
            .collect();
        let elo = preseason_ratings();

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

        let mut world = World {
            teams,
            idx,
            elo: elo.clone(),
            elo_baseline: elo,
            fixtures,
            dates,
            played,
            ensemble: None,
        };
        world.refresh_ratings();
        world
    }

    /// Replay every played result over the preseason ratings, in kick-off
    /// order, so club strength reflects the season so far rather than August.
    ///
    /// Recomputed from `elo_baseline` each time — calling this twice is the
    /// same as calling it once, which matters because `/api/refresh` runs on a
    /// timer and re-sends results already applied.
    pub fn refresh_ratings(&mut self) {
        self.elo.clone_from(&self.elo_baseline);

        let mut order: Vec<usize> = (0..self.fixtures.len()).collect();
        order.sort_by(|&a, &b| {
            (&self.dates[a].date, self.fixtures[a].round)
                .cmp(&(&self.dates[b].date, self.fixtures[b].round))
        });

        for i in order {
            let f = self.fixtures[i];
            let Some(&(hs, as_)) = self.played.get(&(f.home, f.away)) else {
                continue;
            };
            let delta = elo_delta(self.elo[f.home], self.elo[f.away], hs, as_);
            self.elo[f.home] += delta;
            self.elo[f.away] -= delta;
        }
    }

    /// Overlay scraped results onto the recorded set. Results only ever get
    /// added or corrected — an empty or partial scrape never erases the
    /// baseline, so a bad fetch degrades to "no new information".
    ///
    /// Elo ratings are not scraped directly: club ratings start at
    /// `preseason_ratings()` and are replayed through every recorded result.
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
            let score = (m.home_score, m.away_score);
            if self.played.get(&(home, away)) != Some(&score) {
                self.played.insert((home, away), score);
                applied += 1;
            }
        }
        if applied > 0 {
            self.refresh_ratings();
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
        self.lam_pair_with(&self.elo, home, away)
    }

    /// As `lam_pair`, but against a supplied rating vector — used by
    /// `simulate_one`, which draws a season's ratings from their uncertainty.
    fn lam_pair_with(&self, elo: &[f64], home: usize, away: usize) -> (f64, f64) {
        blend_lambdas(
            elo_lambdas(elo[home], elo[away]),
            self.ensemble.as_ref(),
            home,
            away,
        )
    }

    fn sample_poisson(rng: &mut SmallRng, lambda: f64) -> i64 {
        let dist = Poisson::new(lambda).unwrap();
        rng.sample(dist) as i64
    }

    /// ρ for joint-scoreline sampling; `Some` only while the Dixon-Coles
    /// component carries weight, so `ENSEMBLE_WEIGHTS=1,0,0` (and plain
    /// `World::new()`) keep the independent-Poisson behavior.
    fn score_rho(&self) -> Option<f64> {
        score_rho(self.ensemble.as_ref())
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
        probs_from_lambdas(lh, la, self.score_rho().unwrap_or(0.0))
    }

    /// The league's "next matchday" as of `today` (ISO `YYYY-MM-DD`): the
    /// round being played on the date of the earliest unplayed fixture on or
    /// after `today`.
    /// Keyed on dates rather than on the lowest incomplete round, so one
    /// postponed match from an earlier round does not pin the calendar. With
    /// nothing unplayed dated today or later, the earliest round that still
    /// has an unplayed fixture; with the season complete, the last round.
    ///
    /// Two rounds on that date means a rescheduled match landed on a
    /// matchday. Every round with a fixture that day, played or not, and a
    /// game still to come from `today` on is in the running; the one with
    /// more fixtures that day is the one being played, the later round on a
    /// tie, since a match moved onto a matchday is nearly always a postponed
    /// one from an earlier round (one brought forward from a later round
    /// would take the tie). Looking only at unplayed games would
    /// hand the day to the rescheduled match as soon as the matchday's games
    /// that day were in, though the matchday carries on tomorrow. A round
    /// stays in the running only with a game still to come that day or
    /// before the next round starts, so its own game postponed for weeks
    /// does not hold the day. A rescheduled match alone on its date is
    /// simply the next fixture.
    pub fn next_round_on(&self, today: &str) -> u8 {
        let unplayed: Vec<usize> = (0..self.fixtures.len())
            .filter(|&i| {
                let f = self.fixtures[i];
                !self.played.contains_key(&(f.home, f.away))
            })
            .collect();
        let to_come: Vec<usize> = unplayed
            .iter()
            .copied()
            .filter(|&i| self.dates[i].date.as_str() >= today)
            .collect();
        let next_date = to_come.iter().map(|&i| self.dates[i].date.as_str()).min();
        match next_date {
            Some(date) => {
                let mut per_round: BTreeMap<u8, usize> = BTreeMap::new();
                for (i, f) in self.fixtures.iter().enumerate() {
                    if self.dates[i].date == date {
                        *per_round.entry(f.round).or_default() += 1;
                    }
                }
                // A round's own matchday ends where the next round starts; a
                // game of it postponed past that does not keep it running.
                let next_round_start = |round: u8| {
                    (0..self.fixtures.len())
                        .filter(|&i| self.fixtures[i].round == round + 1)
                        .map(|i| self.dates[i].date.as_str())
                        .min()
                };
                let still_running = |round: u8| {
                    let ends = next_round_start(round);
                    to_come.iter().any(|&i| {
                        let d = self.dates[i].date.as_str();
                        self.fixtures[i].round == round
                            && (d == date || ends.is_none_or(|end| d < end))
                    })
                };
                per_round
                    .into_iter()
                    .filter(|&(round, _)| still_running(round))
                    .max_by_key(|&(round, count)| (count, round))
                    .map(|(round, _)| round)
                    .expect("the date came from a fixture still to come")
            }
            None => unplayed
                .iter()
                .map(|&i| self.fixtures[i].round)
                .min()
                .unwrap_or(data::N_ROUNDS as u8),
        }
    }

    /// Unplayed fixtures of the next matchday (`next_round_on`), as of
    /// today in Istanbul; empty once the season is complete.
    pub fn upcoming_matches(&self) -> Vec<LeagueFixture> {
        self.upcoming_matches_on(&today_istanbul())
    }

    /// As `upcoming_matches`, as of a given ISO date.
    pub fn upcoming_matches_on(&self, today: &str) -> Vec<LeagueFixture> {
        let round = self.next_round_on(today);
        self.fixtures
            .iter()
            .filter(|f| f.round == round && !self.played.contains_key(&(f.home, f.away)))
            .copied()
            .collect()
    }

    pub fn simulate_one(&self, rng: &mut SmallRng) -> SeasonResult {
        self.simulate_one_with(rng, &HashMap::new())
    }

    /// As `simulate_one`, with a set of pinned outcomes applied.
    pub fn simulate_one_with(
        &self,
        rng: &mut SmallRng,
        forced: &HashMap<(usize, usize), ForcedOutcome>,
    ) -> SeasonResult {
        let n = self.teams.len();

        // Each trial commits to one draw of what the clubs are actually
        // worth, held fixed for the whole season. Drawn per trial rather than
        // per match: a club's true strength does not resample every weekend,
        // and per-match noise would wash out in the aggregate instead of
        // widening the season-outcome distribution the way real uncertainty
        // does.
        let season_elo: Vec<f64> = if data::RATING_SIGMA > 0.0 {
            let normal = rand_distr::Normal::new(0.0, data::RATING_SIGMA)
                .expect("RATING_SIGMA is finite and non-negative");
            self.elo.iter().map(|e| e + rng.sample(normal)).collect()
        } else {
            self.elo.clone()
        };

        let mut records = vec![TeamRecord::default(); n];
        let mut results: HashMap<(usize, usize), (i64, i64)> =
            HashMap::with_capacity(self.fixtures.len());

        for f in &self.fixtures {
            let (hg, ag) = match self.played.get(&(f.home, f.away)) {
                Some(&(hs, as_)) => (hs as i64, as_ as i64),
                None => {
                    let (lh, la) = self.lam_pair_with(&season_elo, f.home, f.away);
                    match forced.get(&(f.home, f.away)) {
                        None => self.sample_match_score(lh, la, rng),
                        // Keep drawing until the scoreline agrees with the
                        // pinned outcome: the result is fixed, the way it
                        // arrives is still the model's. Bounded so a very
                        // unlikely pin cannot stall a trial; the fallback is
                        // the narrowest scoreline that satisfies it.
                        Some(&want) => {
                            let mut drawn = None;
                            for _ in 0..64 {
                                let (hg, ag) = self.sample_match_score(lh, la, rng);
                                let got = match hg.cmp(&ag) {
                                    std::cmp::Ordering::Greater => ForcedOutcome::Home,
                                    std::cmp::Ordering::Equal => ForcedOutcome::Draw,
                                    std::cmp::Ordering::Less => ForcedOutcome::Away,
                                };
                                if got == want {
                                    drawn = Some((hg, ag));
                                    break;
                                }
                            }
                            drawn.unwrap_or(match want {
                                ForcedOutcome::Home => (1, 0),
                                ForcedOutcome::Draw => (1, 1),
                                ForcedOutcome::Away => (0, 1),
                            })
                        }
                    }
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
        assert!(n_sims > 0, "simulation requires at least one trial");

        let totals = (0..n_sims)
            .into_par_iter()
            .fold(
                || SimAccumulator::new(n),
                |mut totals, i| {
                    let mut rng =
                        SmallRng::seed_from_u64(config.seed.wrapping_add(i as u64 * 2_654_435_761));
                    let season = self.simulate_one_with(&mut rng, &config.forced);
                    totals.record(&season);
                    totals
                },
            )
            .reduce(|| SimAccumulator::new(n), SimAccumulator::merge);

        // Keep one deterministic diagnostic sample without retaining every
        // trial. Aggregate API output is built exclusively from `totals`.
        let representative = {
            let mut rng = SmallRng::seed_from_u64(config.seed.wrapping_add(0x9E37_79B9_7F4A_7C15));
            self.simulate_one_with(&mut rng, &config.forced)
        };

        SimResults {
            n_sims,
            position_counts: totals.position_counts,
            title_counts: totals.title_counts,
            top_two_counts: totals.top_two_counts,
            third_counts: totals.third_counts,
            fourth_counts: totals.fourth_counts,
            top_four_counts: totals.top_four_counts,
            relegation_counts: totals.relegation_counts,
            points_sum: totals.points_sum,
            gd_sum: totals.gd_sum,
            won_sum: totals.won_sum,
            drawn_sum: totals.drawn_sum,
            gf_sum: totals.gf_sum,
            ga_sum: totals.ga_sum,
            pairwise_above: totals.pairwise_above,
            cutoff_point_counts: totals.cutoff_point_counts,
            representative,
        }
    }
}

/// Seven matchdays' worth of fixtures: a part-played season for tests.
#[cfg(test)]
pub(crate) const MID_SEASON: usize = 7 * 9;

#[cfg(test)]
impl World {
    /// The shipped calendar with its recorded results replaced by the first
    /// `n` fixtures in kick-off order, each given a fixed made-up scoreline.
    /// Tests build their season state with this rather than reading whatever
    /// the weekly refresh last committed, so they behave the same on
    /// matchday 1, matchday 34 and every week in between.
    pub(crate) fn with_played(n: usize) -> Self {
        const SCORES: [(u16, u16); 7] = [(2, 1), (1, 1), (0, 1), (3, 0), (1, 0), (2, 2), (0, 2)];
        let mut w = World::new();
        w.played.clear();
        let mut order: Vec<usize> = (0..w.fixtures.len()).collect();
        order.sort_by(|&a, &b| {
            (&w.dates[a].date, w.fixtures[a].round).cmp(&(&w.dates[b].date, w.fixtures[b].round))
        });
        for (k, &i) in order.iter().take(n).enumerate() {
            let f = w.fixtures[i];
            w.played.insert((f.home, f.away), SCORES[k % SCORES.len()]);
        }
        w.refresh_ratings();
        w
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
        let mut recorded = 0;
        for f in data::fixtures() {
            if let (Some(hs), Some(as_)) = (f.home_score, f.away_score) {
                let key = (w.idx[&f.home], w.idx[&f.away]);
                assert_eq!(w.played.get(&key), Some(&(hs, as_)));
                recorded += 1;
            }
        }
        assert_eq!(w.played.len(), recorded, "nothing unrecorded is played");
    }

    #[test]
    fn home_advantage_applies_to_the_fixture_home_side_not_a_team_flag() {
        let w = World::with_played(0);
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
    fn ratings_move_with_results_and_stay_zero_sum() {
        let mut w = World::with_played(0);
        assert_eq!(w.elo, w.elo_baseline, "no results, no movement");

        let (gaz, gs) = (w.idx["Gaziantep"], w.idx["Galatasaray"]);
        let f = *w
            .fixtures
            .iter()
            .find(|f| f.home == gaz && f.away == gs)
            .expect("Gaziantep host Galatasaray once");
        w.played.insert((f.home, f.away), (4, 0));
        w.refresh_ratings();

        // A big upset lifts the winner and drops the loser...
        assert!(w.elo[gaz] > w.elo_baseline[gaz]);
        assert!(w.elo[gs] < w.elo_baseline[gs]);
        // ...by the same amount: rating is only ever transferred.
        let moved = w.elo[gaz] - w.elo_baseline[gaz];
        assert!((moved + (w.elo[gs] - w.elo_baseline[gs])).abs() < 1e-9);
        // An underdog beating a favourite 4-0 is worth a real chunk of rating.
        assert!(moved > 20.0, "upset moved only {moved}");

        let total_before: f64 = w.elo_baseline.iter().sum();
        let total_after: f64 = w.elo.iter().sum();
        assert!((total_before - total_after).abs() < 1e-9);
    }

    #[test]
    fn refreshing_ratings_twice_changes_nothing() {
        let mut w = World::with_played(MID_SEASON);
        let once = w.elo.clone();
        assert_ne!(once, w.elo_baseline, "the recorded results moved ratings");
        w.refresh_ratings();
        assert_eq!(w.elo, once, "recompute must not compound");

        // The same holds through the live path, which re-sends known results.
        // First use a genuinely unplayed fixture so the test also proves that
        // the initial live update moves ratings.
        let f = w
            .fixtures
            .iter()
            .find(|f| !w.played.contains_key(&(f.home, f.away)))
            .copied()
            .expect("a part-played season has unplayed fixtures");
        let live = crate::scraper::LiveData {
            played_matches: vec![crate::scraper::ScrapedMatch {
                round: f.round,
                home: w.teams[f.home].clone(),
                home_score: 5,
                away: w.teams[f.away].clone(),
                away_score: 0,
            }],
            fetched_at: "2026-08-17T00:00:00Z".to_string(),
        };
        w.update_from_live(&live);
        let after_first = w.elo.clone();
        assert_ne!(after_first, once, "a new live result must update ratings");
        w.update_from_live(&live);
        assert_eq!(w.elo, after_first, "re-applying a result must be a no-op");
    }

    #[test]
    fn matchday_one_results_are_already_reflected_in_ratings() {
        // A fresh World must not still be sitting on preseason ratings: it
        // replays whatever results the shipped calendar has recorded.
        let w = World::new();
        let mut replayed = w.clone();
        replayed.refresh_ratings();
        assert_eq!(w.elo, replayed.elo, "World::new replays recorded results");

        let w = World::with_played(9);
        assert_ne!(w.elo, w.elo_baseline, "ratings must reflect results played");
        for (i, r) in w.elo.iter().enumerate() {
            assert!(
                (1000.0..=2200.0).contains(r),
                "{} drifted to {r}",
                w.teams[i]
            );
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
        let w = World::with_played(MID_SEASON);
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
        let w = World::with_played(MID_SEASON);
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

    /// A pinned outcome must actually bind — every trial, not most of them —
    /// and must move the aggregate odds in the direction it implies.
    #[test]
    fn pinned_outcomes_bind_and_move_the_forecast() {
        let w = World::with_played(0);
        let (gaz, gs) = (w.idx["Gaziantep"], w.idx["Galatasaray"]);
        assert!(
            w.fixtures.iter().any(|f| f.home == gaz && f.away == gs),
            "Gaziantep host Galatasaray"
        );

        let mut forced = HashMap::new();
        forced.insert((gaz, gs), ForcedOutcome::Home);

        // Every single trial respects the pin.
        for seed in 0..25u64 {
            let r = w.simulate_one_with(&mut SmallRng::seed_from_u64(seed), &forced);
            assert!(
                r.records[gaz].won >= 1,
                "seed {seed}: pinned home win did not happen"
            );
        }

        let base = SimConfig {
            n_sims: 3000,
            seed: 5,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
        };
        let pinned = SimConfig {
            forced: forced.clone(),
            ..base.clone()
        };
        let r0 = w.simulate(&base);
        let r1 = w.simulate(&pinned);
        assert!(
            r1.points_sum[gaz] > r0.points_sum[gaz],
            "a guaranteed win must raise expected points"
        );
        assert!(r1.points_sum[gs] < r0.points_sum[gs]);
        assert!(
            r1.relegation_counts[gaz] < r0.relegation_counts[gaz],
            "and lower relegation risk"
        );
    }

    /// Pinning the outcome must not dictate the scoreline: the model still
    /// produces a spread of winning margins.
    #[test]
    fn pinned_outcomes_leave_the_scoreline_to_the_model() {
        let w = World::with_played(0);
        let (gaz, gs) = (w.idx["Gaziantep"], w.idx["Galatasaray"]);
        let mut forced = HashMap::new();
        forced.insert((gaz, gs), ForcedOutcome::Home);

        let mut margins = std::collections::HashSet::new();
        for seed in 0..40u64 {
            let r = w.simulate_one_with(&mut SmallRng::seed_from_u64(seed), &forced);
            margins.insert(r.records[gaz].gf - r.records[gaz].ga);
        }
        assert!(
            margins.len() > 5,
            "forced wins collapsed onto one scoreline: {margins:?}"
        );
    }

    #[test]
    fn simulate_is_deterministic_for_same_seed() {
        let w = World::with_played(MID_SEASON);
        let cfg = SimConfig {
            n_sims: 500,
            seed: 999,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
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
        let w = World::with_played(MID_SEASON);
        let cfg = SimConfig {
            n_sims: 400,
            seed: 3,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
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
        let w = World::with_played(MID_SEASON);
        let cfg = SimConfig {
            n_sims: 400,
            seed: 11,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
        };
        let r = w.simulate(&cfg);
        for club in 0..data::N_TEAMS {
            let d = &r.position_counts[club];
            assert_eq!(r.title_counts[club], d[0]);
            assert_eq!(r.top_two_counts[club], d[0] + d[1]);
            assert_eq!(r.third_counts[club], d[2]);
            assert_eq!(r.fourth_counts[club], d[3]);
            assert_eq!(
                r.top_four_counts[club],
                d[..data::TOP_FOUR_PLACES].iter().sum::<usize>()
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
            r.top_two_counts.iter().sum::<usize>(),
            cfg.n_sims * data::TOP_TWO_PLACES
        );
        assert_eq!(r.third_counts.iter().sum::<usize>(), cfg.n_sims);
        assert_eq!(r.fourth_counts.iter().sum::<usize>(), cfg.n_sims);
        assert_eq!(
            r.top_four_counts.iter().sum::<usize>(),
            cfg.n_sims * data::TOP_FOUR_PLACES
        );
        assert_eq!(
            r.relegation_counts.iter().sum::<usize>(),
            cfg.n_sims * data::RELEGATION_SPOTS
        );
    }

    #[test]
    fn pairwise_above_is_complementary() {
        let w = World::with_played(MID_SEASON);
        let cfg = SimConfig {
            n_sims: 300,
            seed: 5,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
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

    /// Rating uncertainty must widen season outcomes without breaking the
    /// ordering: the favourite stays the favourite, but is less of a lock.
    /// (`RATING_SIGMA > 0` is what makes this a real test; if it is ever set
    /// to zero the bounds below simply become a plain over-confidence check.)
    #[test]
    fn rating_uncertainty_widens_the_title_distribution() {
        let w = World::with_played(0);
        let cfg = SimConfig {
            n_sims: 4000,
            seed: 77,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
        };
        let r = w.simulate(&cfg);
        let gs = w.idx["Galatasaray"];
        let gaz = w.idx["Gaziantep"];

        // Ordering survives.
        assert!(r.title_counts[gs] > r.title_counts[gaz]);

        // The favourite is short of certainty and the field keeps real
        // chances: an over-confident model would push these to the extremes.
        let fav = r.title_counts[gs] as f64 / cfg.n_sims as f64;
        assert!((0.2..0.75).contains(&fav), "favourite title share {fav}");
        let with_a_chance = (0..data::N_TEAMS)
            .filter(|&c| r.title_counts[c] > 0)
            .count();
        assert!(with_a_chance >= 6, "only {with_a_chance} clubs ever win");
    }

    #[test]
    fn stronger_clubs_win_the_title_more_often() {
        let w = World::with_played(0);
        let cfg = SimConfig {
            n_sims: 2000,
            seed: 1,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
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
        let w = World::with_played(0);
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

    /// Day `offset` of matchday `round` in `weekly_calendar`.
    fn day(round: u8, offset: u64) -> String {
        let start = chrono::NaiveDate::from_ymd_opt(2030, 1, 7).expect("valid date");
        (start + chrono::Days::new(7 * (round as u64 - 1) + offset)).to_string()
    }

    /// Give every fixture a date the test owns: matchday `r` spread over
    /// days 0-2 of week `r`, so the next-matchday rule is checked against
    /// neither TFF's current dates nor today's.
    fn weekly_calendar(w: &mut World) {
        let mut seen = [0u64; data::N_ROUNDS + 1];
        for (i, f) in w.fixtures.iter().enumerate() {
            let k = &mut seen[f.round as usize];
            w.dates[i].date = day(f.round, *k % 3);
            *k += 1;
        }
    }

    fn record(w: &mut World, i: usize) {
        let f = w.fixtures[i];
        w.played.insert((f.home, f.away), (1, 0));
    }

    /// Fixture indices of `round` dated `date`.
    fn fixtures_on(w: &World, round: u8, date: &str) -> Vec<usize> {
        (0..w.fixtures.len())
            .filter(|&i| w.fixtures[i].round == round && w.dates[i].date == date)
            .collect()
    }

    fn season_through(round: u8) -> World {
        let mut w = World::with_played(0);
        weekly_calendar(&mut w);
        for i in 0..w.fixtures.len() {
            if w.fixtures[i].round <= round {
                record(&mut w, i);
            }
        }
        w
    }

    #[test]
    fn today_in_istanbul_is_an_iso_date() {
        let today = today_istanbul();
        assert_eq!(today.len(), 10, "{today}");
        assert!(chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d").is_ok());
    }

    #[test]
    fn before_the_season_the_opening_round_is_next() {
        let mut w = World::with_played(0);
        weekly_calendar(&mut w);
        let summer = "2029-07-01";
        assert_eq!(w.next_round_on(summer), 1);
        assert_eq!(w.upcoming_matches_on(summer).len(), 9);
    }

    #[test]
    fn next_matchday_follows_the_calendar_not_a_postponed_match() {
        let mut w = season_through(7);
        let today = day(7, 4);
        assert_eq!(w.next_round_on(&today), 8);
        let up = w.upcoming_matches_on(&today);
        assert_eq!(up.len(), 9, "the whole of matchday 8");
        assert!(up.iter().all(|f| f.round == 8), "one round at a time");

        // A matchday-5 game postponed and still unplayed, dated in the past,
        // does not drag the next matchday back to round 5...
        let i = fixtures_on(&w, 5, &day(5, 0))[0];
        let f = w.fixtures[i];
        w.played.remove(&(f.home, f.away));
        assert_eq!(w.next_round_on(&today), 8, "past postponement pinned it");
        assert!(w.upcoming_matches_on(&today).iter().all(|f| f.round == 8));

        // ...nor once it is re-dated after the coming matchday...
        w.dates[i].date = day(8, 3);
        assert_eq!(
            w.next_round_on(&today),
            8,
            "re-dated postponement pinned it"
        );

        // ...nor when it lands on a day matchday 8 is also playing, even
        // once that day's earlier matchday-8 games are in.
        let matchday = day(8, 0);
        w.dates[i].date = matchday.clone();
        assert_eq!(
            w.next_round_on(&today),
            8,
            "the busier round is the matchday"
        );
        let earlier_games = fixtures_on(&w, 8, &matchday);
        for &j in &earlier_games[..2] {
            record(&mut w, j);
        }
        assert_eq!(
            w.next_round_on(&matchday),
            8,
            "the day's played games still count"
        );
        for &j in &earlier_games[..2] {
            let g = w.fixtures[j];
            w.played.remove(&(g.home, g.away));
        }

        // Re-dated to midweek, before the matchday, it is the next fixture.
        w.dates[i].date = day(7, 5);
        assert_eq!(w.next_round_on(&today), 5);
        let up = w.upcoming_matches_on(&today);
        assert_eq!(up.len(), 1);
        assert_eq!((up[0].home, up[0].away), (f.home, f.away));
    }

    /// Matchday 7 complete, one matchday-5 game postponed and unplayed:
    /// the world and that game's fixture index.
    fn season_with_a_postponed_round_5_game() -> (World, usize) {
        let mut w = season_through(7);
        let i = fixtures_on(&w, 5, &day(5, 0))[0];
        let f = w.fixtures[i];
        w.played.remove(&(f.home, f.away));
        (w, i)
    }

    #[test]
    fn a_rescheduled_match_does_not_take_over_a_matchday_still_running() {
        let (mut w, i) = season_with_a_postponed_round_5_game();
        // It lands on matchday 8's first day, and every matchday-8 game that
        // day is in before it kicks off; matchday 8 has two days to go.
        let matchday = day(8, 0);
        w.dates[i].date = matchday.clone();
        for j in fixtures_on(&w, 8, &matchday) {
            record(&mut w, j);
        }
        assert_eq!(
            w.next_round_on(&matchday),
            8,
            "the rescheduled match took over"
        );
        let up = w.upcoming_matches_on(&matchday);
        assert_eq!(up.len(), 6, "matchday 8's last two days");
        assert!(up.iter().all(|f| f.round == 8));
    }

    #[test]
    fn a_matchday_opener_beats_a_rescheduled_match_on_the_same_day() {
        let (mut w, i) = season_with_a_postponed_round_5_game();
        // Matchday 8 opens with a single game the day before its main days,
        // and the rescheduled match is played that day too: one game each,
        // and the regular matchday is the later round.
        let opening_day = day(7, 6);
        let opener = fixtures_on(&w, 8, &day(8, 0))[0];
        w.dates[opener].date = opening_day.clone();
        w.dates[i].date = opening_day.clone();
        for today in [day(7, 4), opening_day.clone()] {
            assert_eq!(w.next_round_on(&today), 8, "as of {today}");
            assert_eq!(w.upcoming_matches_on(&today).len(), 9);
        }

        // The opener in, the rescheduled match still to come that evening:
        // matchday 8 carries on tomorrow, so it is still the matchday.
        record(&mut w, opener);
        assert_eq!(w.next_round_on(&opening_day), 8);
        assert_eq!(w.upcoming_matches_on(&opening_day).len(), 8);
    }

    #[test]
    fn a_rescheduled_match_is_next_once_the_matchday_sharing_its_day_is_over() {
        let (mut w, i) = season_with_a_postponed_round_5_game();
        // It lands on matchday 8's last day, after that day's matchday games.
        let last_day = day(8, 2);
        w.dates[i].date = last_day.clone();
        let earlier_days: Vec<usize> = [day(8, 0), day(8, 1)]
            .iter()
            .flat_map(|d| fixtures_on(&w, 8, d))
            .collect();
        for j in earlier_days {
            record(&mut w, j);
        }
        assert_eq!(w.next_round_on(&last_day), 8);

        // Matchday 8 over, the rescheduled match still to come that evening:
        // a finished matchday must not hold the day with nothing to list.
        for j in fixtures_on(&w, 8, &last_day) {
            record(&mut w, j);
        }
        assert_eq!(w.next_round_on(&last_day), 5, "a finished matchday won");
        let up = w.upcoming_matches_on(&last_day);
        assert_eq!(up.len(), 1);
        assert_eq!(
            (up[0].home, up[0].away),
            (w.fixtures[i].home, w.fixtures[i].away)
        );
        record(&mut w, i);
        assert_eq!(w.next_round_on(&last_day), 9);
    }

    #[test]
    fn a_matchday_game_postponed_for_weeks_does_not_hold_a_shared_day() {
        let (mut w, i) = season_with_a_postponed_round_5_game();
        let last_day = day(8, 2);
        w.dates[i].date = last_day.clone();
        // One matchday-8 game moves out past matchday 9's start; every
        // other matchday-8 game, that day included, is in.
        let far = fixtures_on(&w, 8, &day(8, 0))[0];
        w.dates[far].date = day(12, 1);
        let rest: Vec<usize> = (0..w.fixtures.len())
            .filter(|&j| w.fixtures[j].round == 8 && j != far)
            .collect();
        for j in rest {
            record(&mut w, j);
        }
        assert_eq!(w.next_round_on(&last_day), 5, "a far-off game held the day");
        let up = w.upcoming_matches_on(&last_day);
        assert_eq!(up.len(), 1);
        assert_eq!(up[0].round, 5);
    }

    #[test]
    fn a_round_in_progress_stays_the_next_matchday() {
        let mut w = season_through(7);
        for i in fixtures_on(&w, 8, &day(8, 0)) {
            record(&mut w, i);
        }
        // Mid-afternoon on day 1: one of today's games played, two still to
        // come today, three tomorrow.
        let today = day(8, 1);
        let todays = fixtures_on(&w, 8, &today);
        record(&mut w, todays[0]);
        assert_eq!(w.next_round_on(&today), 8);
        let up = w.upcoming_matches_on(&today);
        assert_eq!(up.len(), 5);
        assert!(up.iter().all(|f| f.round == 8));

        // Today's games all in, the rest tomorrow: still matchday 8.
        for &i in &todays {
            record(&mut w, i);
        }
        assert_eq!(w.next_round_on(&today), 8);

        // Last day, last game still to come: matchday 8 until it is in.
        let last_day = day(8, 2);
        let lasts = fixtures_on(&w, 8, &last_day);
        for &i in &lasts[1..] {
            record(&mut w, i);
        }
        assert_eq!(w.next_round_on(&last_day), 8);
        record(&mut w, lasts[0]);
        assert_eq!(w.next_round_on(&last_day), 9);
    }

    #[test]
    fn with_only_overdue_fixtures_left_the_earliest_incomplete_round_is_next() {
        let mut w = season_through(34);
        let late = fixtures_on(&w, 30, &day(30, 1))[0];
        let early = fixtures_on(&w, 20, &day(20, 2))[0];
        for i in [late, early] {
            let f = w.fixtures[i];
            w.played.remove(&(f.home, f.away));
        }
        let after_the_season = day(34, 10);
        assert_eq!(w.next_round_on(&after_the_season), 20);
        let up = w.upcoming_matches_on(&after_the_season);
        assert_eq!(up.len(), 1);
        assert_eq!(up[0].round, 20);
    }

    #[test]
    fn upcoming_is_empty_for_a_fully_played_season() {
        let mut w = World::new();
        for f in w.fixtures.clone() {
            w.played.insert((f.home, f.away), (1, 1));
        }
        assert!(w.upcoming_matches().is_empty());
        for today in ["2026-07-01", "2026-10-18", "2027-07-01"] {
            assert!(w.upcoming_matches_on(today).is_empty());
            assert_eq!(w.next_round_on(today), data::N_ROUNDS as u8, "last round");
        }
    }

    #[test]
    fn update_from_live_records_scraped_results() {
        let mut w = World::with_played(MID_SEASON);
        let f = w
            .fixtures
            .iter()
            .find(|f| !w.played.contains_key(&(f.home, f.away)))
            .copied()
            .expect("a part-played season has unplayed fixtures");
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
        let mut w = World::with_played(MID_SEASON);
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
        let mut world = World::with_played(0);
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
        // Preseason ratings fix the λs, so the sampled frequency below is
        // the same draw whatever the season has reached.
        let mut world = World::with_played(0);
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
        let mut world = World::with_played(MID_SEASON);
        world.ensemble = Some(Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("ensemble"));
        let config = SimConfig {
            n_sims: 500,
            seed: 42,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
        };
        let r1 = world.simulate(&config);
        let r2 = world.simulate(&config);
        assert_eq!(r1.title_counts, r2.title_counts);
        assert_eq!(r1.position_counts, r2.position_counts);
        assert_eq!(r1.representative.order, r2.representative.order);
    }
}
