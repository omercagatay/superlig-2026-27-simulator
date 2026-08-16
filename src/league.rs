//! League table construction and Süper Lig classification rules.

use std::collections::{HashMap, HashSet};

/// A club's running record over a season.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamRecord {
    pub points: i64,
    pub gf: i64,
    pub ga: i64,
    pub won: u16,
    pub drawn: u16,
    pub lost: u16,
}

impl TeamRecord {
    pub fn gd(&self) -> i64 {
        self.gf - self.ga
    }

    pub fn played(&self) -> u16 {
        self.won + self.drawn + self.lost
    }
}

/// Fold one finished fixture into both clubs' records.
pub fn apply_result(records: &mut [TeamRecord], home: usize, away: usize, hg: i64, ag: i64) {
    records[home].gf += hg;
    records[home].ga += ag;
    records[away].gf += ag;
    records[away].ga += hg;
    match hg.cmp(&ag) {
        std::cmp::Ordering::Greater => {
            records[home].points += 3;
            records[home].won += 1;
            records[away].lost += 1;
        }
        std::cmp::Ordering::Equal => {
            records[home].points += 1;
            records[away].points += 1;
            records[home].drawn += 1;
            records[away].drawn += 1;
        }
        std::cmp::Ordering::Less => {
            records[away].points += 3;
            records[away].won += 1;
            records[home].lost += 1;
        }
    }
}

/// Rank the table by Süper Lig rules and return club indices in finishing
/// order (position 0 first).
///
/// > Rules for classification: 1) Points; 2) Head-to-head points;
/// > 3) Head-to-head goal difference; 4) Head-to-head goals scored;
/// > 5) Goal difference; 6) Goals scored; 7) Play-off.
///
/// Head-to-head is applied **once** per block of clubs tied on points.
/// Clubs still level after that pass fall through to overall GD/GF, not to a
/// fresh mini-table among the remaining pair — federations differ on this and
/// the published rule does not specify it, so one pass is the reading here.
///
/// `results` is keyed `(home, away) -> (home_goals, away_goals)`.
/// `tiebreak` supplies the play-off stand-in; it is called once per club in
/// each multi-club block, so callers control determinism.
pub fn rank_table(
    records: &[TeamRecord],
    results: &HashMap<(usize, usize), (i64, i64)>,
    tiebreak: &mut dyn FnMut() -> u64,
) -> Vec<usize> {
    let n = records.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&c| std::cmp::Reverse(records[c].points));

    let mut final_order: Vec<usize> = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && records[order[j + 1]].points == records[order[i]].points {
            j += 1;
        }
        let block = &order[i..=j];
        if block.len() == 1 {
            final_order.push(block[0]);
        } else {
            final_order.extend(rank_tied_block(block, records, results, tiebreak));
        }
        i = j + 1;
    }
    final_order
}

