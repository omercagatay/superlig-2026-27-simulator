use axum::{
    body::Bytes,
    extract::{rejection::JsonRejection, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};

use crate::forecast_cache::{ResponseCache, SimKey, DEFAULT_N_SIMS, DEFAULT_SEED};
use crate::llm;
use crate::models::{build_response, ScenarioRequest, SimRequest, SimResponse, WhatIf};
use crate::scraper::{self, LiveData};
use crate::sim::{SimConfig, World};
use crate::validation::{
    scenario_overrides, validate_elo_overrides, validate_n_sims, validate_prompt, validate_what_if,
};

/// Distinct simulation responses kept for the current world state.
const SIM_CACHE_ENTRIES: usize = 32;
/// How long a 503 asks the client to wait: a few default runs. A queued run
/// that timed out sends the same, though the queue may then take longer to
/// drain: on a 503 it is a hint. The site waits min(Retry-After,
/// `MAX_WAIT_MS`) and asks again (frontend/src/retry.ts, 10 s), so keep this
/// at or under that cap.
const BUSY_RETRY_AFTER_SECS: u64 = 10;

#[derive(Clone)]
pub struct AppState {
    pub world: Arc<RwLock<World>>,
    /// Bumped whenever a live refresh changes the results or ratings the
    /// simulation reads. Written only under the `world` write lock and read
    /// under its read lock, so a version names exactly one world state.
    pub world_version: Arc<AtomicU64>,
    pub kimi_api_key: Option<String>,
    /// `/api/scenario` answers 404 unless SCENARIO_ENABLED is set: every call
    /// spends money on the LLM API.
    pub scenario_enabled: bool,
    pub live_data: Arc<RwLock<Option<LiveData>>>,
    pub simulation_queue: Arc<SimulationQueue>,
    /// Serialized simulation responses for the current world version.
    pub sim_cache: Arc<ResponseCache<SimKey, Bytes, ApiError>>,
}

impl AppState {
    pub fn new(
        world: World,
        kimi_api_key: Option<String>,
        scenario_enabled: bool,
        simulation_queue: SimulationQueue,
    ) -> Self {
        Self {
            world: Arc::new(RwLock::new(world)),
            world_version: Arc::new(AtomicU64::new(0)),
            kimi_api_key,
            scenario_enabled,
            live_data: Arc::new(RwLock::new(None)),
            simulation_queue: Arc::new(simulation_queue),
            sim_cache: Arc::new(ResponseCache::new(
                SIM_CACHE_ENTRIES,
                Some(SimKey::default_forecast()),
            )),
        }
    }
}

/// Global bound for CPU-heavy season simulations. Rayon already uses all
/// cores within one run, so overlapping runs only add memory pressure and
/// latency. A run that finds every slot busy waits its turn — at most
/// `max_waiting` of them, each for at most `timeout` — and is otherwise
/// turned away with a 503.
pub struct SimulationQueue {
    slots: Arc<Semaphore>,
    waiting: AtomicUsize,
    max_waiting: usize,
    timeout: Duration,
}

impl SimulationQueue {
    pub fn new(slots: usize, max_waiting: usize, timeout: Duration) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(slots)),
            waiting: AtomicUsize::new(0),
            max_waiting,
            timeout,
        }
    }

    async fn acquire(&self) -> Result<OwnedSemaphorePermit, ApiError> {
        if let Ok(permit) = self.slots.clone().try_acquire_owned() {
            return Ok(permit);
        }
        if !self.take_seat() {
            return Err(ApiError::busy("Simulation queue is full; retry shortly"));
        }
        let _seat = QueueSeat(&self.waiting);
        match tokio::time::timeout(self.timeout, self.slots.clone().acquire_owned()).await {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(_)) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Simulation capacity is closed".to_string(),
            )
                .into()),
            Err(_) => Err(ApiError::busy(
                "Timed out waiting for simulation capacity; retry shortly",
            )),
        }
    }

    /// Claim a waiting seat if one is free. An explicit compare-exchange
    /// loop: `fetch_update` is deprecated on current stable and its
    /// replacement `try_update` is newer than the MSRV.
    fn take_seat(&self) -> bool {
        let mut current = self.waiting.load(Ordering::Acquire);
        while current < self.max_waiting {
            match self.waiting.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
        false
    }
}

