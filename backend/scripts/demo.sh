#!/usr/bin/env bash
# ============================================================
#  Inventory Service — end-to-end demo script
#  Run AFTER: docker compose up --build
# ============================================================
set -euo pipefail

BASE="http://localhost:3000"
BOLD='\033[1m'
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RESET='\033[0m'

header() { echo -e "\n${BOLD}${CYAN}━━━  $1  ━━━${RESET}"; }
ok()     { echo -e "${GREEN}✓${RESET} $1"; }
step()   { echo -e "${YELLOW}▶${RESET} $1"; }

# ── 1. Health ─────────────────────────────────────────────────────────────────
header "1 · Health check"
curl -s "$BASE/health" | python3 -m json.tool
ok "Service is up"

# ── 2. Create a product manually ─────────────────────────────────────────────
header "2 · Create a product"
PRODUCT=$(curl -s -X POST "$BASE/api/products" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Pro Widget #0001",
    "description": "A demonstration widget",
    "price_cents": 4999,
    "quantity": 100,
    "category": "Electronics"
  }')
echo "$PRODUCT" | python3 -m json.tool
PRODUCT_ID=$(echo "$PRODUCT" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['id'])")
ok "Created product: $PRODUCT_ID"

# ── 3. Get product (shows per-set lookup timings) ────────────────────────────
header "3 · Get product (shows lookup timings per set type)"
curl -s "$BASE/api/products/$PRODUCT_ID" | python3 -m json.tool
ok "Lookup timings shown above"

# ── 4. Update product ────────────────────────────────────────────────────────
header "4 · Update product"
curl -s -X PUT "$BASE/api/products/$PRODUCT_ID" \
  -H "Content-Type: application/json" \
  -d '{"quantity": 75, "price_cents": 3999}' | python3 -m json.tool
ok "Updated"

# ── 5. Create a devolution ───────────────────────────────────────────────────
header "5 · Create a product devolution (return)"
curl -s -X POST "$BASE/api/devolutions" \
  -H "Content-Type: application/json" \
  -d "{
    \"product_id\": \"$PRODUCT_ID\",
    \"quantity\": 5,
    \"reason\": \"Defective on arrival\"
  }" | python3 -m json.tool
ok "Devolution recorded"

# ── 6. Seed 5 000 products ───────────────────────────────────────────────────
header "6 · Seed 5 000 random products for benchmarking"
step "This will take a few seconds..."
curl -s -X POST "$BASE/api/seed?count=5000" | python3 -m json.tool
ok "Seeded"

# ── 7. Run benchmark ─────────────────────────────────────────────────────────
header "7 · Run set benchmark (HashSet vs LinkedHashSet vs BTreeSet)"
step "Benchmarking..."
BENCH=$(curl -s -X POST "$BASE/api/benchmark/run")
echo "$BENCH" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(d.get('ascii_table', 'No ASCII table'))
"
ok "Benchmark complete — check 'report' field for full JSON"

# ── 8. Sets status ───────────────────────────────────────────────────────────
header "8 · Sets status (sizes + first 5 elements showing order differences)"
curl -s "$BASE/api/benchmark/sets/status" | python3 -m json.tool
ok "Note how BTreeSet shows alphabetical order while LinkedHashSet shows insertion order"

# ── 9. Export metrics ────────────────────────────────────────────────────────
header "9 · Export metrics as JSON (with aggregates + ASCII table)"
curl -s "$BASE/api/benchmark/export/json" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print('Entry count:', d['entry_count'])
print(d.get('ascii_table', ''))
"

header "9b · Download metrics CSV"
curl -s "$BASE/api/benchmark/export/csv" -o /tmp/inventory_metrics.csv
ok "Saved to /tmp/inventory_metrics.csv ($(wc -l < /tmp/inventory_metrics.csv) rows)"

# ── 10. Stress test ──────────────────────────────────────────────────────────
header "10 · Stress test — 20 concurrent users × 50 ops"
step "Simulating concurrent load..."
curl -s -X POST "$BASE/api/stress-test" \
  -H "Content-Type: application/json" \
  -d '{"concurrency": 20, "ops_per_user": 50}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(d.get('ascii_summary', ''))
r = d['report']
print(f\"Throughput: {r['ops_per_second']:.1f} ops/sec\")
print(f\"P95 latency: {r['p95_latency_ms']:.2f} ms\")
"

# ── Done ─────────────────────────────────────────────────────────────────────
header "Done"
echo -e "${GREEN}"
echo "  All endpoints exercised successfully."
echo "  Key takeaways:"
echo "   • HashSet  = fastest insert/lookup (O(1), no ordering)"
echo "   • LinkedHashSet = insertion order + O(1) lookups"
echo "   • BTreeSet = automatically sorted by name (O(log n))"
echo -e "${RESET}"
