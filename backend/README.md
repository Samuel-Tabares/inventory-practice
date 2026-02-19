# Inventory Service — Rust + PostgreSQL + Docker

A production-style inventory REST API built with **Axum**, **SQLx**, and **Tokio**, designed to compare three Rust set types — `HashSet`, `IndexSet`, and `BTreeSet` — across real-time CRUD operations, bulk seeding, benchmarking, and concurrent stress testing.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Axum HTTP Server                         │
│                      (tokio async runtime)                      │
├──────────────┬──────────────────────────┬───────────────────────┤
│  CRUD Layer  │    Benchmark Engine      │   Stress Test Engine  │
│  (sqlx/PG)   │  HashSet · IndexSet · BTree  tokio::JoinSet     │
├──────────────┴──────────────────────────┴───────────────────────┤
│              Shared AppState  (Arc<RwLock<T>>)                  │
│   ┌──────────┐  ┌────────────────────┐  ┌──────────────────┐   │
│   │ PgPool   │  │   SetManager       │  │  MetricsStore    │   │
│   │ (sqlx)   │  │ HashSet            │  │  Vec<Entry>      │   │
│   │          │  │ IndexSet           │  │  CSV / JSON      │   │
│   │          │  │ BTreeSet           │  │  ASCII tables    │   │
│   └──────────┘  └────────────────────┘  └──────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                           │
                    PostgreSQL 16
                   ┌──────────────┐
                   │  products    │
                   │  devolutions │
                   │  benchmarks  │
                   └──────────────┘
```

### Set Comparison Matrix

> **Note on `IndexSet`:** The `linked-hash-set` crate does not exist on crates.io. The idiomatic Rust equivalent of a `LinkedHashSet` is `indexmap::IndexSet` — a hash set backed by a contiguous array that preserves insertion order with O(1) average lookups.

| Property             | `HashSet`             | `IndexSet` (LinkedHashSet) | `BTreeSet`              |
|----------------------|-----------------------|---------------------------|-------------------------|
| Insert               | O(1) average          | O(1) average              | O(log n)                |
| Lookup               | O(1) average          | O(1) average              | O(log n)                |
| Remove               | O(1) average          | O(1) average              | O(log n)                |
| Iteration order      | Arbitrary             | Insertion order (FIFO)    | Sorted by (name, id)    |
| Memory overhead      | Low                   | Medium (index array)      | Low                     |
| Best for             | Fast membership tests | Ordered caching/queues    | Range queries, sorting  |

---

## Quick Start

### With Docker (recommended)

```bash
# 1. Start Postgres + app
docker compose up --build

# 2. Seed 5 000 products
curl -X POST "http://localhost:3000/api/seed?count=5000"

# 3. Run the set benchmark
curl -X POST http://localhost:3000/api/benchmark/run | jq .

# 4. Download metrics as CSV
curl http://localhost:3000/api/benchmark/export/csv -o metrics.csv

# 5. Run stress test (20 concurrent users, 100 ops each)
curl -X POST http://localhost:3000/api/stress-test \
  -H "Content-Type: application/json" \
  -d '{"concurrency": 20, "ops_per_user": 100}'
```

### Local (requires Rust + Postgres)

```bash
# Set up database
createdb inventory
psql inventory -c "CREATE USER inventory_user WITH PASSWORD 'inventory_pass';"
psql inventory -c "GRANT ALL PRIVILEGES ON DATABASE inventory TO inventory_user;"

# Configure (already populated for local dev)
cp .env .env.local   # edit DATABASE_URL if needed

