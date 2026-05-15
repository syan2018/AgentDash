CREATE TABLE IF NOT EXISTS session_runtime_commands (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    transition_id TEXT NOT NULL,
    phase_node TEXT NOT NULL,
    status TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    applied_at_ms INTEGER,
    failed_at_ms INTEGER,
    last_error TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_session_runtime_commands_status_updated
    ON session_runtime_commands(status, updated_at_ms);

CREATE INDEX IF NOT EXISTS idx_session_runtime_commands_session_status
    ON session_runtime_commands(session_id, status);