/// A place in the simulation queue, given back however the wait ends.
struct QueueSeat<'a>(&'a AtomicUsize);

impl Drop for QueueSeat<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A handler error: a status and a plain-text message, plus `Retry-After`
/// when the client should simply come back later.
#[derive(Clone, Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
    retry_after: Option<u64>,
}

impl ApiError {
    fn busy(message: &str) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.to_string(),
            retry_after: Some(BUSY_RETRY_AFTER_SECS),
        }
    }
}

impl From<(StatusCode, String)> for ApiError {
    fn from((status, message): (StatusCode, String)) -> Self {
        Self {
            status,
            message,
            retry_after: None,
        }
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(e: tokio::task::JoinError) -> Self {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Simulation task failed: {e}"),
        )
            .into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self.retry_after {
            Some(secs) => (
                self.status,
                [(header::RETRY_AFTER, secs.to_string())],
                self.message,
            )
                .into_response(),
            None => (self.status, self.message).into_response(),
        }
    }
}

/// A pre-serialized JSON body, sent exactly as `Json` would send it.
fn json_response(body: Bytes) -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body,
    )
        .into_response()
}

/// The current world version, read under the lock that orders its updates.
async fn current_version(state: &AppState) -> u64 {
    let _world = state.world.read().await;
    state.world_version.load(Ordering::Relaxed)
}

pub async fn run_sim(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SimRequest>,
) -> Result<Response, ApiError> {
    let body = cached_simulation(
        &state,
        req.n_sims.unwrap_or(DEFAULT_N_SIMS),
        req.seed.unwrap_or(DEFAULT_SEED),
        req.elo_overrides.unwrap_or_default(),
        req.what_if.as_deref().unwrap_or(&[]),
    )
    .await?;
    Ok(json_response(body))
}

/// The default forecast: the same bytes as `POST /api/simulate {}`, from a
/// cache warmed after every refresh attempt (or at startup when the
/// background refresh is off).
pub async fn forecast(State(state): State<Arc<AppState>>) -> Result<Response, ApiError> {
    Ok(json_response(default_forecast(&state).await?))
}

async fn default_forecast(state: &Arc<AppState>) -> Result<Bytes, ApiError> {
    cached_simulation(state, DEFAULT_N_SIMS, DEFAULT_SEED, HashMap::new(), &[]).await
}

/// Compute the default forecast in the background so the first visitor
/// after startup or a live refresh gets a cache hit. A no-op when the
/// current world's forecast is already cached.
pub fn spawn_forecast_warmup(state: Arc<AppState>) {
    tokio::spawn(async move {
        let version = current_version(&state).await;
        if state
            .sim_cache
            .get(version, &SimKey::default_forecast())
            .is_some()
        {
            return;
        }
        let started = std::time::Instant::now();
        match default_forecast(&state).await {
            Ok(_) => tracing::info!("Default forecast ready in {:.1?}", started.elapsed()),
            Err(e) => tracing::warn!("Default forecast warm-up failed: {}", e.message),
        }
    });
}

