pub mod accuracy;
pub mod data;
pub mod dixoncoles;
pub mod handlers;
pub mod history;
pub mod league;
pub mod llm;
pub mod market;
pub mod models;
pub mod odds;
pub mod piratings;
pub mod rate_limit;
pub mod scraper;
pub mod sim;
pub mod validation;

/// Strength-model blend weights from `ENSEMBLE_WEIGHTS` ("elo,dc,pi"),
/// falling back to the backtested 0.5/0.3/0.2 default. Shared so the server
/// and the forecast logger cannot silently disagree about the active model.
pub fn ensemble_weights() -> (f64, f64, f64) {
    let raw = std::env::var("ENSEMBLE_WEIGHTS").unwrap_or_else(|_| "0.5,0.3,0.2".to_string());
    let parsed: Vec<f64> = raw
        .split(',')
        .filter_map(|w| w.trim().parse().ok())
        .collect();
    match parsed.as_slice() {
        [e, d, p] if *e >= 0.0 && *d >= 0.0 && *p >= 0.0 && e + d + p > 0.0 => (*e, *d, *p),
        _ => {
            tracing::warn!("Invalid ENSEMBLE_WEIGHTS {raw:?}, using 0.5,0.3,0.2");
            (0.5, 0.3, 0.2)
        }
    }
}
