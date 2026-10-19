use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::sim::{SimConfig, SimResults};

#[derive(Deserialize, Clone)]
pub struct SimRequest {
    pub n_sims: Option<usize>,
    pub seed: Option<u64>,
    pub elo_overrides: Option<HashMap<String, f64>>,
    /// "Suppose these results happen" — pinned outcomes for unplayed fixtures.
    pub what_if: Option<Vec<WhatIf>>,
}

/// A pinned outcome: `outcome` is "home", "draw" or "away".
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct WhatIf {
    pub home: String,
    pub away: String,
    pub outcome: String,
}

#[derive(Deserialize, Clone)]
pub struct ScenarioRequest {
    pub prompt: String,
    pub n_sims: Option<usize>,
    pub seed: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct TeamRow {
    pub team: String,
    pub title_pct: f64,
    /// Fair decimal odds implied by `title_pct`; `None` when the club never
    /// won the league in any simulated season.
    pub title_odds: Option<f64>,
    /// Finishing in positions 1-2.
    pub top_two_pct: f64,
    /// Finishing exactly third.
    pub third_pct: f64,
    /// Finishing exactly fourth.
    pub fourth_pct: f64,
    /// Finishing in positions 1-4.
    pub top_four_pct: f64,
    /// Relegation (bottom three).
    pub relegation_pct: f64,
    pub relegation_odds: Option<f64>,
    pub exp_points: f64,
    pub exp_gd: f64,
    pub mean_position: f64,
}

/// One row of the finishing-position grid.
#[derive(Serialize, Clone)]
pub struct PositionRow {
    pub team: String,
    /// `position_pct[i]` = P(finishing in position `i + 1`); sums to 100.
    pub position_pct: Vec<f64>,
}

/// A row of a league table: the projected final table (`SimResponse.table`)
/// or the table as it stands (`MatchesResponse.standings`).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TableRow {
    pub position: usize,
    pub team: String,
    pub played: u16,
    pub won: u16,
    pub drawn: u16,
    pub lost: u16,
    pub gf: u16,
    pub ga: u16,
    pub gd: i64,
    pub points: i64,
}

/// P(`a` finishes above `b`) for a pair of title contenders.
#[derive(Serialize, Clone)]
pub struct RivalryPair {
    pub a: String,
    pub b: String,
    pub a_above_pct: f64,
    pub count: usize,
}

#[derive(Serialize, Clone)]
pub struct UpcomingMatch {
    pub round: u8,
    pub home: String,
    pub away: String,
    pub home_win_pct: f64,
    pub draw_pct: f64,
    pub away_win_pct: f64,
    pub home_odds: Option<f64>,
    pub draw_odds: Option<f64>,
    pub away_odds: Option<f64>,
}

#[derive(Serialize, Clone)]
pub struct UpcomingResponse {
    pub matches: Vec<UpcomingMatch>,
}

/// Model prices for one unplayed fixture: 1X2, over/under 2.5 goals, and
/// both-teams-to-score, each as a probability plus its fair decimal odds
/// (1 / p, no bookmaker margin).
#[derive(Serialize, Clone)]
pub struct MatchForecast {
    pub home_win_pct: f64,
    pub draw_pct: f64,
    pub away_win_pct: f64,
    pub home_odds: Option<f64>,
    pub draw_odds: Option<f64>,
    pub away_odds: Option<f64>,
    pub over25_pct: f64,
    pub over25_odds: Option<f64>,
    pub under25_odds: Option<f64>,
    pub btts_pct: f64,
    pub btts_odds: Option<f64>,
    pub btts_no_odds: Option<f64>,
    /// The model's expected goals (λ) for each side.
    pub home_xg: f64,
    pub away_xg: f64,
    /// The three most probable exact scorelines, most likely first.
    pub likely_scores: Vec<LikelyScore>,
}

#[derive(Serialize, Clone)]
pub struct LikelyScore {
    pub home: u8,
    pub away: u8,
    pub pct: f64,
}

/// One calendar fixture: the real score once played, the model's prices
/// while it isn't. Retrodicting played games with current ratings would be
/// misleading, so a played match carries no forecast.
#[derive(Serialize, Clone)]
pub struct MatchCard {
    pub home: String,
    pub away: String,
    /// Kick-off date, ISO `YYYY-MM-DD`.
    pub date: String,
    /// Kick-off time `HH:MM`, once TFF publishes it.
    pub kickoff: Option<String>,
    pub played: bool,
    pub home_score: Option<u16>,
    pub away_score: Option<u16>,
    pub forecast: Option<MatchForecast>,
}

#[derive(Serialize, Clone)]
pub struct RoundMatches {
    pub round: u8,
    pub matches: Vec<MatchCard>,
}

#[derive(Serialize, Clone)]
pub struct MatchesResponse {
    pub rounds: Vec<RoundMatches>,
    /// The next matchday (`World::next_round_on`, the same rule as
    /// `/api/upcoming`) — the UI's landing round.
    pub current_round: u8,
    /// The table as it stands, from the recorded results
    /// (`current_standings`).
    pub standings: Vec<TableRow>,
    pub as_of: AsOf,
}