/// Validate a simulation request against the current world, then answer it
/// from the cache, by joining an identical run already in flight, or by
/// starting one. Only a fresh run takes a queue seat or a simulation slot,
/// and it gives up its seat if every client waiting for it disconnects.
async fn cached_simulation(
    state: &Arc<AppState>,
    n_sims: usize,
    seed: u64,
    elo_overrides: HashMap<String, f64>,
    what_if: &[WhatIf],
) -> Result<Bytes, ApiError> {
    let n_sims = validate_n_sims(n_sims).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let (version, key, world, forced) = {
        let world = state.world.read().await;
        let version = state.world_version.load(Ordering::Relaxed);
        validate_elo_overrides(&world, &elo_overrides).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        let forced = validate_what_if(&world, what_if).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        let key = SimKey::new(n_sims, seed, &elo_overrides, &forced);
        if let Some(hit) = state.sim_cache.get(version, &key) {
            return Ok(hit);
        }
        (version, key, world.clone(), forced)
    };
    let config = SimConfig {
        n_sims,
        seed,
        elo_overrides,
        forced,
    };
    let queue = state.simulation_queue.clone();
    state
        .sim_cache
        .get_or_compute(
            version,
            key,
            move || async move { queue.acquire().await },
            move |slot| async move {
                let mut world = world;
                world.apply_overrides(&config.elo_overrides);
                let resp = simulate_off_runtime(slot, world, config, None).await?;
                let body = serde_json::to_vec(&resp).map_err(|e| {
                    ApiError::from((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to encode simulation: {e}"),
                    ))
                })?;
                Ok::<_, ApiError>(Bytes::from(body))
            },
        )
        .await
}

/// Run the CPU-bound rayon simulation on the blocking pool so it can't
/// stall tokio's worker threads (and with them /api/health), holding a
/// simulation slot until it ends.
async fn simulate_off_runtime(
    slot: OwnedSemaphorePermit,
    world: World,
    config: SimConfig,
    scenario: Option<String>,
) -> Result<SimResponse, ApiError> {
    let resp = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let results = world.simulate(&config);
        build_response(&world, &results, &config, scenario)
    })
    .await?;
    Ok(resp)
}

pub async fn scenario(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<ScenarioRequest>, JsonRejection>,
) -> Result<Json<SimResponse>, ApiError> {
    if !state.scenario_enabled {
        return Err((
            StatusCode::NOT_FOUND,
            "Scenario analysis is not enabled on this server".to_string(),
        )
            .into());
    }
    let Json(req) = payload.map_err(|e| (e.status(), e.body_text()))?;
    let api_key = state.kimi_api_key.clone().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "KIMI_API_KEY not set — LLM scenario analysis is disabled".to_string(),
    ))?;

    let n_sims = validate_n_sims(req.n_sims.unwrap_or(DEFAULT_N_SIMS))
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    validate_prompt(&req.prompt).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // Upstream bodies and raw model output are logged by `llm`, never echoed.
    let impact = llm::analyze_scenario(&req.prompt, &api_key)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let world = state.world.read().await.clone();
    // The LLM returns rating changes; apply them to the current ratings. An
    // unknown club or an out-of-range change is the model's fault, not the
    // client's, and its text stays in the log.
    let overrides = scenario_overrides(&world, &impact.adjustments).map_err(|e| {
        tracing::warn!("Scenario model returned unusable adjustments: {e}");
        (
            StatusCode::BAD_GATEWAY,
            llm::LlmError::UnusableAnswer.to_string(),
        )
    })?;

    let mut world = world;
    world.apply_overrides(&overrides);

    let config = SimConfig {
        n_sims,
        seed: req.seed.unwrap_or(DEFAULT_SEED),
        elo_overrides: overrides,
        forced: HashMap::new(),
    };
    let slot = state.simulation_queue.acquire().await?;
    let resp = simulate_off_runtime(slot, world, config, Some(impact.analysis)).await?;
    Ok(Json(resp))
}

/// Scrape live data, apply it to the shared `World`, and cache the raw
/// result. Shared by the `/api/refresh` handler and the background
/// refresh task spawned in `main.rs`. Every attempt, failed or not, ends by
/// making sure the current world's default forecast is cached.
pub async fn perform_live_refresh(state: &Arc<AppState>) -> anyhow::Result<LiveData> {
    let result = refresh_once(state).await;
    spawn_forecast_warmup(state.clone());
    result
}

