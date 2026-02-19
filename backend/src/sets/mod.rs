use std::collections::{BTreeSet, HashSet};
use std::time::{Duration, Instant};

use chrono::Utc;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Product;

// ── Timing helpers ────────────────────────────────────────────────────────────

/// Runs `f`, returns its result and the elapsed duration.
pub fn timed<F, R>(f: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    (result, start.elapsed())
}

// ── Per-operation result ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpTiming {
    /// Nanoseconds elapsed
    pub duration_ns: u64,
    pub duration_us: f64,
    pub duration_ms: f64,
}

impl From<Duration> for OpTiming {
    fn from(d: Duration) -> Self {
        let ns = d.as_nanos() as u64;
        Self {
            duration_ns: ns,
            duration_us: ns as f64 / 1_000.0,
            duration_ms: ns as f64 / 1_000_000.0,
        }
    }
}

// ── Benchmark result for one set type ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetBenchmarkResult {
    pub set_type: String,
    /// Description of what makes this set unique
    pub description: String,
    pub product_count: usize,
    pub insert_all: OpTiming,
    pub lookup_hit: OpTiming,
    pub lookup_miss: OpTiming,
    pub iterate_all: OpTiming,
    pub remove_half: OpTiming,
    /// Order observed during iteration (first 10 names)
    pub iteration_order_sample: Vec<String>,
    /// Is the iteration order deterministic / meaningful?
    pub order_guaranteed: bool,
    pub order_type: String,
}

// ── Full benchmark comparison ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub run_at: String,
    pub product_count: usize,
    pub results: Vec<SetBenchmarkResult>,
    pub winner_insert: String,
    pub winner_lookup: String,
    pub winner_iterate: String,
    pub summary_table: Vec<SummaryRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRow {
    pub set_type: String,
    pub insert_ms: f64,
    pub lookup_hit_us: f64,
    pub lookup_miss_us: f64,
    pub iterate_ms: f64,
    pub remove_ms: f64,
    pub order: String,
}

// ── SetManager: holds all three sets ─────────────────────────────────────────

/// Manages the three in-memory sets that are compared during benchmarks.
///
/// - `hash_set`         → `std::collections::HashSet`  — unordered, O(1) ops
/// - `index_set`        → `indexmap::IndexSet`          — insertion-ordered, O(1) ops
///                         (equivalent to the `linked-hash-set` concept:
///                          a hash set backed by a contiguous array that preserves
///                          the insertion order of elements)
/// - `btree_set`        → `std::collections::BTreeSet` — sorted by (name, id), O(log n) ops
pub struct SetManager {
    pub hash_set: HashSet<Product>,
    /// IndexSet is the idiomatic Rust `LinkedHashSet` equivalent:
    /// O(1) average insert/lookup, deterministic insertion-order iteration.
    pub index_set: IndexSet<Product>,
    pub btree_set: BTreeSet<Product>,
    pub last_report: Option<BenchmarkReport>,
}

impl SetManager {
    pub fn new() -> Self {
        Self {
            hash_set: HashSet::new(),
            index_set: IndexSet::new(),
            btree_set: BTreeSet::new(),
            last_report: None,
        }
    }

    /// Sync all three sets from a DB product list (replacing existing contents).
    pub fn sync_from_db(&mut self, products: &[Product]) {
        self.hash_set.clear();
        self.index_set.clear();
        self.btree_set.clear();

        for p in products {
            self.hash_set.insert(p.clone());
            self.index_set.insert(p.clone());
            self.btree_set.insert(p.clone());
        }
    }

    /// Insert a product into all three sets.
    pub fn insert_product(&mut self, product: &Product) {
        self.hash_set.insert(product.clone());
        self.index_set.insert(product.clone());
        self.btree_set.insert(product.clone());
    }

    /// Remove a product from all three sets by ID.
    pub fn remove_product(&mut self, id: Uuid) {
        self.hash_set.retain(|p| p.id != id);
        self.index_set.retain(|p| p.id != id);
        self.btree_set.retain(|p| p.id != id);
    }

