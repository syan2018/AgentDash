DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'runtime_session_execution_anchors_runtime_session_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_execution_anchors
            ADD CONSTRAINT runtime_session_execution_anchors_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES sessions(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'runtime_session_execution_anchors_run_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_execution_anchors
            ADD CONSTRAINT runtime_session_execution_anchors_run_id_fkey
            FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'runtime_session_execution_anchors_agent_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_execution_anchors
            ADD CONSTRAINT runtime_session_execution_anchors_agent_id_fkey
            FOREIGN KEY (agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'runtime_session_execution_anchors_launch_frame_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_execution_anchors
            ADD CONSTRAINT runtime_session_execution_anchors_launch_frame_id_fkey
            FOREIGN KEY (launch_frame_id) REFERENCES agent_frames(id) ON DELETE CASCADE;
    END IF;
END $$;
