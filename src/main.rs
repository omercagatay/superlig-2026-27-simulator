use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    http::{header, HeaderName, HeaderValue},
    routing::{get, post},
    Router,
};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer, limit::RequestBodyLimitLayer, services::ServeDir,
    set_header::SetResponseHeaderLayer,
};
use tracing_subscriber::EnvFilter;

use superlig_sim::{
    handlers::{self, AppState, SimulationQueue},
    rate_limit::RateLimitLayer,
    sim::World,
};

/// `1` or `true` (any case) turns a boolean env var on; anything else, or
/// unset, leaves it off.
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// What the built SPA loads and nothing more: its own scripts, stylesheet and
/// API, and the inline SVG favicon, a `data:` image. The pre-paint theme
/// script is a file (`frontend/public/theme.js`) so scripts need no inline
/// allowance, and React sets `style` props through the CSSOM, which
/// `style-src` does not police, so styles need none either.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self'; img-src 'self' data:; connect-src 'self'; \
     frame-ancestors 'none'; base-uri 'self'; object-src 'none'";

/// Headers every response carries, the SPA's files and errors included.
fn security_headers() -> [(HeaderName, HeaderValue); 4] {
    [
        (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        (
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ),
        (header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")),
        (
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        ),
    ]
}

/// The API, each rate-limited group with its own per-IP allowance, and the
/// built SPA for everything else, all compressed for clients that accept
/// gzip or brotli and all carrying `security_headers`.
fn router(state: Arc<AppState>, trust_proxy: bool) -> Router {
    let [nosniff, referrer, frame, csp] =
        security_headers().map(|(name, value)| SetResponseHeaderLayer::overriding(name, value));
    // No CORS layer: the SPA is served from this origin, and the Vite dev
    // server proxies /api, so no cross-origin caller needs access.
    Router::new()
        .route("/api/health", get(handlers::health))
        .route("/api/live", get(handlers::get_live_data))
        .merge(
            Router::new()
                .route("/api/simulate", post(handlers::run_sim))
                .route_layer(RateLimitLayer::new(30, 60, trust_proxy)),
        )
        // Every page load asks for the default forecast, a cached byte copy
        // (or one shared run while it is computed), so it has its own, larger
        // allowance instead of eating into the calendar's.
        .merge(
            Router::new()
                .route("/api/forecast", get(handlers::forecast))
                .route_layer(RateLimitLayer::new(60, 60, trust_proxy)),
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
        .with_state(state)
        .fallback_service(ServeDir::new("frontend/dist"))
        // Added after the fallback, so the SPA's files get them too.
        .layer(
            ServiceBuilder::new()
                .layer(nosniff)
                .layer(referrer)
                .layer(frame)
                .layer(csp)
                .layer(CompressionLayer::new()),
        )
}

/// How long requests still open when the shutdown signal arrives may take to
/// finish before the process exits without them.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Resolves on Ctrl-C or SIGTERM, which Railway and `docker stop` send. As
/// PID 1 in the container the process has no default SIGTERM action, so
/// without a handler a stop waits out its timeout and ends in SIGKILL.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::warn!("Cannot listen for Ctrl-C: {e}");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sigterm) => {
                sigterm.recv().await;
            }
            Err(e) => {
                tracing::warn!("Cannot listen for SIGTERM: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => tracing::info!("Ctrl-C received, shutting down"),
        () = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(serve());
    // Dropping the runtime would wait for any simulation still running on the
    // blocking pool (the startup warm-up, or a run of up to 200,000 seasons),
    // grace period or not. Its answer could only go to a cache about to
    // vanish, so exit without it.
    runtime.shutdown_background();
    result
}

/// Builds the world and the state, then serves until a shutdown signal.
async fn serve() -> anyhow::Result<()> {
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

    // Each scenario call spends money on the LLM API, so the endpoint is off
    // unless explicitly enabled.
    let scenario_enabled = env_flag("SCENARIO_ENABLED");
    let kimi_api_key = std::env::var("KIMI_API_KEY").ok();
    if !scenario_enabled {
        tracing::info!(
            "Scenario analysis disabled (SCENARIO_ENABLED is not 1/true); /api/scenario is 404"
        );
    } else if kimi_api_key.is_some() {
        tracing::info!("Kimi scenario analysis enabled");
    } else {
        tracing::warn!("SCENARIO_ENABLED is on but KIMI_API_KEY is not set — scenario endpoint will return an error");
    }

    let simulation_capacity = std::env::var("MAX_CONCURRENT_SIMULATIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(1);
    let queue_length = std::env::var("MAX_SIMULATION_QUEUE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8);
    let queue_timeout_secs = std::env::var("SIM_QUEUE_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(20);
    tracing::info!(
        "Simulation concurrency limit: {simulation_capacity}, queue: {queue_length} waiting for up to {queue_timeout_secs}s"
    );

    let state = Arc::new(AppState::new(
        world,
        kimi_api_key,
        scenario_enabled,
        SimulationQueue::new(
            simulation_capacity,
            queue_length,
            Duration::from_secs(queue_timeout_secs),
        ),
    ));

    // Keep the simulation current with the real tournament: refresh live
    // data immediately on startup, then on an interval. LIVE_REFRESH_MINUTES=0
    // disables the background task (manual /api/refresh still works). Each
    // refresh attempt ends by warming the default forecast for the current
    // world; without the task it is warmed straight away.
    let refresh_minutes: u64 = std::env::var("LIVE_REFRESH_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    if refresh_minutes > 0 {
        let bg_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(refresh_minutes * 60));
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
        handlers::spawn_forecast_warmup(state.clone());
    }

    // Only trust X-Forwarded-For for rate limiting when a sanitizing reverse
    // proxy (e.g. Railway's edge) fronts the server; a spoofed header would
    // otherwise give every request a fresh rate-limit bucket.
    let trust_proxy = env_flag("TRUST_PROXY");
    if trust_proxy {
        tracing::info!("TRUST_PROXY on: rate limiting keys on X-Forwarded-For");
    } else {
        tracing::info!("TRUST_PROXY off: rate limiting keys on socket peer address");
    }

    let app = router(state, trust_proxy);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // On the signal the server stops accepting connections and lets open
    // requests finish, for up to SHUTDOWN_GRACE.
    let (stopping, stopped) = tokio::sync::oneshot::channel();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        let _ = stopping.send(());
    });
    let grace = async {
        match stopped.await {
            Ok(()) => tokio::time::sleep(SHUTDOWN_GRACE).await,
            Err(_) => std::future::pending().await,
        }
    };
    tokio::select! {
        result = server => result?,
        () = grace => tracing::warn!(
            "Requests still open {} s after the signal; exiting without them",
            SHUTDOWN_GRACE.as_secs()
        ),
    }
    tracing::info!("Server stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, Bytes},
        extract::ConnectInfo,
        http::{Request, StatusCode},
    };
    use std::io::Read;
    use superlig_sim::forecast_cache::SimKey;
    use tower::Service;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new(
            World::new(),
            None,
            false,
            SimulationQueue::new(1, 0, Duration::from_secs(1)),
        ))
    }

    /// A GET from one peer, offering `encoding` if given.
    async fn fetch(
        app: &mut Router,
        path: &str,
        encoding: Option<&str>,
    ) -> axum::response::Response {
        let mut req = Request::builder().uri(path);
        if let Some(encoding) = encoding {
            req = req.header(header::ACCEPT_ENCODING, encoding);
        }
        let mut req = req.body(Body::empty()).unwrap();
        let peer: SocketAddr = "192.0.2.1:5000".parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));
        app.call(req).await.unwrap()
    }

    /// API answers and the SPA fallback carry them, its 404s included (the
    /// last path is one whether or not frontend/dist is built).
    #[tokio::test]
    async fn every_response_carries_the_security_headers() {
        let mut app = router(test_state(), false);
        for path in ["/api/health", "/api/matches", "/no-such-file.js"] {
            let resp = fetch(&mut app, path, None).await;
            for (name, value) in security_headers() {
                assert_eq!(resp.headers().get(&name), Some(&value), "{name} on {path}");
            }
        }
    }

    /// The calendar, the largest API answer, goes out compressed to a client
    /// that accepts it and inflates to the bytes an identity client gets.
    #[tokio::test]
    async fn large_responses_are_compressed() {
        let mut app = router(test_state(), false);
        let body = |resp: axum::response::Response| async move {
            assert_eq!(resp.status(), StatusCode::OK);
            axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap()
        };

        let resp = fetch(&mut app, "/api/matches", None).await;
        assert!(resp.headers().get(header::CONTENT_ENCODING).is_none());
        let plain = body(resp).await;

        let resp = fetch(&mut app, "/api/matches", Some("gzip")).await;
        assert_eq!(resp.headers()[header::CONTENT_ENCODING], "gzip");
        let gzip = body(resp).await;
        let mut inflated = Vec::new();
        flate2::read::GzDecoder::new(&gzip[..])
            .read_to_end(&mut inflated)
            .unwrap();
        assert_eq!(inflated, plain);
        assert!(
            gzip.len() * 2 < plain.len(),
            "gzip {} of {} bytes",
            gzip.len(),
            plain.len()
        );

        let resp = fetch(&mut app, "/api/matches", Some("br;q=1, gzip;q=0.5")).await;
        assert_eq!(resp.headers()[header::CONTENT_ENCODING], "br");
        let br = body(resp).await;
        assert!(
            br.len() * 2 < plain.len(),
            "br {} of {} bytes",
            br.len(),
            plain.len()
        );
    }

    /// One visitor's page loads must not use up the calendar's allowance.
    #[tokio::test]
    async fn the_forecast_has_its_own_allowance() {
        let state = test_state();
        // A cached default forecast, so no request here runs a simulation.
        state
            .sim_cache
            .get_or_compute(
                0,
                SimKey::default_forecast(),
                || async { Ok(()) },
                |()| async { Ok(Bytes::from_static(b"{}")) },
            )
            .await
            .unwrap();
        let mut app = router(state, false);
        let peer: SocketAddr = "192.0.2.1:5000".parse().unwrap();
        let mut send = |method: &str, path: &str| {
            let body = if method == "POST" { "{}" } else { "" };
            let mut req = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(peer));
            let resp = app.call(req);
            async move { resp.await.unwrap().status() }
        };

        for _ in 0..34 {
            assert_eq!(send("GET", "/api/forecast").await, StatusCode::OK);
        }
        // The calendar's allowance is untouched, and its three routes share it.
        let calendar = ["/api/upcoming", "/api/matches", "/api/accuracy"];
        for path in calendar.iter().cycle().take(30) {
            assert_eq!(send("GET", path).await, StatusCode::OK, "{path}");
        }
        for path in calendar {
            assert_eq!(send("GET", path).await, StatusCode::TOO_MANY_REQUESTS);
        }
        for _ in 34..60 {
            assert_eq!(send("GET", "/api/forecast").await, StatusCode::OK);
        }
        assert_eq!(
            send("GET", "/api/forecast").await,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            send("POST", "/api/simulate").await,
            StatusCode::OK,
            "simulate has an allowance of its own"
        );
    }
}
