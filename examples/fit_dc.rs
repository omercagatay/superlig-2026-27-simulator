// Run a Dixon-Coles fit on the real Süper Lig results data and print a sample.
use chrono::NaiveDate;
use wc2026_sim::dixoncoles;
use wc2026_sim::history;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("wc2026_sim=info")
        .try_init()
        .ok();
    let idx = history::TeamIndex::league();
    let as_of = NaiveDate::from_ymd_opt(2026, 8, 16).unwrap();
    let half_life_days = 1460.0; // ~4 year half-life
    let hist = history::load_history_with_cutoff(history::CUTOFF_YEAR);
    let fits = history::prepare_fit_matches(&hist, &idx, half_life_days, as_of);
    println!(
        "Fitting on {} matches (post-{}, half-life {} days)",
        fits.len(),
        history::CUTOFF_YEAR,
        half_life_days
    );

    let t0 = std::time::Instant::now();
    let params = dixoncoles::fit(&fits, &idx, half_life_days, 300).expect("fit");
    let dt = t0.elapsed();
    println!(
        "mu={:.4} gamma={:.4} rho={:.5}  (fit took {:.2?})",
        params.mu, params.gamma, params.rho, dt
    );
    let base_lambda = params.mu.exp();
    println!(
        "Baseline λ for an average {} fixture: {:.3}",
        history::OTHER_TEAM_NAME,
        base_lambda
    );
    println!("Home boost factor: {:.3}", params.gamma.exp());
    println!(
        "rho: {:.5}  ({})",
        params.rho,
        if params.rho < 0.0 {
            "negative, as expected"
        } else {
            "POSITIVE — unusual"
        }
    );

    // Print attack/defense for a sample of clubs by index order in data::elo().
    let names = idx.league_names.clone();
    let mut ranked: Vec<(usize, String, f64, f64)> = (0..names.len())
        .map(|i| (i, names[i].clone(), params.attack(i), params.defense(i)))
        .collect();
    ranked.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    println!("\nTop 10 attack (alpha):");
    for (_i, n, alpha, beta) in ranked.iter().take(10) {
        println!(
            "  {:<28} α={:+.4} β={:+.4} λ_home_vs_bucket={:.2}",
            n,
            alpha,
            beta,
            (params.mu + params.gamma + alpha).exp()
        );
    }
    println!("Bottom 5 attack:");
    for (_i, n, alpha, beta) in ranked.iter().rev().take(5) {
        println!(
            "  {:<28} α={:+.4} β={:+.4} λ_home_vs_bucket={:.2}",
            n,
            alpha,
            beta,
            (params.mu + params.gamma + alpha).exp()
        );
    }

    // A sample fixture: Galatasaray at home to Fenerbahçe.
    let gs = idx.name_to_idx["Galatasaray"];
    let fb = idx.name_to_idx["Fenerbahçe"];
    let (lh, la) = params.lam(gs, fb, false);
    let (w, d, l) = dixoncoles::match_probs(lh, la, params.rho);
    println!(
        "\nGalatasaray v Fenerbahçe (GS at home): λ_GS={:.2} λ_FB={:.2}  \
         P(GS win)={:.3}  P(draw)={:.3}  P(FB win)={:.3}",
        lh, la, w, d, l
    );

    let out = std::path::Path::new("data/dc_params.json");
    params.save_json(out).expect("save dc_params.json");
    println!("Saved fitted parameters to {}", out.display());
}
