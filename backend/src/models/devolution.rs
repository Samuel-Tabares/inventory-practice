use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProductDevolution {
    pub id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
    pub reason: String,
    pub returned_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDevolution {
    pub product_id: Uuid,
    pub quantity: i32,
    pub reason: String,
    /// Optional override for the return timestamp (defaults to NOW())
    pub returned_at: Option<DateTime<Utc>>,
}

/// Devolution joined with product info for richer API responses.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DevolutionWithProduct {
    pub id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub product_category: String,
    pub quantity: i32,
    pub reason: String,
    pub returned_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
