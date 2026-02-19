use std::time::Instant;

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use serde::Deserialize;
use tracing::info;

use crate::{db, error::AppResult, seed, AppState};

#[derive(Debug, Deserialize)]
pub struct SeedParams {
    /// Number of products to seed (default: 1000, max: 50 000)
    pub count: Option<usize>,
}

// ── POST /api/seed ────────────────────────────────────────────────────────────

pub async fn seed_data(
    State(state): State<AppState>,
    Query(params): Query<SeedParams>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let count = params.count.unwrap_or(1_000).min(50_000);

    let start = Instant::now();
    let products = seed::seed_products(&state.db, count).await?;
    let seed_elapsed = start.elapsed();

    // Sync sets
    let sync_start = Instant::now();
    state.sets.write().await.sync_from_db(&products);
    let sync_elapsed = sync_start.elapsed();

    let total_in_db = db::count_products(&state.db).await?;

    info!(
        seeded = products.len(),
        total_in_db,
        seed_ms = seed_elapsed.as_millis(),
        "Seeding complete"
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "seeded": products.len(),
            "total_in_db": total_in_db,
            "seed_time_ms": seed_elapsed.as_secs_f64() * 1000.0,
            "set_sync_time_ms": sync_elapsed.as_secs_f64() * 1000.0,
        })),
    ))
}

// ── POST /api/benchmark/run ───────────────────────────────────────────────────

pub async fn run_benchmark(
    State(state): State<AppState>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    info!("Starting benchmark run...");

    // Load all products from DB
    let db_start = Instant::now();
    let products = db::fetch_all_products_unbounded(&state.db).await?;
    let db_elapsed = db_start.elapsed();

    if products.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "No products in database. POST /api/seed?count=5000 first.",
                "product_count": 0,
            })),
        ));
    }

    info!(count = products.len(), "Loaded products for benchmark");

    let bench_start = Instant::now();
    let report = state.sets.write().await.run_benchmark(products);
    let bench_elapsed = bench_start.elapsed();

    // Persist to metrics store (appended — history is preserved across runs)
    {
        let mut metrics = state.metrics.write().await;
        for result in &report.results {
            metrics.record_raw(
                "insert_all",
                &result.set_type,
                result.insert_all.duration_ns,
                result.product_count,
            );
            metrics.record_raw(
                "lookup_hit",
                &result.set_type,
                result.lookup_hit.duration_ns,
                1,
            );
            metrics.record_raw(
                "lookup_miss",
                &result.set_type,
                result.lookup_miss.duration_ns,
                1,
            );
            metrics.record_raw(
                "iterate_all",
                &result.set_type,
                result.iterate_all.duration_ns,
                result.product_count,
            );
            metrics.record_raw(
                "remove_half",
                &result.set_type,
                result.remove_half.duration_ns,
                result.product_count / 2,
            );
        }
    }

    // Build ASCII summary table
    let ascii = render_benchmark_ascii_table(&report);

    info!(
        product_count = report.product_count,
        bench_ms = bench_elapsed.as_millis(),
        winner_insert = %report.winner_insert,
        winner_lookup = %report.winner_lookup,
        winner_iterate = %report.winner_iterate,
        "Benchmark complete"
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "report": report,
            "db_load_time_ms": db_elapsed.as_secs_f64() * 1000.0,
            "benchmark_time_ms": bench_elapsed.as_secs_f64() * 1000.0,
            "ascii_table": ascii,
        })),
    ))
}

// ── GET /api/benchmark/report ─────────────────────────────────────────────────

pub async fn get_report(
    State(state): State<AppState>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let sets = state.sets.read().await;
    let (hs, lh, bt) = sets.sizes();

    match &sets.last_report {
        Some(report) => {
            let ascii = render_benchmark_ascii_table(report);
            Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "report": report,
                    "current_set_sizes": {
                        "hash_set": hs,
                        "index_set": lh,
                        "btree_set": bt,
                    },
                    "ascii_table": ascii,
                })),
            ))
        }
        None => Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "No benchmark has been run yet. POST /api/benchmark/run first.",
                "current_set_sizes": {
                    "hash_set": hs,
                    "index_set": lh,
                    "btree_set": bt,
                },
            })),
        )),
    }
}

// ── GET /api/benchmark/sets/status ───────────────────────────────────────────

