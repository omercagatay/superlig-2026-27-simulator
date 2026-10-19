//! Which model made a forecast.
//!
//! The forecast log is scored weeks after it is written, and the model can
//! change in between. Unlabelled, the tracker pools forecasts from different
//! models into one figure and credits the current model with all of them.
//! So every logged forecast carries the label of the model that made it,
//! `accuracy::report` scores each label separately as well as pooled, and
//! `/api/health` reports the running one.
//!
//! A label is `YYYY-MM`, the month the model shipped, with a letter when one
//! month shipped more than one. Labels sort in the order they shipped.
//!
//! | label      | model                                                       | in the log            |
//! |------------|-------------------------------------------------------------|-----------------------|
//! | `2026-08`  | August: ClubElo ratings of 2026-08-16 as the preseason prior, in-season `ELO_K` 20, `D_DIV` 1600, no underdog lift | logged 2026-08-17 and 2026-08-28 |
//! | `2026-10a` | October tuning: preseason ratings computed from results, `ELO_K` 60, `D_DIV` 1200, the underdog lift, Dixon-Coles refit without the 2023 walkovers; on the history that still lacked 2024-25 matchdays 10-12 | logged 2026-10-12 and 2026-10-16 |
//! | `2026-10b` | the same rules on the corrected history (2024-25 matchdays 10-12 recovered, Karagümrük under one name), Dixon-Coles refit | from 2026-10-19 |
//!
//! The log's entries before labels existed were labelled from their
//! `logged_at` and the commit each weekly run was made on: the 2026-10-12 run
//! (23:16 UTC) ran on a tree that already had the October tuning commits.
//! `2026-08` covers two code states: between its two runs (27 August) the
//! history was rebuilt, Dixon-Coles refit and the pi-rating pass changed to
//! update a whole match day at once, while the Elo side, half the blend,
//! stayed the same.
//!
//! `fingerprint()` hashes every input that moves a 1X2 forecast, and a test
//! pins it to `FINGERPRINT`, so a model change cannot ship under the old
//! label. A second test pins the model's preseason forecasts themselves, for
//! a change in code rather than in a constant or a data file.

/// The running model's label. Bump it, add the new label to `LABELS` and
/// update `FINGERPRINT` and `PROBE` together whenever forecasts change.
pub const LABEL: &str = "2026-10b";

/// Every label the forecast log has used, oldest first. The last is `LABEL`.
pub const LABELS: &[&str] = &["2026-08", "2026-10a", LABEL];

/// `fingerprint()` for the model `LABEL` names.
pub const FINGERPRINT: u64 = 0xd46b_9fb0_56c6_d09c;

/// The blend weights (Elo, Dixon-Coles, pi-ratings) the label names: the
/// fallback of `ensemble_weights()` when `ENSEMBLE_WEIGHTS` is unset.
/// Forecasts made under any other weights are a different model.
pub const DEFAULT_ENSEMBLE_WEIGHTS: (f64, f64, f64) = (0.5, 0.3, 0.2);

/// The label of the running model (`LABEL`).
pub fn current() -> &'static str {
    LABEL
}

/// A stable hash of every input that changes a 1X2 forecast: the Elo
/// model's constants, the default blend weights, the club list, the
/// Dixon-Coles fit, the results history behind the preseason and pi
/// ratings, and the constants of the pi-rating and Dixon-Coles modules.
/// `RATING_SIGMA` is left out: it widens simulated seasons, not a match
/// forecast. FNV-1a, so it is the same on every platform and build.
pub fn fingerprint() -> u64 {
    use crate::data;
    let mut h = Fnv::new();
    for (name, value) in [
        ("BASE", data::BASE),
        ("D_DIV", data::D_DIV),
        ("HOME_ADV", data::HOME_ADV),
        ("ELO_DIV", data::ELO_DIV),
        ("ELO_K", data::ELO_K),
        ("PRESEASON_K", data::PRESEASON_K),
        ("NEW_CLUB_ELO", data::NEW_CLUB_ELO),
        ("UNDERDOG_PIVOT", data::UNDERDOG_PIVOT),
        ("UNDERDOG_LIFT", data::UNDERDOG_LIFT),
        ("FAV_FOLLOW", data::FAV_FOLLOW),
        ("CUTOFF_YEAR", crate::history::CUTOFF_YEAR as f64),
        ("W_ELO", DEFAULT_ENSEMBLE_WEIGHTS.0),
        ("W_DC", DEFAULT_ENSEMBLE_WEIGHTS.1),
        ("W_PI", DEFAULT_ENSEMBLE_WEIGHTS.2),
    ] {
        h.text(name);
        h.bytes(&value.to_bits().to_le_bytes());
    }
    // Club order defines the indices the Dixon-Coles fit is read by.
    for club in data::clubs() {
        h.text(club);
    }
    h.file(include_str!("../data/dc_params.json"));
    h.file(include_str!("../data/superlig_results.csv"));
    for source in [include_str!("piratings.rs"), include_str!("dixoncoles.rs")] {
        for line in const_lines(source) {
            h.text(line);
        }
    }
    h.0
}

/// The module-level `const` lines of a source file, outside its tests. The
/// pi-rating and Dixon-Coles parameters are private to their modules, so
/// they are read from the source the build compiles.
fn const_lines(source: &str) -> impl Iterator<Item = &str> {
    let body = source.split("#[cfg(test)]").next().unwrap_or(source);
    body.lines().map(str::trim).filter(|l| {
        ["const ", "pub const ", "pub(crate) const "]
            .iter()
            .any(|p| l.starts_with(p))
    })
}

