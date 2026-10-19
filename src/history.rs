use std::collections::HashMap;
use std::io::Cursor;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::data;

pub const CUTOFF_YEAR: i32 = 2012;
pub const OTHER_TEAM_NAME: &str = "Other Club";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoricalMatch {
    pub date: NaiveDate,
    pub home_team: String,
    pub away_team: String,
    pub home_score: u16,
    pub away_score: u16,
    pub neutral: bool,
}

#[derive(Clone, Debug)]
pub struct TeamIndex {
    pub name_to_idx: HashMap<String, usize>,
    pub idx_to_name: Vec<String>,
    pub league_names: Vec<String>,
    pub other_idx: usize,
}

impl TeamIndex {
    /// Built from `data::clubs()` order so Dixon-Coles and pi-rating team
    /// indices coincide with `World` indices, plus a trailing bucket.
    ///
    /// The bucket absorbs every match of every club not in the current 18 —
    /// relegated and defunct sides alike. It is a *league-average departed
    /// club*, not a newly-promoted baseline.
    pub fn league() -> Self {
        let league_names: Vec<String> = data::clubs().iter().map(|t| t.to_string()).collect();
        let mut idx_to_name = league_names.clone();
        idx_to_name.push(OTHER_TEAM_NAME.to_string());
        let other_idx = idx_to_name.len() - 1;
        let name_to_idx = idx_to_name
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        TeamIndex {
            name_to_idx,
            idx_to_name,
            league_names,
            other_idx,
        }
    }

    pub fn canonical(&self, team: &str) -> usize {
        self.name_to_idx
            .get(team)
            .copied()
            .unwrap_or(self.other_idx)
    }
}

#[derive(Clone, Debug)]
pub struct FitMatch {
    pub home_idx: usize,
    pub away_idx: usize,
    pub home_score: u16,
    pub away_score: u16,
    pub neutral: bool,
    pub weight: f64,
    pub days_ago: i64,
}

pub fn load_history() -> Vec<HistoricalMatch> {
    load_history_with_cutoff(CUTOFF_YEAR)
}

pub fn load_history_with_cutoff(min_year: i32) -> Vec<HistoricalMatch> {
    static CSV: OnceLock<Vec<u8>> = OnceLock::new();
    let bytes = CSV.get_or_init(|| include_bytes!("../data/superlig_results.csv").to_vec());
    parse_csv(bytes, min_year)
}

#[derive(serde::Deserialize)]
struct CsvRow {
    date: String,
    home_team: String,
    away_team: String,
    home_score: String,
    away_score: String,
    #[serde(rename = "tournament")]
    _tournament: String,
    #[serde(rename = "city")]
    _city: String,
    #[serde(rename = "country")]
    _country: String,
    neutral: String,
}

fn parse_csv(bytes: &[u8], min_year: i32) -> Vec<HistoricalMatch> {
    let out: Vec<HistoricalMatch> = parse_rows(bytes, min_year)
        .into_iter()
        .filter(|m| !is_unplayed_award(m.date, &m.home_team, &m.away_team))
        .collect();
    tracing::info!(
        "Loaded {} historical matches (cutoff {}+)",
        out.len(),
        min_year
    );
    out
}

/// Every well-formed CSV row from `min_year` on, unplayed awards included.
fn parse_rows(bytes: &[u8], min_year: i32) -> Vec<HistoricalMatch> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(false)
        .has_headers(true)
        .from_reader(Cursor::new(bytes));
    let mut out = Vec::new();
    for rec in rdr.deserialize::<CsvRow>() {
        let Ok(r) = rec else { continue };
        let Some(date) = parse_date(&r.date) else {
            continue;
        };
        if date.year() < min_year {
            continue;
        }
        let Ok(hs) = r.home_score.trim().parse::<u16>() else {
            continue;
        };
        let Ok(as_) = r.away_score.trim().parse::<u16>() else {
            continue;
        };
        let neutral = matches!(r.neutral.trim().to_ascii_uppercase().as_str(), "TRUE" | "1");
        out.push(HistoricalMatch {
            date,
            home_team: r.home_team,
            away_team: r.away_team,
            home_score: hs,
            away_score: as_,
            neutral,
        });
    }
    out
}

