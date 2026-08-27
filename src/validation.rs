use std::collections::HashMap;

use crate::sim::World;

const MIN_SIMS: usize = 100;
const MAX_SIMS: usize = 200_000;
/// ClubElo's club scale is compressed relative to international Elo: the
/// 2026-27 Süper Lig spans roughly 1509-1779. These bounds leave room for
/// large scenario swings in both directions without admitting nonsense.
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

/// A generous cap: enough to pin a whole matchday and then some, low enough
/// that rejection sampling for improbable pins cannot dominate a run.
pub const MAX_WHAT_IF: usize = 20;

/// Resolve pinned outcomes to fixture indices, rejecting anything that is not
/// a real, still-unplayed fixture. A pin on a played match would silently do
/// nothing, which is worse than an error.
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
        out.insert((h, a), outcome);
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

#[cfg(test)]
mod tests {
    use super::*;

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
        for (club, rating) in crate::data::elo() {
            assert!(
                (MIN_ELO..=MAX_ELO).contains(&rating),
                "{club} baseline {rating} outside [{MIN_ELO}, {MAX_ELO}]"
            );
        }
    }
}
