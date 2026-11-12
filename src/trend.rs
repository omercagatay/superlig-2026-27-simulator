//! The season forecast, written down week by week.
//!
//! `data/forecast_log.json` freezes what the model said about each match.
//! This is the same for the season: every weekly run of
//! `examples/log_forecast.rs` adds one snapshot of the default forecast (each
//! club's title, top-two, top-four and relegation chance, expected points and
//! mean finishing place) to `data/season_log.json`, and a snapshot is never
//! rewritten. The site reads it to show how a club's chances have moved, and
//! at the season's end it is the record of what was published, to be scored
//! against the final table.
//!
//! A snapshot marked `reconstructed` was not published when the league stood
//! there: it was computed afterwards, by the model its `model_version` names,
//! from the results up to `latest_result`. The log starts with one for the
//! preseason and one for each matchday completed before logging began, so the
//! record of how the race moved does not start mid-season. They show what
//! that model would have said, and are left out of any scoring of what was
//! published.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::forecast_cache::{DEFAULT_N_SIMS, DEFAULT_SEED};
use crate::sim::{SimConfig, World};

pub const SEASON_LOG_PATH: &str = "data/season_log.json";

/// One club's season in one snapshot. Percentages as the site shows them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClubOutlook {
    pub team: String,
    pub title_pct: f64,
    /// Finishing first or second.
    pub top_two_pct: f64,
    /// Finishing in the top four.
    pub top_four_pct: f64,
    /// Finishing in the bottom three.
    pub relegation_pct: f64,
    pub exp_points: f64,
    pub mean_position: f64,
}

/// The default forecast as it stood on one state of the league.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeasonSnapshot {
    /// Results the forecast stood on.
    pub played: usize,
    /// Calendar date of the latest of them; `None` before the first.
    pub latest_result: Option<String>,
    /// The day the snapshot was written down.
    pub logged_at: String,
    /// The model that made it (`model_version::current()` at the time).
    pub model_version: String,
    /// Computed after the fact rather than published at the time (see the
    /// module comment). Absent from the file when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reconstructed: bool,
    pub n_sims: usize,
    pub seed: u64,
    /// One entry per club, in `data::clubs()` order.
    pub clubs: Vec<ClubOutlook>,
}

/// What `GET /api/trend` serves: the log, oldest snapshot first.
#[derive(Clone, Debug, Serialize)]
pub struct TrendResponse {
    pub snapshots: Vec<SeasonSnapshot>,
}

/// A figure as the log stores it: to three decimal places. A share of the
/// default forecast's 50,000 seasons is a multiple of 0.002%, so nothing the
/// site shows is lost, and a snapshot reads back from the file exactly.
fn logged(x: f64) -> f64 {
    (x * 1e3).round() / 1e3
}

/// The default forecast of `world`, as a snapshot logged on `logged_at`.
pub fn snapshot(
    world: &World,
    logged_at: &str,
    model_version: &str,
    reconstructed: bool,
) -> SeasonSnapshot {
    snapshot_with(
        world,
        DEFAULT_N_SIMS,
        DEFAULT_SEED,
        logged_at,
        model_version,
        reconstructed,
    )
}

/// As `snapshot`, from a run of `n_sims` seasons at `seed`: the seam the
/// tests use, since the default 50,000 seasons are slow in a debug build.
pub fn snapshot_with(
    world: &World,
    n_sims: usize,
    seed: u64,
    logged_at: &str,
    model_version: &str,
    reconstructed: bool,
) -> SeasonSnapshot {
    let config = SimConfig {
        n_sims,
        seed,
        ..SimConfig::default()
    };
    let results = world.simulate(&config);
    // The same rows the site shows, so a snapshot cannot disagree with it.
    let response = crate::models::build_response(world, &results, &config, None);
    let clubs = world
        .teams
        .iter()
        .map(|team| {
            let row = response
                .teams
                .iter()
                .find(|row| &row.team == team)
                .expect("the forecast has a row for every club");
            ClubOutlook {
                team: team.clone(),
                title_pct: logged(row.title_pct),
                top_two_pct: logged(row.top_two_pct),
                top_four_pct: logged(row.top_four_pct),
                relegation_pct: logged(row.relegation_pct),
                exp_points: logged(row.exp_points),
                mean_position: logged(row.mean_position),
            }
        })
        .collect();
    SeasonSnapshot {
        played: world.played.len(),
        latest_result: latest_result(world),
        logged_at: logged_at.to_string(),
        model_version: model_version.to_string(),
        reconstructed,
        n_sims,
        seed,
        clubs,
    }
}

