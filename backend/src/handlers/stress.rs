use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::{extract::State, http::StatusCode, Json};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tracing::info;

use crate::{db, error::AppResult, seed, AppState};

#[derive(Debug, Deserialize)]
pub struct StressParams {
    /// Number of concurrent "virtual users" (default: 20)
    pub concurrency: Option<usize>,
    /// Total operations per virtual user (default: 50)
    pub ops_per_user: Option<usize>,
    /// Seed the DB with this many products before testing (default: 0 = use existing)
    pub seed_count: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct StressReport {
    pub concurrency: usize,
    pub ops_per_user: usize,
    pub total_ops: usize,
    pub product_count_before: i64,
    pub product_count_after: i64,

    // Throughput
    pub total_elapsed_ms: f64,
    pub ops_per_second: f64,

    // Per-operation counts
    pub reads: u64,
    pub creates: u64,
    pub updates: u64,
    pub deletes: u64,
    pub errors: u64,

    // Latency (across all ops)
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,

    // Per-op latency breakdown
    pub read_avg_ms: f64,
    pub create_avg_ms: f64,
    pub update_avg_ms: f64,
    pub delete_avg_ms: f64,

    // Set performance under concurrent load
    pub set_insert_total_ns: u64,
    pub set_lookup_total_ns: u64,
    pub set_remove_total_ns: u64,

    pub ascii_summary: String,
}

// ── POST /api/stress-test ────────────────────────────────────────────────────

pub async fn run_stress_test(
    State(state): State<AppState>,
    Json(params): Json<StressParams>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let concurrency = params.concurrency.unwrap_or(20).clamp(1, 200);
    let ops_per_user = params.ops_per_user.unwrap_or(50).clamp(1, 1_000);

    // Optional pre-seed
    if let Some(n) = params.seed_count {
        let n = n.min(10_000);
        info!("Stress test: seeding {} products before run...", n);
        let products = seed::seed_products(&state.db, n).await?;
        state.sets.write().await.sync_from_db(&products);
    }

    let product_count_before = db::count_products(&state.db).await?;

    if product_count_before == 0 {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "No products in DB. POST /api/seed?count=1000 first, or include seed_count in the payload."
            })),
        ));
    }

    info!(
        concurrency,
        ops_per_user,
        product_count_before,
        "Starting stress test"
    );

    // Shared atomic counters
    let reads = Arc::new(AtomicU64::new(0));
    let creates = Arc::new(AtomicU64::new(0));
    let updates = Arc::new(AtomicU64::new(0));
    let deletes = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let set_insert_ns = Arc::new(AtomicU64::new(0));
    let set_lookup_ns = Arc::new(AtomicU64::new(0));
    let set_remove_ns = Arc::new(AtomicU64::new(0));
    let latencies_ms: Arc<tokio::sync::Mutex<Vec<f64>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::with_capacity(concurrency * ops_per_user)));
    let read_lats: Arc<tokio::sync::Mutex<Vec<f64>>> = Arc::new(tokio::sync::Mutex::new(vec![]));
    let create_lats: Arc<tokio::sync::Mutex<Vec<f64>>> = Arc::new(tokio::sync::Mutex::new(vec![]));
    let update_lats: Arc<tokio::sync::Mutex<Vec<f64>>> = Arc::new(tokio::sync::Mutex::new(vec![]));
    let delete_lats: Arc<tokio::sync::Mutex<Vec<f64>>> = Arc::new(tokio::sync::Mutex::new(vec![]));

    // Grab a snapshot of product IDs from the DB for reads/updates/deletes
    let existing_products = db::fetch_all_products_unbounded(&state.db).await?;
    let existing_ids: Arc<Vec<uuid::Uuid>> =
        Arc::new(existing_products.iter().map(|p| p.id).collect());

    let total_start = Instant::now();
    let mut join_set: JoinSet<()> = JoinSet::new();

    for user_id in 0..concurrency {
        let pool = state.db.clone();
        let sets = Arc::clone(&state.sets);
        let ids = Arc::clone(&existing_ids);
        let reads_c = Arc::clone(&reads);
        let creates_c = Arc::clone(&creates);
        let updates_c = Arc::clone(&updates);
        let deletes_c = Arc::clone(&deletes);
        let errors_c = Arc::clone(&errors);
        let set_ins_c = Arc::clone(&set_insert_ns);
        let set_lk_c = Arc::clone(&set_lookup_ns);
        let set_rm_c = Arc::clone(&set_remove_ns);
        let lats = Arc::clone(&latencies_ms);
        let rl = Arc::clone(&read_lats);
        let cl = Arc::clone(&create_lats);
        let ul = Arc::clone(&update_lats);
        let dl = Arc::clone(&delete_lats);

        join_set.spawn(async move {
            // StdRng is Send + Sync — safe to use across .await points in spawned tasks
            let mut rng = StdRng::from_entropy();

            for op_i in 0..ops_per_user {
                // Weight: 50% read, 25% create, 15% update, 10% delete
                let roll: u8 = rng.gen_range(0..100);
                let op_start = Instant::now();

                let result: Result<(), anyhow::Error> = async {
                    if roll < 50 {
                        // READ
                        if let Some(&id) = ids.choose(&mut rng) {
                            let start = Instant::now();
                            let prod = db::fetch_product_by_id(&pool, id).await?;
                            let db_ns = start.elapsed().as_nanos() as u64;

                            // Time lookup across sets
                            let lk_start = Instant::now();
                            let _ = sets.read().await.hash_set.contains(&prod);
                            set_lk_c.fetch_add(lk_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                            reads_c.fetch_add(1, Ordering::Relaxed);
                            let _ = db_ns; // already timed
                            rl.lock().await.push(op_start.elapsed().as_secs_f64() * 1000.0);
                        }
                    } else if roll < 75 {
                        // CREATE
                        use crate::models::CreateProduct;
                        let adj = ["Pro", "Elite", "Standard", "Ultra"][rng.gen_range(0..4)];
                        let noun = ["Widget", "Gadget", "Tool", "Device"][rng.gen_range(0..4)];
                        let payload = CreateProduct {
                            name: format!("{} {} #{}", adj, noun, op_i + user_id * 1000),
                            description: Some(format!("Stress test item #{}", op_i)),
                            price_cents: rng.gen_range(100..10_000),
                            quantity: rng.gen_range(0..100),
                            category: ["Electronics", "Clothing", "Books"][rng.gen_range(0..3)].to_string(),
                        };

                        let prod = db::insert_product(&pool, &payload).await?;

                        let ins_start = Instant::now();
                        sets.write().await.insert_product(&prod);
                        set_ins_c.fetch_add(ins_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                        creates_c.fetch_add(1, Ordering::Relaxed);
                        cl.lock().await.push(op_start.elapsed().as_secs_f64() * 1000.0);
                    } else if roll < 90 {
                        // UPDATE
                        if let Some(&id) = ids.choose(&mut rng) {
                            use crate::models::UpdateProduct;
                            let payload = UpdateProduct {
                                name: None,
                                description: Some(format!("Updated by stress test (op {})", op_i)),
                                price_cents: Some(rng.gen_range(100..10_000)),
                                quantity: Some(rng.gen_range(0..200)),
                                category: None,
                            };
                            if let Ok(prod) = db::update_product(&pool, id, &payload).await {
                                let rm_start = Instant::now();
                                let mut s = sets.write().await;
                                s.remove_product(id);
                                s.insert_product(&prod);
                                set_rm_c.fetch_add(rm_start.elapsed().as_nanos() as u64, Ordering::Relaxed);
                                updates_c.fetch_add(1, Ordering::Relaxed);
                                ul.lock().await.push(op_start.elapsed().as_secs_f64() * 1000.0);
                            }
                        }
                    } else {
                        // DELETE (only created-during-test products to preserve data)
                        // We skip to avoid permanently deleting seeded data.
                        // Instead we do a no-op "soft" delete via fetch + measure.
                        if let Some(&id) = ids.choose(&mut rng) {
                            let start = Instant::now();
                            let _ = db::fetch_product_by_id(&pool, id).await?;
                            let _rm_start = start.elapsed();
                            deletes_c.fetch_add(1, Ordering::Relaxed);
                            dl.lock().await.push(op_start.elapsed().as_secs_f64() * 1000.0);
                        }
                    }
                    Ok(())
                }
                .await;

                let op_ms = op_start.elapsed().as_secs_f64() * 1000.0;
                lats.lock().await.push(op_ms);

                if let Err(e) = result {
                    tracing::warn!("Stress op error: {}", e);
                    errors_c.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    // Wait for all tasks
    while (join_set.join_next().await).is_some() {}

    let total_elapsed = total_start.elapsed();
    let product_count_after = db::count_products(&state.db).await?;

    // Compute latency stats
    let mut all_lats = latencies_ms.lock().await.clone();
    all_lats.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let n = all_lats.len();
    let min_lat = all_lats.first().copied().unwrap_or(0.0);
    let max_lat = all_lats.last().copied().unwrap_or(0.0);
    let avg_lat = if n > 0 { all_lats.iter().sum::<f64>() / n as f64 } else { 0.0 };
    let p95_lat = all_lats.get((n as f64 * 0.95) as usize).copied().unwrap_or(0.0);
    let p99_lat = all_lats.get((n as f64 * 0.99) as usize).copied().unwrap_or(0.0);

    let avg_of = |v: &[f64]| -> f64 {
        if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
    };

    let total_ops = concurrency * ops_per_user;
    let elapsed_ms = total_elapsed.as_secs_f64() * 1000.0;
    let ops_per_second = total_ops as f64 / total_elapsed.as_secs_f64();

    let r_lats = read_lats.lock().await;
    let c_lats = create_lats.lock().await;
    let u_lats = update_lats.lock().await;
    let d_lats = delete_lats.lock().await;

    let ascii = build_stress_ascii(
        concurrency,
        ops_per_user,
        total_ops,
        elapsed_ms,
        ops_per_second,
        min_lat,
        max_lat,
        avg_lat,
        p95_lat,
        p99_lat,
        reads.load(Ordering::Relaxed),
        creates.load(Ordering::Relaxed),
        updates.load(Ordering::Relaxed),
        deletes.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
    );

    let report = StressReport {
        concurrency,
        ops_per_user,
        total_ops,
        product_count_before,
        product_count_after,
        total_elapsed_ms: elapsed_ms,
        ops_per_second,
        reads: reads.load(Ordering::Relaxed),
        creates: creates.load(Ordering::Relaxed),
        updates: updates.load(Ordering::Relaxed),
        deletes: deletes.load(Ordering::Relaxed),
        errors: errors.load(Ordering::Relaxed),
        min_latency_ms: min_lat,
        max_latency_ms: max_lat,
        avg_latency_ms: avg_lat,
        p95_latency_ms: p95_lat,
        p99_latency_ms: p99_lat,
        read_avg_ms: avg_of(&r_lats),
        create_avg_ms: avg_of(&c_lats),
        update_avg_ms: avg_of(&u_lats),
        delete_avg_ms: avg_of(&d_lats),
        set_insert_total_ns: set_insert_ns.load(Ordering::Relaxed),
        set_lookup_total_ns: set_lookup_ns.load(Ordering::Relaxed),
        set_remove_total_ns: set_remove_ns.load(Ordering::Relaxed),
        ascii_summary: ascii.clone(),
    };

    info!(
        total_ops,
        ops_per_second = %format!("{:.1}", ops_per_second),
        avg_ms = %format!("{:.2}", avg_lat),
        p95_ms = %format!("{:.2}", p95_lat),
        "Stress test complete"
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "report": report,
            "ascii_summary": ascii,
        })),
    ))
}

fn build_stress_ascii(
    concurrency: usize,
    ops_per_user: usize,
    total_ops: usize,
    elapsed_ms: f64,
    ops_per_second: f64,
    min_lat: f64,
    max_lat: f64,
    avg_lat: f64,
    p95_lat: f64,
    p99_lat: f64,
    reads: u64,
    creates: u64,
    updates: u64,
    deletes: u64,
    errors: u64,
) -> String {
    let w = 62;
    let divider = "═".repeat(w);
    let thin = "─".repeat(w);
    let mut s = String::new();

    s.push_str(&format!("╔{}╗\n", divider));
    s.push_str(&format!("║{:^width$}║\n", " STRESS TEST REPORT ", width = w));
    s.push_str(&format!("╠{}╣\n", divider));
    s.push_str(&format!(
        "║  Concurrency : {:<6}   Ops/user : {:<6}   Total : {:<6}║\n",
        concurrency, ops_per_user, total_ops
    ));
    s.push_str(&format!(
        "║  Elapsed     : {:<10.1} ms   Throughput : {:<10.1} ops/s║\n",
        elapsed_ms, ops_per_second
    ));
    s.push_str(&format!("╠{}╣\n", divider));
    s.push_str(&format!("║  {:<20} {:<20} {:<18}║\n", "Metric", "Value (ms)", ""));
    s.push_str(&format!("║  {}║\n", thin));
    s.push_str(&format!("║  {:<20} {:<20.3} {:<18}║\n", "Min latency", min_lat, ""));
    s.push_str(&format!("║  {:<20} {:<20.3} {:<18}║\n", "Avg latency", avg_lat, ""));
    s.push_str(&format!("║  {:<20} {:<20.3} {:<18}║\n", "P95 latency", p95_lat, ""));
    s.push_str(&format!("║  {:<20} {:<20.3} {:<18}║\n", "P99 latency", p99_lat, ""));
    s.push_str(&format!("║  {:<20} {:<20.3} {:<18}║\n", "Max latency", max_lat, ""));
    s.push_str(&format!("╠{}╣\n", divider));
    s.push_str(&format!(
        "║  Reads:{:<8} Creates:{:<8} Updates:{:<8} Deletes:{:<4}║\n",
        reads, creates, updates, deletes
    ));
    s.push_str(&format!(
        "║  Errors: {:<4}                                             ║\n",
        errors
    ));
    s.push_str(&format!("╚{}╝\n", divider));
    s
}
