CREATE TABLE IF NOT EXISTS runtime_session_execution_anchors (
    runtime_session_id TEXT PRIMARY KEY,
    run_id             UUID NOT NULL,
    launch_frame_id    UUID NOT NULL,
    agent_id           UUID NOT NULL,
    assignment_id      UUID,
    graph_instance_id  UUID,
    activity_key       TEXT,
    attempt            INT,
    created_by_kind    TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_rsea_agent ON runtime_session_execution_anchors(agent_id);
CREATE INDEX IF NOT EXISTS idx_rsea_run ON runtime_session_execution_anchors(run_id);