/// Calendar date of the latest recorded result.
fn latest_result(world: &World) -> Option<String> {
    world
        .fixtures
        .iter()
        .zip(&world.dates)
        .filter(|(f, _)| world.played.contains_key(&(f.home, f.away)))
        .map(|(_, d)| d.date.clone())
        .max()
}

/// Whether `old` already records the forecast `fresh` makes: the same state
/// of the league, the same model and run, the same figures. The day it was
/// written down is not part of what it says.
fn same_forecast(old: &SeasonSnapshot, fresh: &SeasonSnapshot) -> bool {
    old.played == fresh.played
        && old.latest_result == fresh.latest_result
        && old.model_version == fresh.model_version
        && (old.n_sims, old.seed) == (fresh.n_sims, fresh.seed)
        && old.clubs == fresh.clubs
}

/// Add `fresh` to the end of `log` unless the latest snapshot already says
/// the same, and report whether it was added. A snapshot already in the log
/// is never touched, so two runs between matchdays log one snapshot, and a
/// run after new results, a corrected result or a model change logs another.
pub fn append(log: &mut Vec<SeasonSnapshot>, fresh: SeasonSnapshot) -> bool {
    if log.last().is_some_and(|last| same_forecast(last, &fresh)) {
        return false;
    }
    log.push(fresh);
    true
}

/// `world` as it stood at the end of `date` (ISO `YYYY-MM-DD`): results of
/// fixtures dated later are forgotten and the ratings replayed over the
/// rest. `None` is the preseason, with nothing played.
pub fn world_at(world: &World, date: Option<&str>) -> World {
    let mut then = world.clone();
    then.played.clear();
    if let Some(date) = date {
        for (f, d) in world.fixtures.iter().zip(&world.dates) {
            let key = (f.home, f.away);
            if d.date.as_str() <= date {
                if let Some(&score) = world.played.get(&key) {
                    then.played.insert(key, score);
                }
            }
        }
    }
    then.refresh_ratings();
    then
}

/// The states to reconstruct when the log is started mid-season: the
/// preseason, then the end of the last day of every matchday whose games are
/// all played, in date order and without repeats.
pub fn completed_matchday_dates(world: &World) -> Vec<Option<String>> {
    let mut last_day: std::collections::BTreeMap<u8, Option<String>> = Default::default();
    for (f, d) in world.fixtures.iter().zip(&world.dates) {
        let day = last_day
            .entry(f.round)
            .or_insert_with(|| Some(String::new()));
        if !world.played.contains_key(&(f.home, f.away)) {
            // One game still to play: the matchday is not complete.
            *day = None;
        } else if let Some(latest) = day {
            if d.date > *latest {
                latest.clone_from(&d.date);
            }
        }
    }
    let mut dates: Vec<String> = last_day.into_values().flatten().collect();
    dates.sort();
    dates.dedup();
    std::iter::once(None)
        .chain(dates.into_iter().map(Some))
        .collect()
}

fn load_log() -> Vec<SeasonSnapshot> {
    serde_json::from_str(include_str!("../data/season_log.json")).expect(
        "data/season_log.json is embedded at build time and must parse; \
         restore it from git rather than shipping an empty trend",
    )
}

