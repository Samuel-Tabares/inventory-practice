use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One recorded operation timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEntry {
    pub timestamp: DateTime<Utc>,
    pub operation: String,   // "insert" | "lookup" | "remove" | "iterate" | "db_query"
    pub set_type: String,    // "HashSet" | "LinkedHashSet" | "BTreeSet" | "DB"
    pub duration_ns: u64,
    pub duration_us: f64,
    pub duration_ms: f64,
    pub item_count: usize,
    pub success: bool,
    pub notes: Option<String>,
}

impl MetricEntry {
    pub fn new(
        operation: impl Into<String>,
        set_type: impl Into<String>,
        duration_ns: u64,
        item_count: usize,
        success: bool,
        notes: Option<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            operation: operation.into(),
            set_type: set_type.into(),
            duration_ns,
            duration_us: duration_ns as f64 / 1_000.0,
            duration_ms: duration_ns as f64 / 1_000_000.0,
            item_count,
            success,
            notes,
        }
    }
}

/// In-memory store for all timing entries collected across requests.
#[derive(Debug, Default)]
pub struct MetricsStore {
    pub entries: Vec<MetricEntry>,
}

impl MetricsStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, entry: MetricEntry) {
        self.entries.push(entry);
    }

    pub fn record_raw(
        &mut self,
        operation: impl Into<String>,
        set_type: impl Into<String>,
        duration_ns: u64,
        item_count: usize,
    ) {
        self.record(MetricEntry::new(
            operation,
            set_type,
            duration_ns,
            item_count,
            true,
            None,
        ));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Aggregate stats per (operation, set_type) pair.
    pub fn aggregated(&self) -> Vec<AggregatedMetric> {
        let mut map: HashMap<(String, String), Vec<u64>> = HashMap::new();

        for e in &self.entries {
            map.entry((e.operation.clone(), e.set_type.clone()))
                .or_default()
                .push(e.duration_ns);
        }

        let mut out: Vec<AggregatedMetric> = map
            .into_iter()
            .map(|((op, st), durations)| {
                let count = durations.len();
                let total: u64 = durations.iter().sum();
                let avg = total / count as u64;
                let mut sorted = durations.clone();
                sorted.sort_unstable();
                let min = *sorted.first().unwrap_or(&0);
                let max = *sorted.last().unwrap_or(&0);
                let p50 = sorted[count / 2];
                let p95 = sorted[((count as f64 * 0.95) as usize).min(count.saturating_sub(1))];
                let p99 = sorted[((count as f64 * 0.99) as usize).min(count.saturating_sub(1))];

                AggregatedMetric {
                    operation: op,
                    set_type: st,
                    sample_count: count,
                    min_ns: min,
                    max_ns: max,
                    avg_ns: avg,
                    p50_ns: p50,
                    p95_ns: p95,
                    p99_ns: p99,
                    avg_ms: avg as f64 / 1_000_000.0,
                    p95_ms: p95 as f64 / 1_000_000.0,
                }
            })
            .collect();

        out.sort_by(|a, b| a.operation.cmp(&b.operation).then(a.set_type.cmp(&b.set_type)));
        out
    }

    /// Export all entries as a CSV string.
    pub fn to_csv(&self) -> anyhow::Result<String> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record([
            "timestamp",
            "operation",
            "set_type",
            "duration_ns",
            "duration_us",
            "duration_ms",
            "item_count",
            "success",
            "notes",
        ])?;

        for e in &self.entries {
            wtr.write_record([
                e.timestamp.to_rfc3339(),
                e.operation.clone(),
                e.set_type.clone(),
                e.duration_ns.to_string(),
                format!("{:.3}", e.duration_us),
                format!("{:.6}", e.duration_ms),
                e.item_count.to_string(),
                e.success.to_string(),
                e.notes.clone().unwrap_or_default(),
            ])?;
        }

        let data = wtr.into_inner()?;
        Ok(String::from_utf8(data)?)
    }

    /// Render a simple ASCII comparison table.
    pub fn ascii_table(&self) -> String {
        let agg = self.aggregated();
        if agg.is_empty() {
            return "No metrics collected yet.".to_string();
        }

        let mut out = String::new();
        out.push_str(&format!(
            "\n{:<20} {:<18} {:>12} {:>12} {:>12} {:>12} {:>12}\n",
            "Operation", "Set Type", "Samples", "Avg (µs)", "P50 (µs)", "P95 (µs)", "P99 (µs)"
        ));
        out.push_str(&"-".repeat(102));
        out.push('\n');

        for row in &agg {
            out.push_str(&format!(
                "{:<20} {:<18} {:>12} {:>12.2} {:>12.2} {:>12.2} {:>12.2}\n",
                row.operation,
                row.set_type,
                row.sample_count,
                row.avg_ns as f64 / 1_000.0,
                row.p50_ns as f64 / 1_000.0,
                row.p95_ns as f64 / 1_000.0,
                row.p99_ns as f64 / 1_000.0,
            ));
        }

        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetric {
    pub operation: String,
    pub set_type: String,
    pub sample_count: usize,
    pub min_ns: u64,
    pub max_ns: u64,
    pub avg_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub avg_ms: f64,
    pub p95_ms: f64,
}