/// How far the season has got: the results the current table, and every
/// forecast made from the same state of the league, stand on.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AsOf {
    /// Results recorded.
    pub played: usize,
    /// Fixtures in the season.
    pub fixtures: usize,
    /// Calendar date of the latest recorded result, ISO `YYYY-MM-DD`;
    /// `None` before the first.
    pub latest_result: Option<String>,
}

/// The table as it stands: every recorded result, ranked by the league
/// rules, with head-to-head counting only once a level block's meetings are
/// all played and clubs no rule separates listed by name
/// (`league::standings`).
pub fn current_standings(world: &crate::sim::World) -> Vec<TableRow> {
    let results: HashMap<(usize, usize), (i64, i64)> = world
        .played
        .iter()
        .map(|(&fixture, &(hg, ag))| (fixture, (i64::from(hg), i64::from(ag))))
        .collect();
    let (records, order) = crate::league::standings(&world.teams, &results);
    let goals = |g: i64| u16::try_from(g).expect("a season's goals are a small count");
    order
        .into_iter()
        .enumerate()
        .map(|(pos, club)| {
            let r = records[club];
            TableRow {
                position: pos + 1,
                team: world.teams[club].clone(),
                played: r.played(),
                won: r.won,
                drawn: r.drawn,
                lost: r.lost,
                gf: goals(r.gf),
                ga: goals(r.ga),
                gd: r.gd(),
                points: r.points,
            }
        })
        .collect()
}

fn as_of(world: &crate::sim::World) -> AsOf {
    let played: Vec<&str> = world
        .fixtures
        .iter()
        .zip(&world.dates)
        .filter(|(f, _)| world.played.contains_key(&(f.home, f.away)))
        .map(|(_, d)| d.date.as_str())
        .collect();
    AsOf {
        played: played.len(),
        fixtures: world.fixtures.len(),
        latest_result: played.iter().max().map(|d| d.to_string()),
    }
}

pub fn build_matches_response(world: &crate::sim::World) -> MatchesResponse {
    build_matches_response_on(world, &crate::sim::today_istanbul())
}

/// As `build_matches_response`, as of a given ISO date.
pub fn build_matches_response_on(world: &crate::sim::World, today: &str) -> MatchesResponse {
    let odds = crate::odds::decimal_odds_from_pct;
    let mut rounds: Vec<RoundMatches> = Vec::new();
    for (i, f) in world.fixtures.iter().enumerate() {
        if rounds.last().map(|r| r.round) != Some(f.round) {
            rounds.push(RoundMatches {
                round: f.round,
                matches: Vec::new(),
            });
        }
        let played = world.played.get(&(f.home, f.away)).copied();
        let forecast = if played.is_some() {
            None
        } else {
            let p = world.fixture_probs(f.home, f.away);
            Some(MatchForecast {
                home_win_pct: p.home_win_pct,
                draw_pct: p.draw_pct,
                away_win_pct: p.away_win_pct,
                home_odds: odds(p.home_win_pct),
                draw_odds: odds(p.draw_pct),
                away_odds: odds(p.away_win_pct),
                over25_pct: p.over25_pct,
                over25_odds: odds(p.over25_pct),
                under25_odds: odds(100.0 - p.over25_pct),
                btts_pct: p.btts_pct,
                btts_odds: odds(p.btts_pct),
                btts_no_odds: odds(100.0 - p.btts_pct),
                home_xg: p.home_xg,
                away_xg: p.away_xg,
                likely_scores: p
                    .top_scores
                    .iter()
                    .map(|&(h, a, pct)| LikelyScore {
                        home: h,
                        away: a,
                        pct,
                    })
                    .collect(),
            })
        };
        rounds
            .last_mut()
            .expect("round pushed above")
            .matches
            .push(MatchCard {
                home: world.teams[f.home].clone(),
                away: world.teams[f.away].clone(),
                date: world.dates[i].date.clone(),
                kickoff: world.dates[i].kickoff.clone(),
                played: played.is_some(),
                home_score: played.map(|(h, _)| h),
                away_score: played.map(|(_, a)| a),
                forecast,
            });
    }
    MatchesResponse {
        rounds,
        current_round: world.next_round_on(today),
        standings: current_standings(world),
        as_of: as_of(world),
    }
}

