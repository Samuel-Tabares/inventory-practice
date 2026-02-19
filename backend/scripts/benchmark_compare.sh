#!/usr/bin/env bash
# ============================================================
#  Run benchmarks at multiple dataset sizes and compare results
# ============================================================
set -euo pipefail

BASE="http://localhost:3000"
SIZES=(100 500 1000 5000 10000 25000)
OUT_FILE="/tmp/benchmark_comparison.csv"

echo "size,set_type,insert_ms,lookup_hit_us,lookup_miss_us,iterate_ms,remove_ms,order" > "$OUT_FILE"

for SIZE in "${SIZES[@]}"; do
  echo "──────────────────────────────────────"
  echo "  Seeding $SIZE products..."

  # Clear existing products (delete all)
  # (comment this out if you want additive seeding)
  # curl -s -X DELETE "$BASE/api/products/all" > /dev/null

  curl -s -X POST "$BASE/api/seed?count=$SIZE" > /dev/null
  echo "  Running benchmark for $SIZE products..."

  RESULT=$(curl -s -X POST "$BASE/api/benchmark/run")

  # Parse rows from summary_table
  echo "$RESULT" | python3 - << 'PYEOF'
import sys, json, os

data = json.load(sys.stdin)
report = data.get("report", {})
size = report.get("product_count", 0)
out_file = "/tmp/benchmark_comparison.csv"

for row in report.get("summary_table", []):
    line = f"{size},{row['set_type']},{row['insert_ms']:.6f},{row['lookup_hit_us']:.3f},{row['lookup_miss_us']:.3f},{row['iterate_ms']:.6f},{row['remove_ms']:.6f},{row['order']}"
    with open(out_file, "a") as f:
        f.write(line + "\n")
    print(f"  [{row['set_type']:18s}] insert={row['insert_ms']:.3f}ms  lookup={row['lookup_hit_us']:.2f}µs  iterate={row['iterate_ms']:.3f}ms")

print(f"  Winners → insert:{report.get('winner_insert')}  lookup:{report.get('winner_lookup')}  iterate:{report.get('winner_iterate')}")
PYEOF

done

echo ""
echo "══════════════════════════════════════════"
echo "  Benchmark comparison saved to $OUT_FILE"
echo "══════════════════════════════════════════"

# Print summary table
echo ""
python3 << PYEOF
import csv

rows = []
with open("/tmp/benchmark_comparison.csv") as f:
    reader = csv.DictReader(f)
    for row in reader:
        rows.append(row)

# Group by set type
from collections import defaultdict
by_type = defaultdict(list)
for r in rows:
    by_type[r["set_type"]].append(r)

print(f"\n{'Size':>8}  {'Set Type':<20}  {'Insert(ms)':>12}  {'Lookup(µs)':>12}  {'Iterate(ms)':>12}")
print("─" * 75)
for size in sorted(set(r["size"] for r in rows), key=int):
    for r in rows:
        if r["size"] == size:
            print(f"{int(size):>8}  {r['set_type']:<20}  {float(r['insert_ms']):>12.3f}  {float(r['lookup_hit_us']):>12.3f}  {float(r['iterate_ms']):>12.3f}")
    print()
PYEOF
