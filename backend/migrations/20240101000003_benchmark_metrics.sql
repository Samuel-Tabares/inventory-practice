CREATE TABLE IF NOT EXISTS benchmark_runs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    product_count INTEGER NOT NULL,
    report_json  JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_benchmark_runs_at ON benchmark_runs(run_at);
