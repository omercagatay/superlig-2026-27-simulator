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
    /// Built from `data::elo()` order so Dixon-Coles and pi-rating team
    /// indices coincide with `World` indices, plus a trailing bucket.
    ///
    /// The bucket absorbs every match of every club not in the current 18 —
    /// relegated and defunct sides alike. It is a *league-average departed
    /// club*, not a newly-promoted baseline.
    pub fn league() -> Self {
        let league_names: Vec<String> = data::elo().iter().map(|(t, _)| t.to_string()).collect();
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
    tracing::info!(
        "Loaded {} historical matches (cutoff {}+)",
        out.len(),
        min_year
    );
    out
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

    /// The name-drift regression net. Wikipedia display names change between
    /// seasons ("İstanbul B.B." -> "Başakşehir"); an unmapped variant sends a
    /// real club's history into the bucket, silently weakening it. Asserting
    /// per-club season coverage is what makes that failure loud.
    #[test]
    fn every_ever_present_club_has_full_season_coverage() {
        use std::collections::HashSet;
        let ms = load_history();
        let mut seasons: HashMap<&str, HashSet<i32>> = HashMap::new();
        for m in &ms {
            seasons
                .entry(m.home_team.as_str())
                .or_default()
                .insert(m.date.year());
            seasons
                .entry(m.away_team.as_str())
                .or_default()
                .insert(m.date.year());
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
        for (name, _) in crate::data::elo() {
            if matches!(name, "Amedspor" | "Çorum") {
                continue;
            }
            assert!(
                seasons.get(name).map_or(0, |s| s.len()) >= 1,
                "{name} has no history — check ALIASES in scripts/fetch_history.py"
            );
        }
    }
}
