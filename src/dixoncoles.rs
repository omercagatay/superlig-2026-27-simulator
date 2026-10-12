use std::io::Write;
use std::path::Path;

use argmin::core::{CostFunction, Error as ArgminError, Executor, Gradient, State};
use argmin::solver::{linesearch::MoreThuenteLineSearch, quasinewton::LBFGS};
use chrono::NaiveDate;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::data;
use crate::history::{FitMatch, TeamIndex};

pub const MAX_GOALS: usize = 10;
const LAM_MIN: f64 = 1e-3;
const LAM_MAX: f64 = 50.0;
const RHO_CLAMP: f64 = 0.4;
const TAU_FLOOR: f64 = 1e-6;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DcParams {
    pub mu: f64,
    pub gamma: f64,
    pub rho: f64,
    pub alpha: Vec<f64>,
    pub beta: Vec<f64>,
    pub half_life_days: f64,
    pub n_teams: usize,
    pub other_idx: usize,
    pub fitted_at: String,
}

impl DcParams {
    /// Attack/defense lookup. Other-club bucket returns 0.
    pub fn attack(&self, i: usize) -> f64 {
        if i == self.other_idx {
            0.0
        } else {
            self.alpha[i]
        }
    }
    pub fn defense(&self, i: usize) -> f64 {
        if i == self.other_idx {
            0.0
        } else {
            self.beta[i]
        }
    }

    /// λ for the home side. `home` is the home team index, `away` the away team index.
    pub fn lambda_home(&self, home: usize, away: usize, neutral: bool) -> f64 {
        let ln = self.mu
            + if neutral { 0.0 } else { self.gamma }
            + self.attack(home)
            + self.defense(away);
        ln.exp().clamp(LAM_MIN, LAM_MAX)
    }

    pub fn lambda_away(&self, home: usize, away: usize, _neutral: bool) -> f64 {
        let ln = self.mu + self.attack(away) + self.defense(home);
        ln.exp().clamp(LAM_MIN, LAM_MAX)
    }

    pub fn lam(&self, home: usize, away: usize, neutral: bool) -> (f64, f64) {
        (
            self.lambda_home(home, away, neutral),
            self.lambda_away(home, away, neutral),
        )
    }

    pub fn save_json(&self, path: &Path) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self).unwrap();
        let mut f = std::fs::File::create(path)?;
        f.write_all(s.as_bytes())?;
        Ok(())
    }

    pub fn load_json(path: &Path) -> std::io::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&s).unwrap())
    }
}

/// Joint score probability table (rows = home goals 0..=MAX, cols = away goals 0..=MAX).
pub fn score_table(
    lambda_h: f64,
    lambda_a: f64,
    rho: f64,
) -> [[f64; MAX_GOALS + 1]; MAX_GOALS + 1] {
    let lph = poisson_pmf(lambda_h);
    let lpa = poisson_pmf(lambda_a);
    let rho = rho.clamp(-RHO_CLAMP, RHO_CLAMP);

    let mut table = [[0.0_f64; MAX_GOALS + 1]; MAX_GOALS + 1];
    for x in 0..=MAX_GOALS {
        for y in 0..=MAX_GOALS {
            let tau = tau_factor(x, y, lambda_h, lambda_a, rho).max(TAU_FLOOR);
            table[x][y] = tau * lph[x] * lpa[y];
        }
    }
    let z: f64 = table.iter().flat_map(|r| r.iter()).sum();
    if z > 0.0 {
        for r in table.iter_mut() {
            for c in r.iter_mut() {
                *c /= z;
            }
        }
    }
    table
}

/// Inverse-CDF draw of a scoreline from the Dixon-Coles joint distribution.
/// `u` must be in `[0, 1)`; the caller supplies it from its own RNG so the
/// draw stays deterministic under seeded simulation.
pub fn sample_score(lambda_h: f64, lambda_a: f64, rho: f64, u: f64) -> (u16, u16) {
    let table = score_table(lambda_h, lambda_a, rho);
    let mut acc = 0.0;
    for (x, row) in table.iter().enumerate() {
        for (y, &p) in row.iter().enumerate() {
            acc += p;
            if u < acc {
                return (x as u16, y as u16);
            }
        }
    }
    // Floating-point tail: the table sums to 1 within rounding error.
    (MAX_GOALS as u16, MAX_GOALS as u16)
}

