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
    /// Champions League (positions 1-2).
    pub ucl_pct: f64,
    /// Europa League (position 3).
    pub uel_pct: f64,
    /// Conference League (position 4).
    pub uecl_pct: f64,
    /// Any European place (positions 1-4).
    pub europe_pct: f64,
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

/// A row of the representative projected final table.
#[derive(Serialize, Clone)]
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

/// Bookmaker prices beside the model's, plus the gap between them. The model
/// does not see these — they are shown so a reader can judge it.
#[derive(Serialize, Clone)]
pub struct MarketComparison {
    pub home_odds: f64,
    pub draw_odds: f64,
    pub away_odds: f64,
    /// Margin-free implied probabilities, comparable to the model's.
    pub home_pct: f64,
    pub draw_pct: f64,
    pub away_pct: f64,
    /// The book's overround: 1.10 means 10% margin baked into the prices.
    pub overround: f64,
    /// Largest model-minus-market probability gap, in points, and which
    /// outcome it falls on ("1", "X" or "2"). Positive means the model rates
    /// that outcome higher than the market does.
    pub edge_pct: f64,
    pub edge_outcome: String,
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
    pub market: Option<MarketComparison>,
}

#[derive(Serialize, Clone)]
pub struct RoundMatches {
    pub round: u8,
    pub matches: Vec<MatchCard>,
}

#[derive(Serialize, Clone)]
pub struct MatchesResponse {
    pub rounds: Vec<RoundMatches>,
    /// The earliest round with an unplayed fixture — the UI's landing round.
    pub current_round: u8,
}

pub fn build_matches_response(
    world: &crate::sim::World,
    market: Option<&crate::market::MarketSnapshot>,
) -> MatchesResponse {
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
        let home_name = world.teams[f.home].as_str();
        let away_name = world.teams[f.away].as_str();
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
        // Only price unplayed fixtures: a settled match has no market.
        let market_cmp = match (&forecast, market.and_then(|m| m.get(home_name, away_name))) {
            (Some(f), Some(p)) => {
                let (mh, md, ma) = p.implied_pct();
                let gaps = [
                    (f.home_win_pct - mh, "1"),
                    (f.draw_pct - md, "X"),
                    (f.away_win_pct - ma, "2"),
                ];
                let (edge_pct, edge_outcome) = gaps
                    .iter()
                    .max_by(|a, b| a.0.abs().partial_cmp(&b.0.abs()).unwrap())
                    .map(|&(g, o)| (g, o.to_string()))
                    .expect("three outcomes");
                Some(MarketComparison {
                    home_odds: p.home_odds,
                    draw_odds: p.draw_odds,
                    away_odds: p.away_odds,
                    home_pct: mh,
                    draw_pct: md,
                    away_pct: ma,
                    overround: p.overround(),
                    edge_pct,
                    edge_outcome,
                })
            }
            _ => None,
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
                market: market_cmp,
            });
    }
    let current_round = rounds
        .iter()
        .find(|r| r.matches.iter().any(|m| !m.played))
        .map(|r| r.round)
        .unwrap_or(crate::data::N_ROUNDS as u8);
    MatchesResponse {
        rounds,
        current_round,
    }
}

/// What a points total has actually been worth: the distribution of the
/// points held by the club finishing in a given position. Read as "reach this
/// many points and you'd have taken this place in N% of simulated seasons".
#[derive(Serialize, Clone, Debug)]
pub struct RaceThreshold {
    pub position: usize,
    pub label: String,
    /// Points that took the place in 50 / 75 / 90% of seasons.
    pub p50: i64,
    pub p75: i64,
    pub p90: i64,
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
                ucl_pct: pct(results.ucl_counts[i]),
                uel_pct: pct(results.uel_counts[i]),
                uecl_pct: pct(results.uecl_counts[i]),
                europe_pct: pct(results.europe_counts[i]),
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
                // Points first: round(xPts) is monotone in xPts, so the Pts
                // column can never read out of order down the table. Then fit
                // integer W/D to those points — drawn must match points mod 3
                // for 3W + D = points to have an integer solution, so nudge
                // the rounded mean draw count to the nearest valid value.
                let points = (results.points_sum[club] / n).round() as i64;
                let mean_drawn = (results.drawn_sum[club] / n).round() as i64;
                let drawn = (0..=2)
                    .flat_map(|d| [mean_drawn - d, mean_drawn + d])
                    .find(|&d| (0..=points).contains(&d) && (points - d) % 3 == 0)
                    .unwrap_or(points % 3);
                let won = ((points - drawn) / 3) as u16;
                let drawn = drawn as u16;
                let lost = crate::data::N_ROUNDS as u16 - won - drawn;
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
            .then_with(|| results.europe_counts[b].cmp(&results.europe_counts[a]))
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

    // The league's own cut lines: champion, last Champions League place, last
    // European place, and the last safe spot.
    let last_safe = n_teams - crate::data::RELEGATION_SPOTS;
    let marks: [(usize, &str); 4] = [
        (1, "Champion"),
        (crate::data::UCL_SPOTS, "Last Champions League place"),
        (crate::data::EUROPE_SPOTS, "Last European place"),
        (last_safe, "Last safe place"),
    ];
    let thresholds: Vec<RaceThreshold> = marks
        .iter()
        .filter_map(|&(position, label)| {
            let mut pts = results.cutoff_points.get(position - 1)?.clone();
            if pts.is_empty() {
                return None;
            }
            pts.sort_unstable();
            let at = |q: f64| pts[(((pts.len() - 1) as f64) * q).round() as usize];
            Some(RaceThreshold {
                position,
                label: label.to_string(),
                p50: at(0.5),
                p75: at(0.75),
                p90: at(0.9),
            })
        })
        .collect();

    SimResponse {
        n_sims: results.n_sims,
        seed: config.seed,
        teams,
        positions,
        table,
        rivalries,
        // The modal champion across all trials, not whoever happened to win
        // the sampled season.
        thresholds,
        what_if,
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
        let w = World::new();
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
            assert!((t.europe_pct - (t.ucl_pct + t.uel_pct + t.uecl_pct)).abs() < 1e-6);
            assert!(t.title_pct <= t.ucl_pct + 1e-9);
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
            assert_eq!(row.won + row.drawn + row.lost, 34);
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

        // Cut lines must be ordered by position and by demand: taking a
        // higher place needs more points, and being surer needs more still.
        assert_eq!(resp.thresholds.len(), 4);
        for t in &resp.thresholds {
            assert!(t.p50 <= t.p75 && t.p75 <= t.p90, "{}: {t:?}", t.label);
            assert!((0..=102).contains(&t.p90));
        }
        for pair in resp.thresholds.windows(2) {
            assert!(pair[0].position < pair[1].position);
            assert!(
                pair[0].p50 >= pair[1].p50,
                "{} should need at least as many points as {}",
                pair[0].label,
                pair[1].label
            );
        }
    }

    #[test]
    fn matches_response_covers_the_whole_calendar() {
        let w = World::new();
        let resp = build_matches_response(&w, None);

        assert_eq!(resp.rounds.len(), crate::data::N_ROUNDS);
        let total: usize = resp.rounds.iter().map(|r| r.matches.len()).sum();
        assert_eq!(total, crate::data::N_FIXTURES);
        for (i, r) in resp.rounds.iter().enumerate() {
            assert_eq!(r.round as usize, i + 1, "rounds in calendar order");
            assert_eq!(r.matches.len(), 9);
        }

        // The landing round is the earliest one still holding unplayed games.
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
}
