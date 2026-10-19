use std::collections::HashMap;

use crate::sim::World;

const MIN_SIMS: usize = 100;
const MAX_SIMS: usize = 200_000;
/// The 2026-27 preseason ratings (`sim::preseason_ratings`) run from about
/// 1420 to 1855. These bounds leave room for large scenario swings in both
/// directions without admitting nonsense; a scenario change that would
/// cross them is clamped to them (`scenario_overrides`).
const MIN_ELO: f64 = 1200.0;
const MAX_ELO: f64 = 2000.0;
/// Scenario prompts are forwarded verbatim to the paid LLM API, so cap them
/// well below the 1 MB request-body limit to bound per-request token cost.
pub const MAX_PROMPT_CHARS: usize = 2000;

pub fn validate_prompt(prompt: &str) -> Result<(), String> {
    if prompt.trim().is_empty() {
        Err("Scenario prompt must not be empty".to_string())
    } else if prompt.chars().count() > MAX_PROMPT_CHARS {
        Err(format!(
            "Scenario prompt must be at most {MAX_PROMPT_CHARS} characters"
        ))
    } else {
        Ok(())
    }
}

pub fn validate_n_sims(n: usize) -> Result<usize, String> {
    if n < MIN_SIMS {
        Err(format!("n_sims must be at least {MIN_SIMS}"))
    } else if n > MAX_SIMS {
        Err(format!("n_sims must be at most {MAX_SIMS}"))
    } else {
        Ok(n)
    }
}

/// A generous cap: enough to pin a whole matchday and then some.
pub const MAX_WHAT_IF: usize = 20;

/// Resolve pinned outcomes to fixture indices, rejecting anything that is not
/// a real, still-unplayed fixture. A pin on a played match would silently do
/// nothing, which is worse than an error. So would a second pin on one
/// fixture, which would quietly replace the first (and count twice against
/// `MAX_WHAT_IF`), even when it pins the same outcome.
pub fn validate_what_if(
    world: &World,
    what_if: &[crate::models::WhatIf],
) -> Result<HashMap<(usize, usize), crate::sim::ForcedOutcome>, String> {
    if what_if.len() > MAX_WHAT_IF {
        return Err(format!(
            "At most {MAX_WHAT_IF} what-if results, got {}",
            what_if.len()
        ));
    }
    let mut out = HashMap::new();
    for w in what_if {
        let (Some(&h), Some(&a)) = (world.idx.get(&w.home), world.idx.get(&w.away)) else {
            return Err(format!("Unknown club in what-if: {} v {}", w.home, w.away));
        };
        if !world.fixtures.iter().any(|f| f.home == h && f.away == a) {
            return Err(format!(
                "{} v {} is not a fixture this season",
                w.home, w.away
            ));
        }
        if world.played.contains_key(&(h, a)) {
            return Err(format!("{} v {} has already been played", w.home, w.away));
        }
        let outcome = match w.outcome.as_str() {
            "home" => crate::sim::ForcedOutcome::Home,
            "draw" => crate::sim::ForcedOutcome::Draw,
            "away" => crate::sim::ForcedOutcome::Away,
            other => return Err(format!("Unknown outcome {other:?}; use home, draw or away")),
        };
        if out.insert((h, a), outcome).is_some() {
            return Err(format!("{} v {} is pinned twice", w.home, w.away));
        }
    }
    Ok(out)
}

pub fn validate_elo_overrides(
    world: &World,
    overrides: &HashMap<String, f64>,
) -> Result<(), String> {
    for (team, rating) in overrides {
        if !world.idx.contains_key(team) {
            return Err(format!("Unknown team in Elo overrides: {team}"));
        }
        if !rating.is_finite() || *rating < MIN_ELO || *rating > MAX_ELO {
            return Err(format!(
                "Elo rating for {team} must be between {MIN_ELO:.0} and {MAX_ELO:.0}, got {rating:.1}"
            ));
        }
    }
    Ok(())
}

