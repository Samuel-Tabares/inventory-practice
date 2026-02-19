use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use tracing::info;
use uuid::Uuid;

use crate::{
    db,
    error::AppResult,
    metrics::MetricEntry,
    models::{CreateProduct, ProductFilters, UpdateProduct},
    AppState,
};

// ── List ──────────────────────────────────────────────────────────────────────

pub async fn list_products(
    State(state): State<AppState>,
    Query(filters): Query<ProductFilters>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let start = Instant::now();
    let products = db::fetch_all_products(&state.db, &filters).await?;
    let elapsed = start.elapsed();

    info!(
        count = products.len(),
        elapsed_ms = elapsed.as_millis(),
        "Listed products"
    );

    state.metrics.write().await.record_raw(
        "db_query:list",
        "DB",
        elapsed.as_nanos() as u64,
        products.len(),
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "data": products,
            "count": products.len(),
            "query_time_ms": elapsed.as_secs_f64() * 1000.0,
        })),
    ))
}

// ── Create ────────────────────────────────────────────────────────────────────

pub async fn create_product(
    State(state): State<AppState>,
    Json(payload): Json<CreateProduct>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    if payload.name.trim().is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "name must not be empty".to_string(),
        ));
    }
    if payload.price_cents < 0 {
        return Err(crate::error::AppError::BadRequest(
            "price_cents must be >= 0".to_string(),
        ));
    }

    let db_start = Instant::now();
    let product = db::insert_product(&state.db, &payload).await?;
    let db_elapsed = db_start.elapsed();

    // Sync into all three in-memory sets and time each individually
    let set_start = Instant::now();
    state.sets.write().await.insert_product(&product);
    let set_elapsed = set_start.elapsed();

    let mut metrics = state.metrics.write().await;
    metrics.record_raw("db_query:insert", "DB", db_elapsed.as_nanos() as u64, 1);
    metrics.record(MetricEntry::new(
        "insert",
        "HashSet+LinkedHashSet+BTreeSet",
        set_elapsed.as_nanos() as u64,
        1,
        true,
        Some("all three sets updated atomically".to_string()),
    ));

    info!(id = %product.id, name = %product.name, "Created product");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "data": product,
            "db_time_ms": db_elapsed.as_secs_f64() * 1000.0,
            "set_sync_time_ms": set_elapsed.as_secs_f64() * 1000.0,
        })),
    ))
}

// ── Get by ID ─────────────────────────────────────────────────────────────────

pub async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let start = Instant::now();
    let product = db::fetch_product_by_id(&state.db, id).await?;
    let db_elapsed = start.elapsed();

    // Show lookup time across all three in-memory sets
    let sets = state.sets.read().await;

    let hs_start = Instant::now();
    let in_hash = sets.hash_set.contains(&product);
    let hs_elapsed = hs_start.elapsed();

    let lh_start = Instant::now();
    let in_linked = sets.index_set.contains(&product);
    let lh_elapsed = lh_start.elapsed();

    let bt_start = Instant::now();
    let in_btree = sets.btree_set.contains(&product);
    let bt_elapsed = bt_start.elapsed();

    drop(sets);

    let mut metrics = state.metrics.write().await;
    metrics.record_raw("db_query:get", "DB", db_elapsed.as_nanos() as u64, 1);
    metrics.record_raw("lookup", "HashSet", hs_elapsed.as_nanos() as u64, 1);
    metrics.record_raw("lookup", "IndexSet", lh_elapsed.as_nanos() as u64, 1);
    metrics.record_raw("lookup", "BTreeSet", bt_elapsed.as_nanos() as u64, 1);

    info!(id = %id, "Fetched product");

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "data": product,
            "set_presence": {
                "hash_set": in_hash,
                "index_set": in_linked,
                "btree_set": in_btree,
            },
            "lookup_times_ns": {
                "db": db_elapsed.as_nanos(),
                "hash_set": hs_elapsed.as_nanos(),
                "index_set": lh_elapsed.as_nanos(),
                "btree_set": bt_elapsed.as_nanos(),
            },
        })),
    ))
}

// ── Update ────────────────────────────────────────────────────────────────────

pub async fn update_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateProduct>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let db_start = Instant::now();
    let product = db::update_product(&state.db, id, &payload).await?;
    let db_elapsed = db_start.elapsed();

    // Re-insert updated product into sets (remove old, insert new)
    let set_start = Instant::now();
    let mut sets = state.sets.write().await;
    sets.remove_product(id);
    sets.insert_product(&product);
    let set_elapsed = set_start.elapsed();
    drop(sets);

    let mut metrics = state.metrics.write().await;
    metrics.record_raw("db_query:update", "DB", db_elapsed.as_nanos() as u64, 1);
    metrics.record_raw(
        "remove+insert",
        "HashSet+LinkedHashSet+BTreeSet",
        set_elapsed.as_nanos() as u64,
        1,
    );

    info!(id = %id, "Updated product");

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "data": product,
            "db_time_ms": db_elapsed.as_secs_f64() * 1000.0,
            "set_sync_time_ms": set_elapsed.as_secs_f64() * 1000.0,
        })),
    ))
}

// ── Delete ────────────────────────────────────────────────────────────────────

pub async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let db_start = Instant::now();
    db::delete_product(&state.db, id).await?;
    let db_elapsed = db_start.elapsed();

    let set_start = Instant::now();
    state.sets.write().await.remove_product(id);
    let set_elapsed = set_start.elapsed();

    let mut metrics = state.metrics.write().await;
    metrics.record_raw("db_query:delete", "DB", db_elapsed.as_nanos() as u64, 1);
    metrics.record_raw(
        "remove",
        "HashSet+LinkedHashSet+BTreeSet",
        set_elapsed.as_nanos() as u64,
        1,
    );

    info!(id = %id, "Deleted product");

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "message": "Product deleted",
            "id": id,
            "db_time_ms": db_elapsed.as_secs_f64() * 1000.0,
            "set_sync_time_ms": set_elapsed.as_secs_f64() * 1000.0,
        })),
    ))
}