fn rank_tied_block(
    block: &[usize],
    records: &[TeamRecord],
    results: &HashMap<(usize, usize), (i64, i64)>,
    tiebreak: &mut dyn FnMut() -> u64,
) -> Vec<usize> {
    let members: HashSet<usize> = block.iter().copied().collect();

    // Mini-table over only the matches played among the tied clubs.
    let mut h2h: HashMap<usize, TeamRecord> =
        block.iter().map(|&c| (c, TeamRecord::default())).collect();
    for (&(home, away), &(hg, ag)) in results {
        if members.contains(&home) && members.contains(&away) {
            let mut pair = [h2h[&home], h2h[&away]];
            apply_result(&mut pair, 0, 1, hg, ag);
            h2h.insert(home, pair[0]);
            h2h.insert(away, pair[1]);
        }
    }

    let keys: HashMap<usize, u64> = block.iter().map(|&c| (c, tiebreak())).collect();
    let mut sorted = block.to_vec();
    sorted.sort_by_key(|&c| {
        let m = &h2h[&c];
        let o = &records[c];
        std::cmp::Reverse((m.points, m.gd(), m.gf, o.gd(), o.gf, keys[&c]))
    });
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn rec(points: i64, gf: i64, ga: i64) -> TeamRecord {
        TeamRecord {
            points,
            gf,
            ga,
            won: 0,
            drawn: 0,
            lost: 0,
        }
    }

    /// No ties: pure points order.
    #[test]
    fn ranks_by_points_when_nothing_is_tied() {
        let records = vec![rec(70, 60, 30), rec(80, 50, 20), rec(60, 40, 40)];
        let order = rank_table(&records, &HashMap::new(), &mut || 0);
        assert_eq!(order, vec![1, 0, 2]);
    }

    /// The rule that distinguishes Süper Lig from the World Cup: with equal
    /// points, head-to-head decides BEFORE goal difference. Club 0 has the
    /// worse overall GD but beat club 1 home and away, so it finishes above.
    #[test]
    fn head_to_head_outranks_goal_difference() {
        let records = vec![rec(70, 50, 40), rec(70, 80, 40)];
        let mut results = HashMap::new();
        results.insert((0, 1), (2, 0)); // club 0 home: won
        results.insert((1, 0), (0, 1)); // club 0 away: won
        let order = rank_table(&records, &results, &mut || 0);
        assert_eq!(
            order,
            vec![0, 1],
            "head-to-head must be applied before goal difference"
        );
    }

    /// Head-to-head level, so overall goal difference breaks the tie.
    #[test]
    fn falls_through_to_goal_difference_when_head_to_head_is_level() {
        let records = vec![rec(70, 50, 40), rec(70, 80, 40)];
        let mut results = HashMap::new();
        results.insert((0, 1), (1, 1));
        results.insert((1, 0), (2, 2));
        let order = rank_table(&records, &results, &mut || 0);
        assert_eq!(order, vec![1, 0], "club 1 has the better overall GD");
    }

    /// Stated assumption: head-to-head is applied ONCE per tied block. When
    /// it separates one club and leaves two still level, those two fall
    /// through to overall GD — NOT to a fresh mini-table among the pair.
    /// Club 2 wins the 3-way H2H outright. Clubs 0 and 1 are level on H2H
    /// points within the block, so overall GD orders them: club 1 (+20)
    /// ahead of club 0 (+10), even though club 0 beat club 1 head-to-head.
    #[test]
    fn head_to_head_is_applied_once_not_recursively() {
        let records = vec![rec(70, 50, 40), rec(70, 60, 40), rec(70, 45, 40)];
        let mut results = HashMap::new();
        // Club 2 beats both -> 6 H2H points.
        results.insert((2, 0), (1, 0));
        results.insert((2, 1), (1, 0));
        results.insert((0, 2), (0, 1));
        results.insert((1, 2), (0, 1));
        // Clubs 0 and 1 draw both meetings -> 2 H2H points each, level.
        results.insert((0, 1), (0, 0));
        results.insert((1, 0), (0, 0));
        let order = rank_table(&records, &results, &mut || 0);
        assert_eq!(order, vec![2, 1, 0]);
    }

    #[test]
    fn apply_result_updates_both_clubs() {
        let mut records = vec![TeamRecord::default(); 2];
        apply_result(&mut records, 0, 1, 3, 1);
        assert_eq!(records[0].points, 3);
        assert_eq!(records[0].won, 1);
        assert_eq!(records[0].gf, 3);
        assert_eq!(records[0].ga, 1);
        assert_eq!(records[0].gd(), 2);
        assert_eq!(records[1].points, 0);
        assert_eq!(records[1].lost, 1);
        assert_eq!(records[1].gd(), -2);

        apply_result(&mut records, 1, 0, 2, 2);
        assert_eq!(records[0].points, 4);
        assert_eq!(records[1].points, 1);
        assert_eq!(records[0].drawn, 1);
        assert_eq!(records[1].drawn, 1);
        assert_eq!(records[0].played(), 2);
    }

    /// Fully identical clubs are separated by the play-off stand-in, and the
    /// ranking stays a permutation regardless.
    #[test]
    fn identical_clubs_are_separated_deterministically_by_the_tiebreak() {
        let records = vec![rec(70, 50, 50), rec(70, 50, 50)];
        let mut results = HashMap::new();
        results.insert((0, 1), (1, 1));
        results.insert((1, 0), (1, 1));
        let mut seq = [7u64, 9u64].into_iter();
        let order = rank_table(&records, &results, &mut || seq.next().unwrap_or(0));
        assert_eq!(order.len(), 2);
        assert_ne!(order[0], order[1]);
    }
}
