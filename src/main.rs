use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, services::ServeDir};
use tracing_subscriber::EnvFilter;

use superlig_sim::{
    handlers::{self, AppState},
    rate_limit::RateLimitLayer,
    sim::World,
};
use tokio::sync::{RwLock, Semaphore};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("superlig_sim=info".parse()?))
        .init();

    let mut world = World::new();

    // Strength-model ensemble: "elo,dc,pi" weights, e.g. "0.5,0.3,0.2".
    // "1,0,0" (or a failed build) falls back to pure Elo.
    let (w_elo, w_dc, w_pi) = superlig_sim::ensemble_weights();
    if w_dc + w_pi > 0.0 {
        match superlig_sim::sim::Ensemble::from_embedded_data(w_elo, w_dc, w_pi) {
            Ok(ens) => {
                tracing::info!(
                    "Model ensemble active: elo={w_elo} dc={w_dc} (fitted {}) pi={w_pi} ({} matches)",
                    ens.dc.fitted_at,
                    ens.pi.n_matches
                );
                world.ensemble = Some(ens);
            }
            Err(e) => tracing::warn!("Ensemble unavailable, using pure Elo: {e}"),
        }
    } else {
        tracing::info!("Ensemble weights disable DC/pi components; using pure Elo");
    }

    let kimi_api_key = std::env::var("KIMI_API_KEY").ok();
    if kimi_api_key.is_some() {
        tracing::info!("Kimi scenario analysis enabled");
    } else {
        tracing::warn!("KIMI_API_KEY not set — scenario endpoint will return an error");
    }

    let simulation_capacity = std::env::var("MAX_CONCURRENT_SIMULATIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(1);
    tracing::info!("Simulation concurrency limit: {simulation_capacity}");

    let state = Arc::new(AppState {
        world: Arc::new(RwLock::new(world)),
        kimi_api_key,
        live_data: Arc::new(RwLock::new(None)),
        market: Arc::new(RwLock::new(None)),
        simulation_slots: Arc::new(Semaphore::new(simulation_capacity)),
    });

    // Keep the simulation current with the real tournament: refresh live
    // data immediately on startup, then on an interval. LIVE_REFRESH_MINUTES=0
    // disables the background task (manual /api/refresh still works).
    let refresh_minutes: u64 = std::env::var("LIVE_REFRESH_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    if refresh_minutes > 0 {
        let bg_state = state.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(refresh_minutes * 60));
            loop {
                interval.tick().await;
                match handlers::perform_live_refresh(&bg_state).await {
                    Ok(live) => tracing::info!(
                        "Background live refresh ok: {} played fixtures",
                        live.played_matches.len()
                    ),
                    Err(e) => tracing::warn!("Background live refresh failed: {e:#}"),
                }
            }
        });
        tracing::info!("Background live refresh enabled (every {refresh_minutes} min)");
    } else {
        tracing::info!("Background live refresh disabled (LIVE_REFRESH_MINUTES=0)");
    }

    // Only trust X-Forwarded-For for rate limiting when a sanitizing reverse
    // proxy (e.g. Railway's edge) fronts the server; a spoofed header would
    // otherwise give every request a fresh rate-limit bucket.
    let trust_proxy = std::env::var("TRUST_PROXY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if trust_proxy {
        tracing::info!("TRUST_PROXY on: rate limiting keys on X-Forwarded-For");
    } else {
        tracing::info!("TRUST_PROXY off: rate limiting keys on socket peer address");
    }

    let app = Router::new()
        .route("/api/health", get(handlers::health))
        .route("/api/live", get(handlers::get_live_data))
        .merge(
            Router::new()
                .route("/api/simulate", post(handlers::run_sim))
                .route_layer(RateLimitLayer::new(30, 60, trust_proxy)),
        )
        .merge(
            Router::new()
                .route("/api/upcoming", get(handlers::upcoming))
                .route("/api/matches", get(handlers::matches))
                .route("/api/accuracy", get(handlers::accuracy))
                .route_layer(RateLimitLayer::new(30, 60, trust_proxy)),
        )
        .merge(
            Router::new()
                .route("/api/scenario", post(handlers::scenario))
                .route_layer(RateLimitLayer::new(10, 60, trust_proxy)),
        )
        .merge(
            Router::new()
                .route("/api/refresh", post(handlers::refresh_live_data))
                .route_layer(RateLimitLayer::new(5, 60, trust_proxy)),
        )
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state)
        .fallback_service(ServeDir::new("frontend/dist"));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
