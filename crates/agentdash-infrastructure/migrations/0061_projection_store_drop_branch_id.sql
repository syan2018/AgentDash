DROP INDEX IF EXISTS idx_session_compactions_session_branch_kind_status;
DROP INDEX IF EXISTS idx_session_compactions_source_range;
DROP INDEX IF EXISTS idx_session_projection_segments_projection;
DROP INDEX IF EXISTS idx_session_projection_segments_source_range;

ALTER TABLE session_compactions
    DROP COLUMN IF EXISTS branch_id;

ALTER TABLE session_projection_segments
    DROP COLUMN IF EXISTS branch_id;

ALTER TABLE session_projection_heads
    DROP COLUMN IF EXISTS branch_id;

CREATE INDEX IF NOT EXISTS idx_session_compactions_session_kind_status
    ON session_compactions(session_id, projection_kind, status, projection_version);

CREATE INDEX IF NOT EXISTS idx_session_compactions_source_range
    ON session_compactions(session_id, source_start_event_seq, source_end_event_seq);

ALTER TABLE session_projection_segments
    ADD CONSTRAINT session_projection_segments_session_kind_version_order_key
    UNIQUE(session_id, projection_kind, projection_version, sort_order);

CREATE INDEX IF NOT EXISTS idx_session_projection_segments_projection
    ON session_projection_segments(session_id, projection_kind, projection_version, sort_order);

CREATE INDEX IF NOT EXISTS idx_session_projection_segments_source_range
    ON session_projection_segments(session_id, source_start_event_seq, source_end_event_seq);

ALTER TABLE session_projection_heads
    ADD PRIMARY KEY (session_id, projection_kind);

CREATE INDEX IF NOT EXISTS idx_session_projection_heads_active_compaction
    ON session_projection_heads(session_id, active_compaction_id);