/// Marginal win/draw/loss probabilities from (λ_h, λ_a, ρ).
pub fn match_probs(lambda_h: f64, lambda_a: f64, rho: f64) -> (f64, f64, f64) {
    let t = score_table(lambda_h, lambda_a, rho);
    let mut w = 0.0;
    let mut d = 0.0;
    let mut l = 0.0;
    for (x, row) in t.iter().enumerate() {
        for (y, &p) in row.iter().enumerate() {
            if x > y {
                w += p;
            } else if x == y {
                d += p;
            } else {
                l += p;
            }
        }
    }
    (w, d, l)
}

fn poisson_pmf(lambda: f64) -> [f64; MAX_GOALS + 1] {
    let l = lambda.clamp(LAM_MIN, LAM_MAX);
    let mut out = [0.0_f64; MAX_GOALS + 1];
    let mut term = (-l).exp();
    out[0] = term;
    for (k, slot) in out.iter_mut().enumerate().skip(1) {
        term *= l / k as f64;
        *slot = term;
    }
    out
}

fn tau_factor(x: usize, y: usize, lh: f64, la: f64, rho: f64) -> f64 {
    match (x, y) {
        (0, 0) => 1.0 - rho * lh * la,
        (1, 0) => 1.0 + rho * la,
        (0, 1) => 1.0 + rho * lh,
        (1, 1) => 1.0 - rho,
        _ => 1.0,
    }
}

/// Negative weighted log-likelihood over the fit set (single eval).
fn neg_loglik(x: &[f64], fits: &[FitMatch], n_teams: usize, other_idx: usize) -> f64 {
    let mu = x[0];
    let gamma = x[1];
    let rho = x[2].clamp(-RHO_CLAMP, RHO_CLAMP);
    let na = n_teams - 1; // league clubs; the bucket is fixed at 0

    let mut alpha = vec![0.0_f64; n_teams];
    let mut beta = vec![0.0_f64; n_teams];
    for i in 0..na {
        alpha[i] = x[3 + i];
        beta[i] = x[3 + na + i];
    }
    let _ = other_idx;

    let mut total = 0.0_f64;
    for m in fits {
        let gam = if m.neutral { 0.0 } else { gamma };
        let lh = (mu + gam + alpha[m.home_idx] + beta[m.away_idx])
            .exp()
            .clamp(LAM_MIN, LAM_MAX);
        let la = (mu + alpha[m.away_idx] + beta[m.home_idx])
            .exp()
            .clamp(LAM_MIN, LAM_MAX);
        if !lh.is_finite() || !la.is_finite() {
            return 1e20;
        }
        let lph = poisson_pmf(lh);
        let lpa = poisson_pmf(la);

        let sh: f64 = lph.iter().sum();
        let sa: f64 = lpa.iter().sum();
        // Apply τ at the four low-score cells.
        let c00 = (tau_factor(0, 0, lh, la, rho) - 1.0) * lph[0] * lpa[0];
        let c10 = (tau_factor(1, 0, lh, la, rho) - 1.0) * lph[1] * lpa[0];
        let c01 = (tau_factor(0, 1, lh, la, rho) - 1.0) * lph[0] * lpa[1];
        let c11 = (tau_factor(1, 1, lh, la, rho) - 1.0) * lph[1] * lpa[1];
        let mut z = sh * sa + c00 + c10 + c01 + c11;
        if z < 1e-300 {
            z = 1e-300;
        }
        let log_z = z.ln();

        let hs = m.home_score as usize;
        let as_ = m.away_score as usize;
        let (obs_x, obs_y) = (hs.min(MAX_GOALS), as_.min(MAX_GOALS));
        let tau_obs = tau_factor(obs_x, obs_y, lh, la, rho).max(TAU_FLOOR);
        let log_p_obs = tau_obs.ln() + lph[obs_x].ln() + lpa[obs_y].ln() - log_z;
        if log_p_obs.is_finite() {
            total -= m.weight * log_p_obs;
        } else {
            return 1e20;
        }
    }
    // Tiny L2 penalty keeps the optimizer off pathological plateaus.
    let reg: f64 = alpha.iter().map(|a| a * a).sum::<f64>() * 1e-4
        + beta.iter().map(|b| b * b).sum::<f64>() * 1e-4;
    total + reg
}

