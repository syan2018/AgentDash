CREATE TABLE IF NOT EXISTS agent_run_delivery_bindings (
    run_id text NOT NULL,
    agent_id text NOT NULL,
    runtime_session_id text NOT NULL,
    launch_frame_id text NOT NULL,
    orchestration_id text,
    node_path text,
    node_attempt integer,
    status text NOT NULL,
    observed_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    CONSTRAINT agent_run_delivery_bindings_pkey PRIMARY KEY (run_id, agent_id),
    CONSTRAINT agent_run_delivery_bindings_status_check CHECK (
        status IN (
            'ready',
            'running',
            'terminal',
            'lost',
            'frame_missing',
            'delivery_missing'
        )
    ),
    CONSTRAINT agent_run_delivery_bindings_node_coordinate_check CHECK (
        (
            orchestration_id IS NULL
            AND node_path IS NULL
            AND node_attempt IS NULL
        )
        OR (
            orchestration_id IS NOT NULL
            AND node_path IS NOT NULL
            AND node_attempt IS NOT NULL
        )
    )
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'runtime_session_execution_anchors_runtime_session_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_execution_anchors
            DROP CONSTRAINT runtime_session_execution_anchors_runtime_session_id_fkey;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'runtime_session_execution_anchors_runtime_session_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_execution_anchors
            ADD CONSTRAINT runtime_session_execution_anchors_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES sessions(id) ON DELETE RESTRICT;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'agent_run_delivery_bindings_runtime_session_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_delivery_bindings
            DROP CONSTRAINT agent_run_delivery_bindings_runtime_session_id_fkey;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'agent_run_delivery_bindings_run_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_delivery_bindings
            ADD CONSTRAINT agent_run_delivery_bindings_run_id_fkey
            FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'agent_run_delivery_bindings_agent_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_delivery_bindings
            ADD CONSTRAINT agent_run_delivery_bindings_agent_id_fkey
            FOREIGN KEY (agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'agent_run_delivery_bindings_runtime_session_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_delivery_bindings
            ADD CONSTRAINT agent_run_delivery_bindings_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES sessions(id) ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'agent_run_delivery_bindings_launch_frame_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_delivery_bindings
            ADD CONSTRAINT agent_run_delivery_bindings_launch_frame_id_fkey
            FOREIGN KEY (launch_frame_id) REFERENCES agent_frames(id) ON DELETE RESTRICT;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_agent_run_delivery_bindings_runtime_session
    ON agent_run_delivery_bindings (runtime_session_id);

CREATE INDEX IF NOT EXISTS idx_agent_run_delivery_bindings_run_updated
    ON agent_run_delivery_bindings (run_id, updated_at DESC);

INSERT INTO agent_run_delivery_bindings (
    run_id,
    agent_id,
    runtime_session_id,
    launch_frame_id,
    orchestration_id,
    node_path,
    node_attempt,
    status,
    observed_at,
    updated_at
)
SELECT
    agent.run_id,
    agent.id,
    agent.current_delivery_runtime_session_id,
    agent.current_delivery_launch_frame_id,
    agent.current_delivery_orchestration_id,
    agent.current_delivery_node_path,
    agent.current_delivery_node_attempt,
    COALESCE(agent.current_delivery_status, 'ready'),
    COALESCE(agent.current_delivery_observed_at, agent.updated_at, now()),
    COALESCE(agent.updated_at, agent.current_delivery_observed_at, now())
FROM lifecycle_agents AS agent
WHERE agent.current_delivery_runtime_session_id IS NOT NULL
  AND agent.current_delivery_launch_frame_id IS NOT NULL
  AND (
      (
          agent.current_delivery_orchestration_id IS NULL
          AND agent.current_delivery_node_path IS NULL
          AND agent.current_delivery_node_attempt IS NULL
      )
      OR (
          agent.current_delivery_orchestration_id IS NOT NULL
          AND agent.current_delivery_node_path IS NOT NULL
          AND agent.current_delivery_node_attempt IS NOT NULL
      )
  )
ON CONFLICT (run_id, agent_id) DO UPDATE SET
    runtime_session_id = EXCLUDED.runtime_session_id,
    launch_frame_id = EXCLUDED.launch_frame_id,
    orchestration_id = EXCLUDED.orchestration_id,
    node_path = EXCLUDED.node_path,
    node_attempt = EXCLUDED.node_attempt,
    status = EXCLUDED.status,
    observed_at = EXCLUDED.observed_at,
    updated_at = EXCLUDED.updated_at;

DROP INDEX IF EXISTS idx_lifecycle_agents_current_delivery_runtime_session;
DROP INDEX IF EXISTS idx_lifecycle_agents_run_agent_current_delivery_runtime_session;

ALTER TABLE lifecycle_agents
    DROP CONSTRAINT IF EXISTS lifecycle_agents_current_delivery_status_check;

ALTER TABLE lifecycle_agents
    DROP COLUMN IF EXISTS current_delivery_runtime_session_id,
    DROP COLUMN IF EXISTS current_delivery_launch_frame_id,
    DROP COLUMN IF EXISTS current_delivery_orchestration_id,
    DROP COLUMN IF EXISTS current_delivery_node_path,
    DROP COLUMN IF EXISTS current_delivery_node_attempt,
    DROP COLUMN IF EXISTS current_delivery_status,
    DROP COLUMN IF EXISTS current_delivery_observed_at;