pub async fn sets_status(
    State(state): State<AppState>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let sets = state.sets.read().await;
    let (hs, lh, bt) = sets.sizes();

    // Sample first 5 elements from each set
    let hash_sample: Vec<_> = sets
        .hash_set
        .iter()
        .take(5)
        .map(|p| serde_json::json!({"id": p.id, "name": p.name}))
        .collect();

    let linked_sample: Vec<_> = sets
        .index_set
        .iter()
        .take(5)
        .map(|p| serde_json::json!({"id": p.id, "name": p.name}))
        .collect();

    let btree_sample: Vec<_> = sets
        .btree_set
        .iter()
        .take(5)
        .map(|p| serde_json::json!({"id": p.id, "name": p.name}))
        .collect();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "sizes": {
                "hash_set": hs,
                "index_set": lh,
                "btree_set": bt,
            },
            "sample_first_5": {
                "hash_set": {
                    "note": "Arbitrary order (hash-based, not predictable)",
                    "items": hash_sample,
                },
                "index_set": {
                    "note": "Insertion order preserved (FIFO) — IndexSet / LinkedHashSet equivalent",
                    "items": linked_sample,
                },
                "btree_set": {
                    "note": "Alphabetically sorted by product name",
                    "items": btree_sample,
                },
            },
        })),
    ))
}

// ── GET /api/benchmark/export/csv ────────────────────────────────────────────

pub async fn export_csv(State(state): State<AppState>) -> Result<Response, crate::error::AppError> {
    let metrics = state.metrics.read().await;
    let csv = metrics.to_csv().map_err(anyhow::Error::from)?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"benchmark_metrics.csv\"",
        )
        .body(axum::body::Body::from(csv))
        .unwrap())
}

// ── GET /api/benchmark/export/json ───────────────────────────────────────────

pub async fn export_json(
    State(state): State<AppState>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let metrics = state.metrics.read().await;
    let entries = &metrics.entries;
    let aggregated = metrics.aggregated();
    let ascii = metrics.ascii_table();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "entry_count": entries.len(),
            "entries": entries,
            "aggregated": aggregated,
            "ascii_table": ascii,
        })),
    ))
}

// ── DELETE /api/reset ─────────────────────────────────────────────────────────

pub async fn reset_all(
    State(state): State<AppState>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    // 1. Wipe DB (devolutions cascade automatically)
    let rows_deleted = db::delete_all_products(&state.db).await?;

    // 2. Clear in-memory sets + last benchmark report
    state.sets.write().await.reset();

    // 3. Clear accumulated metrics
    state.metrics.write().await.clear();

    info!(rows_deleted, "Full reset: DB, sets, and metrics cleared");

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "deleted_products": rows_deleted,
            "sets_cleared": true,
            "metrics_cleared": true,
        })),
    ))
}

// ── ASCII table renderer ──────────────────────────────────────────────────────

fn render_benchmark_ascii_table(report: &crate::sets::BenchmarkReport) -> String {
    let divider = "─".repeat(110);

    let mut out = String::new();
    out.push_str(&format!("\n┌{}┐\n", divider));
    out.push_str(&format!(
        "│  SET PERFORMANCE BENCHMARK  —  {} products  —  {}  │\n",
        report.product_count, report.run_at
    ));
    out.push_str(&format!("├{}┤\n", divider));
    out.push_str(&format!(
        "│  {:<20} {:<14} {:<14} {:<14} {:<14} {:<14} {:<18}│\n",
        "Set Type", "Insert (ms)", "Lookup✓ (µs)", "Lookup✗ (µs)", "Iterate (ms)", "Remove½ (ms)", "Order"
    ));
    out.push_str(&format!("├{}┤\n", divider));

    for row in &report.summary_table {
        out.push_str(&format!(
            "│  {:<20} {:<14.3} {:<14.3} {:<14.3} {:<14.3} {:<14.3} {:<18}│\n",
            row.set_type,
            row.insert_ms,
            row.lookup_hit_us,
            row.lookup_miss_us,
            row.iterate_ms,
            row.remove_ms,
            &row.order[..row.order.len().min(17)],
        ));
    }

    out.push_str(&format!("├{}┤\n", divider));
    out.push_str(&format!(
        "│  Fastest Insert : {:<20}  Fastest Lookup : {:<20}  Fastest Iterate : {:<12}│\n",
        report.winner_insert, report.winner_lookup, report.winner_iterate
    ));
    out.push_str(&format!("└{}┘\n", divider));

    for r in &report.results {
        out.push_str(&format!(
            "\n  [{}]  Order sample (first 10 names):\n",
            r.set_type
        ));
        for (i, name) in r.iteration_order_sample.iter().enumerate() {
            out.push_str(&format!("    {:>2}. {}\n", i + 1, name));
        }
        out.push_str(&format!("    Order type: {}\n", r.order_type));
    }

    out
}