struct DcProblem {
    fits: Vec<FitMatch>,
    n_teams: usize,
    other_idx: usize,
    fd_step: f64,
}

impl CostFunction for DcProblem {
    type Param = Vec<f64>;
    type Output = f64;
    fn cost(&self, x: &Self::Param) -> Result<Self::Output, ArgminError> {
        Ok(neg_loglik(x, &self.fits, self.n_teams, self.other_idx))
    }
}

impl Gradient for DcProblem {
    type Param = Vec<f64>;
    type Gradient = Vec<f64>;
    fn gradient(&self, x: &Self::Param) -> Result<Self::Gradient, ArgminError> {
        let n = x.len();
        let h = self.fd_step;
        let f0 = self.cost(x)?;
        // Central differences in parallel over dimensions.
        let mut perturbed: Vec<Vec<f64>> = Vec::with_capacity(2 * n);
        for i in 0..n {
            let mut xp = x.clone();
            xp[i] += h;
            perturbed.push(xp);
            let mut xm = x.clone();
            xm[i] -= h;
            perturbed.push(xm);
        }
        let fs: Vec<f64> = perturbed
            .par_iter()
            .map(|p| neg_loglik(p, &self.fits, self.n_teams, self.other_idx))
            .collect();
        let mut g = vec![0.0_f64; n];
        for i in 0..n {
            let fp = fs[2 * i];
            let fm = fs[2 * i + 1];
            g[i] = (fp - fm) / (2.0 * h);
            if !g[i].is_finite() {
                g[i] = 0.0;
            }
            // Same-shape penalty (penalty matches regularizer used in cost).
            // Soft regularization derivative for alpha/beta entries (indices >= 3).
            if i >= 3 {
                g[i] += 2e-4 * x[i];
            }
        }
        let _ = f0;
        Ok(g)
    }
}

pub fn fit(
    fits: &[FitMatch],
    idx: &TeamIndex,
    half_life_days: f64,
    max_iters: u64,
) -> Result<DcParams, ArgminError> {
    let n_teams = idx.idx_to_name.len();
    let other_idx = idx.other_idx;
    let na = n_teams - 1;
    let n_params = 3 + 2 * na;

    // Initial guess: mu = log(overall mean ~ 1.4), gamma = log(1.1)-ish boost, rho slightly negative,
    // all team attack/defense at 0 (relative to the bucket=0 reference).
    let mu0 = data::BASE.ln();
    let gamma0 = 0.2;
    let rho0 = -0.02;
    let mut x0 = vec![0.0_f64; n_params];
    x0[0] = mu0;
    x0[1] = gamma0;
    x0[2] = rho0;

    let problem = DcProblem {
        fits: fits.to_vec(),
        n_teams,
        other_idx,
        fd_step: 1e-6,
    };
    let linesearch = MoreThuenteLineSearch::new().with_c(1e-4, 0.9)?;
    let solver = LBFGS::new(linesearch, 8)
        .with_tolerance_grad(1e-5)?
        .with_tolerance_cost(1e-8)?;

    let res = Executor::new(problem, solver)
        .configure(|state| state.param(x0.clone()).max_iters(max_iters))
        .run()?;

    let best = res.state().get_param().cloned().unwrap_or(x0);
    let mu = best[0];
    let gamma = best[1];
    let rho = best[2].clamp(-RHO_CLAMP, RHO_CLAMP);
    let mut alpha = vec![0.0_f64; n_teams];
    let mut beta = vec![0.0_f64; n_teams];
    for i in 0..na {
        alpha[i] = best[3 + i];
        beta[i] = best[3 + na + i];
    }

    tracing::info!(
        "DC fit done: mu={:.4} gamma={:.4} rho={:.4} cost={:.3} iters={}",
        mu,
        gamma,
        rho,
        res.state().get_cost(),
        res.state().get_iter()
    );

    Ok(DcParams {
        mu,
        gamma,
        rho,
        alpha,
        beta,
        half_life_days,
        n_teams,
        other_idx,
        fitted_at: now_iso(),
    })
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86400) as i64;
    let nd = NaiveDate::from_num_days_from_ce_opt(719162 + days as i32)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(2026, 6, 14).unwrap());
    nd.format("%Y-%m-%d").to_string()
}

