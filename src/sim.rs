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
    /// Per-club strength offsets, in Elo points, acting on the whole model:
    /// they move the blended, lifted λ pair as an Elo gap moves the Elo
    /// component (`offset_lambdas`), in `fixture_probs` and in every
    /// simulated match. Zero unless `apply_overrides` set them.
    pub offset: Vec<f64>,
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

/// A λ pair moved by a strength gap of `gap` Elo points (home minus away)
/// on the whole model, as an Elo gap moves `elo_lambdas`: `λ_home ·
/// 10^(gap/D_DIV)` and `λ_away · 10^(−gap/D_DIV)`, both divided by `scale`
/// and clamped as `blend_lambdas` clamps. No gap and a unit scale return the
/// pair untouched, bit for bit.
fn offset_lambdas((lh, la): (f64, f64), gap: f64, scale: f64) -> (f64, f64) {
    if gap == 0.0 && scale == 1.0 {
        return (lh, la);
    }
    (
        (lh * 10f64.powf(gap / data::D_DIV) / scale).clamp(0.15, 5.0),
        (la * 10f64.powf(-gap / data::D_DIV) / scale).clamp(0.15, 5.0),
    )
}

/// E[10^(Δ/D_DIV)], Δ the difference of two independent N(0, `var`)
/// offsets: exp((ln 10 / D_DIV)² · var). A random gap raises the mean of
/// each λ by this factor (10^x is convex), so dividing by it keeps a random
/// shock from changing the league's scoring rate.
fn shock_inflation(var: f64) -> f64 {
    let c = std::f64::consts::LN_10 / data::D_DIV;
    (c * c * var).exp()
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
/// fixed, not the scoreline — the model still decides how the win happens
/// (`pinned_score`), which keeps goal difference honest instead of
/// dictating 1-0 every time. In a simulated season the pinned result then
/// moves the trial's Elo ratings as the same real result would.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForcedOutcome {
    Home,
    Draw,
    Away,
}

/// `dixoncoles::score_table`'s joint scoreline probabilities.
type ScoreTable = [[f64; crate::dixoncoles::MAX_GOALS + 1]; crate::dixoncoles::MAX_GOALS + 1];

impl ForcedOutcome {
    /// The outcome a scoreline is.
    fn of(home_goals: usize, away_goals: usize) -> Self {
        match home_goals.cmp(&away_goals) {
            std::cmp::Ordering::Greater => ForcedOutcome::Home,
            std::cmp::Ordering::Equal => ForcedOutcome::Draw,
            std::cmp::Ordering::Less => ForcedOutcome::Away,
        }
    }
}

/// A scoreline for a pinned fixture: the joint score table
/// (`dixoncoles::score_table`) restricted to `want`'s cells and
/// renormalized, drawn by inverse CDF with one uniform `u` in `[0, 1)`. The
/// exact conditional distribution, with no rejection loop to bound.
pub fn pinned_score(lh: f64, la: f64, rho: f64, want: ForcedOutcome, u: f64) -> (u16, u16) {
    conditional_cell(&crate::dixoncoles::score_table(lh, la, rho), want, u)
}

/// The inverse-CDF draw of `pinned_score` on a given table.
fn conditional_cell(table: &ScoreTable, want: ForcedOutcome, u: f64) -> (u16, u16) {
    let cells = || {
        table.iter().enumerate().flat_map(move |(x, row)| {
            row.iter()
                .enumerate()
                .filter(move |&(y, _)| ForcedOutcome::of(x, y) == want)
                .map(move |(y, &p)| (x, y, p))
        })
    };
    let target = u * cells().map(|(_, _, p)| p).sum::<f64>();
    let mut acc = 0.0;
    let mut last = (0, 0);
    for (x, y, p) in cells() {
        acc += p;
        last = (x, y);
        if target < acc {
            break;
        }
    }
    // Every outcome has cells in the table, so `last` is always one of
    // `want`'s; rounding can carry `target` to the end of them.
    let goals = |g: usize| u16::try_from(g).expect("the score table is MAX_GOALS wide");
    (goals(last.0), goals(last.1))
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

/// Where a simulated season's random strength offsets act
/// (`Dispersion::shock_on`). Override offsets (`World::offset`) act on the
/// blended λ pair either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShockOn {
    /// Added to the Elo ratings before `elo_lambdas`. Elo carries only its
    /// blend weight of each λ and the underdog lift compresses low λs
    /// further, so an offset moves the whole model by much less than an Elo
    /// gap of its size moves the Elo component. Not mean-preserving: the
    /// convexity of 10^x raises the league's mean λ a little.
    Elo,
    /// Applied to the blended, lifted λ pair (`offset_lambdas`): with Δ =
    /// offset_home − offset_away, `λ_home · 10^(Δ/D_DIV)` and `λ_away ·
    /// 10^(−Δ/D_DIV)`, clamped as `blend_lambdas` clamps. The offset acts on
    /// the whole model the way an Elo gap acts on the Elo component. Both λ
    /// are divided by E[10^(Δ/D_DIV)] (`shock_inflation`, from each club's
    /// variance at that fixture: sigma², plus drift² per step taken), so
    /// the shock spreads strengths without changing the scoring rate.
    Blend,
}

/// How a simulated season spreads around the point forecast. Production
/// simulates with `Dispersion::PRODUCTION`; the other settings are
/// experiment knobs (`examples/season_eval.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dispersion {
    /// SD, in Elo points, of each club's strength offset, drawn once per
    /// simulated season and held for all of it.
    pub sigma: f64,
    /// Where the random offsets (shock and drift) act.
    pub shock_on: ShockOn,
    /// Hot simulation: when positive, every simulated or pinned result
    /// updates the trial's Elo ratings with `elo_delta_k(hot_k, ..)`, so
    /// later fixtures of the trial see them, as production's walk-forward
    /// would once those results were real. 0 is a cold simulation, where
    /// only pinned results update the ratings, with production's `elo_delta`.
    pub hot_k: f64,
    /// Within-season random walk: when positive, every club's offset takes
    /// an independent N(0, drift) step, in Elo points, before each round
    /// still to be played, and keeps it for the rest of the trial.
    pub drift: f64,
}

