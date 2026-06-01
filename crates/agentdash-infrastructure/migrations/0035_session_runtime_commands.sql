CREATE TABLE IF NOT EXISTS agent_frame_transitions (
    id TEXT PRIMARY KEY,
    target_frame_id TEXT NOT NULL REFERENCES agent_frames(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    lifecycle_key TEXT NOT NULL,
    phase_node TEXT NOT NULL,
    capability_keys_json TEXT NOT NULL,
    transition_json TEXT NOT NULL,
    source_turn_id TEXT,
    created_at_ms BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_frame_transitions_target_frame
    ON agent_frame_transitions(target_frame_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_agent_frame_transitions_run_phase
    ON agent_frame_transitions(run_id, lifecycle_key, phase_node);

CREATE TABLE IF NOT EXISTS session_runtime_commands (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    frame_transition_id TEXT NOT NULL,
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

CREATE INDEX IF NOT EXISTS idx_session_runtime_commands_frame_transition
    ON session_runtime_commands(frame_transition_id);
