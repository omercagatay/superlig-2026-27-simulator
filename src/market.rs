//! Bookmaker odds for Süper Lig fixtures, for comparison against the model.
//!
//! Source is Nesine's public pre-match bulletin (İddaa prices). This is a
//! *reference*, not an input: nothing here feeds the simulation. The point is
//! that where model and market disagree is the most informative thing on the
//! page — and that market prices carry information the model does not have
//! (team news, sharp money).
//!
//! Every failure mode degrades to "no market data": a bad fetch, a changed
//! schema, an unmapped club name, or a missing 1X2 market all simply leave
//! the fixture unpriced rather than breaking the response.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const BULLETIN_URL: &str = "https://cdnbulten.nesine.com/api/bulten/getprebultenfull";

/// Nesine's league code for the Trendyol Süper Lig.
const SUPER_LIG_LC: i64 = 584;
/// Market type id for the 1X2 (match result) market.
const MTID_1X2: i64 = 1;

#[derive(Deserialize)]
struct Bulletin {
    sg: EventGroup,
}

#[derive(Deserialize)]
struct EventGroup {
    #[serde(rename = "EA")]
    events: Vec<Event>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "LC")]
    league_code: Option<i64>,
    #[serde(rename = "HN")]
    home: Option<String>,
    #[serde(rename = "AN")]
    away: Option<String>,
    #[serde(rename = "MA", default)]
    markets: Vec<Market>,
}

#[derive(Deserialize)]
struct Market {
    #[serde(rename = "MTID")]
    market_type: Option<i64>,
    #[serde(rename = "OCA", default)]
    outcomes: Vec<Outcome>,
}

#[derive(Deserialize)]
struct Outcome {
    /// 1 = home, 2 = draw, 3 = away.
    #[serde(rename = "N")]
    number: Option<i64>,
    #[serde(rename = "O")]
    odds: Option<f64>,
}

/// Bookmaker prices for one fixture, keyed by canonical club names.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketPrices {
    pub home_odds: f64,
    pub draw_odds: f64,
    pub away_odds: f64,
}

impl MarketPrices {
    /// The book's overround (sum of implied probabilities). 1.0 would be a
    /// fair book; İddaa prices typically land near 1.10-1.20.
    pub fn overround(&self) -> f64 {
        1.0 / self.home_odds + 1.0 / self.draw_odds + 1.0 / self.away_odds
    }