/// The day the newest snapshot of the embedded log was written.
pub fn latest_logged_at() -> Option<String> {
    load_log().into_iter().map(|s| s.logged_at).max()
}

/// The body of `GET /api/trend`, serialized once: the log is embedded, so it
/// cannot change while the server runs.
pub fn response_json() -> &'static [u8] {
    static BODY: OnceLock<Vec<u8>> = OnceLock::new();
    BODY.get_or_init(|| {
        serde_json::to_vec(&TrendResponse {
            snapshots: load_log(),
        })
        .expect("the season log serializes")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;
    use crate::sim::MID_SEASON;

    /// Enough seasons for every figure to be a real share, few enough for a
    /// debug build.
    const TEST_SIMS: usize = 300;

    fn test_snapshot(world: &World) -> SeasonSnapshot {
        snapshot_with(world, TEST_SIMS, 7, "2026-11-12", "test-model", false)
    }

    #[test]
    fn a_snapshot_says_what_the_forecast_says() {
        let world = World::with_played(MID_SEASON);
        let snap = test_snapshot(&world);
        let config = SimConfig {
            n_sims: TEST_SIMS,
            seed: 7,
            ..SimConfig::default()
        };
        let response =
            crate::models::build_response(&world, &world.simulate(&config), &config, None);

        assert_eq!(snap.played, MID_SEASON);
        assert_eq!((snap.n_sims, snap.seed), (TEST_SIMS, 7));
        let teams: Vec<&str> = snap.clubs.iter().map(|c| c.team.as_str()).collect();
        assert_eq!(teams, data::clubs(), "one entry per club, in club order");
        for club in &snap.clubs {
            let row = response.teams.iter().find(|r| r.team == club.team).unwrap();
            let close = |logged: f64, shown: f64| (logged - shown).abs() <= 0.0005;
            assert!(close(club.title_pct, row.title_pct), "{}", club.team);
            assert!(close(club.top_two_pct, row.top_two_pct), "{}", club.team);
            assert!(close(club.top_four_pct, row.top_four_pct), "{}", club.team);
            assert!(
                close(club.relegation_pct, row.relegation_pct),
                "{}",
                club.team
            );
            assert!(close(club.exp_points, row.exp_points), "{}", club.team);
            assert!(
                close(club.mean_position, row.mean_position),
                "{}",
                club.team
            );
        }
    }

    #[test]
    fn a_snapshot_dates_the_state_it_stands_on() {
        assert_eq!(test_snapshot(&World::with_played(0)).latest_result, None);
        let world = World::with_played(MID_SEASON);
        let latest = world
            .fixtures
            .iter()
            .zip(&world.dates)
            .filter(|(f, _)| world.played.contains_key(&(f.home, f.away)))
            .map(|(_, d)| d.date.clone())
            .max();
        assert!(latest.is_some());
        assert_eq!(test_snapshot(&world).latest_result, latest);
    }

    #[test]
    fn a_snapshot_reads_back_from_the_log_exactly() {
        let log = vec![
            test_snapshot(&World::with_played(0)),
            SeasonSnapshot {
                reconstructed: true,
                ..test_snapshot(&World::with_played(MID_SEASON))
            },
        ];
        let text = serde_json::to_string_pretty(&log).unwrap();
        let back: Vec<SeasonSnapshot> = serde_json::from_str(&text).unwrap();
        assert_eq!(back, log);
        assert_eq!(serde_json::to_string_pretty(&back).unwrap(), text);
        // The flag is written only where it is set.
        assert_eq!(text.matches("\"reconstructed\": true").count(), 1);
        assert_eq!(text.matches("reconstructed").count(), 1);
    }

    #[test]
    fn an_unchanged_forecast_is_not_logged_twice() {
        let world = World::with_played(MID_SEASON);
        let mut log = vec![test_snapshot(&world)];
        // The next day's run, with nothing new: the first entry stands.
        let again = SeasonSnapshot {
            logged_at: "2026-11-13".to_string(),
            ..test_snapshot(&world)
        };
        assert!(!append(&mut log, again));
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].logged_at, "2026-11-12");
    }

    #[test]
    fn new_results_or_a_new_model_are_a_new_snapshot() {
        let mut log = Vec::new();
        assert!(append(
            &mut log,
            test_snapshot(&World::with_played(MID_SEASON))
        ));
        assert!(append(
            &mut log,
            test_snapshot(&World::with_played(MID_SEASON + 9))
        ));
        let relabelled = SeasonSnapshot {
            model_version: "next-model".to_string(),
            ..test_snapshot(&World::with_played(MID_SEASON + 9))
        };
        assert!(append(&mut log, relabelled));
        assert_eq!(log.len(), 3);
        // Earlier entries are untouched.
        assert_eq!(log[0], test_snapshot(&World::with_played(MID_SEASON)));
        assert_eq!(log[1].model_version, "test-model");
    }

    #[test]
    fn world_at_forgets_what_came_later() {
        let world = World::with_played(MID_SEASON);
        let mut days: Vec<&str> = world
            .fixtures
            .iter()
            .zip(&world.dates)
            .filter(|(f, _)| world.played.contains_key(&(f.home, f.away)))
            .map(|(_, d)| d.date.as_str())
            .collect();
        days.sort();
        days.dedup();
        assert!(days.len() > 2, "the part-played season spans several days");

        let preseason = world_at(&world, None);
        assert!(preseason.played.is_empty());
        assert_eq!(preseason.elo, preseason.elo_baseline);

        let mut before = 0;
        for day in &days {
            let then = world_at(&world, Some(day));
            assert!(then.played.len() > before, "{day} adds results");
            before = then.played.len();
            for (f, d) in then.fixtures.iter().zip(&then.dates) {
                let key = (f.home, f.away);
                let kept = then.played.get(&key);
                if d.date.as_str() <= *day {
                    assert_eq!(kept, world.played.get(&key));
                } else {
                    assert_eq!(kept, None, "{} is after {day}", d.date);
                }
            }
            // Ratings are the ones those results alone give.
            let mut replayed = then.clone();
            replayed.refresh_ratings();
            assert_eq!(then.elo, replayed.elo);
        }
        let last = world_at(&world, days.last().copied());
        assert_eq!(last.played, world.played);
        assert_eq!(last.elo, world.elo);
    }

    #[test]
    fn only_completed_matchdays_are_reconstructed() {
        assert_eq!(completed_matchday_dates(&World::with_played(0)), [None]);

        let world = World::with_played(MID_SEASON);
        let dates = completed_matchday_dates(&world);
        assert_eq!(dates[0], None);
        assert!(dates.len() > 1);
        let mut sorted = dates.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(dates, sorted, "in date order, without repeats");

        let complete = |round: u8| {
            world
                .fixtures
                .iter()
                .filter(|f| f.round == round)
                .all(|f| world.played.contains_key(&(f.home, f.away)))
        };
        let last_day = |round: u8| {
            world
                .fixtures
                .iter()
                .zip(&world.dates)
                .filter(|(f, _)| f.round == round)
                .map(|(_, d)| d.date.clone())
                .max()
        };
        for round in 1..=data::N_ROUNDS as u8 {
            let day = last_day(round);
            assert_eq!(
                dates.contains(&day),
                complete(round) || {
                    // Another, completed matchday may end on the same day.
                    (1..=data::N_ROUNDS as u8).any(|r| complete(r) && last_day(r) == day)
                },
                "matchday {round}"
            );
        }
        // One more result does not complete another matchday.
        let more = completed_matchday_dates(&World::with_played(MID_SEASON + 1));
        assert!(more.len() <= dates.len() + 1);
    }

    /// The shipped log is embedded and served as it is; a malformed one would
    /// panic at the first `/api/trend`. This parses it the way `load_log`
    /// does and checks what the site and the season-end scoring rely on.
    #[test]
    fn the_shipped_season_log_is_well_formed() {
        let log = load_log();
        assert!(!log.is_empty(), "data/season_log.json has no snapshots");
        let clubs = data::clubs();
        let mut published = false;
        let mut last_logged = String::new();
        for (i, snap) in log.iter().enumerate() {
            let at = format!("snapshot {i} ({} played)", snap.played);
            assert!(snap.played <= data::N_FIXTURES, "{at}");
            assert_eq!(snap.latest_result.is_none(), snap.played == 0, "{at}");
            assert_eq!(
                (snap.n_sims, snap.seed),
                (DEFAULT_N_SIMS, DEFAULT_SEED),
                "{at}"
            );
            assert!(
                crate::model_version::LABELS.contains(&snap.model_version.as_str()),
                "{at}: unknown model {:?}",
                snap.model_version
            );
            chrono::NaiveDate::parse_from_str(&snap.logged_at, "%Y-%m-%d")
                .unwrap_or_else(|e| panic!("{at}: logged_at {:?}: {e}", snap.logged_at));
            if let Some(date) = &snap.latest_result {
                chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .unwrap_or_else(|e| panic!("{at}: latest_result {date:?}: {e}"));
                assert!(date <= &snap.logged_at, "{at}: logged before its results");
            }
            // Reconstructed snapshots open the log; published ones follow in
            // the order they were written.
            if snap.reconstructed {
                assert!(!published, "{at}: reconstructed after a published one");
            } else {
                published = true;
                assert!(snap.logged_at >= last_logged, "{at}: out of order");
                last_logged.clone_from(&snap.logged_at);
            }

            let teams: Vec<&str> = snap.clubs.iter().map(|c| c.team.as_str()).collect();
            assert_eq!(teams, clubs, "{at}");
            for c in &snap.clubs {
                for pct in [c.title_pct, c.top_two_pct, c.top_four_pct, c.relegation_pct] {
                    assert!((0.0..=100.0).contains(&pct), "{at}: {} {pct}", c.team);
                }
                assert!(c.title_pct <= c.top_two_pct, "{at}: {}", c.team);
                assert!(c.top_two_pct <= c.top_four_pct, "{at}: {}", c.team);
                assert!((0.0..=102.0).contains(&c.exp_points), "{at}: {}", c.team);
                assert!((1.0..=18.0).contains(&c.mean_position), "{at}: {}", c.team);
            }
            // One champion, two top-two places, four top-four, three down.
            let total = |pick: fn(&ClubOutlook) -> f64| snap.clubs.iter().map(pick).sum::<f64>();
            let places = [
                (total(|c| c.title_pct), 1.0),
                (total(|c| c.top_two_pct), data::TOP_TWO_PLACES as f64),
                (total(|c| c.top_four_pct), data::TOP_FOUR_PLACES as f64),
                (total(|c| c.relegation_pct), data::RELEGATION_SPOTS as f64),
            ];
            for (sum, n) in places {
                assert!((sum - 100.0 * n).abs() < 0.02, "{at}: {sum} for {n} places");
            }
        }
    }

    /// Snapshots are frozen: reading the log and writing it back must not
    /// move a digit, or every weekly run would rewrite the record.
    #[test]
    fn the_shipped_season_log_rewrites_byte_for_byte() {
        let text = include_str!("../data/season_log.json");
        let log: Vec<SeasonSnapshot> = serde_json::from_str(text).unwrap();
        assert_eq!(serde_json::to_string_pretty(&log).unwrap(), text);
    }

    #[test]
    fn the_trend_response_is_the_log() {
        let body: serde_json::Value = serde_json::from_slice(response_json()).unwrap();
        let snapshots = body["snapshots"].as_array().unwrap();
        assert_eq!(snapshots.len(), load_log().len());
        assert_eq!(
            snapshots[0]["clubs"].as_array().unwrap().len(),
            data::N_TEAMS
        );
    }
}
