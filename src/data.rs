use serde::Deserialize;

/// Baseline expected goals for an evenly-matched fixture.
pub const BASE: f64 = 1.35;
/// Elo points corresponding to one decade of scoring-rate ratio.
pub const D_DIV: f64 = 1600.0;
/// Home-ground advantage, in Elo points, applied to the fixture's home side.
/// Used both when converting a rating gap to expected goals (via `D_DIV`) and
/// when forming a win expectancy for in-season rating updates (via `ELO_DIV`)
/// — it is the same quantity in rating points, only the conversion differs.
pub const HOME_ADV: f64 = 80.0;

/// Classic Elo denominator: a `ELO_DIV`-point edge is a 10:1 win expectancy.
/// Distinct from `D_DIV`, which maps a rating gap to a *scoring-rate* ratio.
pub const ELO_DIV: f64 = 400.0;

/// Elo update rate for one league match, before the goal-difference
/// multiplier. 20 is the usual club-football value and is what ClubElo-scale
/// ratings expect; larger values chase form, smaller ones ignore it.
pub const ELO_K: f64 = 20.0;

pub const N_TEAMS: usize = 18;
pub const N_ROUNDS: usize = 34;
pub const N_FIXTURES: usize = 306;

pub const UCL_SPOTS: usize = 2;
pub const UEL_SPOTS: usize = 1;
pub const UECL_SPOTS: usize = 1;
pub const EUROPE_SPOTS: usize = UCL_SPOTS + UEL_SPOTS + UECL_SPOTS;
pub const RELEGATION_SPOTS: usize = 3;

/// Compile-time: the European and relegation zones must not overlap, or
/// `simulate`'s position bucketing would count one club in both.
const _: () = {
    assert!(UCL_SPOTS + UEL_SPOTS + UECL_SPOTS == EUROPE_SPOTS);
    assert!(EUROPE_SPOTS + RELEGATION_SPOTS < N_TEAMS);
};

/// 2026-27 Trendyol Süper Lig clubs with ClubElo ratings as of 2026-08-16.
/// Order defines team indices throughout the simulator; `history::TeamIndex`
/// is built from this same order so Dixon-Coles and pi-rating indices agree.
pub fn elo() -> Vec<(&'static str, f64)> {
    vec![
        ("Galatasaray", 1779.0),
        ("Fenerbahçe", 1764.0),
        ("Beşiktaş", 1667.0),
        ("Amedspor", 1663.0),
        ("Trabzonspor", 1647.0),
        ("Başakşehir", 1616.0),
        ("Göztepe", 1607.0),
        ("Samsunspor", 1603.0),
        ("Erzurumspor", 1574.0),
        ("Gençlerbirliği", 1548.0),
        ("Rizespor", 1541.0),
        ("Alanyaspor", 1541.0),
        ("Çorum", 1539.0),
        ("Kocaelispor", 1538.0),
        ("Eyüpspor", 1538.0),
        ("Konyaspor", 1530.0),
        ("Kasımpaşa", 1527.0),
        ("Gaziantep", 1509.0),
    ]
}

/// One scheduled league fixture. `home_score`/`away_score` are `Some` once the
/// match has actually been played; the simulator then uses the real result
/// instead of sampling one.
#[derive(Clone, Debug, Deserialize)]
pub struct Fixture {
    pub round: u8,
    pub home: String,
    pub away: String,
    pub home_score: Option<u16>,
    pub away_score: Option<u16>,
    /// Kick-off date, ISO `YYYY-MM-DD`. TFF publishes the whole calendar up
    /// front, so this is present for every fixture.
    pub date: String,
    /// Kick-off time `HH:MM`; TFF fills it in only once a matchday is close.
    pub kickoff: Option<String>,
}

/// The official TFF 2026-27 calendar, refreshed by `scripts/fetch_fixtures.py`.
pub fn fixtures() -> Vec<Fixture> {
    serde_json::from_str(include_str!("../data/fixtures_2026_27.json"))
        .expect("data/fixtures_2026_27.json is valid; regenerate with scripts/fetch_fixtures.py")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn elo_has_eighteen_distinct_clubs() {
        let e = elo();
        assert_eq!(e.len(), N_TEAMS);
        let names: HashSet<&str> = e.iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), N_TEAMS, "club names must be distinct");
    }

    #[test]
    fn fixtures_form_a_complete_double_round_robin() {
        let f = fixtures();
        assert_eq!(f.len(), N_FIXTURES);

        let known: HashSet<&str> = elo().iter().map(|(n, _)| *n).collect();
        let mut per_club: HashMap<&str, usize> = HashMap::new();
        let mut ordered: HashSet<(&str, &str)> = HashSet::new();
        let mut unordered: HashMap<(&str, &str), usize> = HashMap::new();
        let mut per_round: HashMap<u8, usize> = HashMap::new();

        for fx in &f {
            assert!(known.contains(fx.home.as_str()), "unknown club {}", fx.home);
            assert!(known.contains(fx.away.as_str()), "unknown club {}", fx.away);
            assert_ne!(fx.home, fx.away, "a club cannot play itself");
            *per_club.entry(fx.home.as_str()).or_default() += 1;
            *per_club.entry(fx.away.as_str()).or_default() += 1;
            *per_round.entry(fx.round).or_default() += 1;
            assert!(
                ordered.insert((fx.home.as_str(), fx.away.as_str())),
                "duplicate ordered pair {} v {}",
                fx.home,
                fx.away
            );
            let key = if fx.home < fx.away {
                (fx.home.as_str(), fx.away.as_str())
            } else {
                (fx.away.as_str(), fx.home.as_str())
            };
            *unordered.entry(key).or_default() += 1;
        }

        assert_eq!(per_club.len(), N_TEAMS);
        assert!(per_club.values().all(|&c| c == 34), "every club plays 34");
        assert_eq!(unordered.len(), 153);
        assert!(
            unordered.values().all(|&c| c == 2),
            "every pair meets twice"
        );
        assert_eq!(per_round.len(), N_ROUNDS);
        assert!(per_round.values().all(|&c| c == 9), "9 fixtures per round");
    }

    #[test]
    fn every_fixture_has_a_calendar_date_in_season_order() {
        let f = fixtures();
        for fx in &f {
            assert_eq!(fx.date.len(), 10, "ISO date for {} v {}", fx.home, fx.away);
            assert!(fx.date.starts_with("2026-") || fx.date.starts_with("2027-"));
        }
        // Rounds run in calendar order: each round's earliest date is no
        // earlier than the previous round's earliest.
        let mut first: Vec<(u8, &str)> = Vec::new();
        for fx in &f {
            match first.iter_mut().find(|(r, _)| *r == fx.round) {
                Some(slot) => {
                    if fx.date.as_str() < slot.1 {
                        slot.1 = &fx.date;
                    }
                }
                None => first.push((fx.round, &fx.date)),
            }
        }
        first.sort_by_key(|(r, _)| *r);
        for pair in first.windows(2) {
            assert!(
                pair[0].1 <= pair[1].1,
                "round {} starts {} but round {} starts {}",
                pair[0].0,
                pair[0].1,
                pair[1].0,
                pair[1].1
            );
        }
    }

    #[test]
    fn played_fixtures_have_both_scores_or_neither() {
        for fx in fixtures() {
            assert_eq!(
                fx.home_score.is_some(),
                fx.away_score.is_some(),
                "half-recorded score for {} v {}",
                fx.home,
                fx.away
            );
        }
    }
}
