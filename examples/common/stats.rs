//! Paired-comparison statistics shared by `examples/compare.rs` and
//! `examples/market.rs`, included by each with
//! `#[path = "common/stats.rs"] mod stats;` (an `examples/` subdirectory
//! without a `main.rs` is not an example of its own).
//!
//! Every function takes per-block sums of B − A with their row counts, so a
//! mean is always over rows, never over blocks: date blocks for the paired
//! bootstrap, seasons for the cluster t interval and the cluster bootstrap.
//! The numbers are what `compare` printed before they moved here; its tests
//! cover them.

// Each including example uses only part of this module.
#![allow(dead_code)]

use std::collections::BTreeMap;

/// Per key, in key order: the summed B − A of each aspect over the rows
/// `(key, a, b)` with that key, and their number.
pub fn blocks_by<K: Ord, const N: usize>(
    rows: impl Iterator<Item = (K, [f64; N], [f64; N])>,
) -> Vec<([f64; N], usize)> {
    let mut by_key: BTreeMap<K, ([f64; N], usize)> = BTreeMap::new();
    for (key, a, b) in rows {
        let block = by_key.entry(key).or_insert(([0.0; N], 0));
        for (k, sum) in block.0.iter_mut().enumerate() {
            *sum += b[k] - a[k];
        }
        block.1 += 1;
    }
    by_key.into_values().collect()
}

/// Student's t 0.975 quantiles, `T975[df - 1]` for 1 to 60 degrees of
/// freedom.
pub const T975: [f64; 60] = [
    12.7062, 4.3027, 3.1824, 2.7764, 2.5706, 2.4469, 2.3646, 2.3060, 2.2622, 2.2281, 2.2010,
    2.1788, 2.1604, 2.1448, 2.1314, 2.1199, 2.1098, 2.1009, 2.0930, 2.0860, 2.0796, 2.0739, 2.0687,
    2.0639, 2.0595, 2.0555, 2.0518, 2.0484, 2.0452, 2.0423, 2.0395, 2.0369, 2.0345, 2.0322, 2.0301,
    2.0281, 2.0262, 2.0244, 2.0227, 2.0211, 2.0195, 2.0181, 2.0167, 2.0154, 2.0141, 2.0129, 2.0117,
    2.0106, 2.0096, 2.0086, 2.0076, 2.0066, 2.0057, 2.0049, 2.0040, 2.0032, 2.0025, 2.0017, 2.0010,
    2.0003,
];

/// The 0.975 quantile of Student's t with `df` degrees of freedom. Past
/// the table df 60's stands in, within 2.1% of the normal's 1.96.
pub fn t975(df: usize) -> f64 {
    assert!(df >= 1, "Student's t needs a degree of freedom");
    T975[df.min(T975.len()) - 1]
}

/// 95% season-cluster t interval for a ratio-of-sums mean, from each
/// season's summed difference and row count `(S_g, N_g)`: the mean is
/// Σ S_g / Σ N_g, its cluster-robust standard error
/// √(G / (G − 1) · Σ (S_g − mean · N_g)²) / Σ N_g, and the quantile t's
/// with G − 1 degrees of freedom. `None` for fewer than two seasons.
pub fn season_t_ci(seasons: &[(f64, usize)]) -> Option<(f64, f64)> {
    if seasons.len() < 2 {
        return None;
    }
    let g = seasons.len() as f64;
    let n = seasons.iter().map(|s| s.1).sum::<usize>() as f64;
    let mean = seasons.iter().map(|s| s.0).sum::<f64>() / n;
    let ss: f64 = seasons
        .iter()
        .map(|&(sum, rows)| (sum - mean * rows as f64).powi(2))
        .sum();
    let half = t975(seasons.len() - 1) * (g / (g - 1.0) * ss).sqrt() / n;
    Some((mean - half, mean + half))
}

/// `season_t_ci` of each aspect, from per-season blocks.
pub fn season_t_cis<const N: usize>(blocks: &[([f64; N], usize)]) -> [Option<(f64, f64)>; N] {
    std::array::from_fn(|k| {
        let seasons: Vec<(f64, usize)> = blocks.iter().map(|b| (b.0[k], b.1)).collect();
        season_t_ci(&seasons)
    })
}

/// An interval as printed, `n/a` when there is none.
pub fn interval_text(ci: Option<(f64, f64)>) -> String {
    match ci {
        Some((lo, hi)) => format!("[{lo:+.5}, {hi:+.5}]"),
        None => "n/a".to_string(),
    }
}

/// SplitMix64: a small fixed generator, so a seed gives the same intervals
/// on every platform and with every `rand` release.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform on `0..n` by multiply-shift; the bias is far below anything a
    /// few thousand dates could show.
    pub fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next()) * n as u128) >> 64) as usize
    }
}

/// Linear interpolation between order statistics of `sorted`.
pub fn quantile(sorted: &[f64], q: f64) -> f64 {
    let h = (sorted.len() - 1) as f64 * q;
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
}

/// 95% percentile intervals for each aspect's mean B − A. Each of
/// `resamples` draws picks as many blocks as there are, with replacement,
/// and takes the mean over all the rows of the blocks it picked.
pub fn bootstrap_ci<const N: usize>(
    blocks: &[([f64; N], usize)],
    resamples: usize,
    seed: u64,
) -> [(f64, f64); N] {
    let mut rng = SplitMix64(seed);
    let mut draws: [Vec<f64>; N] = std::array::from_fn(|_| Vec::with_capacity(resamples));
    for _ in 0..resamples {
        let (mut sums, mut count) = ([0.0; N], 0usize);
        for _ in 0..blocks.len() {
            let (block, n) = &blocks[rng.below(blocks.len())];
            for k in 0..N {
                sums[k] += block[k];
            }
            count += n;
        }
        for k in 0..N {
            draws[k].push(sums[k] / count as f64);
        }
    }
    draws.map(|mut d| {
        d.sort_by(f64::total_cmp);
        (quantile(&d, 0.025), quantile(&d, 0.975))
    })
}

/// Diebold-Mariano-style t: the mean of the per-date summed differences
/// over its standard error across dates.
pub fn dm_t(sums: &[f64]) -> f64 {
    let d = sums.len() as f64;
    let mean = sums.iter().sum::<f64>() / d;
    let var = sums.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (d - 1.0);
    mean / (var / d).sqrt()
}

/// A season label, `2016-17`, from its end year.
pub fn season_label(year: i32) -> String {
    format!("{}-{:02}", year - 1, year % 100)
}
