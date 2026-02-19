use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

mod config;
mod db;
mod error;
mod handlers;
mod metrics;
mod models;
mod seed;
mod sets;

use crate::config::Config;
use crate::metrics::MetricsStore;
use crate::sets::SetManager;

/// Shared application state — cheap to clone (all heap behind Arc).
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub sets: Arc<RwLock<SetManager>>,
    pub metrics: Arc<RwLock<MetricsStore>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env if present (ignored in production where env vars are injected)
    dotenv::dotenv().ok();

    // Structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,inventory_service=debug".parse().unwrap()),
        )
        .with_target(false)
        .compact()
        .init();

    let config = Config::from_env()?;

    info!("╔══════════════════════════════════════╗");
    info!("║  Inventory Service  — Rust + Axum    ║");
    info!("║  HashSet · LinkedHashSet · BTreeSet  ║");
    info!("╚══════════════════════════════════════╝");

    // DB pool (up to 20 connections so stress tests can run concurrently)
    info!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await?;
    info!("Database connection pool established.");

    // Run pending migrations
    info!("Running migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    info!("Migrations complete.");

    let state = AppState {
        db: pool,
        sets: Arc::new(RwLock::new(SetManager::new())),
        metrics: Arc::new(RwLock::new(MetricsStore::new())),
    };

    let app = build_router(state);

    let addr = format!("{}:{}", config.host, config.port);
    info!("Listening on http://{}", addr);
    info!("Quick-start: POST http://{}/api/seed?count=5000  →  then POST http://{}/api/benchmark/run", addr, addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        // ── Health ──────────────────────────────────────────────────────────
        .route("/health", get(handlers::health))

        // ── Products CRUD ───────────────────────────────────────────────────
        .route(
            "/api/products",
            get(handlers::products::list_products).post(handlers::products::create_product),
        )
        .route(
            "/api/products/:id",
            get(handlers::products::get_product)
                .put(handlers::products::update_product)
                .delete(handlers::products::delete_product),
        )

        // ── Product Devolutions ─────────────────────────────────────────────
        .route(
            "/api/devolutions",
            get(handlers::devolutions::list_devolutions)
                .post(handlers::devolutions::create_devolution),
        )
        .route(
            "/api/devolutions/:id",
            get(handlers::devolutions::get_devolution),
        )

        // ── Seed ────────────────────────────────────────────────────────────
        .route("/api/seed", post(handlers::benchmark::seed_data))

        // ── Benchmark ───────────────────────────────────────────────────────
        .route("/api/benchmark/run", post(handlers::benchmark::run_benchmark))
        .route("/api/benchmark/report", get(handlers::benchmark::get_report))
        .route(
            "/api/benchmark/sets/status",
            get(handlers::benchmark::sets_status),
        )
        .route(
            "/api/benchmark/export/csv",
            get(handlers::benchmark::export_csv),
        )
        .route(
            "/api/benchmark/export/json",
            get(handlers::benchmark::export_json),
        )

        // ── Stress test ─────────────────────────────────────────────────────
        .route("/api/stress-test", post(handlers::stress::run_stress_test))

        // ── Middleware ──────────────────────────────────────────────────────
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
