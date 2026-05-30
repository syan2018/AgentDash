CREATE TABLE IF NOT EXISTS activity_execution_claims (
    claim_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    activity_key TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    executor_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    executor_run_ref TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_activity_execution_claims_run_id
    ON activity_execution_claims(run_id);

CREATE UNIQUE INDEX IF NOT EXISTS ux_activity_execution_claims_active_attempt
    ON activity_execution_claims(run_id, activity_key, attempt)
    WHERE status IN ('claiming', 'running');