/// Largest rating change a scenario may apply to one club, in Elo points.
pub const MAX_SCENARIO_SHIFT: f64 = 150.0;

/// Turn an LLM scenario's per-club rating changes into absolute overrides on
/// the world's current ratings. The LLM's output is untrusted, so names and
/// the size of every change are checked here. A change within
/// ±`MAX_SCENARIO_SHIFT` that would carry a club past `MIN_ELO` or
/// `MAX_ELO` lands on the bound, logged, instead of failing the scenario:
/// the model kept to its cap, and how near the bound the club already
/// stood is not its doing. (`validate_elo_overrides` still refuses an
/// API caller's override outside the bound.)
pub fn scenario_overrides(
    world: &World,
    shifts: &HashMap<String, f64>,
) -> Result<HashMap<String, f64>, String> {
    let mut overrides = HashMap::with_capacity(shifts.len());
    for (team, shift) in shifts {
        let Some(&i) = world.idx.get(team) else {
            return Err(format!("Unknown team in scenario: {team}"));
        };
        if !shift.is_finite() || shift.abs() > MAX_SCENARIO_SHIFT {
            return Err(format!(
                "Scenario change for {team} must be within ±{MAX_SCENARIO_SHIFT:.0} points, got {shift:.1}"
            ));
        }
        let wanted = world.elo[i] + shift;
        let rating = wanted.clamp(MIN_ELO, MAX_ELO);
        if rating != wanted {
            tracing::warn!(
                "Scenario change of {shift:+.1} for {team} would rate it {wanted:.1}; clamped to {rating:.0}"
            );
        }
        overrides.insert(team.clone(), rating);
    }
    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_changes_apply_to_current_ratings_and_are_bounded() {
        let world = World::with_played(crate::sim::MID_SEASON);
        let gs = world.idx["Galatasaray"];
        assert_ne!(world.elo[gs], world.elo_baseline[gs], "results moved it");
        let shifts = HashMap::from([("Galatasaray".to_string(), -40.0)]);
        let o = scenario_overrides(&world, &shifts).unwrap();
        assert!((o["Galatasaray"] - (world.elo[gs] - 40.0)).abs() < 1e-9);

        let too_big = HashMap::from([("Galatasaray".to_string(), -500.0)]);
        assert!(scenario_overrides(&world, &too_big).is_err());
        let unknown = HashMap::from([("Real Madrid".to_string(), 10.0)]);
        assert!(scenario_overrides(&world, &unknown).is_err());
        let nan = HashMap::from([("Galatasaray".to_string(), f64::NAN)]);
        assert!(scenario_overrides(&world, &nan).is_err());
    }

    /// A change within ±`MAX_SCENARIO_SHIFT` that would carry a club past
    /// the rating bound lands on the bound instead of failing the scenario;
    /// the other clubs' changes are applied as they are.
    #[test]
    fn a_scenario_change_past_the_rating_bound_is_clamped_to_it() {
        let mut world = World::with_played(0);
        let (gs, fb) = (world.idx["Galatasaray"], world.idx["Fenerbahçe"]);
        world.elo[gs] = MIN_ELO + 112.0;
        world.elo[fb] = MAX_ELO - 30.0;
        let shifts = HashMap::from([
            ("Galatasaray".to_string(), -120.0),
            ("Fenerbahçe".to_string(), MAX_SCENARIO_SHIFT),
            ("Göztepe".to_string(), -40.0),
        ]);
        let o = scenario_overrides(&world, &shifts).expect("in-bounds changes are usable");
        assert_eq!(o["Galatasaray"], MIN_ELO);
        assert_eq!(o["Fenerbahçe"], MAX_ELO);
        let gz = world.idx["Göztepe"];
        assert!((o["Göztepe"] - (world.elo[gz] - 40.0)).abs() < 1e-9);
        assert!(validate_elo_overrides(&world, &o).is_ok());
        // The API's own overrides are still refused outside the bound.
        let below = HashMap::from([("Galatasaray".to_string(), MIN_ELO - 8.0)]);
        assert!(validate_elo_overrides(&world, &below).is_err());
    }

    fn pin(home: &str, away: &str, outcome: &str) -> crate::models::WhatIf {
        crate::models::WhatIf {
            home: home.to_string(),
            away: away.to_string(),
            outcome: outcome.to_string(),
        }
    }

    /// One pin per fixture: a second one is refused whether it contradicts
    /// the first or repeats it.
    #[test]
    fn a_fixture_pinned_twice_is_refused() {
        let world = World::with_played(0);
        let named = |i: usize| {
            let f = world.fixtures[i];
            (world.teams[f.home].as_str(), world.teams[f.away].as_str())
        };
        let ((h, a), (h2, a2)) = (named(0), named(1));
        let once = [pin(h, a, "home"), pin(h2, a2, "draw")];
        let forced = validate_what_if(&world, &once).expect("two fixtures, one pin each");
        assert_eq!(forced.len(), 2);

        for second in ["away", "home"] {
            let twice = [pin(h, a, "home"), pin(h2, a2, "draw"), pin(h, a, second)];
            assert_eq!(
                validate_what_if(&world, &twice),
                Err(format!("{h} v {a} is pinned twice")),
                "a second {second} pin"
            );
        }
        // The reverse fixture is another match, not the same one twice.
        let both_legs = [pin(h, a, "home"), pin(a, h, "home")];
        assert_eq!(validate_what_if(&world, &both_legs).map(|f| f.len()), Ok(2));
    }

    #[test]
    fn n_sims_within_bounds_is_ok() {
        assert_eq!(validate_n_sims(100).unwrap(), 100);
        assert_eq!(validate_n_sims(50_000).unwrap(), 50_000);
        assert_eq!(validate_n_sims(200_000).unwrap(), 200_000);
    }

    #[test]
    fn n_sims_out_of_bounds_is_rejected() {
        assert!(validate_n_sims(50).is_err());
        assert!(validate_n_sims(200_001).is_err());
        assert!(validate_n_sims(0).is_err());
    }

    #[test]
    fn prompt_within_limit_is_ok() {
        assert!(validate_prompt("Mbappé injured in training").is_ok());
        assert!(validate_prompt(&"x".repeat(MAX_PROMPT_CHARS)).is_ok());
    }

    #[test]
    fn prompt_over_limit_is_rejected() {
        assert!(validate_prompt(&"x".repeat(MAX_PROMPT_CHARS + 1)).is_err());
    }

    #[test]
    fn empty_or_blank_prompt_is_rejected() {
        assert!(validate_prompt("").is_err());
        assert!(validate_prompt("   \n\t").is_err());
    }

    #[test]
    fn elo_overrides_validated() {
        let world = World::new();
        let mut overrides = HashMap::new();
        overrides.insert("Galatasaray".to_string(), 1850.0);
        assert!(validate_elo_overrides(&world, &overrides).is_ok());

        overrides.insert("Atlantis FC".to_string(), 1600.0);
        assert!(validate_elo_overrides(&world, &overrides).is_err());
        overrides.remove("Atlantis FC");

        overrides.insert("Galatasaray".to_string(), MIN_ELO - 1.0);
        assert!(validate_elo_overrides(&world, &overrides).is_err());

        overrides.insert("Galatasaray".to_string(), MAX_ELO + 1.0);
        assert!(validate_elo_overrides(&world, &overrides).is_err());

        overrides.insert("Galatasaray".to_string(), f64::NAN);
        assert!(validate_elo_overrides(&world, &overrides).is_err());
    }

    /// Every club's starting rating must sit inside the accepted range, or a
    /// scenario could never restore a club to its own baseline.
    #[test]
    fn every_club_baseline_rating_is_within_bounds() {
        let world = crate::sim::World::new();
        for (club, &rating) in world.teams.iter().zip(&world.elo_baseline) {
            assert!(
                (MIN_ELO..=MAX_ELO).contains(&rating),
                "{club} baseline {rating} outside [{MIN_ELO}, {MAX_ELO}]"
            );
        }
    }
}
