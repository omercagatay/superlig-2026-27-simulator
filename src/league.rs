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
/// Head-to-head is taken over whatever meetings `results` holds. That is the
/// rule at the season's end, when every pair has met twice, which is what a
/// simulated season ranks; the table during the season is `standings`.
///
/// `results` is keyed `(home, away) -> (home_goals, away_goals)`.
/// `tiebreak` supplies the play-off stand-in; it is called once per club in
/// each multi-club block, so callers control determinism.
pub fn rank_table(
    records: &[TeamRecord],
    results: &HashMap<(usize, usize), (i64, i64)>,
    tiebreak: &mut dyn FnMut() -> u64,
) -> Vec<usize> {
    rank_table_by(records, results, &mut |_| tiebreak())
}

/// As `rank_table`, with the play-off stand-in keyed by club: of the clubs
/// no rule separates, the one with the larger `key(club)` ranks higher.
/// `key` is called exactly when, and in the order, `rank_table` calls
/// `tiebreak`, so a simulated season draws the same numbers through either.
pub fn rank_table_by(
    records: &[TeamRecord],
    results: &HashMap<(usize, usize), (i64, i64)>,
    key: &mut dyn FnMut(usize) -> u64,
) -> Vec<usize> {
    rank(records, results, HeadToHead::Always, key)
}

/// When a block of clubs level on points goes to the head-to-head
/// mini-table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeadToHead {
    /// Always, over whatever meetings `results` holds: the season-end rule
    /// (`rank_table`). At the end of a double round-robin every pair has met
    /// twice.
    Always,
    /// Only once every pair in the block has played both its meetings, home
    /// and away; until then the block goes straight to overall goal
    /// difference. The live table's rule (`standings`).
    WhenMeetingsComplete,
}

fn rank(
    records: &[TeamRecord],
    results: &HashMap<(usize, usize), (i64, i64)>,
    head_to_head: HeadToHead,
    key: &mut dyn FnMut(usize) -> u64,
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
            final_order.extend(rank_tied_block(block, records, results, head_to_head, key));
        }
        i = j + 1;
    }
    final_order
}

fn rank_tied_block(
    block: &[usize],
    records: &[TeamRecord],
    results: &HashMap<(usize, usize), (i64, i64)>,
    head_to_head: HeadToHead,
    key: &mut dyn FnMut(usize) -> u64,
) -> Vec<usize> {
    let members: HashSet<usize> = block.iter().copied().collect();

    // Mini-table over only the matches played among the tied clubs. Left
    // empty (every club level on it) when the rule does not apply yet, so
    // overall goal difference decides.
    let mut h2h: HashMap<usize, TeamRecord> =
        block.iter().map(|&c| (c, TeamRecord::default())).collect();
    let applies = match head_to_head {
        HeadToHead::Always => true,
        HeadToHead::WhenMeetingsComplete => meetings_complete(block, results),
    };
    if applies {
        for (&(home, away), &(hg, ag)) in results {
            if members.contains(&home) && members.contains(&away) {
                let mut pair = [h2h[&home], h2h[&away]];
                apply_result(&mut pair, 0, 1, hg, ag);
                h2h.insert(home, pair[0]);
                h2h.insert(away, pair[1]);
            }
        }
    }

    // Called for every club of the block whatever decides it, so a simulated
    // season's random stream does not depend on the table.
    let keys: HashMap<usize, u64> = block.iter().map(|&c| (c, key(c))).collect();
    let mut sorted = block.to_vec();
    sorted.sort_by_key(|&c| {
        let m = &h2h[&c];
        let o = &records[c];
        std::cmp::Reverse((m.points, m.gd(), m.gf, o.gd(), o.gf, keys[&c]))
    });
    sorted
}

/// Whether every pair of clubs in `block` has played both its meetings,
/// home and away.
fn meetings_complete(block: &[usize], results: &HashMap<(usize, usize), (i64, i64)>) -> bool {
    block.iter().all(|&a| {
        block
            .iter()
            .all(|&b| a == b || results.contains_key(&(a, b)))
    })
}

