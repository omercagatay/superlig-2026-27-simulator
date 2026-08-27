use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The official TFF fixture-and-standings page. Authoritative for the current
/// season and updated immediately; Wikipedia lags and is used only for
/// completed historical seasons.
const TFF_URL: &str = "https://www.tff.org/default.aspx?pageID=198";

/// TFF's sponsored legal names mapped to canonical club names. Must cover all
/// 18 current clubs; `parse_fixtures` skips anything unmapped rather than
/// guessing.
pub const TFF_NAMES: [(&str, &str); 18] = [
    ("AMED SPORTİF FAALİYETLER", "Amedspor"),
    ("ARCA ÇORUM FK", "Çorum"),
    ("BEŞİKTAŞ A.Ş.", "Beşiktaş"),
    ("CORENDON ALANYASPOR", "Alanyaspor"),
    ("ERZURUMSPOR FK", "Erzurumspor"),
    ("EYÜPSPOR", "Eyüpspor"),
    ("FENERBAHÇE A.Ş.", "Fenerbahçe"),
    ("GALATASARAY A.Ş.", "Galatasaray"),
    ("GAZİANTEP FUTBOL KULÜBÜ A.Ş.", "Gaziantep"),
    ("GENÇLERBİRLİĞİ", "Gençlerbirliği"),
    ("GÖZTEPE A.Ş.", "Göztepe"),
    ("KASIMPAŞA A.Ş.", "Kasımpaşa"),
    ("KOCAELİSPOR", "Kocaelispor"),
    ("SAMSUNSPOR A.Ş.", "Samsunspor"),
    ("TRABZONSPOR A.Ş.", "Trabzonspor"),
    ("TÜMOSAN KONYASPOR", "Konyaspor"),
    ("ÇAYKUR RİZESPOR A.Ş.", "Rizespor"),
    ("İSTANBUL BAŞAKŞEHİR FK", "Başakşehir"),
];

