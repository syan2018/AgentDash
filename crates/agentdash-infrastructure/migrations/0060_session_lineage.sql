CREATE TABLE IF NOT EXISTS session_lineage (
    child_session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    parent_session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    relation_kind TEXT NOT NULL,
    fork_point_event_seq BIGINT,
    fork_point_ref_json TEXT NOT NULL DEFAULT '{}',
    fork_point_compaction_id TEXT REFERENCES session_compactions(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    CHECK (child_session_id <> parent_session_id)
);

CREATE INDEX IF NOT EXISTS idx_session_lineage_parent_status_kind
    ON session_lineage(parent_session_id, status, relation_kind, created_at_ms, child_session_id);

CREATE INDEX IF NOT EXISTS idx_session_lineage_fork_point
    ON session_lineage(parent_session_id, fork_point_event_seq, fork_point_compaction_id);