    pub fn sizes(&self) -> (usize, usize, usize) {
        (
            self.hash_set.len(),
            self.index_set.len(),
            self.btree_set.len(),
        )
    }

    // ── Benchmark runner ──────────────────────────────────────────────────────

    pub fn run_benchmark(&mut self, products: Vec<Product>) -> BenchmarkReport {
        let count = products.len();

        let hash_result = benchmark_hash_set(&products);
        let index_result = benchmark_index_set(&products);
        let btree_result = benchmark_btree_set(&products);

        // Re-sync manager sets after benchmark
        self.sync_from_db(&products);

        let winner_insert = fastest_insert(&hash_result, &index_result, &btree_result);
        let winner_lookup = fastest_lookup(&hash_result, &index_result, &btree_result);
        let winner_iterate = fastest_iterate(&hash_result, &index_result, &btree_result);

        let summary_table = vec![
            summary_row(&hash_result),
            summary_row(&index_result),
            summary_row(&btree_result),
        ];

        let report = BenchmarkReport {
            run_at: Utc::now().to_rfc3339(),
            product_count: count,
            results: vec![hash_result, index_result, btree_result],
            winner_insert,
            winner_lookup,
            winner_iterate,
            summary_table,
        };

        self.last_report = Some(report.clone());
        report
    }
}

// ── Individual set benchmarks ─────────────────────────────────────────────────

fn benchmark_hash_set(products: &[Product]) -> SetBenchmarkResult {
    let mut set: HashSet<Product> = HashSet::with_capacity(products.len());

    // Insert all
    let (_, insert_dur) = timed(|| {
        for p in products {
            set.insert(p.clone());
        }
    });

    // Lookup hit (first product)
    let lookup_hit_dur = if let Some(first) = products.first() {
        timed(|| set.contains(first)).1
    } else {
        Duration::ZERO
    };

    // Lookup miss (fake UUID)
    let fake = make_fake_product();
    let lookup_miss_dur = timed(|| set.contains(&fake)).1;

    // Iterate all — collect first 10 for the sample, time the full iteration
    let (order_sample, iterate_dur) = timed(|| {
        set.iter()
            .take(10)
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
    });

    // Remove half
    let half: Vec<Product> = set.iter().take(products.len() / 2).cloned().collect();
    let (_, remove_dur) = timed(|| {
        for p in &half {
            set.remove(p);
        }
    });

    SetBenchmarkResult {
        set_type: "HashSet".to_string(),
        description: "Unordered. O(1) average insert/lookup/remove. No iteration order guarantee.".to_string(),
        product_count: products.len(),
        insert_all: insert_dur.into(),
        lookup_hit: lookup_hit_dur.into(),
        lookup_miss: lookup_miss_dur.into(),
        iterate_all: iterate_dur.into(),
        remove_half: remove_dur.into(),
        iteration_order_sample: order_sample,
        order_guaranteed: false,
        order_type: "Arbitrary (hash-based)".to_string(),
    }
}

/// `IndexSet` (from the `indexmap` crate) is the idiomatic Rust equivalent of
/// a `LinkedHashSet`: it stores elements in a flat array (preserving insertion
/// order) while maintaining a hash-map index for O(1) average lookups.
fn benchmark_index_set(products: &[Product]) -> SetBenchmarkResult {
    let mut set: IndexSet<Product> = IndexSet::with_capacity(products.len());

    let (_, insert_dur) = timed(|| {
        for p in products {
            set.insert(p.clone());
        }
    });

    let lookup_hit_dur = if let Some(first) = products.first() {
        timed(|| set.contains(first)).1
    } else {
        Duration::ZERO
    };

    let fake = make_fake_product();
    let lookup_miss_dur = timed(|| set.contains(&fake)).1;

    let (order_sample, iterate_dur) = timed(|| {
        set.iter()
            .take(10)
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
    });

    let half: Vec<Product> = set.iter().take(products.len() / 2).cloned().collect();
    let (_, remove_dur) = timed(|| {
        for p in &half {
            set.swap_remove(p);
        }
    });

    SetBenchmarkResult {
        set_type: "IndexSet (LinkedHashSet)".to_string(),
        description: "Insertion-ordered. O(1) average insert/lookup. Preserves insertion order. Uses indexmap crate.".to_string(),
        product_count: products.len(),
        insert_all: insert_dur.into(),
        lookup_hit: lookup_hit_dur.into(),
        lookup_miss: lookup_miss_dur.into(),
        iterate_all: iterate_dur.into(),
        remove_half: remove_dur.into(),
        iteration_order_sample: order_sample,
        order_guaranteed: true,
        order_type: "Insertion order (FIFO)".to_string(),
    }
}

