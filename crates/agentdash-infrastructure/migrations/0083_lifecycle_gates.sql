-- Lifecycle gates are durable wait/review/resume anchors scoped by run/agent/frame.
-- The clean baseline already creates this target schema; this migration keeps
-- existing dev databases aligned with the same columns and indexes.

CREATE TABLE IF NOT EXISTS lifecycle_gates (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id TEXT,
    frame_id TEXT,
    gate_kind TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    payload_json TEXT,
    resolved_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMPTZ
);

ALTER TABLE lifecycle_gates
    ADD COLUMN IF NOT EXISTS frame_id TEXT;
ALTER TABLE lifecycle_gates
    ADD COLUMN IF NOT EXISTS correlation_id TEXT NOT NULL DEFAULT '';
ALTER TABLE lifecycle_gates
    ADD COLUMN IF NOT EXISTS payload_json TEXT;
ALTER TABLE lifecycle_gates
    ADD COLUMN IF NOT EXISTS resolved_by TEXT;

UPDATE lifecycle_gates
SET status = 'open'
WHERE status = 'pending';

DROP INDEX IF EXISTS idx_lifecycle_gates_pending;

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_run_id
    ON lifecycle_gates(run_id);

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_agent_status
    ON lifecycle_gates(agent_id, status)
    WHERE agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_correlation
    ON lifecycle_gates(correlation_id);
