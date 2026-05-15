CREATE TABLE IF NOT EXISTS session_terminal_effects (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    terminal_event_seq INTEGER NOT NULL,
    effect_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    last_error TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_session_terminal_effects_status_updated
    ON session_terminal_effects(status, updated_at_ms);

CREATE INDEX IF NOT EXISTS idx_session_terminal_effects_session_turn
    ON session_terminal_effects(session_id, turn_id);

CREATE INDEX IF NOT EXISTS idx_session_terminal_effects_terminal_event
    ON session_terminal_effects(session_id, terminal_event_seq);