/// 64-bit FNV-1a: tiny, dependency-free and stable across platforms, which
/// is all a change detector needs.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Length-prefixed, so moving text from one field to the next still
    /// moves the hash.
    fn text(&mut self, s: &str) {
        self.bytes(&(s.len() as u64).to_le_bytes());
        self.bytes(s.as_bytes());
    }

    /// A data file, with carriage returns dropped: a Windows checkout of the
    /// same file is the same model.
    fn file(&mut self, content: &str) {
        let unix: Vec<u8> = content.bytes().filter(|&b| b != b'\r').collect();
        self.bytes(&(unix.len() as u64).to_le_bytes());
        self.bytes(&unix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash of the model's own preseason forecasts (`probe()`).
    const PROBE: u64 = 0x4522_bdaa_bfe1_e492;

    const BUMP: &str = "bump LABEL in src/model_version.rs, add it to LABELS, \
         and set FINGERPRINT and PROBE to the values these tests print";

    #[test]
    fn the_label_names_the_shipped_model_inputs() {
        let now = fingerprint();
        assert_eq!(
            now, FINGERPRINT,
            "model inputs changed (fingerprint {now:#018x}, labelled {FINGERPRINT:#018x}). \
             Forecasts logged from now on would be filed under {LABEL:?}, a model that \
             did not make them: {BUMP}"
        );
    }

    /// Every ordered pair of clubs priced preseason by the shipped blend,
    /// plus a grid of in-season Elo updates, rounded well below anything
    /// shown and hashed. Built only from the club list and the embedded
    /// history, never from the season's calendar or results.
    fn probe() -> u64 {
        let (w_elo, w_dc, w_pi) = DEFAULT_ENSEMBLE_WEIGHTS;
        let mut w = crate::sim::World::with_played(0);
        w.ensemble = Some(
            crate::sim::Ensemble::from_embedded_data(w_elo, w_dc, w_pi)
                .expect("the embedded ensemble data loads"),
        );
        let mut h = Fnv::new();
        let mut put = |v: f64| h.bytes(&((v * 1e6).round() as i64).to_le_bytes());
        for home in 0..w.teams.len() {
            for away in 0..w.teams.len() {
                if home != away {
                    let p = w.fixture_probs(home, away);
                    put(p.home_win_pct);
                    put(p.draw_pct);
                    put(p.away_win_pct);
                }
            }
        }
        for gap in [-300.0, -80.0, 0.0, 120.0, 400.0] {
            for (hs, as_) in [(0, 0), (1, 0), (0, 2), (4, 1)] {
                put(crate::sim::elo_delta(1500.0 + gap, 1500.0, hs, as_));
            }
        }
        h.0
    }

    /// For a change in code: a reworked blend, margin rule or score table
    /// moves the forecasts without touching anything `fingerprint` reads.
    #[test]
    fn the_label_names_the_shipped_model_forecasts() {
        let now = probe();
        assert_eq!(
            now, PROBE,
            "the model's preseason forecasts changed (probe {now:#018x}, labelled \
             {PROBE:#018x}) under the label {LABEL:?}: {BUMP}"
        );
    }

    #[test]
    fn labels_sort_in_the_order_they_shipped() {
        assert_eq!(current(), LABEL);
        assert_eq!(LABELS.last(), Some(&LABEL), "LABEL is the newest label");
        assert!(LABELS.windows(2).all(|p| p[0] < p[1]), "{LABELS:?}");
        for label in LABELS {
            let b = label.as_bytes();
            let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
            let ok = matches!(b.len(), 7 | 8)
                && digits(0..4)
                && b[4] == b'-'
                && digits(5..7)
                && b[7..].iter().all(u8::is_ascii_lowercase);
            assert!(ok, "{label:?} is not YYYY-MM with an optional letter");
        }
    }

    /// The label names the default blend, so the default must be the one
    /// `ensemble_weights()` falls back to. Skipped when the environment sets
    /// other weights, since that is the point of the variable.
    #[test]
    fn the_default_weights_are_the_ones_the_server_uses() {
        if std::env::var_os("ENSEMBLE_WEIGHTS").is_some() {
            return;
        }
        assert_eq!(crate::ensemble_weights(), DEFAULT_ENSEMBLE_WEIGHTS);
    }

    #[test]
    fn the_fingerprint_reads_the_private_model_constants() {
        let pi: Vec<&str> = const_lines(include_str!("piratings.rs")).collect();
        assert_eq!(pi.len(), 4, "{pi:?}");
        assert!(pi.iter().any(|l| l.starts_with("const LAMBDA: f64")));
        let dc: Vec<&str> = const_lines(include_str!("dixoncoles.rs")).collect();
        assert!(dc.iter().any(|l| l.starts_with("pub const MAX_GOALS")));
        assert!(dc.iter().any(|l| l.starts_with("const RHO_CLAMP")));
    }

    #[test]
    fn fnv_matches_the_reference_vectors() {
        let hash = |s: &str| {
            let mut h = Fnv::new();
            h.bytes(s.as_bytes());
            h.0
        };
        assert_eq!(hash(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(hash("foobar"), 0x8594_4171_f739_67e8);
        // A Windows checkout hashes the same.
        let (mut unix, mut windows) = (Fnv::new(), Fnv::new());
        unix.file("a,b\n1,2\n");
        windows.file("a,b\r\n1,2\r\n");
        assert_eq!(unix.0, windows.0);
    }
}
