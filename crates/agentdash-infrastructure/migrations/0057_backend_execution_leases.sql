CREATE TABLE IF NOT EXISTS backend_execution_leases (
    id TEXT PRIMARY KEY,
    backend_id TEXT NOT NULL REFERENCES backends(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    executor_id TEXT NOT NULL,
    workspace_id TEXT,
    root_ref TEXT,
    selection_mode TEXT NOT NULL,
    state TEXT NOT NULL,
    claim_reason TEXT,
    terminal_kind TEXT,
    release_reason TEXT,
    claimed_at TIMESTAMPTZ NOT NULL,
    activated_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ,
    last_seen_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(session_id, turn_id),
    CONSTRAINT backend_execution_leases_selection_mode_check CHECK (
        selection_mode IN ('explicit', 'auto_idle', 'workspace_binding')
    ),
    CONSTRAINT backend_execution_leases_state_check CHECK (
        state IN ('claimed', 'running', 'released', 'lost', 'failed')
    ),
    CONSTRAINT backend_execution_leases_terminal_kind_check CHECK (
        terminal_kind IS NULL OR terminal_kind IN ('completed', 'failed', 'interrupted')
    )
);

CREATE INDEX IF NOT EXISTS idx_backend_execution_leases_backend_state
    ON backend_execution_leases(backend_id, state);

CREATE INDEX IF NOT EXISTS idx_backend_execution_leases_active_backend
    ON backend_execution_leases(backend_id)
    WHERE state IN ('claimed', 'running');

CREATE INDEX IF NOT EXISTS idx_backend_execution_leases_session
    ON backend_execution_leases(session_id);
