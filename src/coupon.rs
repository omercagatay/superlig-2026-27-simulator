//! A conservative, transparent "Günün Kuponu" built from the model's 1X2
//! probabilities and fresh İddaa prices.
//!
//! Market prices never alter the season simulation. They are used here only
//! to find places where the already-computed model probability is materially
//! above the margin-free probability implied by the licensed market. If no
//! selection clears every guardrail, the honest output is `NoValue`.

use std::cmp::Ordering;

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveTime, Utc};
use serde::Serialize;

use crate::market::{MarketPrices, MarketSnapshot};
use crate::sim::World;

pub const MAX_SELECTIONS: usize = 3;
pub const MIN_MODEL_PCT: f64 = 30.0;
pub const MIN_EDGE_PCT: f64 = 2.0;
pub const MIN_VALUE_INDEX: f64 = 1.02;
pub const MAX_MARKET_ODDS: f64 = 4.0;
pub const MARKET_MAX_AGE_MINUTES: i64 = 90;

const TURKEY_UTC_OFFSET_SECONDS: i32 = 3 * 60 * 60;
const OPERATOR_VERIFIED_AT: &str = "2026-08-27";

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CouponStatus {
    Ready,
    NoValue,
    MarketUnavailable,
    MarketStale,
}

#[derive(Serialize, Clone, Debug)]
pub struct CouponSelection {
    pub round: u8,
    pub date: String,
    pub kickoff: Option<String>,
    pub home: String,
    pub away: String,
    /// İddaa 1X2 code: `1`, `X`, or `2`.
    pub outcome: String,
    pub model_pct: f64,
    /// Market-implied probability after removing the overround.
    pub market_pct: f64,
    pub market_odds: f64,
    /// Model probability minus margin-free market probability, in points.
    pub edge_pct: f64,
    /// `model_probability * decimal_odds`; above 1.0 is positive expected
    /// value if (and only if) the model probability is well calibrated.
    pub value_index: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct LicensedOperator {
    pub name: &'static str,
    pub url: &'static str,
    /// A first-party page that identifies the site as a legal/authorized
    /// Spor Toto outlet. Kept separate from the destination for auditability.
    pub verification_url: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct CouponSource {
    pub odds_provider: &'static str,
    pub odds_url: &'static str,
    pub regulator: &'static str,
    pub regulator_url: &'static str,
    pub operator_verified_at: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct DailyCouponResponse {
    pub status: CouponStatus,
    pub round: Option<u8>,
    pub generated_at: String,
    pub market_fetched_at: Option<String>,
    pub window_from: Option<String>,
    pub window_to: Option<String>,
    pub selections: Vec<CouponSelection>,
    pub combined_odds: Option<f64>,
    /// Approximation obtained by multiplying the individual model
    /// probabilities. It is descriptive, not a guarantee.
    pub combined_model_pct: Option<f64>,
    pub source: CouponSource,
    pub licensed_operators: Vec<LicensedOperator>,
}

#[derive(Clone, Debug)]
struct Candidate {
    selection: CouponSelection,
}

/// Six first-party sites that currently identify themselves as legal Spor
/// Toto outlets. Plain links only: this project has no affiliate relationship
/// and does not submit wagers.
fn licensed_operators() -> Vec<LicensedOperator> {
    vec![
        LicensedOperator {
            name: "Nesine",
            url: "https://www.nesine.com/",
            verification_url: "https://www.nesine.com/",
        },
        LicensedOperator {
            name: "Bilyoner",
            url: "https://www.bilyoner.com/",
            verification_url: "https://www.bilyoner.com/",
        },
        LicensedOperator {
            name: "Misli",
            url: "https://www.misli.com/",
            verification_url: "https://www.misli.com/hakkimizda",
        },
        LicensedOperator {
            name: "Oley",
            url: "https://www.oley.com/",
            verification_url: "https://www.oley.com/hakkimizda",
        },
        LicensedOperator {
            name: "Birebin",
            url: "https://www.birebin.com/",
            verification_url: "https://www.birebin.com/",
        },
        LicensedOperator {
            name: "iddaa.com",
            url: "https://www.iddaa.com/",
            verification_url: "https://www.iddaa.com/yardim/detay/neden-bayi-secmeliyim-29874",
        },
    ]
}

fn source() -> CouponSource {
    CouponSource {
        odds_provider: "Nesine İddaa bulletin",
        odds_url: "https://www.nesine.com/iddaa",
        regulator: "Spor Toto Teşkilat Başkanlığı",
        regulator_url: "https://www.sportoto.gov.tr/",
        operator_verified_at: OPERATOR_VERIFIED_AT,
    }
}

fn base_response(now: DateTime<Utc>, status: CouponStatus) -> DailyCouponResponse {
    DailyCouponResponse {
        status,
        round: None,
        generated_at: now.to_rfc3339(),
        market_fetched_at: None,
        window_from: None,
        window_to: None,
        selections: Vec::new(),
        combined_odds: None,
        combined_model_pct: None,
        source: source(),
        licensed_operators: licensed_operators(),
    }
}

/// Odds move during the day. A stale last-good snapshot is useful on the
/// match cards as historical context, but must not be presented as a coupon.
pub fn snapshot_is_fresh(snapshot: &MarketSnapshot, now: DateTime<Utc>) -> bool {
    let Ok(fetched) = DateTime::parse_from_rfc3339(&snapshot.fetched_at) else {
        return false;
    };
    let fetched = fetched.with_timezone(&Utc);
    let age = now.signed_duration_since(fetched);
    age >= Duration::minutes(-5) && age <= Duration::minutes(MARKET_MAX_AGE_MINUTES)
}

fn fixture_is_upcoming(date: &str, kickoff: Option<&str>, now: DateTime<Utc>) -> bool {
    let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
        return false;
    };
    let turkey = FixedOffset::east_opt(TURKEY_UTC_OFFSET_SECONDS).expect("valid Turkey offset");
    let local_now = now.with_timezone(&turkey).naive_local();
    match date.cmp(&local_now.date()) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => kickoff
            .and_then(|value| NaiveTime::parse_from_str(value, "%H:%M").ok())
            .map(|time| date.and_time(time) > local_now)
            // A date without a published kick-off remains eligible until the
            // day ends; hiding it would be a stronger claim than the data.
            .unwrap_or(true),
    }
}

fn best_candidate_for_fixture(
    round: u8,
    date: &str,
    kickoff: Option<&str>,
    home: &str,
    away: &str,
    model: (f64, f64, f64),
    prices: &MarketPrices,
) -> Option<Candidate> {
    let (market_home, market_draw, market_away) = prices.implied_pct();
    let outcomes = [
        ("1", model.0, market_home, prices.home_odds),
        ("X", model.1, market_draw, prices.draw_odds),
        ("2", model.2, market_away, prices.away_odds),
    ];

    outcomes
        .into_iter()
        .filter_map(|(outcome, model_pct, market_pct, market_odds)| {
            let edge_pct = model_pct - market_pct;
            let value_index = model_pct / 100.0 * market_odds;
            let clears_guardrails = model_pct >= MIN_MODEL_PCT
                && edge_pct >= MIN_EDGE_PCT
                && value_index >= MIN_VALUE_INDEX
                && market_odds.is_finite()
                && market_odds > 1.0
                && market_odds <= MAX_MARKET_ODDS;
            clears_guardrails.then(|| Candidate {
                selection: CouponSelection {
                    round,
                    date: date.to_string(),
                    kickoff: kickoff.map(str::to_string),
                    home: home.to_string(),
                    away: away.to_string(),
                    outcome: outcome.to_string(),
                    model_pct,
                    market_pct,
                    market_odds,
                    edge_pct,
                    value_index,
                },
            })
        })
        .max_by(|a, b| {
            a.selection
                .value_index
                .partial_cmp(&b.selection.value_index)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    a.selection
                        .model_pct
                        .partial_cmp(&b.selection.model_pct)
                        .unwrap_or(Ordering::Equal)
                })
        })
}