pub fn canonical_club(tff_name: &str) -> Option<&'static str> {
    let trimmed = tff_name.trim();
    TFF_NAMES
        .iter()
        .find(|(tff, _)| *tff == trimmed)
        .map(|(_, c)| *c)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScrapedMatch {
    pub round: u8,
    pub home: String,
    pub home_score: u16,
    pub away: String,
    pub away_score: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveData {
    pub played_matches: Vec<ScrapedMatch>,
    pub fetched_at: String,
}

#[derive(Default)]
struct ParsedPage {
    played_matches: Vec<ScrapedMatch>,
    total_rows: usize,
    recognized_rows: usize,
    fixtures_per_round: HashMap<u8, usize>,
    fixture_pairs: HashSet<(String, String)>,
}

/// Parse played fixtures out of the TFF fixture table.
///
/// Rows look like `<td>HOME</td><td><a macId=..>2 - 1</a></td><td>AWAY</td>`,
/// grouped under `N. Hafta` headings. Unplayed fixtures render the score cell
/// as `-` and are skipped — never coerced to 0-0.
fn parse_page(html: &str) -> ParsedPage {
    let week = Regex::new(r"(\d+)\.\s*Hafta").expect("valid regex");
    let row = Regex::new(
        r#"(?s)<td[^>]*class="altCizgi"[^>]*>\s*<a[^>]*kulupId=\d+[^>]*>(.*?)</a>\s*</td>\s*<td[^>]*>\s*<a[^>]*macId=\d+[^>]*>(.*?)</a>\s*</td>\s*<td[^>]*>\s*<a[^>]*kulupId=\d+[^>]*>(.*?)</a>\s*</td>"#,
    )
    .expect("valid regex");
    let score = Regex::new(r"^\s*(\d+)\s*-\s*(\d+)\s*$").expect("valid regex");

    // Walk weeks and rows in document order so each row inherits its heading.
    let mut boundaries: Vec<(usize, u8)> = week
        .captures_iter(html)
        .filter_map(|c| {
            let m = c.get(0)?;
            Some((m.start(), c.get(1)?.as_str().parse().ok()?))
        })
        .collect();
    boundaries.sort_by_key(|&(pos, _)| pos);

    let mut page = ParsedPage::default();
    for caps in row.captures_iter(html) {
        page.total_rows += 1;
        let at = caps.get(0).map_or(0, |m| m.start());
        let round = boundaries
            .iter()
            .rev()
            .find(|&&(pos, _)| pos <= at)
            .map(|&(_, w)| w)
            .unwrap_or(0);
        let (Some(home), Some(away)) = (canonical_club(&caps[1]), canonical_club(&caps[3])) else {
            continue;
        };
        page.recognized_rows += 1;
        *page.fixtures_per_round.entry(round).or_default() += 1;
        page.fixture_pairs
            .insert((home.to_string(), away.to_string()));
        let Some(s) = score.captures(&caps[2]) else {
            continue;
        };
        let (Ok(hs), Ok(as_)) = (s[1].parse::<u16>(), s[2].parse::<u16>()) else {
            continue;
        };
        page.played_matches.push(ScrapedMatch {
            round,
            home: home.to_string(),
            home_score: hs,
            away: away.to_string(),
            away_score: as_,
        });
    }
    page
}

pub fn parse_fixtures(html: &str) -> Vec<ScrapedMatch> {
    parse_page(html).played_matches
}

fn validate_page(page: &ParsedPage) -> Result<()> {
    anyhow::ensure!(
        page.total_rows == crate::data::N_FIXTURES,
        "TFF page contained {} fixture rows; expected {}",
        page.total_rows,
        crate::data::N_FIXTURES
    );
    anyhow::ensure!(
        page.recognized_rows == crate::data::N_FIXTURES,
        "recognized {} of {} TFF fixture rows; a club-name mapping may be stale",
        page.recognized_rows,
        page.total_rows
    );
    anyhow::ensure!(
        page.fixtures_per_round.len() == crate::data::N_ROUNDS
            && (1..=crate::data::N_ROUNDS as u8)
                .all(|round| page.fixtures_per_round.get(&round) == Some(&9)),
        "TFF page does not contain 34 complete nine-match rounds"
    );

    let expected: HashSet<(String, String)> = crate::data::fixtures()
        .into_iter()
        .map(|fixture| (fixture.home, fixture.away))
        .collect();
    let missing: Vec<_> = expected.difference(&page.fixture_pairs).take(3).collect();
    let unexpected: Vec<_> = page.fixture_pairs.difference(&expected).take(3).collect();
    anyhow::ensure!(
        page.fixture_pairs == expected,
        "TFF fixture set differs from the embedded calendar (missing {missing:?}, unexpected {unexpected:?})"
    );
    Ok(())
}

/// tff.org serves an incomplete TLS chain: the leaf only, without the
/// GlobalSign intermediate that signs it. Browsers paper over this by chasing
/// the certificate's AIA URL; reqwest does not, and every fetch fails with
/// "unable to get local issuer certificate". Supplying the intermediate the
/// server should have sent keeps full verification on — this is not a bypass.
/// Refresh with `scripts/fetch_fixtures.py`'s pinned AIA URL if it rotates.
const TFF_INTERMEDIATE_PEM: &[u8] = include_bytes!("../data/tff_intermediate.pem");

pub async fn fetch_all() -> Result<LiveData> {
    let intermediate = reqwest::Certificate::from_pem(TFF_INTERMEDIATE_PEM)
        .context("parsing the embedded TFF intermediate certificate")?;
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "superlig-sim/",
            env!("CARGO_PKG_VERSION"),
            " (educational project)"
        ))
        .timeout(std::time::Duration::from_secs(30))
        .add_root_certificate(intermediate)
        .build()?;

    let bytes = client
        .get(TFF_URL)
        .send()
        .await
        .context("fetching the TFF fixture page")?
        .error_for_status()
        .context("TFF fixture page returned an error status")?
        .bytes()
        .await?;

    // TFF serves windows-1254. Decoding as UTF-8 mangles every Turkish club
    // name, which would silently drop them all from the name mapping.
    let (html, _, _) = encoding_rs::WINDOWS_1254.decode(&bytes);

    let page = parse_page(&html);
    validate_page(&page).context("validating the TFF fixture page")?;
    let played_matches = page.played_matches;
    tracing::info!("TFF scrape: {} played fixtures", played_matches.len());

    Ok(LiveData {
        played_matches,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
      <td>1. Hafta</td>
      <tr>
        <td class="altCizgi"><a href="/Default.aspx?pageID=28&kulupId=3">GALATASARAY A.Ş.</a></td>
        <td><a href="/Default.aspx?pageID=29&macId=317790">2 - 2</a></td>
        <td class="altCizgi"><a href="/Default.aspx?pageID=28&kulupId=9">ARCA ÇORUM FK</a></td>
      </tr>
      <tr>
        <td class="altCizgi"><a href="/Default.aspx?pageID=28&kulupId=4">KASIMPAŞA A.Ş.</a></td>
        <td><a href="/Default.aspx?pageID=29&macId=317791"> - </a></td>
        <td class="altCizgi"><a href="/Default.aspx?pageID=28&kulupId=5">TRABZONSPOR A.Ş.</a></td>
      </tr>
    "#;

    #[test]
    fn parses_played_fixtures_with_canonical_names() {
        let ms = parse_fixtures(SAMPLE);
        assert_eq!(ms.len(), 1, "only the played fixture is returned");
        assert_eq!(ms[0].round, 1);
        assert_eq!(ms[0].home, "Galatasaray");
        assert_eq!(ms[0].away, "Çorum");
        assert_eq!((ms[0].home_score, ms[0].away_score), (2, 2));
    }

    /// The case historical seasons never exercise and the live page always
    /// will: an unplayed fixture renders its score cell as "-" and must be
    /// skipped, never coerced to 0-0.
    #[test]
    fn unplayed_fixtures_are_skipped_not_zeroed() {
        let ms = parse_fixtures(SAMPLE);
        assert!(
            !ms.iter().any(|m| m.home == "Kasımpaşa"),
            "an unplayed fixture must not appear as a 0-0 result"
        );
    }

    #[test]
    fn every_current_club_has_a_tff_name_mapping() {
        for (canonical, _) in crate::data::elo() {
            assert!(
                TFF_NAMES.iter().any(|(_, c)| *c == canonical),
                "no TFF name maps to {canonical}"
            );
        }
        assert_eq!(TFF_NAMES.len(), crate::data::N_TEAMS);
    }

    #[test]
    fn unknown_club_names_are_dropped_without_panicking() {
        let html = SAMPLE.replace("GALATASARAY A.Ş.", "SOME NEW CLUB A.Ş.");
        let ms = parse_fixtures(&html);
        assert!(ms.is_empty(), "unmapped clubs are skipped, not panicked on");
    }

    #[test]
    fn partial_or_unrecognized_pages_are_rejected_before_refresh() {
        assert!(validate_page(&parse_page(SAMPLE)).is_err());
        let unknown = SAMPLE.replace("GALATASARAY A.Ş.", "SOME NEW CLUB A.Ş.");
        assert!(validate_page(&parse_page(&unknown)).is_err());
    }
}