fn benchmark_btree_set(products: &[Product]) -> SetBenchmarkResult {
    let mut set: BTreeSet<Product> = BTreeSet::new();

    let (_, insert_dur) = timed(|| {
        for p in products {
            set.insert(p.clone());
        }
    });

    let lookup_hit_dur = if let Some(first) = products.first() {
        timed(|| set.contains(first)).1
    } else {
        Duration::ZERO
    };

    let fake = make_fake_product();
    let lookup_miss_dur = timed(|| set.contains(&fake)).1;

    let (order_sample, iterate_dur) = timed(|| {
        set.iter()
            .take(10)
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
    });

    let half: Vec<Product> = set.iter().take(products.len() / 2).cloned().collect();
    let (_, remove_dur) = timed(|| {
        for p in &half {
            set.remove(p);
        }
    });

    SetBenchmarkResult {
        set_type: "BTreeSet".to_string(),
        description: "Sorted by (name, id). O(log n) insert/lookup/remove. Always alphabetically ordered.".to_string(),
        product_count: products.len(),
        insert_all: insert_dur.into(),
        lookup_hit: lookup_hit_dur.into(),
        lookup_miss: lookup_miss_dur.into(),
        iterate_all: iterate_dur.into(),
        remove_half: remove_dur.into(),
        iteration_order_sample: order_sample,
        order_guaranteed: true,
        order_type: "Sorted alphabetically by name".to_string(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_fake_product() -> Product {
    Product {
        id: Uuid::new_v4(),
        name: "zzz_nonexistent_product".to_string(),
        description: None,
        price_cents: 0,
        quantity: 0,
        category: "none".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn fastest_insert(h: &SetBenchmarkResult, l: &SetBenchmarkResult, b: &SetBenchmarkResult) -> String {
    [
        (&h.set_type, h.insert_all.duration_ns),
        (&l.set_type, l.insert_all.duration_ns),
        (&b.set_type, b.insert_all.duration_ns),
    ]
    .iter()
    .min_by_key(|x| x.1)
    .map(|x| x.0.as_str())
    .unwrap_or("N/A")
    .to_string()
}

fn fastest_lookup(h: &SetBenchmarkResult, l: &SetBenchmarkResult, b: &SetBenchmarkResult) -> String {
    [
        (&h.set_type, h.lookup_hit.duration_ns),
        (&l.set_type, l.lookup_hit.duration_ns),
        (&b.set_type, b.lookup_hit.duration_ns),
    ]
    .iter()
    .min_by_key(|x| x.1)
    .map(|x| x.0.as_str())
    .unwrap_or("N/A")
    .to_string()
}

fn fastest_iterate(h: &SetBenchmarkResult, l: &SetBenchmarkResult, b: &SetBenchmarkResult) -> String {
    [
        (&h.set_type, h.iterate_all.duration_ns),
        (&l.set_type, l.iterate_all.duration_ns),
        (&b.set_type, b.iterate_all.duration_ns),
    ]
    .iter()
    .min_by_key(|x| x.1)
    .map(|x| x.0.as_str())
    .unwrap_or("N/A")
    .to_string()
}

fn summary_row(r: &SetBenchmarkResult) -> SummaryRow {
    SummaryRow {
        set_type: r.set_type.clone(),
        insert_ms: r.insert_all.duration_ms,
        lookup_hit_us: r.lookup_hit.duration_us,
        lookup_miss_us: r.lookup_miss.duration_us,
        iterate_ms: r.iterate_all.duration_ms,
        remove_ms: r.remove_half.duration_ms,
        order: r.order_type.clone(),
    }
}