pub fn build_daily_coupon(world: &World, market: Option<&MarketSnapshot>) -> DailyCouponResponse {
    build_daily_coupon_at(world, market, Utc::now())
}

fn build_daily_coupon_at(
    world: &World,
    market: Option<&MarketSnapshot>,
    now: DateTime<Utc>,
) -> DailyCouponResponse {
    let Some(market) = market else {
        return base_response(now, CouponStatus::MarketUnavailable);
    };
    if !snapshot_is_fresh(market, now) {
        let mut response = base_response(now, CouponStatus::MarketStale);
        response.market_fetched_at = Some(market.fetched_at.clone());
        return response;
    }

    // Embedded fixture scores can lag the live refresh. Determine the active
    // round only from genuinely future, unplayed fixtures so past postponed
    // rows cannot pull the coupon backwards.
    let round = world
        .fixtures
        .iter()
        .enumerate()
        .filter(|(i, fixture)| {
            !world.played.contains_key(&(fixture.home, fixture.away))
                && fixture_is_upcoming(
                    &world.dates[*i].date,
                    world.dates[*i].kickoff.as_deref(),
                    now,
                )
        })
        .map(|(_, fixture)| fixture.round)
        .min();

    let Some(round) = round else {
        let mut response = base_response(now, CouponStatus::NoValue);
        response.market_fetched_at = Some(market.fetched_at.clone());
        return response;
    };

    let mut candidates = Vec::new();
    let mut priced_fixtures = 0usize;
    for (i, fixture) in world.fixtures.iter().enumerate() {
        if fixture.round != round
            || world.played.contains_key(&(fixture.home, fixture.away))
            || !fixture_is_upcoming(&world.dates[i].date, world.dates[i].kickoff.as_deref(), now)
        {
            continue;
        }
        let home = world.teams[fixture.home].as_str();
        let away = world.teams[fixture.away].as_str();
        let Some(prices) = market.get(home, away) else {
            continue;
        };
        priced_fixtures += 1;
        let p = world.fixture_probs(fixture.home, fixture.away);
        if let Some(candidate) = best_candidate_for_fixture(
            round,
            &world.dates[i].date,
            world.dates[i].kickoff.as_deref(),
            home,
            away,
            (p.home_win_pct, p.draw_pct, p.away_win_pct),
            prices,
        ) {
            candidates.push(candidate);
        }
    }

    candidates.sort_by(|a, b| {
        b.selection
            .value_index
            .partial_cmp(&a.selection.value_index)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.selection.date.cmp(&b.selection.date))
            .then_with(|| a.selection.home.cmp(&b.selection.home))
    });
    candidates.truncate(MAX_SELECTIONS);
    let selections: Vec<CouponSelection> = candidates
        .into_iter()
        .map(|candidate| candidate.selection)
        .collect();

    let status = if !selections.is_empty() {
        CouponStatus::Ready
    } else if priced_fixtures == 0 {
        // A successfully fetched bulletin can still contain no matching
        // prices (schema drift, a temporarily incomplete feed, or an
        // unpriced round). That is missing market coverage, not evidence
        // that the model found no value.
        CouponStatus::MarketUnavailable
    } else {
        CouponStatus::NoValue
    };
    let mut response = base_response(now, status);
    response.round = Some(round);
    response.market_fetched_at = Some(market.fetched_at.clone());
    response.window_from = selections.iter().map(|s| s.date.clone()).min();
    response.window_to = selections.iter().map(|s| s.date.clone()).max();
    if !selections.is_empty() {
        response.combined_odds = Some(selections.iter().map(|s| s.market_odds).product());
        response.combined_model_pct = Some(
            selections
                .iter()
                .map(|s| s.model_pct / 100.0)
                .product::<f64>()
                * 100.0,
        );
    }
    response.selections = selections;
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn now() -> DateTime<Utc> {
        "2026-08-27T12:00:00Z".parse().expect("valid test time")
    }

    fn snapshot(prices: HashMap<(String, String), MarketPrices>) -> MarketSnapshot {
        MarketSnapshot {
            prices,
            fetched_at: "2026-08-27T11:45:00Z".to_string(),
        }
    }

    #[test]
    fn missing_and_stale_market_never_emit_selections() {
        let world = World::new();
        let missing = build_daily_coupon_at(&world, None, now());
        assert_eq!(missing.status, CouponStatus::MarketUnavailable);
        assert!(missing.selections.is_empty());

        let empty = build_daily_coupon_at(&world, Some(&snapshot(HashMap::new())), now());
        assert_eq!(empty.status, CouponStatus::MarketUnavailable);
        assert!(empty.selections.is_empty());

        let stale = MarketSnapshot {
            prices: HashMap::new(),
            fetched_at: "2026-08-27T08:00:00Z".to_string(),
        };
        let stale = build_daily_coupon_at(&world, Some(&stale), now());
        assert_eq!(stale.status, CouponStatus::MarketStale);
        assert!(stale.selections.is_empty());
    }

    #[test]
    fn coupon_keeps_one_guardrailed_pick_per_fixture_and_caps_the_size() {
        let world = World::new();
        let mut prices = HashMap::new();

        // Price four future round-three fixtures so the three-selection cap
        // is exercised. The model's likeliest outcome gets an intentionally
        // generous test price; the two other outcomes cannot pass value.
        for (i, fixture) in world
            .fixtures
            .iter()
            .enumerate()
            .filter(|(i, fixture)| {
                fixture.round == 3
                    && fixture_is_upcoming(
                        &world.dates[*i].date,
                        world.dates[*i].kickoff.as_deref(),
                        now(),
                    )
            })
            .take(4)
        {
            let p = world.fixture_probs(fixture.home, fixture.away);
            let mut odds = [1.01, 1.01, 1.01];
            let best = [p.home_win_pct, p.draw_pct, p.away_win_pct]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(index, _)| index)
                .unwrap();
            odds[best] = 3.5;
            prices.insert(
                (
                    world.teams[fixture.home].clone(),
                    world.teams[fixture.away].clone(),
                ),
                MarketPrices {
                    home_odds: odds[0],
                    draw_odds: odds[1],
                    away_odds: odds[2],
                },
            );
            assert!(fixture_is_upcoming(
                &world.dates[i].date,
                world.dates[i].kickoff.as_deref(),
                now()
            ));
        }

        let response = build_daily_coupon_at(&world, Some(&snapshot(prices)), now());
        assert_eq!(response.status, CouponStatus::Ready);
        assert_eq!(response.round, Some(3));
        assert_eq!(response.selections.len(), MAX_SELECTIONS);

        let fixtures: HashSet<_> = response
            .selections
            .iter()
            .map(|s| (&s.home, &s.away))
            .collect();
        assert_eq!(fixtures.len(), response.selections.len());
        for selection in &response.selections {
            assert!(selection.model_pct >= MIN_MODEL_PCT);
            assert!(selection.edge_pct >= MIN_EDGE_PCT);
            assert!(selection.value_index >= MIN_VALUE_INDEX);
            assert!(selection.market_odds <= MAX_MARKET_ODDS);
        }

        let expected_odds: f64 = response.selections.iter().map(|s| s.market_odds).product();
        let expected_pct: f64 = response
            .selections
            .iter()
            .map(|s| s.model_pct / 100.0)
            .product::<f64>()
            * 100.0;
        assert!((response.combined_odds.unwrap() - expected_odds).abs() < 1e-12);
        assert!((response.combined_model_pct.unwrap() - expected_pct).abs() < 1e-12);
    }

    #[test]
    fn no_value_is_an_explicit_result_instead_of_a_forced_coupon() {
        let world = World::new();
        let mut prices = HashMap::new();
        for (i, fixture) in world.fixtures.iter().enumerate().filter(|(i, fixture)| {
            fixture.round == 3
                && fixture_is_upcoming(
                    &world.dates[*i].date,
                    world.dates[*i].kickoff.as_deref(),
                    now(),
                )
        }) {
            prices.insert(
                (
                    world.teams[fixture.home].clone(),
                    world.teams[fixture.away].clone(),
                ),
                MarketPrices {
                    home_odds: 1.01,
                    draw_odds: 1.01,
                    away_odds: 1.01,
                },
            );
            assert!(fixture_is_upcoming(
                &world.dates[i].date,
                world.dates[i].kickoff.as_deref(),
                now()
            ));
        }

        let response = build_daily_coupon_at(&world, Some(&snapshot(prices)), now());
        assert_eq!(response.status, CouponStatus::NoValue);
        assert!(response.selections.is_empty());
        assert!(response.combined_odds.is_none());
    }

    #[test]
    fn operator_links_are_https_and_auditable() {
        let operators = licensed_operators();
        assert_eq!(operators.len(), 6);
        for operator in operators {
            assert!(operator.url.starts_with("https://"));
            assert!(operator.verification_url.starts_with("https://"));
        }
    }
}
