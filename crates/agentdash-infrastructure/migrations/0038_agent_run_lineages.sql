CREATE TABLE IF NOT EXISTS agent_run_lineages (
    id text PRIMARY KEY,
    parent_run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    parent_agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    child_run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    child_agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    relation_kind text NOT NULL,
    fork_point_event_seq bigint,
    fork_point_ref text,
    parent_runtime_session_id text NOT NULL,
    child_runtime_session_id text NOT NULL,
    forked_by_user_id text NOT NULL,
    metadata text,
    created_at timestamp with time zone NOT NULL,
    CONSTRAINT agent_run_lineages_relation_kind_check CHECK (relation_kind = 'fork'),
    CONSTRAINT agent_run_lineages_distinct_run_check CHECK (parent_run_id <> child_run_id),
    CONSTRAINT agent_run_lineages_child_unique UNIQUE (child_run_id, child_agent_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_run_lineages_parent
    ON agent_run_lineages(parent_run_id, parent_agent_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_agent_run_lineages_child
    ON agent_run_lineages(child_run_id, child_agent_id);

CREATE INDEX IF NOT EXISTS idx_agent_run_lineages_parent_runtime
    ON agent_run_lineages(parent_runtime_session_id);

CREATE INDEX IF NOT EXISTS idx_agent_run_lineages_child_runtime
    ON agent_run_lineages(child_runtime_session_id);