/// The points a place takes: for finishing in `position` or higher, the
/// fewest final points that gave a 50, 75 and 90% chance of it in the
/// simulated seasons. `p50` is the fewest points p such that at least half
/// of the simulated club-seasons ending on p points finished in `position`
/// or higher, and so did at least half of those ending on every higher total
/// seen at least `MIN_CLUB_SEASONS` times; likewise `p75` and `p90`. So
/// "finish on `p50` points and you take the place at least half the time".
///
/// It used to be the spread of the points the club finishing in `position`
/// ended on. The median of that reads the same way but asks for more: a club
/// on it also took the place in the seasons the place went for fewer points.
/// On the default forecast of 19 October 2026 (63 results; a one-off
/// measurement), a club on the old 50, 75 and 90% figures took the place
/// 66-74%, 90-93% and 96-98% of the time across the four places.
///
/// `None` when no total qualifies: too few simulated seasons to say. Runs
/// under about 1,000 seasons leave figures `None`, most of them below about
/// 500: on the same forecast (seeds 12345, 1 and 2; a one-off measurement)
/// all twelve at 100 seasons, nine at 200 and 300, five or six at 500, two
/// to four at 700, and one or two at 1,000 (the surest title figures). From
/// about 1,500 seasons there are none or one; the default 50,000 leaves none.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RaceThreshold {
    /// The lowest place that counts: 1 for the title, 2 for the top two, 4
    /// for the top four, the last place above the relegation zone.
    pub position: usize,
    pub label: String,
    pub p50: Option<i64>,
    pub p75: Option<i64>,
    pub p90: Option<i64>,
}

/// A points total that fewer simulated club-seasons than this ended on is
/// too rare to judge a place by (`RaceThreshold`). At 100, one season either
/// way moves its share by a point, and its binomial standard error is at
/// most 5 points (3 at a 90% share). The default forecast's 50,000 seasons
/// are 900,000 club-seasons, so this drops only the far tails, where one
/// freak season (a runner-up on 95 points, say) would otherwise decide
/// whether every total below it is enough. A run of 1,000 seasons can leave
/// the surest title figure without enough seasons to say.
pub const MIN_CLUB_SEASONS: usize = 100;

/// The fewest points on which at least `pct`% of the club-seasons in
/// `counts` (`SimResults::position_point_counts`) finished in the top
/// `places`, with every higher total seen at least `min_seen` times doing as
/// well. Rarer totals are skipped, and cannot be the answer themselves.
fn points_for_place(
    counts: &[Vec<usize>],
    places: usize,
    pct: usize,
    min_seen: usize,
) -> Option<i64> {
    let totals = counts.first().map_or(0, Vec::len);
    let mut fewest = None;
    for points in (0..totals).rev() {
        let seen: usize = counts.iter().map(|at| at[points]).sum();
        if seen < min_seen {
            continue;
        }
        let took: usize = counts[..places].iter().map(|at| at[points]).sum();
        if took * 100 < pct * seen {
            break;
        }
        fewest = Some(points as i64);
    }
    fewest
}

/// The four marks of the points-needed table. UEFA qualification is
/// deliberately not claimed: cup outcomes and the season's access list can
/// move it.
fn race_thresholds(results: &SimResults) -> Vec<RaceThreshold> {
    let counts = &results.position_point_counts;
    let last_safe = counts.len() - crate::data::RELEGATION_SPOTS;
    let marks: [(usize, &str); 4] = [
        (1, "Champion"),
        (crate::data::TOP_TWO_PLACES, "Top two"),
        (crate::data::TOP_FOUR_PLACES, "Top four"),
        (last_safe, "Safe"),
    ];
    marks
        .iter()
        .map(|&(position, label)| {
            let need = |pct| points_for_place(counts, position, pct, MIN_CLUB_SEASONS);
            RaceThreshold {
                position,
                label: label.to_string(),
                p50: need(50),
                p75: need(75),
                p90: need(90),
            }
        })
        .collect()
}

#[derive(Serialize, Clone)]
pub struct SimResponse {
    pub n_sims: usize,
    pub seed: u64,
    pub teams: Vec<TeamRow>,
    pub positions: Vec<PositionRow>,
    pub table: Vec<TableRow>,
    pub rivalries: Vec<RivalryPair>,
    pub consensus_champion: String,
    pub thresholds: Vec<RaceThreshold>,
    /// Echoed back so the client can show what the forecast assumed.
    pub what_if: Vec<WhatIf>,
    pub elo_overrides: HashMap<String, f64>,
    pub scenario_applied: Option<String>,
}

/// Integer `(points, won, drawn, lost)` for one projected-table row from a
/// club's expected points and draws over `games` matches.
///
/// Points first: round(xPts) is monotone in xPts, so the Pts column can never
/// read out of order down the table. Then fit integer W/D to those points:
/// 3W + D = points needs drawn ≡ points (mod 3), and W + D <= games caps
/// drawn at (3 * games - points) / 2, a cap a near-unbeaten leader hits when
/// xPts and xD are rounded independently (88.6 and 6.6 asked for 35 results
/// from 34 games). So take the valid draw count nearest the rounded mean.
/// The one total no record reaches, 3 * games - 1, is shaded down a point.
fn projected_record(mean_points: f64, mean_drawn: f64, games: u16) -> (i64, u16, u16, u16) {
    let games = games as i64;
    let points = (mean_points.round() as i64).clamp(0, 3 * games);
    let points = if points == 3 * games - 1 {
        points - 1
    } else {
        points
    };
    let max_drawn = points.min((3 * games - points) / 2);
    let mean_drawn = mean_drawn.round() as i64;
    let drawn = (0..=games)
        .flat_map(|d| [mean_drawn - d, mean_drawn + d])
        .find(|&d| (0..=max_drawn).contains(&d) && (points - d) % 3 == 0)
        .expect("every total but 3 * games - 1 has a record");
    let won = (points - drawn) / 3;
    (
        points,
        won as u16,
        drawn as u16,
        (games - won - drawn) as u16,
    )
}

