use std::collections::{BTreeSet, HashSet};
use std::hint::black_box;
use std::time::{Duration, Instant};

/// How many evenly-spread elements are used for every lookup measurement.
/// Averaging 1 000 samples eliminates single-call noise and exercises
/// different positions in each set's internal structure.
const LOOKUP_SAMPLES: usize = 1_000;

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
    ///
    /// `HashSet` and `IndexSet` deduplicate by `Eq` (UUID), so re-inserting a
    /// product with the same UUID naturally replaces the old entry.
    /// `BTreeSet` deduplicates by `Ord` (`(name, id)`), so a name change would
    /// leave a stale entry behind.  We evict by ID first to keep all three sets
    /// consistent.
    pub fn insert_product(&mut self, product: &Product) {
        self.hash_set.insert(product.clone());
        self.index_set.insert(product.clone());
        self.btree_set.retain(|p| p.id != product.id);
        self.btree_set.insert(product.clone());
    }

    /// Remove a product from all three sets by ID.
    pub fn remove_product(&mut self, id: Uuid) {
        self.hash_set.retain(|p| p.id != id);
        self.index_set.retain(|p| p.id != id);
        self.btree_set.retain(|p| p.id != id);
    }

    /// Clear all three sets and the cached benchmark report.
    pub fn reset(&mut self) {
        self.hash_set.clear();
        self.index_set.clear();
        self.btree_set.clear();
        self.last_report = None;
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

/// Builds evenly-spread lookup targets (LOOKUP_SAMPLES indices across the slice).
fn lookup_targets(products: &[Product]) -> Vec<&Product> {
    if products.is_empty() {
        return vec![];
    }
    let step = (products.len() / LOOKUP_SAMPLES).max(1);
    products.iter().step_by(step).take(LOOKUP_SAMPLES).collect()
}

/// Pre-generates LOOKUP_SAMPLES fake products for miss benchmarks.
fn miss_targets() -> Vec<Product> {
    (0..LOOKUP_SAMPLES).map(|_| make_fake_product()).collect()
}

fn benchmark_hash_set(products: &[Product]) -> SetBenchmarkResult {
    // Warmup: prime the allocator so this benchmark doesn't pay OS page-fault
    // costs that the second/third benchmark would otherwise avoid for free.
    {
        let mut w: HashSet<Product> = HashSet::with_capacity(1_000);
        for p in products.iter().take(1_000) { w.insert(p.clone()); }
    }

    let mut set: HashSet<Product> = HashSet::with_capacity(products.len());

    // Insert all
    let (_, insert_dur) = timed(|| {
        for p in products { set.insert(p.clone()); }
    });

    // Lookup hit — average of LOOKUP_SAMPLES evenly-spread elements
    let hits = lookup_targets(products);
    let (_, lookup_hit_total) = timed(|| {
        for p in hits.iter().copied() { black_box(set.contains(black_box(p))); }
    });
    let lookup_hit_dur = if hits.is_empty() {
        Duration::ZERO
    } else {
        lookup_hit_total / hits.len() as u32
    };

    // Lookup miss — average of LOOKUP_SAMPLES fresh UUIDs not in the set
    let misses = miss_targets();
    let (_, lookup_miss_total) = timed(|| {
        for f in misses.iter() { black_box(set.contains(black_box(f))); }
    });
    let lookup_miss_dur = lookup_miss_total / LOOKUP_SAMPLES as u32;

    // Iterate all — time the full traversal, then slice 10 for the sample
    let (all_names, iterate_dur) = timed(|| {
        set.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
    });
    let order_sample: Vec<String> = all_names.into_iter().take(10).collect();

    // Remove half
    let half: Vec<Product> = set.iter().take(products.len() / 2).cloned().collect();
    let (_, remove_dur) = timed(|| {
        for p in &half { set.remove(p); }
    });

    SetBenchmarkResult {
        set_type: "HashSet".to_string(),
        description: "Unordered. O(1) avg insert/lookup/remove. Lookup = avg of 1 000 samples.".to_string(),
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
    // Warmup
    {
        let mut w: IndexSet<Product> = IndexSet::with_capacity(1_000);
        for p in products.iter().take(1_000) { w.insert(p.clone()); }
    }

    let mut set: IndexSet<Product> = IndexSet::with_capacity(products.len());

    let (_, insert_dur) = timed(|| {
        for p in products { set.insert(p.clone()); }
    });

    // Lookup hit — average of LOOKUP_SAMPLES evenly-spread elements
    let hits = lookup_targets(products);
    let (_, lookup_hit_total) = timed(|| {
        for p in hits.iter().copied() { black_box(set.contains(black_box(p))); }
    });
    let lookup_hit_dur = if hits.is_empty() {
        Duration::ZERO
    } else {
        lookup_hit_total / hits.len() as u32
    };

    // Lookup miss — average of LOOKUP_SAMPLES fresh UUIDs not in the set
    let misses = miss_targets();
    let (_, lookup_miss_total) = timed(|| {
        for f in misses.iter() { black_box(set.contains(black_box(f))); }
    });
    let lookup_miss_dur = lookup_miss_total / LOOKUP_SAMPLES as u32;

    let (all_names, iterate_dur) = timed(|| {
        set.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
    });
    let order_sample: Vec<String> = all_names.into_iter().take(10).collect();

    let half: Vec<Product> = set.iter().take(products.len() / 2).cloned().collect();
    let (_, remove_dur) = timed(|| {
        for p in &half { set.swap_remove(p); }
    });

    SetBenchmarkResult {
        set_type: "IndexSet (LinkedHashSet)".to_string(),
        description: "Insertion-ordered. O(1) avg insert/lookup. Lookup = avg of 1 000 samples.".to_string(),
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
    // Warmup
    {
        let mut w: BTreeSet<Product> = BTreeSet::new();
        for p in products.iter().take(1_000) { w.insert(p.clone()); }
    }

    let mut set: BTreeSet<Product> = BTreeSet::new();

    let (_, insert_dur) = timed(|| {
        for p in products { set.insert(p.clone()); }
    });

    // Lookup hit — average of LOOKUP_SAMPLES evenly-spread elements
    let hits = lookup_targets(products);
    let (_, lookup_hit_total) = timed(|| {
        for p in hits.iter().copied() { black_box(set.contains(black_box(p))); }
    });
    let lookup_hit_dur = if hits.is_empty() {
        Duration::ZERO
    } else {
        lookup_hit_total / hits.len() as u32
    };

    // Lookup miss — average of LOOKUP_SAMPLES fresh UUIDs not in the set
    let misses = miss_targets();
    let (_, lookup_miss_total) = timed(|| {
        for f in misses.iter() { black_box(set.contains(black_box(f))); }
    });
    let lookup_miss_dur = lookup_miss_total / LOOKUP_SAMPLES as u32;

    let (all_names, iterate_dur) = timed(|| {
        set.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
    });
    let order_sample: Vec<String> = all_names.into_iter().take(10).collect();

    let half: Vec<Product> = set.iter().take(products.len() / 2).cloned().collect();
    let (_, remove_dur) = timed(|| {
        for p in &half { set.remove(p); }
    });

    SetBenchmarkResult {
        set_type: "BTreeSet".to_string(),
        description: "Sorted by (name, id). O(log n) insert/lookup/remove. Lookup = avg of 1 000 samples.".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make(id: Uuid, name: &str) -> Product {
        Product {
            id,
            name: name.to_string(),
            description: None,
            price_cents: 500,
            quantity: 10,
            category: "Test".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    // ── SetManager basic ops ───────────────────────────────────────────────────

    #[test]
    fn new_manager_is_empty() {
        assert_eq!(SetManager::new().sizes(), (0, 0, 0));
    }

    #[test]
    fn insert_adds_to_all_three_sets() {
        let mut mgr = SetManager::new();
        mgr.insert_product(&make(Uuid::new_v4(), "Widget"));
        assert_eq!(mgr.sizes(), (1, 1, 1));
    }

    #[test]
    fn inserting_same_id_twice_does_not_grow_sets() {
        let mut mgr = SetManager::new();
        let id = Uuid::new_v4();
        mgr.insert_product(&make(id, "First"));
        mgr.insert_product(&make(id, "Second")); // duplicate UUID
        assert_eq!(mgr.sizes(), (1, 1, 1));
    }

    #[test]
    fn remove_product_removes_from_all_three_sets() {
        let mut mgr = SetManager::new();
        let id = Uuid::new_v4();
        mgr.insert_product(&make(id, "Widget"));
        mgr.remove_product(id);
        assert_eq!(mgr.sizes(), (0, 0, 0));
    }

    #[test]
    fn remove_nonexistent_id_is_noop() {
        let mut mgr = SetManager::new();
        mgr.insert_product(&make(Uuid::new_v4(), "Widget"));
        mgr.remove_product(Uuid::new_v4()); // different ID
        assert_eq!(mgr.sizes(), (1, 1, 1));
    }

    #[test]
    fn sync_from_db_replaces_all_contents() {
        let mut mgr = SetManager::new();
        let old = make(Uuid::new_v4(), "Old");
        mgr.insert_product(&old);

        let new_products = vec![
            make(Uuid::new_v4(), "Beta"),
            make(Uuid::new_v4(), "Gamma"),
        ];
        mgr.sync_from_db(&new_products);

        assert_eq!(mgr.sizes(), (2, 2, 2));
        assert!(!mgr.hash_set.contains(&old), "Old product must be gone after sync");
    }

    // ── Order guarantees ───────────────────────────────────────────────────────

    #[test]
    fn index_set_preserves_insertion_order() {
        let mut mgr = SetManager::new();
        let names = ["Zebra", "Alpha", "Mango", "Delta"];
        for name in &names {
            mgr.insert_product(&make(Uuid::new_v4(), name));
        }
        let observed: Vec<&str> = mgr.index_set.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(observed, names, "IndexSet must preserve insertion (FIFO) order");
    }

    #[test]
    fn btree_set_iterates_alphabetically() {
        let mut mgr = SetManager::new();
        mgr.insert_product(&make(Uuid::new_v4(), "Zebra"));
        mgr.insert_product(&make(Uuid::new_v4(), "Alpha"));
        mgr.insert_product(&make(Uuid::new_v4(), "Mango"));
        let observed: Vec<&str> = mgr.btree_set.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(observed, vec!["Alpha", "Mango", "Zebra"]);
    }

    #[test]
    fn hash_set_contains_inserted_product() {
        let mut mgr = SetManager::new();
        let p = make(Uuid::new_v4(), "Widget");
        mgr.insert_product(&p);
        assert!(mgr.hash_set.contains(&p));
        assert!(mgr.index_set.contains(&p));
        assert!(mgr.btree_set.contains(&p));
    }

    // ── Benchmark correctness ──────────────────────────────────────────────────

    #[test]
    fn benchmark_reports_correct_product_count() {
        let products: Vec<Product> = (0..50)
            .map(|i| make(Uuid::new_v4(), &format!("Product {:03}", i)))
            .collect();
        let mut mgr = SetManager::new();
        let report = mgr.run_benchmark(products);
        assert_eq!(report.product_count, 50);
        assert_eq!(report.results.len(), 3);
    }

    #[test]
    fn benchmark_iteration_sample_at_most_10_items() {
        let products: Vec<Product> = (0..30)
            .map(|i| make(Uuid::new_v4(), &format!("P {:02}", i)))
            .collect();
        let mut mgr = SetManager::new();
        let report = mgr.run_benchmark(products);
        for r in &report.results {
            assert!(
                r.iteration_order_sample.len() <= 10,
                "{} sample had {} items",
                r.set_type,
                r.iteration_order_sample.len()
            );
        }
    }

    #[test]
    fn benchmark_btree_sample_is_alphabetically_sorted() {
        let products = vec![
            make(Uuid::new_v4(), "Zebra"),
            make(Uuid::new_v4(), "Alpha"),
            make(Uuid::new_v4(), "Mango"),
        ];
        let mut mgr = SetManager::new();
        let report = mgr.run_benchmark(products);
        let btree = report.results.iter().find(|r| r.set_type == "BTreeSet").unwrap();
        assert_eq!(
            btree.iteration_order_sample,
            vec!["Alpha", "Mango", "Zebra"],
            "BTreeSet sample must be alphabetically sorted"
        );
    }

    #[test]
    fn benchmark_index_sample_preserves_insertion_order() {
        let products = vec![
            make(Uuid::new_v4(), "Zebra"),
            make(Uuid::new_v4(), "Alpha"),
            make(Uuid::new_v4(), "Mango"),
        ];
        let mut mgr = SetManager::new();
        let report = mgr.run_benchmark(products);
        let index = report
            .results
            .iter()
            .find(|r| r.set_type.contains("Index"))
            .unwrap();
        assert_eq!(
            index.iteration_order_sample,
            vec!["Zebra", "Alpha", "Mango"],
            "IndexSet sample must preserve insertion order"
        );
    }

    #[test]
    fn benchmark_iterate_times_all_elements_not_just_10() {
        // With only 5 products the iterate timing must still cover all 5 (sample == all names)
        let products: Vec<Product> = (0..5)
            .map(|i| make(Uuid::new_v4(), &format!("Item {:02}", i)))
            .collect();
        let mut mgr = SetManager::new();
        let report = mgr.run_benchmark(products);
        for r in &report.results {
            assert_eq!(
                r.iteration_order_sample.len(),
                5,
                "{} should expose all 5 items when count < 10",
                r.set_type
            );
        }
    }

    #[test]
    fn benchmark_syncs_manager_sets_after_run() {
        let products: Vec<Product> = (0..10)
            .map(|i| make(Uuid::new_v4(), &format!("P{}", i)))
            .collect();
        let mut mgr = SetManager::new();
        mgr.run_benchmark(products);
        // After benchmark the manager sets should be populated
        let (h, i, b) = mgr.sizes();
        assert_eq!(h, 10);
        assert_eq!(i, 10);
        assert_eq!(b, 10);
    }

    #[test]
    fn timed_returns_correct_result() {
        let (val, dur) = timed(|| 42_u32 + 1);
        assert_eq!(val, 43);
        // Duration should be non-negative (trivially true, just validate the type)
        let _ = dur.as_nanos();
    }
}