/// The table as it stands after `results` (keyed as for `rank_table`): each
/// club's record, and the clubs in table order.
///
/// TFF's regulation, as the press reports its text (tff.org was not
/// reachable from the development environment), ranks clubs level on points
/// by head-to-head at the end of the season (`rank_table`), when every pair
/// has met twice. Press sources differ on the table during the season: some
/// say head-to-head counts once the clubs' meetings are complete, others
/// that the weekly table uses overall goal difference until the season
/// ends. Assumed here is the first reading, which agrees with the
/// season-end rule: within a block of clubs level on points, the
/// head-to-head mini-table applies only if every pair in the block has
/// played both its meetings, home and away. Otherwise the block
/// goes by overall goal difference, then goals scored. So one first-half 1-0
/// does not lift the winner above a club level on points with the better
/// goal difference. Where the mini-table applies, it applies once, as in
/// `rank_table`. At the season's end every pair has met twice, so this is
/// `rank_table`'s order.
///
/// The rules end in a play-off, which a live table has none of. Clubs that
/// no rule separates (before the first result, every club) are listed by
/// name in Turkish alphabetical order instead, so the table is the same on
/// every request.
pub fn standings(
    names: &[String],
    results: &HashMap<(usize, usize), (i64, i64)>,
) -> (Vec<TeamRecord>, Vec<usize>) {
    let mut records = vec![TeamRecord::default(); names.len()];
    for (&(home, away), &(hg, ag)) in results {
        apply_result(&mut records, home, away, hg, ag);
    }
    let name_key = name_keys(names);
    let order = rank(
        &records,
        results,
        HeadToHead::WhenMeetingsComplete,
        &mut |club| name_key[club],
    );
    (records, order)
}

/// The live table's stand-in for the play-off: each club's key, larger for
/// a name earlier in the Turkish alphabet, so that name ranks higher.
fn name_keys(names: &[String]) -> Vec<u64> {
    let mut by_name: Vec<usize> = (0..names.len()).collect();
    by_name.sort_by(|&a, &b| turkish_order(&names[a], &names[b]));
    let mut name_key = vec![0; names.len()];
    for (place, &club) in by_name.iter().enumerate() {
        name_key[club] = (names.len() - place) as u64;
    }
    name_key
}