    /// Implied probabilities with the margin divided out proportionally, as
    /// percentages summing to 100 — directly comparable to model output.
    pub fn implied_pct(&self) -> (f64, f64, f64) {
        let z = self.overround();
        (
            100.0 / self.home_odds / z,
            100.0 / self.draw_odds / z,
            100.0 / self.away_odds / z,
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketSnapshot {
    /// Keyed `(home, away)` with canonical club names.
    pub prices: HashMap<(String, String), MarketPrices>,
    pub fetched_at: String,
}

impl MarketSnapshot {
    pub fn get(&self, home: &str, away: &str) -> Option<&MarketPrices> {
        self.prices.get(&(home.to_string(), away.to_string()))
    }
}

/// Nesine spellings that do not fall out of `normalize` alone.
const ALIASES: [(&str, &str); 4] = [
    ("amed sk", "Amedspor"),
    ("amed sportif", "Amedspor"),
    ("corum fk", "Çorum"),
    ("rams basaksehir", "Başakşehir"),
];

/// Fold a bookmaker's club name onto our canonical one.
///
/// Bookmakers append and drop corporate suffixes at will ("Gaziantep FK",
/// "Çaykur Rizespor"), so compare on a stripped, ASCII-folded form and accept
/// a containment match in either direction. Anything still unmatched is
/// dropped — guessing would attach real money to the wrong fixture.
pub fn canonical_club(raw: &str) -> Option<&'static str> {
    let n = normalize(raw);
    if n.is_empty() {
        return None;
    }
    if let Some((_, c)) = ALIASES.iter().find(|(a, _)| *a == n) {
        return Some(c);
    }
    let mut hit: Option<&'static str> = None;
    for (club, _) in crate::data::elo() {
        let c = normalize(club);
        if c == n || n.contains(&c) || c.contains(&n) {
            // An ambiguous name (matching two clubs) is worse than none.
            if hit.is_some() {
                return None;
            }
            hit = Some(club);
        }
    }
    hit
}

fn normalize(s: &str) -> String {
    let folded: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'ı' | 'î' => 'i',
            'ş' => 's',
            'ğ' => 'g',
            'ü' => 'u',
            'ö' => 'o',
            'ç' => 'c',
            other => other,
        })
        .collect();
    let mut out = folded
        .replace("a.s.", " ")
        .replace(" as ", " ")
        .replace(" fk", " ")
        .replace(" sk", " ")
        .replace(" spor kulubu", " ");
    out.retain(|c| c.is_ascii_alphanumeric() || c == ' ');
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse a bulletin payload into canonical-name-keyed 1X2 prices.
pub fn parse_bulletin(body: &str) -> Result<HashMap<(String, String), MarketPrices>> {
    let b: Bulletin = serde_json::from_str(body).context("parsing the Nesine bulletin")?;
    let mut out = HashMap::new();
    for e in b.sg.events {
        if e.league_code != Some(SUPER_LIG_LC) {
            continue;
        }
        let (Some(raw_home), Some(raw_away)) = (e.home.as_deref(), e.away.as_deref()) else {
            continue;
        };
        let (Some(home), Some(away)) = (canonical_club(raw_home), canonical_club(raw_away)) else {
            tracing::debug!("market: unmapped club in {raw_home} v {raw_away}");
            continue;
        };
        let Some(m) = e.markets.iter().find(|m| m.market_type == Some(MTID_1X2)) else {
            continue;
        };
        let pick = |n: i64| {
            m.outcomes
                .iter()
                .find(|o| o.number == Some(n))
                .and_then(|o| o.odds)
                .filter(|v| *v > 1.0)
        };
        let (Some(h), Some(d), Some(a)) = (pick(1), pick(2), pick(3)) else {
            continue;
        };
        out.insert(
            (home.to_string(), away.to_string()),
            MarketPrices {
                home_odds: h,
                draw_odds: d,
                away_odds: a,
            },
        );
    }
    Ok(out)
}

pub async fn fetch() -> Result<MarketSnapshot> {
    let client = reqwest::Client::builder()
        .user_agent("superlig-sim/0.1 (educational project)")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let body = client
        .get(BULLETIN_URL)
        .send()
        .await
        .context("fetching the Nesine bulletin")?
        .text()
        .await?;
    let prices = parse_bulletin(&body)?;
    tracing::info!("market odds: {} Süper Lig fixtures priced", prices.len());
    Ok(MarketSnapshot {
        prices,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"sg":{"EA":[
      {"LC":584,"HN":"Fenerbahçe","AN":"Konyaspor","MA":[
        {"MTID":1,"OCA":[{"N":1,"O":1.12},{"N":2,"O":5.38},{"N":3,"O":8.87}]}]},
      {"LC":584,"HN":"Eyüpspor","AN":"Gaziantep FK","MA":[
        {"MTID":1,"OCA":[{"N":1,"O":2.10},{"N":2,"O":3.30},{"N":3,"O":3.50}]}]},
      {"LC":39172,"HN":"Beşiktaş","AN":"Zalgiris Kaunas","MA":[
        {"MTID":1,"OCA":[{"N":1,"O":1.20},{"N":2,"O":6.0},{"N":3,"O":12.0}]}]},
      {"LC":584,"HN":"Some Other Club","AN":"Galatasaray","MA":[
        {"MTID":1,"OCA":[{"N":1,"O":4.0},{"N":2,"O":3.6},{"N":3,"O":1.9}]}]}
    ]}}"#;

    #[test]
    fn parses_super_lig_fixtures_only() {
        let p = parse_bulletin(SAMPLE).expect("parses");
        // The European tie and the unmapped club are both dropped.
        assert_eq!(p.len(), 2);
        assert!(p.contains_key(&("Fenerbahçe".to_string(), "Konyaspor".to_string())));
        assert!(!p.contains_key(&("Beşiktaş".to_string(), "Zalgiris Kaunas".to_string())));
    }

    #[test]
    fn corporate_suffixes_fold_onto_canonical_names() {
        assert_eq!(canonical_club("Gaziantep FK"), Some("Gaziantep"));
        assert_eq!(canonical_club("Çaykur Rizespor"), Some("Rizespor"));
        assert_eq!(canonical_club("Galatasaray A.Ş."), Some("Galatasaray"));
        assert_eq!(canonical_club("Amed SK"), Some("Amedspor"));
        assert_eq!(canonical_club("Zalgiris Kaunas"), None);
        assert_eq!(canonical_club(""), None);
    }

    /// Every club we simulate must survive a round trip, or its fixtures
    /// would silently show no market price.
    #[test]
    fn every_current_club_maps_to_itself() {
        for (club, _) in crate::data::elo() {
            assert_eq!(canonical_club(club), Some(club), "{club} lost itself");
        }
    }

    #[test]
    fn implied_probabilities_strip_the_margin() {
        let p = parse_bulletin(SAMPLE).expect("parses")
            [&("Fenerbahçe".to_string(), "Konyaspor".to_string())]
            .clone();
        assert!(p.overround() > 1.0, "a real book carries a margin");
        let (h, d, a) = p.implied_pct();
        assert!((h + d + a - 100.0).abs() < 1e-9);
        assert!(h > d && d > a, "short price means likeliest outcome");
    }
}
