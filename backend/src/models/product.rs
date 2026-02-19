use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use uuid::Uuid;

/// Core product entity. Hash/Eq are by UUID so all three set types work correctly.
/// Ord is by (name, id) so BTreeSet demonstrates automatic alphabetical sorting.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Price stored as integer cents (e.g. 999 = $9.99)
    pub price_cents: i64,
    pub quantity: i32,
    pub category: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Hash for Product {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for Product {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Product {}

impl Ord for Product {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name
            .cmp(&other.name)
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl PartialOrd for Product {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Product {
    /// Price as a floating-point dollar amount for display purposes.
    pub fn price_dollars(&self) -> f64 {
        self.price_cents as f64 / 100.0
    }
}

// ── Request payloads ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateProduct {
    pub name: String,
    pub description: Option<String>,
    /// Price in cents
    pub price_cents: i64,
    pub quantity: i32,
    pub category: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProduct {
    pub name: Option<String>,
    pub description: Option<String>,
    pub price_cents: Option<i64>,
    pub quantity: Option<i32>,
    pub category: Option<String>,
}

// ── Query parameters ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct ProductFilters {
    pub category: Option<String>,
    pub min_price_cents: Option<i64>,
    pub max_price_cents: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