/// Results recorded but never played. After the 6 February 2023 earthquake
/// Hatayspor and Gaziantep withdrew, and every remaining 2022-23 fixture of
/// theirs was awarded 3-0. Those scorelines say nothing about strength.
fn is_unplayed_award(date: NaiveDate, home: &str, away: &str) -> bool {
    let quake = NaiveDate::from_ymd_opt(2023, 2, 6).expect("valid date");
    let season_end = NaiveDate::from_ymd_opt(2023, 7, 31).expect("valid date");
    (quake..=season_end).contains(&date)
        && [home, away]
            .iter()
            .any(|t| *t == "Hatayspor" || *t == "Gaziantep")
}

fn parse_date(raw: &str) -> Option<NaiveDate> {
    let s = raw.trim();
    ["%Y-%m-%d", "%m/%d/%Y", "%-m/%-d/%Y"]
        .iter()
        .find_map(|fmt| NaiveDate::parse_from_str(s, fmt).ok())
}

pub fn prepare_fit_matches(
    history: &[HistoricalMatch],
    idx: &TeamIndex,
    half_life_days: f64,
    as_of: NaiveDate,
) -> Vec<FitMatch> {
    let xi = if half_life_days > 0.0 {
        std::f64::consts::LN_2 / half_life_days
    } else {
        0.0
    };
    let mut out = Vec::with_capacity(history.len());
    for m in history {
        if m.date > as_of {
            continue;
        }
        let h = idx.canonical(&m.home_team);
        let a = idx.canonical(&m.away_team);
        let days_ago = (as_of - m.date).num_days().max(0);
        let weight = (-xi * days_ago as f64).exp();
        out.push(FitMatch {
            home_idx: h,
            away_idx: a,
            home_score: m.home_score,
            away_score: m.away_score,
            neutral: m.neutral,
            weight,
            days_ago,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_maps_known_club_and_bucket_for_unknown() {
        let idx = TeamIndex::league();
        assert_eq!(idx.canonical("Galatasaray"), idx.name_to_idx["Galatasaray"]);
        assert_eq!(idx.canonical("Sivasspor"), idx.other_idx);
        assert_eq!(idx.canonical("Nonexistent FC"), idx.other_idx);
        assert_eq!(idx.idx_to_name[idx.other_idx], OTHER_TEAM_NAME);
        assert_eq!(idx.idx_to_name.len(), crate::data::N_TEAMS + 1);
    }

    #[test]
    fn history_loads_and_uses_canonical_club_names() {
        let ms = load_history();
        assert!(
            ms.len() > 3_000,
            "expected a full history, got {}",
            ms.len()
        );
        for m in &ms {
            assert!(!m.home_team.is_empty());
            assert!(!m.neutral, "league matches are never on neutral ground");
        }
    }

    /// After the 6 February 2023 earthquake, Hatayspor's and Gaziantep's
    /// remaining fixtures were awarded, not played. The rows stay in the CSV
    /// and are dropped at load.
    #[test]
    fn earthquake_walkover_awards_are_not_treated_as_results() {
        let ms = load_history();
        let after_quake = |m: &&HistoricalMatch| {
            m.date >= NaiveDate::from_ymd_opt(2023, 2, 6).unwrap()
                && m.date <= NaiveDate::from_ymd_opt(2023, 7, 31).unwrap()
        };
        for club in ["Hatayspor", "Gaziantep"] {
            let involved = |m: &&HistoricalMatch| m.home_team == club || m.away_team == club;
            assert!(
                ms.iter().filter(after_quake).filter(involved).count() == 0,
                "{club}"
            );
            // Their real 2022-23 matches before the withdrawal are kept.
            assert!(ms
                .iter()
                .filter(|m| m.date.year() == 2023 && m.date.month() == 2 && m.date.day() < 6)
                .any(|m| involved(&m)));
        }
        // Everyone else's post-quake matches are untouched.
        assert!(ms.iter().filter(after_quake).count() > 100);
    }

    /// The name-drift regression net. Source display names change over time;
    /// an unmapped variant sends a real club's history into the bucket,
    /// silently weakening it. Asserting season coverage makes that loud.
    #[test]
    fn every_ever_present_club_has_full_season_coverage() {
        use std::collections::HashSet;
        let ms = load_history();
        let mut seasons: HashMap<&str, HashSet<i32>> = HashMap::new();
        for m in &ms {
            let season = season_of(m.date);
            seasons
                .entry(m.home_team.as_str())
                .or_default()
                .insert(season);
            seasons
                .entry(m.away_team.as_str())
                .or_default()
                .insert(season);
        }
        for club in ["Galatasaray", "Fenerbahçe", "Beşiktaş", "Trabzonspor"] {
            let n = seasons.get(club).map_or(0, |s| s.len());
            assert_eq!(n, 14, "{club} should appear in all 14 seasons, found {n}");
        }
        // Promoted clubs with no top-flight history — 0 is correct here.
        for club in ["Amedspor", "Çorum"] {
            assert_eq!(seasons.get(club).map_or(0, |s| s.len()), 0, "{club}");
        }
        // Every other current club must have at least one season, or its
        // alias mapping is broken.
        for name in crate::data::clubs() {
            if matches!(name, "Amedspor" | "Çorum") {
                continue;
            }
            assert!(
                seasons.get(name).map_or(0, |s| s.len()) >= 1,
                "{name} has no history — check the *_ALIASES tables in scripts/fetch_history.py"
            );
        }
        // One club, two sources: the archive (2020-21..2023-24, spelled
        // "Karagumruk" there) and TFF (2025-26). Under two names the club
        // re-entered 2025-26 at NEW_CLUB_ELO. It spent 2024-25 in the 1. Lig.
        assert_eq!(
            seasons.get("Karagümrük"),
            Some(&HashSet::from([2020, 2021, 2022, 2023, 2025]))
        );
        assert!(!seasons.contains_key("Karagumruk"));
    }

    const CSV: &[u8] = include_bytes!("../data/superlig_results.csv");

    /// The season a match belongs to, named for its starting year. August is
    /// the boundary; it keeps the pandemic-delayed July 2020 fixtures in
    /// 2019-20.
    fn season_of(date: NaiveDate) -> i32 {
        if date.month() >= 8 {
            date.year()
        } else {
            date.year() - 1
        }
    }

    /// Clubs in each top-flight season, by starting year.
    const CLUBS_PER_SEASON: &[(i32, usize)] = &[
        (2012, 18),
        (2013, 18),
        (2014, 18),
        (2015, 18),
        (2016, 18),
        (2017, 18),
        (2018, 18),
        (2019, 18),
        (2020, 21),
        (2021, 20),
        (2022, 19),
        (2023, 20),
        (2024, 19),
        (2025, 18),
    ];

    /// Fixtures a season of the CSV lacks on purpose, as (season, home, away,
    /// why). Any other missing or repeated pair fails the round-robin test.
    const NOT_IN_CSV: &[(i32, &str, &str, &str)] = &[(
        2024,
        "Galatasaray",
        "Ad. Demirspor",
        "2025-02-09: Adana Demirspor left the pitch in the first half and the \
         TFF awarded the match 3-0. The score was not decided on the pitch, \
         so fetch_history.py's EXCLUDED_AWARDS keeps it out of the CSV.",
    )];

    /// 2022-23 earthquake walkovers: in the CSV, dropped at load by
    /// `is_unplayed_award` (Hatayspor 15, Gaziantep 14; they had already met).
    const QUAKE_WALKOVERS: usize = 29;

    /// Every ordered pair of a season's clubs not met exactly once (a club
    /// never meets itself), as (season, home, away, times met), sorted.
    fn round_robin_gaps(rows: &[HistoricalMatch]) -> Vec<(i32, String, String, usize)> {
        use std::collections::{BTreeMap, BTreeSet};
        let mut seasons: BTreeMap<i32, HashMap<(&str, &str), usize>> = BTreeMap::new();
        for m in rows {
            *seasons
                .entry(season_of(m.date))
                .or_default()
                .entry((m.home_team.as_str(), m.away_team.as_str()))
                .or_default() += 1;
        }
        let mut gaps = Vec::new();
        for (&season, met) in &seasons {
            let clubs: BTreeSet<&str> = met.keys().flat_map(|&(h, a)| [h, a]).collect();
            for &h in &clubs {
                for &a in &clubs {
                    let n = met.get(&(h, a)).copied().unwrap_or(0);
                    if n != usize::from(h != a) {
                        gaps.push((season, h.to_string(), a.to_string(), n));
                    }
                }
            }
        }
        gaps
    }

    /// Every season of the CSV is a full double round-robin: each club hosts
    /// every other club exactly once, bar `NOT_IN_CSV`. The 2024-25 season
    /// once lacked three whole matchdays and nothing noticed.
    #[test]
    fn every_season_is_a_complete_double_round_robin() {
        let rows = parse_rows(CSV, 0);
        let lines = std::str::from_utf8(CSV)
            .expect("UTF-8")
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .count();
        assert_eq!(rows.len(), lines, "a CSV row did not parse");

        let mut clubs: HashMap<i32, std::collections::HashSet<&str>> = HashMap::new();
        for m in &rows {
            let season = clubs.entry(season_of(m.date)).or_default();
            season.insert(m.home_team.as_str());
            season.insert(m.away_team.as_str());
        }
        let mut found: Vec<(i32, usize)> = clubs.iter().map(|(&s, c)| (s, c.len())).collect();
        found.sort_unstable();
        assert_eq!(found, CLUBS_PER_SEASON, "seasons or club counts changed");

        let mut expected: Vec<(i32, String, String, usize)> = NOT_IN_CSV
            .iter()
            .map(|&(s, h, a, _)| (s, h.to_string(), a.to_string(), 0))
            .collect();
        expected.sort();
        assert_eq!(round_robin_gaps(&rows), expected);
    }

    /// What the model trains on: the CSV minus exactly the earthquake
    /// walkovers, each a 3-0 to the opponent of the withdrawn club.
    #[test]
    fn loading_drops_exactly_the_earthquake_walkovers() {
        let raw = parse_rows(CSV, CUTOFF_YEAR);
        let withdrawn = |t: &str| matches!(t, "Hatayspor" | "Gaziantep");
        let dropped: Vec<&HistoricalMatch> = raw
            .iter()
            .filter(|m| is_unplayed_award(m.date, &m.home_team, &m.away_team))
            .collect();
        assert_eq!(dropped.len(), QUAKE_WALKOVERS);
        for m in &dropped {
            assert_eq!(season_of(m.date), 2022);
            let awarded = if withdrawn(&m.home_team) {
                (0, 3)
            } else {
                (3, 0)
            };
            assert_eq!(
                (m.home_score, m.away_score),
                awarded,
                "{} {} v {}",
                m.date,
                m.home_team,
                m.away_team
            );
        }

        let loaded = load_history();
        assert_eq!(loaded.len(), raw.len() - QUAKE_WALKOVERS);
        let mut expected: Vec<(i32, String, String, usize)> = NOT_IN_CSV
            .iter()
            .map(|&(s, h, a, _)| (s, h.to_string(), a.to_string(), 0))
            .chain(
                dropped
                    .iter()
                    .map(|m| (2022, m.home_team.clone(), m.away_team.clone(), 0)),
            )
            .collect();
        expected.sort();
        assert_eq!(round_robin_gaps(&loaded), expected);
    }

    /// Names that fold together but are different clubs: Gaziantep FK (the
    /// former Gaziantep Büyükşehir Belediyespor, 2019-20 on) and Gaziantepspor
    /// (to 2016-17, since dissolved).
    const DISTINCT_LOOKALIKES: &[[&str; 2]] = &[["Gaziantep", "Gaziantepspor"]];

    /// A club name with case, Turkish letters, accents, punctuation, spaces
    /// and a trailing "spor" (which sources add or drop) removed.
    fn fold(name: &str) -> String {
        let mut out = String::new();
        // 'İ' lowercases to 'i' plus a combining dot, which is not
        // alphanumeric and so drops out below.
        for c in name.chars().flat_map(char::to_lowercase) {
            let c = match c {
                'ç' => 'c',
                'ğ' => 'g',
                'ı' | 'î' | 'ï' | 'í' => 'i',
                'ö' | 'ô' | 'ó' => 'o',
                'ş' => 's',
                'ü' | 'û' | 'ú' => 'u',
                'â' | 'á' | 'à' | 'ä' => 'a',
                'é' | 'è' | 'ê' | 'ë' => 'e',
                c => c,
            };
            if c.is_alphanumeric() {
                assert!(
                    c.is_ascii_alphanumeric(),
                    "{name}: teach fold() to plain {c:?} so its look-alikes are found"
                );
                out.push(c);
            }
        }
        match out.strip_suffix("spor") {
            Some(stem) if !stem.is_empty() => stem.to_string(),
            _ => out,
        }
    }

    /// The Karagumruk/Karagümrük class of bug: one club under two spellings
    /// is two clubs to the ratings. History names and the current clubs must
    /// not collide once folded, unless listed as distinct clubs.
    #[test]
    fn no_two_club_names_fold_together() {
        use std::collections::{BTreeMap, BTreeSet};
        assert_eq!(fold("Karagümrük"), fold("Karagumruk"));
        assert_eq!(fold("İstanbulspor"), fold("Istanbul"));
        assert_eq!(fold("Ad. Demirspor"), fold("Ad Demir"));

        let rows = parse_rows(CSV, 0);
        let names = rows
            .iter()
            .flat_map(|m| [m.home_team.as_str(), m.away_team.as_str()])
            .chain(crate::data::clubs());
        let mut by_key: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for name in names {
            by_key.entry(fold(name)).or_default().insert(name);
        }
        let allowed: BTreeSet<BTreeSet<&str>> = DISTINCT_LOOKALIKES
            .iter()
            .map(|pair| pair.iter().copied().collect())
            .collect();
        let collisions: BTreeSet<BTreeSet<&str>> =
            by_key.into_values().filter(|n| n.len() > 1).collect();
        // Equality, not a subset: a listed pair that no longer collides is a
        // stale entry.
        assert_eq!(collisions, allowed, "one club under two spellings?");
    }

    #[test]
    fn completed_2025_26_history_has_one_canonical_name_per_club() {
        use std::collections::HashSet;
        let start = NaiveDate::from_ymd_opt(2025, 8, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        let season: Vec<_> = load_history()
            .into_iter()
            .filter(|m| m.date >= start && m.date < end)
            .collect();
        assert_eq!(season.len(), crate::data::N_FIXTURES);

        let actual: HashSet<&str> = season
            .iter()
            .flat_map(|m| [m.home_team.as_str(), m.away_team.as_str()])
            .collect();
        let expected: HashSet<&str> = [
            "Alanyaspor",
            "Antalyaspor",
            "Başakşehir",
            "Beşiktaş",
            "Eyüpspor",
            "Fenerbahçe",
            "Galatasaray",
            "Gaziantep",
            "Gençlerbirliği",
            "Göztepe",
            "Karagümrük",
            "Kasımpaşa",
            "Kayserispor",
            "Kocaelispor",
            "Konyaspor",
            "Rizespor",
            "Samsunspor",
            "Trabzonspor",
        ]
        .into_iter()
        .collect();
        assert_eq!(actual, expected, "stale sponsor-name alias in history");
    }
}