impl Dispersion {
    /// What production simulates: one N(0, `RATING_SIGMA`) shock per club
    /// per season on the whole model, mean-preserving, cold, no drift.
    pub const PRODUCTION: Dispersion = Dispersion {
        sigma: data::RATING_SIGMA,
        shock_on: ShockOn::Blend,
        hot_k: 0.0,
        drift: 0.0,
    };

    /// Whether ratings or offsets move during the season, which makes the
    /// order fixtures are played in matter.
    fn moves_in_season(&self) -> bool {
        self.hot_k > 0.0 || self.drift > 0.0
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
            offset: vec![0.0; data::N_TEAMS],
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

        for i in self.kickoff_order() {
            let f = self.fixtures[i];
            let Some(&(hs, as_)) = self.played.get(&(f.home, f.away)) else {
                continue;
            };
            let delta = elo_delta(self.elo[f.home], self.elo[f.away], hs, as_);
            self.elo[f.home] += delta;
            self.elo[f.away] -= delta;
        }
    }

    /// Fixture indices in kick-off order: by date, then round, calendar
    /// order within those (a stable sort).
    fn kickoff_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.fixtures.len()).collect();
        order.sort_by(|&a, &b| {
            (&self.dates[a].date, self.fixtures[a].round)
                .cmp(&(&self.dates[b].date, self.fixtures[b].round))
        });
        order
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

    /// Rate each overridden club at its requested rating on the whole
    /// model: its offset becomes the rating minus its current Elo, and the
    /// Elo ratings stay as the results made them. Set on the Elo ratings, an
    /// override would move λ by only the Elo component's share of the gap.
    pub fn apply_overrides(&mut self, overrides: &HashMap<String, f64>) {
        for (team, rating) in overrides {
            if let Some(&i) = self.idx.get(team) {
                self.offset[i] = *rating - self.elo[i];
            }
        }
    }

    /// Expected goals `(λ_home, λ_away)` for a fixture, override offsets
    /// included. The home-ground boost goes to `home`, not to a per-team
    /// host flag.
    fn lam_pair(&self, home: usize, away: usize) -> (f64, f64) {
        offset_lambdas(
            self.lam_pair_with(&self.elo, home, away),
            self.offset[home] - self.offset[away],
            1.0,
        )
    }

    /// The blended, lifted λ pair from a supplied rating vector, before any
    /// offset: the simulator's trial ratings move during a season.
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
    /// which reduces to independent Poissons. Override offsets move it as
    /// they move the simulation; with none it is the model's own forecast.
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
        self.simulate_one_dispersed(rng, forced, &Dispersion::PRODUCTION)
    }

    /// One simulated season under `disp`. `Dispersion::PRODUCTION` is
    /// `simulate_one_with`, draw for draw. Without pins, hot updates or
    /// drift the fixtures are played in calendar order; with any of them,
    /// in kick-off order (date, then round, as `refresh_ratings` walks
    /// them), so a result or a drift step moves only what is played after
    /// it.
    pub fn simulate_one_dispersed(
        &self,
        rng: &mut SmallRng,
        forced: &HashMap<(usize, usize), ForcedOutcome>,
        disp: &Dispersion,
    ) -> SeasonResult {
        let order = self.play_order(forced, disp);
        self.play_season(rng, forced, disp, &order, data::ELO_K)
    }

    /// The order a trial plays the fixtures in (`simulate_one_dispersed`).
    fn play_order(
        &self,
        forced: &HashMap<(usize, usize), ForcedOutcome>,
        disp: &Dispersion,
    ) -> Vec<usize> {
        if forced.is_empty() && !disp.moves_in_season() {
            (0..self.fixtures.len()).collect()
        } else {
            self.kickoff_order()
        }
    }

    /// One simulated season with the fixtures played in `order`. In a cold
    /// simulation a pinned result moves the trial's Elo ratings by
    /// `elo_delta_k(pin_k, ..)`; production's `pin_k` is `data::ELO_K`.
    fn play_season(
        &self,
        rng: &mut SmallRng,
        forced: &HashMap<(usize, usize), ForcedOutcome>,
        disp: &Dispersion,
        order: &[usize],
        pin_k: f64,
    ) -> SeasonResult {
        let n = self.teams.len();

        // Each trial commits to one draw of what the clubs are actually
        // worth, held fixed for the whole season. Drawn per trial rather than
        // per match: a club's true strength does not resample every weekend,
        // and per-match noise would wash out in the aggregate instead of
        // widening the season-outcome distribution the way real uncertainty
        // does.
        let mut shock: Vec<f64> = if disp.sigma > 0.0 {
            let normal = rand_distr::Normal::new(0.0, disp.sigma)
                .expect("the shock's sigma is finite and non-negative");
            self.elo.iter().map(|_| rng.sample(normal)).collect()
        } else {
            vec![0.0; self.elo.len()]
        };
        // Each club's variance of `shock`: sigma², and drift² more for every
        // drift step taken.
        let mut shock_var = if disp.sigma > 0.0 {
            disp.sigma * disp.sigma
        } else {
            0.0
        };
        let mut inflation = shock_inflation(shock_var);
        let drift = (disp.drift > 0.0).then(|| {
            rand_distr::Normal::new(0.0, disp.drift).expect("drift is finite and non-negative")
        });
        // The trial's own Elo ratings: production's, moved by pinned results
        // (in a hot simulation, by every result).
        let mut elo = self.elo.clone();
        // Rounds whose first unplayed fixture has come up, and with it the
        // drift step taken before the round.
        let mut rounds_begun = [false; 256];

        let mut records = vec![TeamRecord::default(); n];
        let mut results: HashMap<(usize, usize), (i64, i64)> =
            HashMap::with_capacity(self.fixtures.len());

        for &i in order {
            let f = self.fixtures[i];
            let (hg, ag) = match self.played.get(&(f.home, f.away)) {
                Some(&(hs, as_)) => (hs as i64, as_ as i64),
                None => {
                    if let Some(step) = drift {
                        if !std::mem::replace(&mut rounds_begun[usize::from(f.round)], true) {
                            for o in shock.iter_mut() {
                                *o += rng.sample(step);
                            }
                            shock_var += disp.drift * disp.drift;
                            inflation = shock_inflation(shock_var);
                        }
                    }
                    let want = forced.get(&(f.home, f.away)).copied();
                    let (lh, la) = self.dispersed_lambdas(
                        &elo,
                        &shock,
                        inflation,
                        disp.shock_on,
                        f.home,
                        f.away,
                    );
                    let (hg, ag) = self.draw_score(lh, la, want, rng);
                    let goals =
                        |g: i64| u16::try_from(g).expect("a simulated score is a small count");
                    if disp.hot_k > 0.0 {
                        // The win expectancy is the one the match was drawn
                        // with: rating plus offset (shock, drift and any
                        // override). The update is then noise around the
                        // strength the trial committed to, which is what a
                        // hot simulation adds. Taking it from the rating
                        // alone would let results drawn at rating + offset
                        // pull the rating towards that, so later in a season
                        // λ would carry the offset up to twice over: a
                        // compounded shock, not in-season learning.
                        // (Production's margin multiplier still gives
                        // favourites a small upward drift, as it does in the
                        // real walk.)
                        let d = elo_delta_k(
                            disp.hot_k,
                            elo[f.home] + (shock[f.home] + self.offset[f.home]),
                            elo[f.away] + (shock[f.away] + self.offset[f.away]),
                            goals(hg),
                            goals(ag),
                        );
                        elo[f.home] += d;
                        elo[f.away] -= d;
                    } else if want.is_some() {
                        // A pin says what will happen, so it moves the
                        // ratings as `refresh_ratings` would were it real:
                        // from the trial's Elo ratings, without the offsets,
                        // which are the trial's draw and the user's override
                        // rather than anything the results taught. (A pin
                        // dated before results already in lands after them
                        // here, where the walk would slot it in by date.)
                        let d = elo_delta_k(pin_k, elo[f.home], elo[f.away], goals(hg), goals(ag));
                        elo[f.home] += d;
                        elo[f.away] -= d;
                    }
                    (hg, ag)
                }
            };
            apply_result(&mut records, f.home, f.away, hg, ag);
            results.insert((f.home, f.away), (hg, ag));
        }

        let order = rank_table(&records, &results, &mut || rng.gen::<u64>());
        SeasonResult { order, records }
    }

    /// `(λ_home, λ_away)` from the trial's ratings, each club's random
    /// offset acting where `shock_on` says (divided by `inflation` on the
    /// blend) and its override offset on the blended pair.
    fn dispersed_lambdas(
        &self,
        elo: &[f64],
        shock: &[f64],
        inflation: f64,
        shock_on: ShockOn,
        home: usize,
        away: usize,
    ) -> (f64, f64) {
        let override_gap = self.offset[home] - self.offset[away];
        match shock_on {
            ShockOn::Elo => offset_lambdas(
                blend_lambdas(
                    elo_lambdas(elo[home] + shock[home], elo[away] + shock[away]),
                    self.ensemble.as_ref(),
                    home,
                    away,
                ),
                override_gap,
                1.0,
            ),
            ShockOn::Blend => offset_lambdas(
                self.lam_pair_with(elo, home, away),
                (shock[home] - shock[away]) + override_gap,
                inflation,
            ),
        }
    }

    /// A scoreline drawn from `(lh, la)`; from the pinned outcome's own
    /// cells of the score table when there is one (`pinned_score`).
    fn draw_score(
        &self,
        lh: f64,
        la: f64,
        want: Option<ForcedOutcome>,
        rng: &mut SmallRng,
    ) -> (i64, i64) {
        match want {
            None => self.sample_match_score(lh, la, rng),
            Some(want) => {
                let rho = self.score_rho().unwrap_or(0.0);
                let (hg, ag) = pinned_score(lh, la, rho, want, rng.gen::<f64>());
                (i64::from(hg), i64::from(ag))
            }
        }
    }

    pub fn simulate(&self, config: &SimConfig) -> SimResults {
        let n_sims = config.n_sims;
        let n = self.teams.len();
        assert!(n_sims > 0, "simulation requires at least one trial");
        let disp = Dispersion::PRODUCTION;
        // Every trial plays the fixtures in the same order.
        let order = self.play_order(&config.forced, &disp);

        let totals = (0..n_sims)
            .into_par_iter()
            .fold(
                || SimAccumulator::new(n),
                |mut totals, i| {
                    let mut rng =
                        SmallRng::seed_from_u64(config.seed.wrapping_add(i as u64 * 2_654_435_761));
                    let season =
                        self.play_season(&mut rng, &config.forced, &disp, &order, data::ELO_K);
                    totals.record(&season);
                    totals
                },
            )
            .reduce(|| SimAccumulator::new(n), SimAccumulator::merge);

        // Keep one deterministic diagnostic sample without retaining every
        // trial. Aggregate API output is built exclusively from `totals`.
        let representative = {
            let mut rng = SmallRng::seed_from_u64(config.seed.wrapping_add(0x9E37_79B9_7F4A_7C15));
            self.play_season(&mut rng, &config.forced, &disp, &order, data::ELO_K)
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
    fn an_override_sets_an_offset_and_leaves_the_ratings_alone() {
        let mut w = World::with_played(MID_SEASON);
        let gs = w.idx["Galatasaray"];
        let elo = w.elo.clone();
        w.apply_overrides(&HashMap::from([(
            "Galatasaray".to_string(),
            elo[gs] - 200.0,
        )]));
        assert_eq!(w.elo, elo, "the ratings are the results'");
        assert!((w.offset[gs] + 200.0).abs() < 1e-9, "{}", w.offset[gs]);
        assert!((0..w.teams.len())
            .filter(|&c| c != gs)
            .all(|c| w.offset[c] == 0.0));
        // Applying the same overrides again changes nothing.
        let once = w.offset.clone();
        w.apply_overrides(&HashMap::from([(
            "Galatasaray".to_string(),
            elo[gs] - 200.0,
        )]));
        assert_eq!(w.offset, once);
    }

    /// Every field of two forecasts, bit for bit.
    fn same_bits(a: &FixtureProbs, b: &FixtureProbs) -> bool {
        let bits = |p: &FixtureProbs| {
            let mut v = vec![
                p.home_win_pct,
                p.draw_pct,
                p.away_win_pct,
                p.over25_pct,
                p.btts_pct,
                p.home_xg,
                p.away_xg,
            ];
            for &(x, y, pct) in &p.top_scores {
                v.extend([f64::from(x), f64::from(y), pct]);
            }
            v.into_iter().map(f64::to_bits).collect::<Vec<u64>>()
        };
        bits(a) == bits(b)
    }

    #[test]
    fn zero_offsets_leave_the_fixture_forecast_bit_for_bit() {
        let mut w = shipped(World::with_played(MID_SEASON));
        let rho = w.score_rho().expect("Dixon-Coles has weight");
        let model = |w: &World, f: LeagueFixture| {
            let (lh, la) = blend_lambdas(
                elo_lambdas(w.elo[f.home], w.elo[f.away]),
                w.ensemble.as_ref(),
                f.home,
                f.away,
            );
            probs_from_lambdas(lh, la, rho)
        };
        for &f in &w.fixtures {
            assert!(same_bits(&w.fixture_probs(f.home, f.away), &model(&w, f)));
        }

        // An override to a club's own rating is no override.
        let gs = w.idx["Galatasaray"];
        w.apply_overrides(&HashMap::from([("Galatasaray".to_string(), w.elo[gs])]));
        assert_eq!(w.offset[gs], 0.0);
        for &f in &w.fixtures {
            assert!(same_bits(&w.fixture_probs(f.home, f.away), &model(&w, f)));
        }

        // A real one moves the club's fixtures and no others.
        w.apply_overrides(&HashMap::from([(
            "Galatasaray".to_string(),
            w.elo[gs] + 30.0,
        )]));
        for &f in &w.fixtures {
            let same = same_bits(&w.fixture_probs(f.home, f.away), &model(&w, f));
            assert_eq!(same, f.home != gs && f.away != gs, "{f:?}");
        }
    }

    #[test]
    fn an_override_moves_the_whole_model_not_only_its_elo_part() {
        let w = shipped(World::with_played(0));
        let (club, rival) = (w.idx["Göztepe"], w.idx["Kocaelispor"]);
        let ratio = 10f64.powf(100.0 / data::D_DIV);
        let xg = |w: &World, home: usize, away: usize| {
            let p = w.fixture_probs(home, away);
            (p.home_xg, p.away_xg)
        };
        let at_home = xg(&w, club, rival);
        let away = xg(&w, rival, club);

        let mut whole = w.clone();
        whole.apply_overrides(&HashMap::from([(
            "Göztepe".to_string(),
            w.elo[club] + 100.0,
        )]));
        let close = |a: f64, b: f64| (a / b - 1.0).abs() < 1e-12;
        let (h, a) = xg(&whole, club, rival);
        assert!(close(h, at_home.0 * ratio) && close(a, at_home.1 / ratio));
        let (h, a) = xg(&whole, rival, club);
        assert!(close(h, away.0 / ratio) && close(a, away.1 * ratio));

        // The same 100 points on the Elo ratings reach λ only through the
        // Elo component, so they move it a good deal less.
        let mut elo_only = w.clone();
        elo_only.elo[club] += 100.0;
        let (h, _) = xg(&elo_only, club, rival);
        let share = (h / at_home.0).ln() / ratio.ln();
        assert!((0.3..0.9).contains(&share), "Elo-only share {share}");
    }

    /// Mean goals for `club` per simulated season, over `trials` seasons.
    fn mean_goals_for(w: &World, disp: &Dispersion, club: usize, trials: u64, seed: u64) -> f64 {
        let total: i64 = (0..trials)
            .into_par_iter()
            .map(|t| {
                let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(t * 2_654_435_761));
                w.simulate_one_dispersed(&mut rng, &HashMap::new(), disp)
                    .records[club]
                    .gf
            })
            .sum();
        total as f64 / trials as f64
    }

    /// The simulator scores an overridden club's goals at the λs
    /// `fixture_probs` shows for it, wherever the random shock acts.
    #[test]
    fn the_simulator_takes_an_override_as_the_fixture_forecast_does() {
        let mut w = shipped(World::with_played(0));
        let club = w.idx["Göztepe"];
        w.apply_overrides(&HashMap::from([(
            "Göztepe".to_string(),
            w.elo[club] + 100.0,
        )]));
        let expected: f64 = w
            .fixtures
            .iter()
            .map(|f| {
                let p = w.fixture_probs(f.home, f.away);
                if f.home == club {
                    p.home_xg
                } else if f.away == club {
                    p.away_xg
                } else {
                    0.0
                }
            })
            .sum();
        for (disp, tol) in [
            (dispersion(0.0, ShockOn::Blend, 0.0, 0.0), 0.8),
            (dispersion(0.0, ShockOn::Elo, 0.0, 0.0), 0.8),
            (Dispersion::PRODUCTION, 1.2),
        ] {
            let got = mean_goals_for(&w, &disp, club, 1500, 31);
            assert!(
                (got - expected).abs() < tol,
                "{disp:?}: {got:.2} goals simulated, {expected:.2} expected"
            );
        }
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

    /// An unpinned `simulate_one` as it stood before `Dispersion` existed,
    /// kept verbatim as the reference a 75-point shock on the Elo ratings
    /// must reproduce.
    fn simulate_one_before_dispersion(w: &World, rng: &mut SmallRng) -> SeasonResult {
        let n = w.teams.len();
        let season_elo: Vec<f64> = if data::RATING_SIGMA > 0.0 {
            let normal = rand_distr::Normal::new(0.0, data::RATING_SIGMA)
                .expect("RATING_SIGMA is finite and non-negative");
            w.elo.iter().map(|e| e + rng.sample(normal)).collect()
        } else {
            w.elo.clone()
        };
        let mut records = vec![TeamRecord::default(); n];
        let mut results: HashMap<(usize, usize), (i64, i64)> =
            HashMap::with_capacity(w.fixtures.len());
        for f in &w.fixtures {
            let (hg, ag) = match w.played.get(&(f.home, f.away)) {
                Some(&(hs, as_)) => (hs as i64, as_ as i64),
                None => {
                    let (lh, la) = w.lam_pair_with(&season_elo, f.home, f.away);
                    w.sample_match_score(lh, la, rng)
                }
            };
            apply_result(&mut records, f.home, f.away, hg, ag);
            results.insert((f.home, f.away), (hg, ag));
        }
        let order = rank_table(&records, &results, &mut || rng.gen::<u64>());
        SeasonResult { order, records }
    }

    /// An unpinned, cold season with the shock on the blend, written out
    /// plainly: one N(0, sigma) offset per club, then the fixtures in
    /// calendar order or, with drift, in kick-off order with one N(0, drift)
    /// step per club before each round that still has a fixture to play.
    fn blend_season_reference(
        w: &World,
        rng: &mut SmallRng,
        sigma: f64,
        drift: f64,
    ) -> SeasonResult {
        let n = w.teams.len();
        let normal = rand_distr::Normal::new(0.0, sigma).expect("finite sigma");
        let mut shock: Vec<f64> = (0..n).map(|_| rng.sample(normal)).collect();
        let mut var = sigma * sigma;
        let order: Vec<usize> = if drift > 0.0 {
            w.kickoff_order()
        } else {
            (0..w.fixtures.len()).collect()
        };
        let mut begun = std::collections::HashSet::new();
        let mut records = vec![TeamRecord::default(); n];
        let mut results = HashMap::new();
        for i in order {
            let f = w.fixtures[i];
            let (hg, ag) = match w.played.get(&(f.home, f.away)) {
                Some(&(hs, as_)) => (hs as i64, as_ as i64),
                None => {
                    if drift > 0.0 && begun.insert(f.round) {
                        let step = rand_distr::Normal::new(0.0, drift).expect("finite drift");
                        for o in shock.iter_mut() {
                            *o += rng.sample(step);
                        }
                        var += drift * drift;
                    }
                    let (lh, la) = offset_lambdas(
                        w.lam_pair_with(&w.elo, f.home, f.away),
                        shock[f.home] - shock[f.away],
                        shock_inflation(var),
                    );
                    w.sample_match_score(lh, la, rng)
                }
            };
            apply_result(&mut records, f.home, f.away, hg, ag);
            results.insert((f.home, f.away), (hg, ag));
        }
        let order = rank_table(&records, &results, &mut || rng.gen::<u64>());
        SeasonResult { order, records }
    }

    fn shipped(mut w: World) -> World {
        w.ensemble = Some(Ensemble::from_embedded_data(0.5, 0.3, 0.2).expect("ensemble"));
        w
    }

    fn dispersion(sigma: f64, shock_on: ShockOn, hot_k: f64, drift: f64) -> Dispersion {
        Dispersion {
            sigma,
            shock_on,
            hot_k,
            drift,
        }
    }

    /// Mean over clubs of the SD of final points across `trials` seasons:
    /// how widely the simulation spreads a club's outcome.
    fn points_spread(w: &World, disp: &Dispersion, trials: u64, seed: u64) -> f64 {
        let n = w.teams.len();
        let mut points = vec![Vec::with_capacity(trials as usize); n];
        for t in 0..trials {
            let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(t * 2_654_435_761));
            let r = w.simulate_one_dispersed(&mut rng, &HashMap::new(), disp);
            for (c, rec) in r.records.iter().enumerate() {
                points[c].push(rec.points as f64);
            }
        }
        let sd = |v: &[f64]| {
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
        };
        points.iter().map(|p| sd(p)).sum::<f64>() / n as f64
    }

    /// `simulate_one_dispersed` and a reference on the same seeds: the same
    /// seasons and not one random number more or fewer.
    fn assert_draw_for_draw(
        w: &World,
        disp: &Dispersion,
        reference: impl Fn(&mut SmallRng) -> SeasonResult,
        case: &str,
    ) {
        for seed in 0..6u64 {
            let mut want = SmallRng::seed_from_u64(seed);
            let before = reference(&mut want);
            let mut got = SmallRng::seed_from_u64(seed);
            let now = w.simulate_one_dispersed(&mut got, &HashMap::new(), disp);
            assert_eq!(before.order, now.order, "{case} seed {seed}");
            assert_eq!(before.records, now.records, "{case} seed {seed}");
            assert_eq!(want.gen::<u64>(), got.gen::<u64>(), "{case} seed {seed}");
        }
    }

    #[test]
    fn an_elo_shock_reproduces_the_simulator_before_dispersion_draw_for_draw() {
        let elo_shock = dispersion(data::RATING_SIGMA, ShockOn::Elo, 0.0, 0.0);
        let mid = World::with_played(MID_SEASON);
        for (k, w) in [shipped(World::with_played(0)), shipped(mid.clone()), mid]
            .iter()
            .enumerate()
        {
            assert_draw_for_draw(
                w,
                &elo_shock,
                |rng| simulate_one_before_dispersion(w, rng),
                &format!("case {k}"),
            );
        }
    }

    /// Production with nothing pinned: calendar order and the shock's
    /// draws, as before pins and hot updates existed. With drift, one step
    /// per round still to be played: the rounds already complete take none,
    /// and a part-played round takes one.
    #[test]
    fn unpinned_blend_seasons_follow_their_reference_draw_for_draw() {
        assert_eq!(
            Dispersion::PRODUCTION,
            dispersion(data::RATING_SIGMA, ShockOn::Blend, 0.0, 0.0)
        );
        let pre = shipped(World::with_played(0));
        let mid = shipped(World::with_played(MID_SEASON + 4));
        let played_in = |round: u8| {
            let f = mid.fixtures.iter().filter(|f| f.round == round);
            let played = f
                .clone()
                .filter(|f| mid.played.contains_key(&(f.home, f.away)));
            (played.count(), f.count())
        };
        let rounds: Vec<(usize, usize)> = (1..=data::N_ROUNDS as u8).map(played_in).collect();
        assert!(rounds.iter().any(|&(p, n)| p == n), "a complete round");
        assert!(
            rounds.iter().any(|&(p, n)| 0 < p && p < n),
            "a part-played one"
        );
        for (k, w) in [&pre, &mid].into_iter().enumerate() {
            assert_draw_for_draw(
                w,
                &Dispersion::PRODUCTION,
                |rng| blend_season_reference(w, rng, data::RATING_SIGMA, 0.0),
                &format!("production, case {k}"),
            );
            let with = w.simulate_one_with(&mut SmallRng::seed_from_u64(3), &HashMap::new());
            let blend =
                blend_season_reference(w, &mut SmallRng::seed_from_u64(3), data::RATING_SIGMA, 0.0);
            assert_eq!(
                with.records, blend.records,
                "simulate_one_with is production"
            );
            assert_draw_for_draw(
                w,
                &dispersion(50.0, ShockOn::Blend, 0.0, 12.0),
                |rng| blend_season_reference(w, rng, 50.0, 12.0),
                &format!("drift, case {k}"),
            );
        }
    }

    /// The whole-model shock spreads λ around the point forecast without
    /// raising its mean: over many draws of the two clubs' offsets, each
    /// shocked λ averages the λ the model gives the fixture.
    #[test]
    fn a_blend_shock_keeps_each_lambda_on_average() {
        let w = shipped(World::with_played(0));
        let (home, away) = (w.idx["Göztepe"], w.idx["Kocaelispor"]);
        let (lh, la) = w.lam_pair_with(&w.elo, home, away);
        let mut rng = SmallRng::seed_from_u64(8);
        let draws = 200_000;
        // A season's shock, and the same after ten drift steps of 15.
        for var in [75.0f64 * 75.0, 75.0 * 75.0 + 10.0 * 15.0 * 15.0] {
            assert!(shock_inflation(var) > 1.02, "the bias removed would show");
            let normal = rand_distr::Normal::new(0.0, var.sqrt()).expect("finite");
            let mut shock = vec![0.0; w.teams.len()];
            let (mut sum_h, mut sum_a) = (0.0, 0.0);
            for _ in 0..draws {
                shock[home] = rng.sample(normal);
                shock[away] = rng.sample(normal);
                let (h, a) = w.dispersed_lambdas(
                    &w.elo,
                    &shock,
                    shock_inflation(var),
                    ShockOn::Blend,
                    home,
                    away,
                );
                sum_h += h;
                sum_a += a;
            }
            let (mh, ma) = (sum_h / draws as f64 / lh, sum_a / draws as f64 / la);
            assert!(
                (mh - 1.0).abs() < 0.003 && (ma - 1.0).abs() < 0.003,
                "variance {var}: shocked λs average {mh:.4} and {ma:.4} of the point λs"
            );
        }
    }

    /// Over whole simulated seasons the shock leaves the league's goals
    /// where the cold simulation has them, with drift as without.
    #[test]
    fn a_blend_shock_keeps_the_league_scoring_rate() {
        let w = shipped(World::with_played(0));
        let goals = |disp: &Dispersion| {
            let trials = 1000u64;
            let total: i64 = (0..trials)
                .into_par_iter()
                .map(|t| {
                    let mut rng = SmallRng::seed_from_u64(77 + t * 2_654_435_761);
                    let season = w.simulate_one_dispersed(&mut rng, &HashMap::new(), disp);
                    season.records.iter().map(|r| r.gf).sum::<i64>()
                })
                .sum();
            total as f64 / trials as f64
        };
        let cold = goals(&dispersion(0.0, ShockOn::Blend, 0.0, 0.0));
        for disp in [
            Dispersion::PRODUCTION,
            dispersion(data::RATING_SIGMA, ShockOn::Blend, 0.0, 15.0),
        ] {
            let shocked = goals(&disp);
            assert!(
                (shocked / cold - 1.0).abs() < 0.006,
                "{disp:?}: {shocked:.1} goals a season against {cold:.1} cold"
            );
        }
    }

    #[test]
    fn a_pinned_score_follows_the_score_table_restricted_to_the_pin() {
        const N: usize = crate::dixoncoles::MAX_GOALS + 1;
        let fitted = shipped(World::with_played(0))
            .score_rho()
            .expect("Dixon-Coles has weight");
        let outcomes = [
            ForcedOutcome::Home,
            ForcedOutcome::Draw,
            ForcedOutcome::Away,
        ];
        let mut rng = SmallRng::seed_from_u64(12);
        let draws = 100_000;
        for (lh, la, rho) in [(1.6, 1.1, fitted), (0.7, 2.4, 0.0)] {
            let table = crate::dixoncoles::score_table(lh, la, rho);
            for want in outcomes {
                let mass: f64 = (0..N)
                    .flat_map(|x| (0..N).map(move |y| (x, y)))
                    .filter(|&(x, y)| ForcedOutcome::of(x, y) == want)
                    .map(|(x, y)| table[x][y])
                    .sum();
                let mut counts = [[0usize; N]; N];
                for _ in 0..draws {
                    let (x, y) = conditional_cell(&table, want, rng.gen::<f64>());
                    counts[usize::from(x)][usize::from(y)] += 1;
                }
                for x in 0..N {
                    for y in 0..N {
                        let freq = counts[x][y] as f64 / draws as f64;
                        if ForcedOutcome::of(x, y) != want {
                            assert_eq!(counts[x][y], 0, "a {want:?} pin drew {x}-{y}");
                            continue;
                        }
                        let p = table[x][y] / mass;
                        let se = (p * (1.0 - p) / draws as f64).sqrt();
                        assert!(
                            (freq - p).abs() < 4.5 * se + 1e-4,
                            "{want:?} {x}-{y}: drawn {freq:.5}, table {p:.5}"
                        );
                    }
                }
            }
            // The ends of the inverse CDF: the outcome's first cell, and
            // still the outcome at the top.
            assert_eq!(conditional_cell(&table, ForcedOutcome::Home, 0.0), (1, 0));
            assert_eq!(conditional_cell(&table, ForcedOutcome::Draw, 0.0), (0, 0));
            assert_eq!(conditional_cell(&table, ForcedOutcome::Away, 0.0), (0, 1));
            let top = 1.0 - f64::EPSILON / 2.0;
            for want in outcomes {
                let (x, y) = conditional_cell(&table, want, top);
                assert_eq!(ForcedOutcome::of(usize::from(x), usize::from(y)), want);
            }
        }
        assert_eq!(
            pinned_score(1.6, 1.1, fitted, ForcedOutcome::Draw, 0.4),
            conditional_cell(
                &crate::dixoncoles::score_table(1.6, 1.1, fitted),
                ForcedOutcome::Draw,
                0.4
            )
        );
    }

    /// `club`'s title share over `trials` production seasons with `forced`
    /// pinned, pinned results moving ratings by `pin_k`.
    fn title_share(
        w: &World,
        forced: &HashMap<(usize, usize), ForcedOutcome>,
        pin_k: f64,
        club: usize,
        trials: u64,
    ) -> f64 {
        let disp = Dispersion::PRODUCTION;
        let order = w.play_order(forced, &disp);
        let titles = (0..trials)
            .into_par_iter()
            .filter(|&t| {
                let mut rng = SmallRng::seed_from_u64(5 + t * 2_654_435_761);
                w.play_season(&mut rng, forced, &disp, &order, pin_k).order[0] == club
            })
            .count();
        titles as f64 / trials as f64
    }

    /// A pinned derby win moves the title race as far as the same win
    /// entered as played, because the trial's ratings learn from it: between
    /// a one-goal and a two-goal win, as the pin's own margins fall. With the
    /// update switched off the pin moves it much less.
    #[test]
    fn a_pinned_result_moves_the_ratings_as_the_same_real_result_would() {
        let w = shipped(World::with_played(MID_SEASON));
        let (fb, gs) = (w.idx["Fenerbahçe"], w.idx["Galatasaray"]);
        let derby = w
            .fixtures
            .iter()
            .filter(|f| (f.home == fb && f.away == gs) || (f.home == gs && f.away == fb))
            .find(|f| !w.played.contains_key(&(f.home, f.away)))
            .copied()
            .expect("an unplayed derby");
        let fb_wins = if derby.home == fb {
            ForcedOutcome::Home
        } else {
            ForcedOutcome::Away
        };
        let forced = HashMap::from([((derby.home, derby.away), fb_wins)]);
        let trials = 4000;
        let as_played = |margin: u16| {
            let mut played = w.clone();
            let score = if derby.home == fb {
                (1 + margin, 1)
            } else {
                (1, 1 + margin)
            };
            played.played.insert((derby.home, derby.away), score);
            played.refresh_ratings();
            title_share(&played, &HashMap::new(), data::ELO_K, gs, trials)
        };
        let (by_one, by_two) = (as_played(1), as_played(2));
        let pinned = title_share(&w, &forced, data::ELO_K, gs, trials);
        let frozen = title_share(&w, &forced, 0.0, gs, trials);
        let shares = format!(
            "played 2-1 {by_one:.3}, 3-1 {by_two:.3}; pinned {pinned:.3}, no update {frozen:.3}"
        );
        assert!(frozen - by_one > 0.06, "{shares}");
        assert!(
            by_two - 0.015 < pinned && pinned < by_one + 0.015,
            "{shares}"
        );

        // `simulate` plays the pinned season as `simulate_one_with` does.
        let one = w.simulate(&SimConfig {
            n_sims: 1,
            seed: 9,
            elo_overrides: HashMap::new(),
            forced: forced.clone(),
        });
        let trial = w.simulate_one_with(&mut SmallRng::seed_from_u64(9), &forced);
        let points: Vec<f64> = trial.records.iter().map(|r| r.points as f64).collect();
        assert_eq!(one.points_sum, points);
    }

    /// A hot simulation, or one with a pin, plays a fixture by its date.
    /// Re-dated before the season, a round-20 match is played first: the
    /// season is the same whether the match is listed in round 20's place
    /// or first, and whatever round it carries. A cold, unpinned simulation
    /// keeps the calendar's order.
    #[test]
    fn hot_or_pinned_seasons_play_an_early_fixture_first_wherever_it_is_listed() {
        let mut listed_late = World::with_played(MID_SEASON);
        let i = (0..listed_late.fixtures.len())
            .find(|&i| {
                let f = listed_late.fixtures[i];
                f.round == 20 && !listed_late.played.contains_key(&(f.home, f.away))
            })
            .expect("an unplayed round-20 fixture");
        listed_late.dates[i].date = "2026-01-01".to_string();
        assert!((0..listed_late.fixtures.len())
            .filter(|&j| j != i)
            .all(|j| listed_late.dates[j].date > listed_late.dates[i].date));
        let mut listed_first = listed_late.clone();
        let f = listed_first.fixtures.remove(i);
        let d = listed_first.dates.remove(i);
        listed_first.fixtures.insert(0, f);
        listed_first.dates.insert(0, d);
        let mut relabelled = listed_late.clone();
        relabelled.fixtures[i].round = 1;

        let hot = dispersion(data::RATING_SIGMA, ShockOn::Elo, 40.0, 0.0);
        let nothing = HashMap::new();
        let early = listed_late.fixtures[i];
        let pinned = HashMap::from([((early.home, early.away), ForcedOutcome::Home)]);
        let run = |w: &World, forced, disp: &Dispersion, seed: u64| {
            w.simulate_one_dispersed(&mut SmallRng::seed_from_u64(seed), forced, disp)
                .records
        };
        for seed in 0..4u64 {
            for (forced, disp) in [(&nothing, &hot), (&pinned, &Dispersion::PRODUCTION)] {
                let late = run(&listed_late, forced, disp, seed);
                let case = format!("seed {seed}, {} pins, {disp:?}", forced.len());
                assert_eq!(
                    late,
                    run(&listed_first, forced, disp, seed),
                    "{case}: listing"
                );
                assert_eq!(late, run(&relabelled, forced, disp, seed), "{case}: round");
            }
            assert_ne!(
                run(&listed_late, &nothing, &Dispersion::PRODUCTION, seed),
                run(&listed_first, &nothing, &Dispersion::PRODUCTION, seed),
                "seed {seed}: a cold season plays the calendar's order"
            );
        }
    }

    #[test]
    fn a_zero_blend_shock_is_the_cold_zero_sigma_simulation() {
        let w = shipped(World::with_played(MID_SEASON));
        let cold = dispersion(0.0, ShockOn::Elo, 0.0, 0.0);
        let blend = dispersion(0.0, ShockOn::Blend, 0.0, 0.0);
        for seed in 0..8u64 {
            let a = w.simulate_one_dispersed(
                &mut SmallRng::seed_from_u64(seed),
                &HashMap::new(),
                &cold,
            );
            let b = w.simulate_one_dispersed(
                &mut SmallRng::seed_from_u64(seed),
                &HashMap::new(),
                &blend,
            );
            assert_eq!(a.order, b.order, "seed {seed}");
            assert_eq!(a.records, b.records, "seed {seed}");
        }
    }

    /// The same shock moves the whole model on the blend, and only the Elo
    /// half of it (and less where the lift bites) on the Elo ratings.
    #[test]
    fn a_blend_shock_spreads_seasons_wider_than_the_same_elo_shock() {
        let w = shipped(World::with_played(0));
        let cold = points_spread(&w, &dispersion(0.0, ShockOn::Elo, 0.0, 0.0), 400, 21);
        let elo = points_spread(&w, &dispersion(150.0, ShockOn::Elo, 0.0, 0.0), 400, 21);
        let blend = points_spread(&w, &dispersion(150.0, ShockOn::Blend, 0.0, 0.0), 400, 21);
        assert!(elo > cold * 1.03, "Elo shock {elo} vs cold {cold}");
        assert!(blend > elo * 1.08, "blend shock {blend} vs Elo shock {elo}");
    }

    #[test]
    fn hot_simulation_widens_the_spread_of_final_points() {
        let w = World::with_played(0);
        let cold = points_spread(&w, &dispersion(0.0, ShockOn::Elo, 0.0, 0.0), 600, 5);
        let hot = points_spread(&w, &dispersion(0.0, ShockOn::Elo, 40.0, 0.0), 600, 5);
        assert!(hot > cold * 1.08, "hot {hot} vs cold {cold}");
    }

    #[test]
    fn drift_widens_the_spread_of_final_points() {
        let w = World::with_played(0);
        let cold = points_spread(&w, &dispersion(0.0, ShockOn::Elo, 0.0, 0.0), 600, 9);
        for shock_on in [ShockOn::Elo, ShockOn::Blend] {
            let drifting = points_spread(&w, &dispersion(0.0, shock_on, 0.0, 15.0), 600, 9);
            assert!(
                drifting > cold * 1.08,
                "{shock_on:?} drift {drifting} vs cold {cold}"
            );
        }
    }

    /// Hot updates and drift steps follow the season's unplayed fixtures:
    /// with every result in, nothing is left for them to move.
    #[test]
    fn a_finished_season_is_its_own_table_under_any_dispersion() {
        let w = World::with_played(data::N_FIXTURES);
        let every_knob = dispersion(100.0, ShockOn::Blend, 60.0, 20.0);
        let a = w.simulate_one_dispersed(
            &mut SmallRng::seed_from_u64(1),
            &HashMap::new(),
            &every_knob,
        );
        let b = w.simulate_one_dispersed(
            &mut SmallRng::seed_from_u64(2),
            &HashMap::new(),
            &dispersion(0.0, ShockOn::Elo, 0.0, 0.0),
        );
        assert_eq!(a.records, b.records);
    }

    #[test]
    fn every_dispersion_is_deterministic_for_a_seed() {
        let w = shipped(World::with_played(MID_SEASON));
        let pin = w
            .fixtures
            .iter()
            .find(|f| !w.played.contains_key(&(f.home, f.away)))
            .map(|f| (f.home, f.away))
            .expect("an unplayed fixture");
        let forced = HashMap::from([(pin, ForcedOutcome::Draw)]);
        for disp in [
            dispersion(50.0, ShockOn::Blend, 40.0, 10.0),
            dispersion(75.0, ShockOn::Elo, 20.0, 0.0),
            dispersion(0.0, ShockOn::Elo, 0.0, 15.0),
        ] {
            let run =
                |seed| w.simulate_one_dispersed(&mut SmallRng::seed_from_u64(seed), &forced, &disp);
            let (a, b) = (run(17), run(17));
            assert_eq!(a.order, b.order, "{disp:?}");
            assert_eq!(a.records, b.records, "{disp:?}");
            assert_ne!(a.records, run(18).records, "{disp:?}: a seed must matter");
        }
    }
}