pub fn build_response(
    world: &crate::sim::World,
    results: &SimResults,
    config: &SimConfig,
    scenario: Option<String>,
) -> SimResponse {
    let what_if: Vec<WhatIf> = config
        .forced
        .iter()
        .map(|(&(h, a), &o)| WhatIf {
            home: world.teams[h].clone(),
            away: world.teams[a].clone(),
            outcome: match o {
                crate::sim::ForcedOutcome::Home => "home",
                crate::sim::ForcedOutcome::Draw => "draw",
                crate::sim::ForcedOutcome::Away => "away",
            }
            .to_string(),
        })
        .collect();
    let n = results.n_sims as f64;
    let n_teams = world.teams.len();
    let pct = |c: usize| c as f64 / n * 100.0;
    let mean_position = |i: usize| {
        results.position_counts[i]
            .iter()
            .enumerate()
            .map(|(pos, &c)| (pos + 1) as f64 * c as f64)
            .sum::<f64>()
            / n
    };

    let mut teams: Vec<TeamRow> = (0..n_teams)
        .map(|i| {
            let title_pct = pct(results.title_counts[i]);
            let relegation_pct = pct(results.relegation_counts[i]);
            TeamRow {
                team: world.teams[i].clone(),
                title_pct,
                title_odds: crate::odds::decimal_odds_from_pct(title_pct),
                top_two_pct: pct(results.top_two_counts[i]),
                third_pct: pct(results.third_counts[i]),
                fourth_pct: pct(results.fourth_counts[i]),
                top_four_pct: pct(results.top_four_counts[i]),
                relegation_pct,
                relegation_odds: crate::odds::decimal_odds_from_pct(relegation_pct),
                exp_points: results.points_sum[i] / n,
                exp_gd: results.gd_sum[i] / n,
                mean_position: mean_position(i),
            }
        })
        .collect();
    teams.sort_by(|a, b| {
        b.title_pct
            .partial_cmp(&a.title_pct)
            .unwrap()
            .then(a.mean_position.partial_cmp(&b.mean_position).unwrap())
    });

    // Same order as the grid's rows in the UI: best mean position first.
    let positions: Vec<PositionRow> = {
        let mut idx: Vec<usize> = (0..n_teams).collect();
        idx.sort_by(|&a, &b| mean_position(a).partial_cmp(&mean_position(b)).unwrap());
        idx.into_iter()
            .map(|i| PositionRow {
                team: world.teams[i].clone(),
                position_pct: results.position_counts[i].iter().map(|&c| pct(c)).collect(),
            })
            .collect()
    };

    // Projected table from per-club EXPECTED records, not a single sampled
    // season. A representative trial looks plausible at the top, where the
    // position distributions are sharp, but positions 4-15 are nearly flat —
    // so any single season is Monte Carlo noise there and contradicts the
    // aggregate odds shown right next to it (e.g. a club with a 7% relegation
    // probability sampled into 17th). Averages are stable and self-consistent.
    let table: Vec<TableRow> = {
        let mut order: Vec<usize> = (0..n_teams).collect();
        order.sort_by(|&a, &b| {
            results.points_sum[b]
                .partial_cmp(&results.points_sum[a])
                .unwrap()
                .then(mean_position(a).partial_cmp(&mean_position(b)).unwrap())
        });
        order
            .into_iter()
            .enumerate()
            .map(|(pos, club)| {
                let (points, won, drawn, lost) = projected_record(
                    results.points_sum[club] / n,
                    results.drawn_sum[club] / n,
                    crate::data::N_ROUNDS as u16,
                );
                let gf = (results.gf_sum[club] / n).round() as u16;
                let ga = (results.ga_sum[club] / n).round() as u16;
                TableRow {
                    position: pos + 1,
                    team: world.teams[club].clone(),
                    played: crate::data::N_ROUNDS as u16,
                    won,
                    drawn,
                    lost,
                    gf,
                    ga,
                    gd: gf as i64 - ga as i64,
                    points,
                }
            })
            .collect()
    };

    // Title race: pairwise ordering among the six clubs with the best title
    // odds, most uncertain pairings first.
    let mut contenders: Vec<usize> = (0..n_teams).collect();
    contenders.sort_by(|&a, &b| {
        results.title_counts[b]
            .cmp(&results.title_counts[a])
            .then_with(|| results.top_four_counts[b].cmp(&results.top_four_counts[a]))
    });
    contenders.truncate(6);
    let mut rivalries: Vec<RivalryPair> = Vec::new();
    for (i, &a) in contenders.iter().enumerate() {
        for &b in &contenders[i + 1..] {
            let count = results.pairwise_above[a * n_teams + b];
            rivalries.push(RivalryPair {
                a: world.teams[a].clone(),
                b: world.teams[b].clone(),
                a_above_pct: pct(count),
                count,
            });
        }
    }
    rivalries.sort_by(|x, y| {
        (x.a_above_pct - 50.0)
            .abs()
            .partial_cmp(&(y.a_above_pct - 50.0).abs())
            .unwrap()
    });
    rivalries.truncate(8);

    SimResponse {
        n_sims: results.n_sims,
        seed: config.seed,
        teams,
        positions,
        table,
        rivalries,
        thresholds: race_thresholds(results),
        what_if,
        // The modal champion across all trials, not whoever happened to win
        // the sampled season.
        consensus_champion: {
            let champ = (0..n_teams)
                .max_by_key(|&i| results.title_counts[i])
                .unwrap_or(0);
            world.teams[champ].clone()
        },
        elo_overrides: config.elo_overrides.clone(),
        scenario_applied: scenario,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{SimConfig, World};

    #[test]
    fn response_percentages_are_consistent_with_each_other() {
        let w = World::with_played(crate::sim::MID_SEASON);
        let cfg = SimConfig {
            n_sims: 300,
            seed: 21,
            elo_overrides: HashMap::new(),
            forced: HashMap::new(),
        };
        let results = w.simulate(&cfg);
        let resp = build_response(&w, &results, &cfg, None);

        assert_eq!(resp.teams.len(), crate::data::N_TEAMS);
        assert_eq!(resp.positions.len(), crate::data::N_TEAMS);
        assert_eq!(resp.table.len(), crate::data::N_TEAMS);
        assert_eq!(resp.n_sims, 300);

        let title: f64 = resp.teams.iter().map(|t| t.title_pct).sum();
        assert!((title - 100.0).abs() < 1e-6, "title odds sum to {title}");

        for t in &resp.teams {
            assert!((t.top_four_pct - (t.top_two_pct + t.third_pct + t.fourth_pct)).abs() < 1e-6);
            assert!(t.title_pct <= t.top_two_pct + 1e-9);
            assert!(t.exp_points >= 0.0 && t.exp_points <= 102.0);
            assert!(t.mean_position >= 1.0 && t.mean_position <= 18.0);
        }

        // Sorted strongest-first by title odds.
        for pair in resp.teams.windows(2) {
            assert!(pair[0].title_pct >= pair[1].title_pct);
        }

        for p in &resp.positions {
            assert_eq!(p.position_pct.len(), crate::data::N_TEAMS);
            let s: f64 = p.position_pct.iter().sum();
            assert!((s - 100.0).abs() < 1e-6, "{} sums to {s}", p.team);
        }

        // The projected table is a real table: positions 1..=18, in order.
        let positions: Vec<usize> = resp.table.iter().map(|r| r.position).collect();
        assert_eq!(positions, (1..=crate::data::N_TEAMS).collect::<Vec<_>>());
        for row in &resp.table {
            assert_eq!(row.played, 34);
            // Widened: in a release build a u16 sum would wrap a negative
            // loss count straight back to 34.
            assert_eq!(row.won as u32 + row.drawn as u32 + row.lost as u32, 34);
            assert_eq!(row.points, 3 * row.won as i64 + row.drawn as i64);
            assert_eq!(row.gd, row.gf as i64 - row.ga as i64);
        }
        // Expected points strictly ordered down the table.
        for pair in resp.table.windows(2) {
            assert!(pair[0].points >= pair[1].points, "table must be sorted");
        }

        // The table is built from aggregates, so it must AGREE with them:
        // each club's table position lands within two places of the rank of
        // its mean position. Two places, not one, because these are different
        // statistics — expected points and mean finish order near-tied clubs
        // slightly differently, and at this trial count that shows. A single
        // sampled season would miss by far more.
        let mut by_mean: Vec<&TeamRow> = resp.teams.iter().collect();
        by_mean.sort_by(|a, b| a.mean_position.partial_cmp(&b.mean_position).unwrap());
        for row in &resp.table {
            let mean_rank = by_mean.iter().position(|t| t.team == row.team).unwrap() + 1;
            assert!(
                (row.position as i64 - mean_rank as i64).abs() <= 2,
                "{} at table position {} but mean-position rank {}",
                row.team,
                row.position,
                mean_rank
            );
        }

        // Champion = modal champion = the top row of the odds table.
        assert_eq!(resp.consensus_champion, resp.teams[0].team);

        assert_eq!(resp.thresholds.len(), 4);
        assert_thresholds_ordered(&resp.thresholds);
    }

    /// Points needed are ordered by place and by chance: a higher place
    /// needs at least as many points, and being surer of it at least as many
    /// again. Whichever figure is missing (too few seasons to say) is a
    /// surer or higher one than every figure present.
    fn assert_thresholds_ordered(thresholds: &[RaceThreshold]) {
        // `a` should need no more points than `b`.
        let ordered = |a: Option<i64>, b: Option<i64>, what: &str| match (a, b) {
            (Some(a), Some(b)) => assert!(a <= b, "{what}: {a} then {b}: {thresholds:?}"),
            (None, Some(_)) => panic!("{what}: only the harder one has a figure: {thresholds:?}"),
            _ => {}
        };
        for t in thresholds {
            ordered(t.p50, t.p75, &t.label);
            ordered(t.p75, t.p90, &t.label);
            for p in [t.p50, t.p75, t.p90].into_iter().flatten() {
                assert!((0..=102).contains(&p), "{t:?}");
            }
        }
        for pair in thresholds.windows(2) {
            assert!(pair[0].position < pair[1].position);
            let what = format!("{} against {}", pair[1].label, pair[0].label);
            ordered(pair[1].p50, pair[0].p50, &what);
            ordered(pair[1].p75, pair[0].p75, &what);
            ordered(pair[1].p90, pair[0].p90, &what);
        }
    }

    /// `SimResults` with nothing but the joint position-points counts filled
    /// in, from `(times, season)` pairs: `season` lists the final points by
    /// finishing position, and the season is counted `times` times.
    fn results_from_seasons(seasons: &[(usize, Vec<i64>)]) -> SimResults {
        let n = crate::data::N_TEAMS;
        let mut counts = vec![vec![0; crate::data::N_ROUNDS * 3 + 1]; n];
        for (times, points) in seasons {
            assert_eq!(points.len(), n);
            assert!(points.windows(2).all(|p| p[0] >= p[1]), "{points:?}");
            for (position, &p) in points.iter().enumerate() {
                counts[position][p as usize] += times;
            }
        }
        SimResults {
            n_sims: seasons.iter().map(|(times, _)| times).sum(),
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
            position_point_counts: counts,
            representative: crate::sim::SeasonResult {
                order: (0..n).collect(),
                records: vec![Default::default(); n],
            },
        }
    }

    /// A season whose top two finish on `first` and `second` points, the
    /// other sixteen clubs well below them.
    fn season(first: i64, second: i64) -> Vec<i64> {
        let mut points = vec![first, second];
        points.extend((0..16).map(|i| 50 - 2 * i));
        points
    }

    /// The share of club-seasons on `points` that finished in the top
    /// `places`, in percent.
    fn share(results: &SimResults, points: usize, places: usize) -> f64 {
        let counts = &results.position_point_counts;
        let seen: usize = counts.iter().map(|at| at[points]).sum();
        let took: usize = counts[..places].iter().map(|at| at[points]).sum();
        100.0 * took as f64 / seen as f64
    }

    /// Four kinds of season, equally likely. Twice the title goes for 70 or
    /// 71 points with a runner-up one point behind; twice for 80 or 81 with
    /// nobody near. The champion's median total is 80, which the old table
    /// gave as the points for a 50% chance, but a club on 80 points took the
    /// title every time, and one on 71 did too. The fewest points that gave
    /// an even chance were 70 (the champion once, the runner-up once).
    #[test]
    fn points_needed_are_the_fewest_points_that_gave_the_chance() {
        let m = MIN_CLUB_SEASONS;
        let results = results_from_seasons(&[
            (m, season(70, 69)),
            (m, season(71, 70)),
            (m, season(80, 60)),
            (m, season(81, 60)),
        ]);
        let champion = &race_thresholds(&results)[0];
        assert_eq!(champion.label, "Champion");

        // The old figures: quantiles of the champion's points.
        let old = |q: f64| {
            let counts = &results.position_point_counts[0];
            let total: usize = counts.iter().sum();
            let target = ((total - 1) as f64 * q).round() as usize;
            let mut cumulative = 0;
            counts
                .iter()
                .position(|&c| {
                    cumulative += c;
                    cumulative > target
                })
                .unwrap()
        };
        assert_eq!([old(0.5), old(0.75), old(0.9)], [80, 80, 81]);
        assert_eq!(share(&results, 80, 1), 100.0, "the old 50% figure");

        assert_eq!(
            (champion.p50, champion.p75, champion.p90),
            (Some(70), Some(71), Some(71))
        );
        assert_eq!(share(&results, 70, 1), 50.0);
        assert_eq!(share(&results, 69, 1), 0.0);
        assert_eq!(share(&results, 71, 1), 100.0);
        // Every club on 60 points or more finished in the top two.
        assert_eq!(race_thresholds(&results)[1].p90, Some(60));
    }

    /// A total too few club-seasons ended on is skipped. One freak season,
    /// with the title going for 96 points over a runner-up on 95, would
    /// otherwise put 95 points short of a 50% chance of the title and the
    /// first total that qualifies at 96. Seen often enough, it counts.
    #[test]
    fn points_needed_skip_totals_too_rare_to_judge() {
        let m = MIN_CLUB_SEASONS;
        let usual = [
            (m, season(70, 69)),
            (m, season(71, 70)),
            (m, season(80, 60)),
            (m, season(81, 60)),
        ];
        let mut seasons = usual.to_vec();
        seasons.push((m - 1, season(96, 95)));
        let champion = &race_thresholds(&results_from_seasons(&seasons))[0];
        assert_eq!(champion.p50, Some(70));

        seasons.last_mut().unwrap().0 = m;
        let champion = &race_thresholds(&results_from_seasons(&seasons))[0];
        assert_eq!(champion.p50, Some(96));

        // Nothing seen often enough: no figure at all.
        let rare = results_from_seasons(&[(m / 2, season(80, 60))]);
        for t in race_thresholds(&rare) {
            assert_eq!((t.p50, t.p75, t.p90), (None, None, None), "{}", t.label);
        }
    }

    /// Ordered by chance and by place, on simulated seasons from preseason
    /// and part-way through, with seasons enough to fill the tails and with
    /// few enough that some figures go missing.
    #[test]
    fn points_needed_are_ordered_by_chance_and_place() {
        for played in [0, crate::sim::MID_SEASON] {
            let w = World::with_played(played);
            for n_sims in [100, 2_000] {
                let cfg = SimConfig {
                    n_sims,
                    seed: 7,
                    elo_overrides: HashMap::new(),
                    forced: HashMap::new(),
                };
                let thresholds = race_thresholds(&w.simulate(&cfg));
                assert_eq!(thresholds.len(), 4);
                assert_thresholds_ordered(&thresholds);
                if n_sims == 2_000 {
                    assert!(
                        thresholds.iter().all(|t| t.p90.is_some()),
                        "{played} played: {thresholds:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn matches_response_covers_the_whole_calendar() {
        let w = World::with_played(crate::sim::MID_SEASON);
        let today = "2026-10-18";
        let resp = build_matches_response_on(&w, today);

        assert_eq!(resp.rounds.len(), crate::data::N_ROUNDS);
        let total: usize = resp.rounds.iter().map(|r| r.matches.len()).sum();
        assert_eq!(total, crate::data::N_FIXTURES);
        for (i, r) in resp.rounds.iter().enumerate() {
            assert_eq!(r.round as usize, i + 1, "rounds in calendar order");
            assert_eq!(r.matches.len(), 9);
        }

        // The landing round is the next matchday — the round `/api/upcoming`
        // lists — and it still holds unplayed games.
        assert_eq!(resp.current_round, w.next_round_on(today));
        let upcoming = w.upcoming_matches_on(today);
        assert!(!upcoming.is_empty());
        assert!(upcoming.iter().all(|f| f.round == resp.current_round));
        assert!(resp
            .rounds
            .iter()
            .find(|r| r.round == resp.current_round)
            .expect("current round exists")
            .matches
            .iter()
            .any(|m| !m.played));

        for r in &resp.rounds {
            for m in &r.matches {
                if m.played {
                    assert!(m.home_score.is_some() && m.away_score.is_some());
                    assert!(m.forecast.is_none(), "played games are not retrodicted");
                } else {
                    let f = m.forecast.as_ref().expect("unplayed games are priced");
                    let s = f.home_win_pct + f.draw_pct + f.away_win_pct;
                    assert!((s - 100.0).abs() < 1e-9, "{} v {}: {s}", m.home, m.away);
                    for (pct, odds) in [
                        (f.home_win_pct, f.home_odds),
                        (f.draw_pct, f.draw_odds),
                        (f.away_win_pct, f.away_odds),
                        (f.over25_pct, f.over25_odds),
                        (f.btts_pct, f.btts_odds),
                    ] {
                        let o = odds.expect("all league outcomes are possible");
                        assert!((o - 100.0 / pct).abs() < 0.02, "odds {o} vs pct {pct}");
                    }
                    assert!(f.home_xg > 0.0 && f.away_xg > 0.0);
                    assert_eq!(f.likely_scores.len(), 3);
                    assert!(f.likely_scores[0].pct >= f.likely_scores[1].pct);
                    assert!(f.likely_scores[1].pct >= f.likely_scores[2].pct);
                }
            }
        }
    }

    #[test]
    fn a_complete_season_lands_on_the_last_round() {
        let mut w = World::with_played(0);
        for f in w.fixtures.clone() {
            w.played.insert((f.home, f.away), (1, 1));
        }
        let resp = build_matches_response_on(&w, "2026-10-18");
        assert_eq!(resp.current_round as usize, crate::data::N_ROUNDS);
        assert!(resp
            .rounds
            .iter()
            .all(|r| r.matches.iter().all(|m| m.played)));
        assert_eq!(resp.as_of.played, crate::data::N_FIXTURES);
        assert_eq!(
            resp.as_of.latest_result,
            w.dates.iter().map(|d| d.date.clone()).max()
        );
        assert!(resp.standings.iter().all(|r| r.played == 34));
    }

    /// The current table is the recorded results added up: each row checked
    /// against a tally of the club's own results, rows in points order.
    #[test]
    fn current_standings_add_up_the_recorded_results() {
        let w = World::with_played(crate::sim::MID_SEASON);
        let resp = build_matches_response_on(&w, "2026-10-18");
        let table = &resp.standings;

        let positions: Vec<usize> = table.iter().map(|r| r.position).collect();
        assert_eq!(positions, (1..=crate::data::N_TEAMS).collect::<Vec<_>>());
        for pair in table.windows(2) {
            assert!(pair[0].points >= pair[1].points, "{pair:?}");
        }
        for row in table {
            let club = w.idx[&row.team];
            let mut tally = TableRow {
                position: row.position,
                team: row.team.clone(),
                played: 0,
                won: 0,
                drawn: 0,
                lost: 0,
                gf: 0,
                ga: 0,
                gd: 0,
                points: 0,
            };
            for (&(home, away), &(hg, ag)) in &w.played {
                let (scored, conceded) = match club {
                    c if c == home => (hg, ag),
                    c if c == away => (ag, hg),
                    _ => continue,
                };
                tally.played += 1;
                tally.gf += scored;
                tally.ga += conceded;
                match scored.cmp(&conceded) {
                    std::cmp::Ordering::Greater => tally.won += 1,
                    std::cmp::Ordering::Equal => tally.drawn += 1,
                    std::cmp::Ordering::Less => tally.lost += 1,
                }
            }
            tally.gd = tally.gf as i64 - tally.ga as i64;
            tally.points = 3 * tally.won as i64 + tally.drawn as i64;
            assert_eq!(*row, tally);
        }
        let games: u32 = table.iter().map(|r| u32::from(r.played)).sum();
        assert_eq!(games as usize, 2 * crate::sim::MID_SEASON);

        // As of the seven matchdays' worth `with_played` records, the first
        // fixtures by date.
        let mut dates: Vec<&str> = w.dates.iter().map(|d| d.date.as_str()).collect();
        dates.sort();
        assert_eq!(
            resp.as_of,
            AsOf {
                played: crate::sim::MID_SEASON,
                fixtures: crate::data::N_FIXTURES,
                latest_result: Some(dates[crate::sim::MID_SEASON - 1].to_string()),
            }
        );
    }

    /// Before the first result every club is on nothing, listed by name.
    #[test]
    fn the_preseason_table_lists_the_clubs_by_name() {
        let w = World::with_played(0);
        let resp = build_matches_response_on(&w, "2026-08-01");
        assert_eq!(
            resp.as_of,
            AsOf {
                played: 0,
                fixtures: crate::data::N_FIXTURES,
                latest_result: None,
            }
        );
        assert!(resp
            .standings
            .iter()
            .all(|r| r.played == 0 && r.points == 0 && r.gf == 0 && r.ga == 0));
        let listed: Vec<&str> = resp.standings.iter().map(|r| r.team.as_str()).collect();
        assert_eq!(
            &listed[..6],
            [
                "Alanyaspor",
                "Amedspor",
                "Başakşehir",
                "Beşiktaş",
                "Çorum",
                "Erzurumspor"
            ]
        );
        assert_eq!(listed.last(), Some(&"Trabzonspor"));
    }

    /// A match postponed from an early round, still unplayed and dated in
    /// the past, must not drag the landing round back to it.
    #[test]
    fn a_postponed_match_does_not_pin_the_landing_round() {
        let mut w = World::with_played(crate::sim::MID_SEASON);
        let mut by_date: Vec<usize> = (0..w.fixtures.len()).collect();
        by_date.sort_by(|&a, &b| w.dates[a].date.cmp(&w.dates[b].date));
        // Today is the day of the first unrecorded fixture; postpone the
        // season opener, which is now overdue.
        let next = by_date
            .iter()
            .copied()
            .find(|&i| {
                !w.played
                    .contains_key(&(w.fixtures[i].home, w.fixtures[i].away))
            })
            .expect("a part-played season has unplayed fixtures");
        let today = w.dates[next].date.clone();
        let landing = w.next_round_on(&today);
        let opener = w.fixtures[by_date[0]];
        assert!(w.dates[by_date[0]].date < today && opener.round < landing);
        w.played.remove(&(opener.home, opener.away));

        let resp = build_matches_response_on(&w, &today);
        assert_eq!(resp.current_round, landing);
        assert!(w
            .upcoming_matches_on(&today)
            .iter()
            .all(|f| f.round == landing));
    }

    /// An unbeaten-leader-like expected record: rounding xPts and xD apart
    /// asked for 27 wins and 8 draws, 35 results from 34 games, and the loss
    /// column wrapped (or panicked in a debug build).
    #[test]
    fn projected_record_fits_the_games_in_a_season() {
        assert_eq!(projected_record(88.6, 6.6, 34), (89, 28, 5, 1));

        let mut last_points = 0;
        for tenths in 0..=1020 {
            let mean_points = tenths as f64 / 10.0;
            for halves in 0..=68 {
                let mean_drawn = halves as f64 / 2.0;
                if mean_drawn > mean_points {
                    break; // every draw is a point
                }
                let (points, won, drawn, lost) = projected_record(mean_points, mean_drawn, 34);
                let at = format!("xPts {mean_points}, xD {mean_drawn}");
                assert_eq!(won as u32 + drawn as u32 + lost as u32, 34, "{at}");
                assert_eq!(points, 3 * won as i64 + drawn as i64, "{at}");
                // Rounded, except 101, which no 34-game record reaches.
                assert!((points as f64 - mean_points).abs() <= 1.5, "{at}");
                assert!(points >= last_points, "{at}: Pts must stay monotone");
                last_points = points;
            }
        }
    }
}