# Run
cargo run --release
```

---

## API Reference

### Health

| Method | Path      | Description       |
|--------|-----------|-------------------|
| GET    | `/health` | Service liveness  |

### Products

| Method | Path                  | Description                        |
|--------|-----------------------|------------------------------------|
| GET    | `/api/products`       | List products (filterable)         |
| POST   | `/api/products`       | Create a product                   |
| GET    | `/api/products/:id`   | Get product + per-set lookup times |
| PUT    | `/api/products/:id`   | Update product                     |
| DELETE | `/api/products/:id`   | Delete product                     |

**Query params for GET /api/products:**
- `category` — filter by category
- `min_price_cents` / `max_price_cents` — price range
- `limit` (max 10 000) / `offset`

**Create product body:**
```json
{
  "name": "Ultra Widget #001",
  "description": "Optional description",
  "price_cents": 2999,
  "quantity": 50,
  "category": "Electronics"
}
```

### Product Devolutions

| Method | Path                    | Description                  |
|--------|-------------------------|------------------------------|
| GET    | `/api/devolutions`      | List all devolutions (joined)|
| POST   | `/api/devolutions`      | Record a product return      |
| GET    | `/api/devolutions/:id`  | Get devolution by ID         |

**Create devolution body:**
```json
{
  "product_id": "<uuid>",
  "quantity": 3,
  "reason": "Defective on arrival",
  "returned_at": "2024-03-15T10:00:00Z"
}
```

### Seeding & Benchmarking

| Method | Path                            | Description                                   |
|--------|---------------------------------|-----------------------------------------------|
| POST   | `/api/seed?count=N`             | Bulk-insert N random products (max 50 000)    |
| POST   | `/api/benchmark/run`            | Run full set comparison benchmark             |
| GET    | `/api/benchmark/report`         | Retrieve last benchmark report + ASCII table  |
| GET    | `/api/benchmark/sets/status`    | Show sizes + first-5 items from each set      |
| GET    | `/api/benchmark/export/csv`     | Download all metrics as CSV                   |
| GET    | `/api/benchmark/export/json`    | Download metrics as JSON with aggregates      |

### Stress Testing

| Method | Path                | Description                         |
|--------|---------------------|-------------------------------------|
| POST   | `/api/stress-test`  | Simulate concurrent API load        |

**Stress test body:**
```json
{
  "concurrency": 20,
  "ops_per_user": 100,
  "seed_count": 2000
}
```

---

## Example Benchmark Output

```
┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│  SET PERFORMANCE BENCHMARK  —  500 products  —  2026-02-19T14:17:47Z  │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│  Set Type             Insert (ms)    Lookup✓ (µs)  Lookup✗ (µs)  Iterate (ms)  Remove½ (ms)  Order          │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│  HashSet              0.207          0.167         0.375         0.007         0.037          Arbitrary      │
│  IndexSet (LinkedHashSet) 0.106      0.292         0.125         0.001         0.036          Insertion FIFO │
│  BTreeSet             0.202          0.292         0.334         0.001         0.119          Sorted alpha   │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│  Fastest Insert : IndexSet     Fastest Lookup : HashSet     Fastest Iterate : IndexSet        │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

  [HashSet]  Order sample (first 10 names):
     1. Rapid Panel #00097          ← unpredictable hash order
     2. Turbo System #00178
     ...

  [IndexSet (LinkedHashSet)]  Order sample (first 10 names):
     1. Signature Bundle #00000     ← exactly insertion order
     2. Mega Assembly #00001
     ...

  [BTreeSet]  Order sample (first 10 names):
     1. Advanced Adapter #00108     ← always alphabetical
     2. Advanced Component #00063
     ...
```

---

## Performance Insights

| Scenario                          | Best Set    | Why                                                    |
|-----------------------------------|-------------|--------------------------------------------------------|
| Fast membership test (lookup)     | `HashSet`   | O(1) average, no ordering overhead                    |
| Preserve insertion order          | `IndexSet`  | Contiguous index array + hash map (like LinkedHashSet) |
| Alphabetically sorted iteration   | `BTreeSet`  | B-tree keeps elements sorted at all times              |
| Range scans (price, name prefix)  | `BTreeSet`  | `range()` method, O(log n) seek                        |
| Bulk inserts (high throughput)    | `HashSet`   | No tree rebalancing, no index bookkeeping              |
| Concurrent reads (low contention) | `HashSet`   | Simpler structure, less cache pressure                 |

---

## Project Structure

```
inventory-practice/
├── Cargo.toml
├── Dockerfile
├── docker-compose.yml
├── .env
├── migrations/
│   ├── 20240101000001_create_products.sql
│   ├── 20240101000002_create_devolutions.sql
│   └── 20240101000003_benchmark_metrics.sql
├── scripts/
│   ├── demo.sh                — Full end-to-end walkthrough
│   └── benchmark_compare.sh  — Multi-scale comparison (100 → 25 000 products)
└── src/
    ├── main.rs          — App entry point, router
    ├── config.rs        — Environment config
    ├── error.rs         — AppError + IntoResponse
    ├── models/
    │   ├── product.rs   — Product (Hash/Eq/Ord), CreateProduct, UpdateProduct
    │   └── devolution.rs
    ├── db/
    │   └── mod.rs       — All sqlx queries
    ├── sets/
    │   └── mod.rs       — SetManager, benchmark runner, OpTiming
    ├── metrics/
    │   └── mod.rs       — MetricsStore, CSV/JSON export, ASCII table
    ├── seed/
    │   └── mod.rs       — Bulk seeder (UNNEST batch inserts)
    └── handlers/
        ├── products.rs  — CRUD with per-set timing on each request
        ├── devolutions.rs
        ├── benchmark.rs — Seed, run, report, export
        └── stress.rs    — Concurrent load simulation with JoinSet
```

---

## Crates Used

| Crate                | Purpose                                          |
|----------------------|--------------------------------------------------|
| `axum 0.7`           | HTTP framework                                   |
| `tokio 1`            | Async runtime                                    |
| `sqlx 0.7`           | Async PostgreSQL driver + migrations             |
| `indexmap 2`         | `IndexSet` — insertion-ordered set (LinkedHashSet equivalent) |
| `serde / serde_json` | Serialization                                    |
| `uuid`               | UUID v4 for primary keys                         |
| `chrono`             | Timestamps                                       |
| `tracing`            | Structured logging                               |
| `tower-http`         | CORS + request tracing middleware                |
| `rand`               | Random data generation for seeding               |
| `csv`                | CSV export for metrics                           |
| `dotenv`             | `.env` file loading for local development        |

---

## Docker Notes

The builder stage uses `rust:latest` to stay compatible with whatever version of Cargo generated `Cargo.lock` on the host. If you need a reproducible pinned version, replace `rust:latest` with the output of `rustc --version` on your machine (e.g. `rust:1.93`).
