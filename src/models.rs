use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::sim::{SimConfig, SimResults};

#[derive(Deserialize, Clone)]
pub struct SimRequest {
    pub n_sims: Option<usize>,
    pub seed: Option<u64>,
    pub elo_overrides: Option<HashMap<String, f64>>,
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

#[derive(Serialize, Clone)]
pub struct SimResponse {
    pub n_sims: usize,
    pub seed: u64,
    pub teams: Vec<TeamRow>,
    pub positions: Vec<PositionRow>,
    pub table: Vec<TableRow>,
    pub rivalries: Vec<RivalryPair>,
    pub consensus_champion: String,
    pub elo_overrides: HashMap<String, f64>,
    pub scenario_applied: Option<String>,
}

pub fn build_response(
    world: &crate::sim::World,
    results: &SimResults,
    config: &SimConfig,
    scenario: Option<String>,
) -> SimResponse {
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

    let rep = &results.representative;
    let table: Vec<TableRow> = rep
        .order
        .iter()
        .enumerate()
        .map(|(pos, &club)| {
            let r = &rep.records[club];
            TableRow {
                position: pos + 1,
                team: world.teams[club].clone(),
                played: r.played(),
                won: r.won,
                drawn: r.drawn,
                lost: r.lost,
                gf: r.gf as u16,
                ga: r.ga as u16,
                gd: r.gd(),
                points: r.points,
            }
        })
        .collect();

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

    SimResponse {
        n_sims: results.n_sims,
        seed: config.seed,
        teams,
        positions,
        table,
        rivalries,
        consensus_champion: world.teams[rep.order[0]].clone(),
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
            assert_eq!(row.gd, row.gf as i64 - row.ga as i64);
        }
        assert_eq!(resp.consensus_champion, resp.table[0].team);
    }
}