async fn refresh_once(state: &AppState) -> anyhow::Result<LiveData> {
    let live = scraper::fetch_all().await?;
    let applied = apply_live(state, &live).await?;
    tracing::info!("Live data applied to simulation: {applied} results");

    *state.live_data.write().await = Some(live.clone());

    Ok(live)
}

/// Fold a scrape into the shared `World`. The world version moves only when
/// the results or ratings the simulation reads actually changed, so a
/// refresh that brings nothing new keeps every cached response.
async fn apply_live(state: &AppState, live: &LiveData) -> anyhow::Result<usize> {
    let mut world = state.world.write().await;
    anyhow::ensure!(
        live.played_matches.len() >= world.played.len(),
        "TFF refresh regressed from {} recorded results to {}; keeping the last good state",
        world.played.len(),
        live.played_matches.len()
    );
    let played_before = world.played.clone();
    let elo_before = world.elo.clone();
    let applied = world.update_from_live(live);
    if world.played != played_before || world.elo != elo_before {
        let version = state.world_version.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::info!("World changed; now at version {version}");
    }
    Ok(applied)
}

pub async fn refresh_live_data(
    State(state): State<Arc<AppState>>,
) -> Result<Json<LiveData>, (StatusCode, String)> {
    let live = perform_live_refresh(&state)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Scrape error: {e}")))?;
    Ok(Json(live))
}

pub async fn get_live_data(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Option<LiveData>>, (StatusCode, String)> {
    let data = state.live_data.read().await.clone();
    Ok(Json(data))
}

/// Win/draw/loss forecasts for the next matchday's unplayed fixtures.
pub async fn upcoming(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::models::UpcomingResponse>, (StatusCode, String)> {
    let world = state.world.read().await.clone();
    let resp = tokio::task::spawn_blocking(move || {
        let matches = world
            .upcoming_matches()
            .into_iter()
            .map(|f| {
                let (home_win_pct, draw_pct, away_win_pct) = world.match_win_probs(f.home, f.away);
                crate::models::UpcomingMatch {
                    round: f.round,
                    home: world.teams[f.home].clone(),
                    away: world.teams[f.away].clone(),
                    home_win_pct,
                    draw_pct,
                    away_win_pct,
                    home_odds: crate::odds::decimal_odds_from_pct(home_win_pct),
                    draw_odds: crate::odds::decimal_odds_from_pct(draw_pct),
                    away_odds: crate::odds::decimal_odds_from_pct(away_win_pct),
                }
            })
            .collect();
        crate::models::UpcomingResponse { matches }
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Upcoming forecast task failed: {e}"),
        )
    })?;
    Ok(Json(resp))
}

/// The full 306-fixture calendar with model prices for every unplayed game.
pub async fn matches(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::models::MatchesResponse>, (StatusCode, String)> {
    let world = state.world.read().await.clone();
    let resp = tokio::task::spawn_blocking(move || crate::models::build_matches_response(&world))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Match forecast task failed: {e}"),
            )
        })?;
    Ok(Json(resp))
}

/// How the model has actually done against its own frozen predictions.
pub async fn accuracy(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::accuracy::AccuracyReport>, (StatusCode, String)> {
    let world = state.world.read().await.clone();
    let report = tokio::task::spawn_blocking(move || crate::accuracy::report(&world))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Accuracy report failed: {e}"),
            )
        })?;
    Ok(Json(report))
}

