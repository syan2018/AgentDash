ALTER TABLE lifecycle_agents
    ADD COLUMN IF NOT EXISTS current_delivery_runtime_session_id text,
    ADD COLUMN IF NOT EXISTS current_delivery_launch_frame_id text,
    ADD COLUMN IF NOT EXISTS current_delivery_orchestration_id text,
    ADD COLUMN IF NOT EXISTS current_delivery_node_path text,
    ADD COLUMN IF NOT EXISTS current_delivery_node_attempt integer,
    ADD COLUMN IF NOT EXISTS current_delivery_status text,
    ADD COLUMN IF NOT EXISTS current_delivery_observed_at timestamp with time zone;

ALTER TABLE lifecycle_agents
    DROP CONSTRAINT IF EXISTS lifecycle_agents_current_delivery_status_check;

ALTER TABLE lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_current_delivery_status_check
    CHECK (
        current_delivery_status IS NULL
        OR current_delivery_status IN (
            'ready',
            'running',
            'terminal',
            'lost',
            'frame_missing',
            'delivery_missing'
        )
    );

CREATE INDEX IF NOT EXISTS idx_lifecycle_agents_current_delivery_runtime_session
    ON lifecycle_agents (current_delivery_runtime_session_id);

CREATE INDEX IF NOT EXISTS idx_lifecycle_agents_run_agent_current_delivery_runtime_session
    ON lifecycle_agents (run_id, id, current_delivery_runtime_session_id);
