CREATE TABLE IF NOT EXISTS session_compactions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    branch_id TEXT NOT NULL DEFAULT '',
    projection_kind TEXT NOT NULL,
    projection_version BIGINT NOT NULL,
    lifecycle_item_id TEXT NOT NULL,
    start_event_seq BIGINT NOT NULL,
    completed_event_seq BIGINT,
    failed_event_seq BIGINT,
    status TEXT NOT NULL,
    trigger TEXT NOT NULL,
    reason TEXT,
    phase TEXT,
    strategy TEXT NOT NULL,
    budget_scope TEXT,
    base_head_event_seq BIGINT,
    source_start_event_seq BIGINT,
    source_end_event_seq BIGINT,
    first_kept_event_seq BIGINT,
    summary TEXT NOT NULL DEFAULT '',
    replacement_projection_json TEXT NOT NULL DEFAULT '{}',
    token_stats_json TEXT NOT NULL DEFAULT '{}',
    diagnostics_json TEXT NOT NULL DEFAULT '{}',
    created_by TEXT,
    created_at_ms BIGINT NOT NULL,
    completed_at_ms BIGINT
);

CREATE INDEX IF NOT EXISTS idx_session_compactions_session_branch_kind_status
    ON session_compactions(session_id, branch_id, projection_kind, status, projection_version);

CREATE INDEX IF NOT EXISTS idx_session_compactions_lifecycle_item
    ON session_compactions(session_id, lifecycle_item_id);

CREATE INDEX IF NOT EXISTS idx_session_compactions_source_range
    ON session_compactions(session_id, branch_id, source_start_event_seq, source_end_event_seq);

CREATE TABLE IF NOT EXISTS session_projection_segments (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    branch_id TEXT NOT NULL DEFAULT '',
    projection_kind TEXT NOT NULL,
    projection_version BIGINT NOT NULL,
    sort_order BIGINT NOT NULL,
    segment_type TEXT NOT NULL,
    origin TEXT NOT NULL,
    synthetic BOOLEAN NOT NULL DEFAULT FALSE,
    source_start_event_seq BIGINT,
    source_end_event_seq BIGINT,
    source_refs_json TEXT NOT NULL DEFAULT '[]',
    generated_by_compaction_id TEXT REFERENCES session_compactions(id) ON DELETE SET NULL,
    content_json TEXT NOT NULL,
    token_estimate BIGINT,
    created_at_ms BIGINT NOT NULL,
    UNIQUE(session_id, branch_id, projection_kind, projection_version, sort_order)
);

CREATE INDEX IF NOT EXISTS idx_session_projection_segments_projection
    ON session_projection_segments(session_id, branch_id, projection_kind, projection_version, sort_order);

CREATE INDEX IF NOT EXISTS idx_session_projection_segments_source_range
    ON session_projection_segments(session_id, branch_id, source_start_event_seq, source_end_event_seq);

CREATE TABLE IF NOT EXISTS session_projection_heads (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    branch_id TEXT NOT NULL DEFAULT '',
    projection_kind TEXT NOT NULL,
    projection_version BIGINT NOT NULL,
    head_event_seq BIGINT NOT NULL,
    active_compaction_id TEXT REFERENCES session_compactions(id) ON DELETE SET NULL,
    updated_by_event_seq BIGINT,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (session_id, branch_id, projection_kind)
);

CREATE INDEX IF NOT EXISTS idx_session_projection_heads_active_compaction
    ON session_projection_heads(session_id, active_compaction_id);