pub async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let live_fetched_at = state
        .live_data
        .read()
        .await
        .as_ref()
        .map(|l| l.fetched_at.clone());
    let (model, world_version) = {
        let world = state.world.read().await;
        let model = match &world.ensemble {
            Some(e) => serde_json::json!({
                "kind": "ensemble",
                "weights": { "elo": e.w_elo, "dixon_coles": e.w_dc, "pi_ratings": e.w_pi },
                "dc_fitted_at": e.dc.fitted_at,
                "pi_matches": e.pi.n_matches,
                "score_sampler": if e.w_dc > 0.0 { "dixon_coles_joint" } else { "independent_poisson" },
            }),
            None => serde_json::json!({ "kind": "elo", "score_sampler": "independent_poisson" }),
        };
        (model, state.world_version.load(Ordering::Relaxed))
    };
    let forecast_cached = state
        .sim_cache
        .get(world_version, &SimKey::default_forecast())
        .is_some();
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "live_fetched_at": live_fetched_at,
        "model": model,
        // The label forecast-log entries are filed under; it names the
        // default blend, so read it together with `model.weights`.
        "model_version": crate::model_version::current(),
        "world_version": world_version,
        "forecast_cached": forecast_cached,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scraper::ScrapedMatch;
    use serde_json::json;

    /// A part-played season of made-up results, so the tests behave the
    /// same whatever the shipped calendar holds.
    fn test_state(slots: usize, max_waiting: usize, timeout: Duration) -> Arc<AppState> {
        Arc::new(AppState::new(
            World::with_played(crate::sim::MID_SEASON),
            None,
            false,
            SimulationQueue::new(slots, max_waiting, timeout),
        ))
    }

    fn sim_request(body: serde_json::Value) -> Json<SimRequest> {
        Json(serde_json::from_value(body).expect("valid simulation request"))
    }

    async fn body_bytes(resp: Response) -> Bytes {
        axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("readable body")
    }

    fn hold_every_slot(state: &AppState) -> OwnedSemaphorePermit {
        state
            .simulation_queue
            .slots
            .clone()
            .try_acquire_owned()
            .expect("a free slot")
    }

    /// Every recorded result, as a scrape would report it.
    fn scrape_of(world: &World) -> Vec<ScrapedMatch> {
        world
            .fixtures
            .iter()
            .filter_map(|f| {
                let &(hs, as_) = world.played.get(&(f.home, f.away))?;
                Some(ScrapedMatch {
                    round: f.round,
                    home: world.teams[f.home].clone(),
                    home_score: hs,
                    away: world.teams[f.away].clone(),
                    away_score: as_,
                })
            })
            .collect()
    }

    #[tokio::test]
    async fn forecast_and_an_empty_simulate_request_share_one_cache_entry() {
        let state = test_state(1, 0, Duration::from_secs(5));
        let stub = Bytes::from_static(br#"{"stub":true}"#);
        let cached = stub.clone();
        state
            .sim_cache
            .get_or_compute(
                0,
                SimKey::default_forecast(),
                || async { Ok(()) },
                move |()| async move { Ok(cached) },
            )
            .await
            .unwrap();

        // Every slot is held, so anything but a hit on that entry is a 503.
        let _held = hold_every_slot(&state);
        let from_forecast = forecast(State(state.clone())).await.into_response();
        let from_simulate = run_sim(State(state.clone()), sim_request(json!({})))
            .await
            .into_response();
        for resp in [from_forecast, from_simulate] {
            assert_eq!(resp.status(), StatusCode::OK);
            assert_eq!(resp.headers()[header::CONTENT_TYPE], "application/json");
            assert_eq!(body_bytes(resp).await, stub);
        }
    }

    #[tokio::test]
    async fn a_full_queue_answers_503_with_retry_after() {
        let state = test_state(1, 0, Duration::from_secs(5));
        let _held = hold_every_slot(&state);
        let resp = run_sim(State(state.clone()), sim_request(json!({ "n_sims": 100 })))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            resp.headers()[header::RETRY_AFTER],
            BUSY_RETRY_AFTER_SECS.to_string()
        );
    }

    #[tokio::test]
    async fn a_queued_request_that_times_out_answers_503_with_retry_after() {
        let state = test_state(1, 1, Duration::from_millis(20));
        let _held = hold_every_slot(&state);
        let resp = run_sim(State(state.clone()), sim_request(json!({ "n_sims": 100 })))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(resp.headers().contains_key(header::RETRY_AFTER));
        assert_eq!(state.simulation_queue.waiting.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_queued_request_runs_once_a_slot_frees() {
        let state = test_state(1, 1, Duration::from_secs(10));
        let held = hold_every_slot(&state);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            drop(held);
        });
        let resp = run_sim(State(state.clone()), sim_request(json!({ "n_sims": 100 })))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn an_identical_request_joins_the_run_in_flight_without_a_queue_seat() {
        let state = test_state(1, 1, Duration::from_secs(10));
        let held = hold_every_slot(&state);
        let request = || sim_request(json!({ "n_sims": 100, "seed": 3 }));
        let first = tokio::spawn(run_sim(State(state.clone()), request()));
        while state.simulation_queue.waiting.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        // The only queue seat is taken: had the joiner queued too, it would
        // have been turned away.
        let second = tokio::spawn(run_sim(State(state.clone()), request()));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(state.simulation_queue.waiting.load(Ordering::SeqCst), 1);
        drop(held);

        let first = first.await.unwrap().into_response();
        let second = second.await.unwrap().into_response();
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(second.status(), StatusCode::OK);
        assert_eq!(body_bytes(first).await, body_bytes(second).await);
    }

    #[tokio::test]
    async fn a_queued_request_whose_client_leaves_gives_up_its_seat() {
        let state = test_state(1, 1, Duration::from_secs(10));
        let held = hold_every_slot(&state);
        let waiting = || state.simulation_queue.waiting.load(Ordering::SeqCst);
        let until = |n: usize| {
            tokio::time::timeout(Duration::from_secs(1), async move {
                while waiting() != n {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
        };
        let left = tokio::spawn(run_sim(
            State(state.clone()),
            sim_request(json!({ "n_sims": 100, "seed": 1 })),
        ));
        until(1).await.expect("the first request queues");
        left.abort();
        assert!(left.await.unwrap_err().is_cancelled());
        until(0)
            .await
            .expect("the abandoned run gives its seat back");

        // The only seat is free again, so the client's next request queues
        // instead of getting a 503, and gets the slot first.
        let next = tokio::spawn(run_sim(
            State(state.clone()),
            sim_request(json!({ "n_sims": 100, "seed": 2 })),
        ));
        until(1).await.expect("the next request queues");
        drop(held);
        let next = next.await.unwrap().into_response();
        assert_eq!(next.status(), StatusCode::OK);
        let abandoned = SimKey::new(100, 1, &HashMap::new(), &HashMap::new());
        assert!(
            state.sim_cache.get(0, &abandoned).is_none(),
            "the abandoned run never ran"
        );
    }

    #[tokio::test]
    async fn a_cached_request_in_any_order_needs_no_simulation_slot() {
        let state = test_state(1, 0, Duration::from_secs(5));
        let pins: Vec<serde_json::Value> = {
            let world = state.world.read().await;
            world
                .fixtures
                .iter()
                .filter(|f| !world.played.contains_key(&(f.home, f.away)))
                .take(3)
                .zip(["home", "draw", "away"])
                .map(|(f, outcome)| {
                    json!({
                        "home": world.teams[f.home],
                        "away": world.teams[f.away],
                        "outcome": outcome,
                    })
                })
                .collect()
        };
        assert_eq!(pins.len(), 3, "three unplayed fixtures to pin");
        let reversed: Vec<serde_json::Value> = pins.iter().rev().cloned().collect();
        let overrides = json!({ "Galatasaray": 1700.0, "Fenerbahçe": 1690.0 });

        let first = run_sim(
            State(state.clone()),
            sim_request(
                json!({ "n_sims": 100, "seed": 1, "what_if": pins, "elo_overrides": overrides }),
            ),
        )
        .await
        .into_response();
        assert_eq!(first.status(), StatusCode::OK);
        let first = body_bytes(first).await;

        let _held = hold_every_slot(&state);
        let again = run_sim(
            State(state.clone()),
            sim_request(json!({ "n_sims": 100, "seed": 1, "what_if": reversed, "elo_overrides": overrides })),
        )
        .await
        .into_response();
        assert_eq!(again.status(), StatusCode::OK, "a cache hit takes no slot");
        assert_eq!(body_bytes(again).await, first);

        let other = run_sim(
            State(state.clone()),
            sim_request(json!({ "n_sims": 100, "seed": 2 })),
        )
        .await
        .into_response();
        assert_eq!(other.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn a_fixture_pinned_twice_is_a_bad_request() {
        let state = test_state(1, 1, Duration::from_secs(5));
        let (home, away) = {
            let world = state.world.read().await;
            let f = world
                .fixtures
                .iter()
                .find(|f| !world.played.contains_key(&(f.home, f.away)))
                .expect("an unplayed fixture");
            (world.teams[f.home].clone(), world.teams[f.away].clone())
        };
        for second in ["away", "home"] {
            let pins = json!([
                { "home": home, "away": away, "outcome": "home" },
                { "home": home, "away": away, "outcome": second },
            ]);
            let resp = run_sim(
                State(state.clone()),
                sim_request(json!({ "n_sims": 100, "seed": 1, "what_if": pins })),
            )
            .await
            .into_response();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{second}");
            assert_eq!(
                body_bytes(resp).await,
                format!("{home} v {away} is pinned twice")
            );
        }
    }

    #[tokio::test]
    async fn only_a_real_world_change_bumps_the_version() {
        let state = test_state(1, 0, Duration::from_secs(5));
        let (mut played, next) = {
            let world = state.world.read().await;
            let next = world
                .fixtures
                .iter()
                .find(|f| !world.played.contains_key(&(f.home, f.away)))
                .copied()
                .expect("an unplayed fixture");
            let next = ScrapedMatch {
                round: next.round,
                home: world.teams[next.home].clone(),
                home_score: 2,
                away: world.teams[next.away].clone(),
                away_score: 0,
            };
            (scrape_of(&world), next)
        };
        let live = |played: &[ScrapedMatch]| LiveData {
            played_matches: played.to_vec(),
            fetched_at: "2026-10-18T00:00:00Z".to_string(),
        };

        let first = run_sim(State(state.clone()), sim_request(json!({ "n_sims": 100 })))
            .await
            .into_response();
        assert_eq!(first.status(), StatusCode::OK);
        let key = SimKey::new(100, DEFAULT_SEED, &HashMap::new(), &HashMap::new());
        assert!(state.sim_cache.get(0, &key).is_some());

        apply_live(&state, &live(&played)).await.unwrap();
        assert_eq!(current_version(&state).await, 0, "nothing new, no bump");
        assert!(state.sim_cache.get(0, &key).is_some());

        played.push(next);
        assert_eq!(apply_live(&state, &live(&played)).await.unwrap(), 1);
        assert_eq!(current_version(&state).await, 1);
        assert!(
            state.sim_cache.get(1, &key).is_none(),
            "the old answer is stale"
        );

        apply_live(&state, &live(&played)).await.unwrap();
        assert_eq!(current_version(&state).await, 1, "re-sent results, no bump");
    }

    #[tokio::test]
    async fn scenario_is_404_unless_enabled() {
        let state = test_state(1, 0, Duration::from_secs(5));
        let req: ScenarioRequest =
            serde_json::from_value(json!({ "prompt": "Galatasaray's striker is injured" }))
                .unwrap();
        let resp = scenario(State(state), Ok(Json(req))).await.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            body_bytes(resp).await,
            "Scenario analysis is not enabled on this server"
        );
    }

    #[tokio::test]
    async fn health_reports_world_version_and_forecast_cache() {
        let state = test_state(1, 0, Duration::from_secs(5));
        let Json(health) = health(State(state)).await;
        assert_eq!(health["status"], "ok");
        assert_eq!(health["model_version"], crate::model_version::current());
        assert_eq!(health["world_version"], 0);
        assert_eq!(health["forecast_cached"], false);
    }
}