/// Fit then persist to disk (used by /api/refresh).
pub fn fit_and_save(
    fits: &[FitMatch],
    idx: &TeamIndex,
    half_life_days: f64,
    path: &Path,
    max_iters: u64,
) -> std::io::Result<DcParams> {
    let params = fit(fits, idx, half_life_days, max_iters)
        .map_err(|e| std::io::Error::other(format!("argmin: {e}")))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    params.save_json(path)?;
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoricalMatch;

    fn idx_fixture(n_clubs: usize) -> TeamIndex {
        let mut league_names: Vec<String> = (0..n_clubs).map(|i| format!("T{}", i)).collect();
        let mut idx_to_name = league_names.clone();
        idx_to_name.push(crate::history::OTHER_TEAM_NAME.to_string());
        let other_idx = idx_to_name.len() - 1;
        let name_to_idx = idx_to_name
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let _ = &mut league_names;
        TeamIndex {
            name_to_idx,
            idx_to_name,
            league_names,
            other_idx,
        }
    }

    #[test]
    fn score_table_normalizes_to_one() {
        for &(lh, la, rho) in &[(1.4, 1.0, 0.0), (2.5, 0.4, -0.05), (0.6, 0.6, 0.1)] {
            let t = score_table(lh, la, rho);
            let s: f64 = t.iter().flat_map(|r| r.iter()).sum();
            assert!(
                (s - 1.0).abs() < 1e-9,
                "table sum = {} for ({},{},{})",
                s,
                lh,
                la,
                rho
            );
            assert!(t.iter().flat_map(|r| r.iter()).all(|&p| p >= 0.0));
        }
    }

    #[test]
    fn match_probs_partition_to_one_and_respect_dc_correction() {
        let (w, d, l) = match_probs(1.4, 1.0, -0.05);
        assert!((w + d + l - 1.0).abs() < 1e-9);
        // A negative rho inflates 1-1 draws relative to independent Poisson.
        let (w0, d0, l0) = match_probs(1.4, 1.0, 0.0);
        let t = score_table(1.4, 1.0, -0.05);
        let t0 = score_table(1.4, 1.0, 0.0);
        assert!(t[1][1] > t0[1][1], "DC rho=-0.05 should raise p(1,1)");
        assert!(t[0][0] > t0[0][0], "DC rho=-0.05 should raise p(0,0)");
        let _ = (w, d, l, w0, d0, l0);
    }

    #[test]
    fn sample_score_follows_the_joint_table() {
        let (lh, la, rho) = (1.4, 1.0, -0.05);
        let table = score_table(lh, la, rho);

        // u=0 lands in the first cell, u→1 in the tail.
        assert_eq!(sample_score(lh, la, rho, 0.0), (0, 0));
        assert_eq!(
            sample_score(lh, la, rho, 1.0 - 1e-15),
            (MAX_GOALS as u16, MAX_GOALS as u16)
        );

        // Empirical frequencies match table cells (deterministic seeded RNG).
        let mut rng = SmallRng::seed_from_u64(7);
        let n = 200_000;
        let mut c00 = 0usize;
        let mut c11 = 0usize;
        for _ in 0..n {
            let (x, y) = sample_score(lh, la, rho, rng.gen::<f64>());
            if (x, y) == (0, 0) {
                c00 += 1;
            }
            if (x, y) == (1, 1) {
                c11 += 1;
            }
        }
        let (p00, p11) = (c00 as f64 / n as f64, c11 as f64 / n as f64);
        assert!(
            (p00 - table[0][0]).abs() < 0.005,
            "p00 {p00} vs {}",
            table[0][0]
        );
        assert!(
            (p11 - table[1][1]).abs() < 0.005,
            "p11 {p11} vs {}",
            table[1][1]
        );
        // Negative rho must inflate the sampled 1-1 rate vs independent Poisson.
        let indep = score_table(lh, la, 0.0);
        assert!(p11 > indep[1][1], "rho<0 should raise 1-1 frequency");
    }

    #[test]
    fn fit_recovers_synthetic_params() {
        // Two strong teams and one weak team + a Other-club bucket.
        let idx = idx_fixture(3);
        let true_params = DcParams {
            mu: 0.3_f64.ln(),
            gamma: 0.25,
            rho: -0.05,
            alpha: vec![0.8, 0.3, -0.5, 0.0], // T0 strong attack, T2 weak
            beta: vec![-0.5, 0.0, 0.7, 0.0],  // T0 strong defense (low beta), T2 leaky
            half_life_days: 1e9,
            n_teams: 4,
            other_idx: 3,
            fitted_at: "1970-01-01".to_string(),
        };

        // Generate many matches from the true params.
        let mut rng = SmallRng::seed_from_u64(2024);
        let mut history: Vec<HistoricalMatch> = Vec::new();
        let as_of = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let teams = ["T0", "T1", "T2"];
        for _ in 0..4000 {
            let h = teams[(rng.gen::<usize>()) % 3];
            let a = teams[(rng.gen::<usize>()) % 3];
            let hi = idx.canonical(h);
            let ai = idx.canonical(a);
            // Force some bucket matches by occasionally picking it as away.
            let (ai, away_name) = if rng.gen::<bool>() {
                (idx.other_idx, crate::history::OTHER_TEAM_NAME)
            } else {
                (ai, a)
            };
            let (lh, la) = true_params.lam(hi, ai, false);
            let gh = sample_poisson(&mut rng, lh);
            let ga = sample_poisson(&mut rng, la);
            history.push(HistoricalMatch {
                date: as_of,
                home_team: h.to_string(),
                away_team: away_name.to_string(),
                home_score: gh as u16,
                away_score: ga as u16,
                neutral: false,
            });
        }

        let fits = crate::history::prepare_fit_matches(&history, &idx, 1e9, as_of);
        let fitted = fit(&fits, &idx, 1e9, 300).expect("fit");

        // mu and gamma should be recovered within ~0.15.
        assert!(
            (fitted.mu - true_params.mu).abs() < 0.15,
            "mu: {}",
            fitted.mu
        );
        assert!(
            (fitted.gamma - true_params.gamma).abs() < 0.15,
            "gamma: {}",
            fitted.gamma
        );
        // rho recovered within ~0.04.
        assert!(
            (fitted.rho - true_params.rho).abs() < 0.05,
            "rho: {}",
            fitted.rho
        );
        // Per-team attack ordering preserved.
        assert!(fitted.attack(0) > fitted.attack(1));
        assert!(fitted.attack(1) > fitted.attack(2));
        // Defense: lower beta = stronger.
        assert!(fitted.defense(0) < fitted.defense(1));
        assert!(fitted.defense(1) < fitted.defense(2));
    }

    use rand::{rngs::SmallRng, Rng, SeedableRng};

    fn sample_poisson(rng: &mut SmallRng, lambda: f64) -> i64 {
        let l = lambda.max(1e-3);
        // Knuth's algorithm.
        let lmax = f64::exp(-l);
        let mut k = 0;
        let mut p = 1.0;
        loop {
            k += 1;
            p *= rng.gen::<f64>();
            if p <= lmax {
                break;
            }
            if k > 50 {
                break;
            }
        }
        k - 1
    }
}