/// Turkish alphabetical order, ignoring case: Ç after C, Ğ after G, I (ı)
/// before İ (i), Ö after O, Ş after S, Ü after U. Byte order would put Çorum
/// after Trabzonspor.
fn turkish_order(a: &str, b: &str) -> std::cmp::Ordering {
    const ALPHABET: &str = "abcçdefgğhıijklmnoöpqrsştuüvwxyz";
    let key = |name: &str| -> Vec<u32> {
        name.chars()
            .map(|c| {
                let c = match c {
                    'I' => 'ı',
                    'İ' => 'i',
                    c => c.to_lowercase().next().unwrap_or(c),
                };
                // Spaces, digits and punctuation before the letters, any
                // other character after them.
                match ALPHABET.chars().position(|l| l == c) {
                    Some(i) => 0x100 + i as u32,
                    None if c.is_ascii() => c as u32,
                    None => 0x1_0000 + c as u32,
                }
            })
            .collect()
    };
    key(a).cmp(&key(b)).then_with(|| a.cmp(b))
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

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// `rank_table` is `rank_table_by` with a key that ignores the club, and
    /// a simulated season draws the same numbers through either: the stand-in
    /// is called once for every club of every block level on points, even
    /// the clubs an earlier rule separates, blocks from the top, clubs in
    /// index order within a block, and never for a club alone on its points.
    #[test]
    fn rank_table_and_rank_table_by_draw_the_same_numbers() {
        use rand::rngs::SmallRng;
        use rand::{Rng, SeedableRng};

        // Clubs 0, 2 and 4 level on 10 points with nothing to separate them
        // (they have not met); clubs 1 and 5 level on 7, which goal
        // difference separates; club 3 alone on 3.
        let records = vec![
            rec(10, 8, 4),
            rec(7, 6, 6),
            rec(10, 8, 4),
            rec(3, 2, 9),
            rec(10, 8, 4),
            rec(7, 9, 5),
        ];
        let mut results = HashMap::new();
        results.insert((1, 5), (1, 1));
        results.insert((5, 1), (0, 0));

        let mut rng = SmallRng::seed_from_u64(20261019);
        let mut drawn = Vec::new();
        let order = rank_table(&records, &results, &mut || {
            let x = rng.gen::<u64>();
            drawn.push(x);
            x
        });

        let mut rng = SmallRng::seed_from_u64(20261019);
        let mut calls = Vec::new();
        let order_by = rank_table_by(&records, &results, &mut |club| {
            let x = rng.gen::<u64>();
            calls.push((club, x));
            x
        });

        assert_eq!(order, order_by);
        assert_eq!(drawn, calls.iter().map(|&(_, x)| x).collect::<Vec<_>>());
        assert_eq!(
            calls.iter().map(|&(club, _)| club).collect::<Vec<_>>(),
            [0, 2, 4, 1, 5]
        );
        // The draws decide the three-way tie: the largest ranks highest.
        let mut top = calls[..3].to_vec();
        top.sort_by_key(|&(_, x)| std::cmp::Reverse(x));
        let top: Vec<usize> = top.iter().map(|&(club, _)| club).collect();
        assert_eq!(order[..3], top[..]);
        assert_eq!(order[3..], [5, 1, 3]);
    }

    /// In the live table one meeting is not yet head-to-head. Alanyaspor and
    /// Beşiktaş are level on 3 points and Alanyaspor won the only meeting,
    /// 1-0, but the return is still to come, so Beşiktaş's better goal
    /// difference (+5 against +1) puts it above.
    #[test]
    fn standings_use_goal_difference_until_the_meetings_are_complete() {
        let clubs = names(&["Alanyaspor", "Beşiktaş", "Çorum", "Rizespor"]);
        let mut results = HashMap::new();
        results.insert((0, 1), (1, 0)); // Alanyaspor 1-0 Beşiktaş
        results.insert((1, 2), (6, 0)); // Beşiktaş 6-0 Çorum
        let (records, order) = standings(&clubs, &results);

        assert_eq!(
            records.iter().map(|r| r.points).collect::<Vec<_>>(),
            [3, 3, 0, 0]
        );
        assert_eq!(
            records.iter().map(TeamRecord::gd).collect::<Vec<_>>(),
            [1, 5, -6, 0]
        );
        // Rizespor and Çorum, level on nothing and never met, go by goal
        // difference too.
        assert_eq!(order, vec![1, 0, 3, 2]);
        // The season-end rule would put the 1-0 first.
        let by_head_to_head = rank_table_by(&records, &results, &mut |_| 0);
        assert_eq!(by_head_to_head[..2], [0, 1]);
    }

    /// The same two clubs once the return is played, 0-0: still level on
    /// points, both meetings in, so head-to-head (Alanyaspor 4 points,
    /// Beşiktaş 1) decides before Beşiktaş's goal difference.
    #[test]
    fn standings_use_head_to_head_once_both_meetings_are_played() {
        let clubs = names(&["Alanyaspor", "Beşiktaş", "Çorum", "Rizespor"]);
        let mut results = HashMap::new();
        results.insert((0, 1), (1, 0)); // Alanyaspor 1-0 Beşiktaş
        results.insert((1, 2), (6, 0)); // Beşiktaş 6-0 Çorum
        results.insert((1, 0), (0, 0)); // Beşiktaş 0-0 Alanyaspor
        let (records, order) = standings(&clubs, &results);

        assert_eq!(
            records.iter().map(|r| r.points).collect::<Vec<_>>(),
            [4, 4, 0, 0]
        );
        assert!(records[1].gd() > records[0].gd());
        assert_eq!(order, vec![0, 1, 3, 2]);
    }

    /// Head-to-head needs every pair in the block to have met twice. Three
    /// clubs level on 10 points: A has beaten B twice and taken 4 points off
    /// C, but B and C have met only once, so the block goes by goal
    /// difference, C (+13), B (+5), A (−1), the reverse of head-to-head.
    #[test]
    fn standings_use_goal_difference_when_any_pair_of_a_block_has_met_once() {
        let clubs = names(&["A", "B", "C", "D", "E", "F"]);
        let results = HashMap::from([
            ((0, 1), (2, 0)), // A 2-0 B
            ((1, 0), (0, 1)), // B 0-1 A
            ((0, 2), (1, 0)), // A 1-0 C
            ((2, 0), (0, 0)), // C 0-0 A
            ((1, 2), (1, 0)), // B 1-0 C, the return still to play
            ((3, 0), (5, 0)), // D 5-0 A
            ((1, 3), (3, 0)), // B 3-0 D
            ((1, 4), (4, 0)), // B 4-0 E
            ((5, 1), (1, 1)), // F 1-1 B
            ((2, 3), (5, 0)), // C 5-0 D
            ((2, 4), (5, 0)), // C 5-0 E
            ((2, 5), (5, 0)), // C 5-0 F
        ]);
        let (records, order) = standings(&clubs, &results);

        assert_eq!(
            records.iter().map(|r| r.points).collect::<Vec<_>>(),
            [10, 10, 10, 3, 0, 1]
        );
        assert_eq!(
            records[..3].iter().map(TeamRecord::gd).collect::<Vec<_>>(),
            [-1, 5, 13]
        );
        assert_eq!(order, vec![2, 1, 0, 3, 5, 4]);
        // Head-to-head over the meetings so far would give A, B, C.
        let by_head_to_head = rank_table_by(&records, &results, &mut |_| 0);
        assert_eq!(by_head_to_head[..3], [0, 1, 2]);
    }

    /// At the season's end every pair has met twice, so the live table is
    /// `rank_table`'s order, with the names as the tie key. Checked over
    /// random low-scoring double round-robins, where clubs level on points
    /// are common and head-to-head often orders them otherwise than goal
    /// difference would.
    #[test]
    fn a_complete_season_is_ranked_as_rank_table_ranks_it() {
        use rand::rngs::SmallRng;
        use rand::{Rng, SeedableRng};

        let clubs = names(&[
            "Trabzonspor",
            "Çorum",
            "Göztepe",
            "Alanyaspor",
            "İstanbulspor",
            "Iğdır",
        ]);
        let key = name_keys(&clubs);
        let mut rng = SmallRng::seed_from_u64(4242);
        let mut head_to_head_mattered = 0;
        for _ in 0..200 {
            let mut results = HashMap::new();
            for home in 0..clubs.len() {
                for away in 0..clubs.len() {
                    if home != away {
                        results.insert((home, away), (rng.gen_range(0..3), rng.gen_range(0..3)));
                    }
                }
            }
            let (records, order) = standings(&clubs, &results);
            assert_eq!(order, rank_table_by(&records, &results, &mut |c| key[c]));

            let mut by_goal_difference: Vec<usize> = (0..clubs.len()).collect();
            by_goal_difference.sort_by_key(|&c| {
                let r = &records[c];
                std::cmp::Reverse((r.points, r.gd(), r.gf, key[c]))
            });
            if order != by_goal_difference {
                head_to_head_mattered += 1;
            }
        }
        assert!(head_to_head_mattered >= 20, "{head_to_head_mattered}");
    }

    /// No play-off to draw on: clubs no rule separates are listed by name, in
    /// Turkish alphabetical order, whatever order the results arrive in.
    #[test]
    fn standings_list_clubs_no_rule_separates_by_name() {
        let clubs = names(&[
            "Trabzonspor",
            "Çorum",
            "İstanbulspor",
            "Alanyaspor",
            "Iğdır",
            "Beşiktaş",
        ]);
        // Before the first result every club is level.
        let (records, order) = standings(&clubs, &HashMap::new());
        assert!(records.iter().all(|r| *r == TeamRecord::default()));
        let listed: Vec<&str> = order.iter().map(|&c| clubs[c].as_str()).collect();
        assert_eq!(
            listed,
            [
                "Alanyaspor",
                "Beşiktaş",
                "Çorum",
                "Iğdır",
                "İstanbulspor",
                "Trabzonspor"
            ]
        );

        // Trabzonspor and Çorum draw 1-1 and are level on everything: Çorum,
        // earlier in the alphabet, lists first.
        let one = HashMap::from([((0, 1), (1, 1))]);
        let (_, order) = standings(&clubs, &one);
        assert_eq!(&order[..2], [1, 0]);

        // A whole set of results, inserted in two different orders, gives
        // one table.
        let played = [
            ((0, 3), (2, 2)),
            ((1, 4), (0, 0)),
            ((2, 5), (2, 2)),
            ((3, 1), (1, 0)),
        ];
        let forward: HashMap<_, _> = played.iter().copied().collect();
        let backward: HashMap<_, _> = played.iter().rev().copied().collect();
        assert_eq!(standings(&clubs, &forward), standings(&clubs, &backward));
    }
}
