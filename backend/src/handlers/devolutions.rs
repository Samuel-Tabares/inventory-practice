use std::time::Instant;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tracing::info;
use uuid::Uuid;

use crate::{db, error::AppResult, models::CreateDevolution, AppState};

pub async fn list_devolutions(
    State(state): State<AppState>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let start = Instant::now();
    let devolutions = db::fetch_all_devolutions(&state.db).await?;
    let elapsed = start.elapsed();

    info!(count = devolutions.len(), "Listed devolutions");

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "data": devolutions,
            "count": devolutions.len(),
            "query_time_ms": elapsed.as_secs_f64() * 1000.0,
        })),
    ))
}

pub async fn create_devolution(
    State(state): State<AppState>,
    Json(payload): Json<CreateDevolution>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let start = Instant::now();
    let devolution = db::insert_devolution(&state.db, &payload).await?;
    let elapsed = start.elapsed();

    info!(
        id = %devolution.id,
        product_id = %devolution.product_id,
        quantity = devolution.quantity,
        "Created devolution"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "data": devolution,
            "db_time_ms": elapsed.as_secs_f64() * 1000.0,
        })),
    ))
}

pub async fn get_devolution(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let start = Instant::now();
    let devolution = db::fetch_devolution_by_id(&state.db, id).await?;
    let elapsed = start.elapsed();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "data": devolution,
            "query_time_ms": elapsed.as_secs_f64() * 1000.0,
        })),
    ))
}
