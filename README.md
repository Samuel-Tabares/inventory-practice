# Inventory Practice

A full-stack application for exploring and benchmarking Rust's `HashSet`, `IndexSet`, and `BTreeSet` on real inventory data — with a visual frontend to test everything without touching the terminal.

```
inventory-practice/
├── backend/    — Rust · Axum · SQLx · PostgreSQL
├── frontend/   — Vanilla HTML / CSS / JS dashboard
└── README.md   ← you are here
```

---

## Prerequisites

| Tool | Purpose | Check |
|---|---|---|
| [Rust + Cargo](https://rustup.rs) | Build and run the backend | `cargo --version` |
| [PostgreSQL 16](https://www.postgresql.org/download/) | Database | `psql --version` |
| Python 3 | Serve the frontend | `python3 --version` |
| [Docker + Compose](https://docs.docker.com/get-docker/) | Optional all-in-one start | `docker --version` |

---

## Option A — Local (two terminals)

### Terminal 1 — Backend

```bash
# 1. Create the database and user
createdb inventory
psql inventory -c "CREATE USER inventory_user WITH PASSWORD 'inventory_pass';"
psql inventory -c "GRANT ALL PRIVILEGES ON DATABASE inventory TO inventory_user;"

# 2. Set the connection string
export DATABASE_URL="postgres://inventory_user:inventory_pass@localhost:5432/inventory"

# 3. Start the backend (migrations run automatically on startup)
cd backend
cargo run
```

Backend is ready when you see:
```
Listening on http://127.0.0.1:3000
```

### Terminal 2 — Frontend

```bash
cd frontend
python3 -m http.server 8080
```

Open **http://localhost:8080** in your browser.

---

## Option B — Docker (one command)

```bash
cd backend
docker compose up --build
```

Docker starts PostgreSQL 16 and the Rust app together. Migrations run automatically.
Backend → `http://localhost:3000`

Then serve the frontend from a second terminal:

```bash
cd frontend
python3 -m http.server 8080
```

Open **http://localhost:8080**.

---

## First Run — Recommended Flow

Once both backend and frontend are running, follow these steps in the browser:

### 1. Seed data
Go to **🌱 Seed Data** → set the slider to `5000` → click **Seed**.

The backend bulk-inserts 5 000 random products and loads them into all three in-memory sets.

### 2. Run the benchmark
Go to **⚡ Benchmark** → click **▶ Run Benchmark**.

You'll see three color-coded cards — one per set — with proportional bar charts comparing:
- Insert all
- Lookup hit / miss
- Iterate all (full collection, not just 10 items)
- Remove half

Winner badges show which set was fastest in each category.

### 3. Inspect the sets live
Go to **🔍 Set Inspector**.

Three columns show the first 5 items from each set right now:
- **HashSet** — arbitrary hash-based order, changes every run
- **IndexSet** — exactly the insertion order (FIFO)
- **BTreeSet** — always alphabetical by product name

### 4. Browse products
Go to **📦 Products** → click **View** on any row.

A modal opens showing the product's fields plus a live lookup panel: how long each of the three sets took to find this product (in microseconds).

### 5. Record a return
Go to **↩ Devolutions** → fill the right-hand form → click **Record Return**.

Select a product from the dropdown, enter quantity and reason. The history table updates instantly.

### 6. Run a stress test
Go to **💪 Stress Test** → set concurrency to `20`, ops per user to `100` → click **▶ Run**.

The backend spawns 20 concurrent tasks, each performing 100 operations (50% reads, 25% creates, 15% updates, 10% deletes). Results show throughput, p95/p99 latency, and per-operation breakdown.

### 7. Export metrics
Go to **📈 Metrics** → click **⬇ CSV** to download all timing data accumulated across every benchmark run.

---

## What the Backend Does

The backend is a Rust REST API built with **Axum + SQLx** that keeps three in-memory sets synchronized with a PostgreSQL database:

| Set | Order | Complexity |
|---|---|---|
| `std::collections::HashSet` | Arbitrary (hash-based) | O(1) average |
| `indexmap::IndexSet` | Insertion order (FIFO) | O(1) average |
| `std::collections::BTreeSet` | Sorted by name | O(log n) |

Every product write (create / update / delete) is reflected in all three sets instantly. The benchmark engine times insert, lookup, iterate, and remove on all three sets simultaneously, using the same dataset, so the comparison is fair.

See [`backend/README.md`](backend/README.md) for the full API reference, architecture diagram, performance insights, and testing guide.

---

## What the Frontend Does

A single-page dashboard with eight sections, all talking to the backend via `fetch`:

| Page | What you can do |
|---|---|
| 📊 Dashboard | Health status, live set sizes, last benchmark winners |
| 📦 Products | Filter, paginate, create, edit, delete — view per-set lookup times per product |
| ↩ Devolutions | Return history + create returns from a product dropdown |
| 🌱 Seed Data | Slider (100–50 k) + quick buttons, shows timing results |
| ⚡ Benchmark | Run or reload — bar charts, winner badges, iteration order samples |
| 🔍 Set Inspector | Live first-5-items view proving each set's iteration order |
| 💪 Stress Test | Concurrent load with latency p95/p99 and op breakdown |
| 📈 Metrics | Aggregated µs table across all runs + CSV download |

See [`frontend/README.md`](frontend/README.md) for a detailed breakdown of every page and component.

---

## Project Structure

```
inventory-practice/
│
├── backend/
│   ├── src/
│   │   ├── main.rs           — Axum router, AppState
│   │   ├── config.rs         — Environment config (DATABASE_URL, HOST, PORT)
│   │   ├── error.rs          — AppError → HTTP response mapping
│   │   ├── models/
│   │   │   ├── product.rs    — Product struct (Hash/Eq by UUID, Ord by name)
│   │   │   └── devolution.rs — Devolution + join view
│   │   ├── db/
│   │   │   └── mod.rs        — All SQLx queries (products + devolutions)
│   │   ├── sets/
│   │   │   └── mod.rs        — SetManager, benchmark runner, 25 unit tests
│   │   ├── metrics/
│   │   │   └── mod.rs        — MetricsStore, CSV/JSON export, ASCII table
│   │   ├── seed/
│   │   │   └── mod.rs        — Bulk seeder via UNNEST batch inserts
│   │   └── handlers/
│   │       ├── products.rs   — CRUD + per-set timing on every request
│   │       ├── devolutions.rs
│   │       ├── benchmark.rs  — Seed, run, report, export endpoints
│   │       └── stress.rs     — Concurrent load simulation with JoinSet
│   ├── migrations/
│   │   ├── ..._create_products.sql
│   │   ├── ..._create_devolutions.sql
│   │   └── ..._benchmark_metrics.sql
│   ├── Dockerfile
│   ├── docker-compose.yml    — PostgreSQL 16 + app service
│   └── README.md             — Full backend docs
│
└── frontend/
    ├── index.html            — 8-page SPA structure + 2 modals
    ├── style.css             — Dark theme design system
    ├── app.js                — All API calls + render logic
    └── README.md             — Full frontend docs
```

---

## Running Tests

```bash
cd backend
cargo test
```

25 unit tests covering `Product` trait implementations (`Hash`, `Eq`, `Ord`) and all `SetManager` operations — no database required.

---

## Ports at a Glance

| Service | Port | URL |
|---|---|---|
| Backend API | 3000 | http://localhost:3000 |
| Frontend | 8080 | http://localhost:8080 |
| PostgreSQL | 5432 | postgres://inventory_user:inventory_pass@localhost:5432/inventory |
