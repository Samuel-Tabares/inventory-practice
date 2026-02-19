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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashSet};
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    fn make(id: Uuid, name: &str) -> Product {
        Product {
            id,
            name: name.to_string(),
            description: None,
            price_cents: 100,
            quantity: 1,
            category: "Test".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn hash_of(p: &Product) -> u64 {
        let mut h = DefaultHasher::new();
        p.hash(&mut h);
        h.finish()
    }

    // ── Eq / Hash ──────────────────────────────────────────────────────────────

    #[test]
    fn eq_same_id_different_name() {
        let id = Uuid::new_v4();
        let p1 = make(id, "Alpha");
        let mut p2 = p1.clone();
        p2.name = "Beta".to_string();
        assert_eq!(p1, p2, "Products with the same ID must be equal regardless of name");
    }

    #[test]
    fn neq_different_ids_same_name() {
        let p1 = make(Uuid::new_v4(), "Alpha");
        let p2 = make(Uuid::new_v4(), "Alpha");
        assert_ne!(p1, p2, "Products with different IDs must not be equal");
    }

    #[test]
    fn hash_equal_products_have_equal_hash() {
        let id = Uuid::new_v4();
        let p1 = make(id, "Alpha");
        let mut p2 = p1.clone();
        p2.name = "Beta".to_string();
        assert_eq!(hash_of(&p1), hash_of(&p2), "Equal products must have equal hashes");
    }

    #[test]
    fn hash_set_deduplicates_by_id() {
        let id = Uuid::new_v4();
        let p1 = make(id, "Alpha");
        let mut p2 = p1.clone();
        p2.name = "Beta".to_string();
        let mut set = HashSet::new();
        set.insert(p1);
        set.insert(p2); // same ID → duplicate
        assert_eq!(set.len(), 1, "HashSet must deduplicate products by ID");
    }

    #[test]
    fn hash_set_two_distinct_ids() {
        let mut set = HashSet::new();
        set.insert(make(Uuid::new_v4(), "Alpha"));
        set.insert(make(Uuid::new_v4(), "Alpha")); // different ID → distinct
        assert_eq!(set.len(), 2);
    }

    // ── Ord / BTreeSet ─────────────────────────────────────────────────────────

    #[test]
    fn ord_reflexive() {
        let p = make(Uuid::new_v4(), "Test");
        assert_eq!(p.cmp(&p), std::cmp::Ordering::Equal);
    }

    #[test]
    fn btree_set_iterates_alphabetically() {
        let mut set = BTreeSet::new();
        set.insert(make(Uuid::new_v4(), "Zebra"));
        set.insert(make(Uuid::new_v4(), "Alpha"));
        set.insert(make(Uuid::new_v4(), "Mango"));
        let names: Vec<&str> = set.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Mango", "Zebra"]);
    }

    #[test]
    fn btree_set_tiebreak_by_uuid() {
        let id1 = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let id2 = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let mut set = BTreeSet::new();
        set.insert(make(id2, "Same"));
        set.insert(make(id1, "Same"));
        let ids: Vec<Uuid> = set.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![id1, id2], "Same-name products must be ordered by UUID as tiebreak");
    }

    #[test]
    fn price_dollars_conversion() {
        let p = make(Uuid::new_v4(), "Test");
        assert!((p.price_dollars() - 1.0).abs() < f64::EPSILON);
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
