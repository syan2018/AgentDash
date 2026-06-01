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

DELETE FROM session_runtime_commands;

ALTER TABLE session_runtime_commands
    ADD COLUMN IF NOT EXISTS frame_transition_id TEXT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'session_runtime_commands'
          AND column_name = 'transition_id'
    ) THEN
        UPDATE session_runtime_commands
        SET frame_transition_id = transition_id
        WHERE frame_transition_id IS NULL;
        ALTER TABLE session_runtime_commands DROP COLUMN transition_id;
    END IF;
END $$;

ALTER TABLE session_runtime_commands
    ALTER COLUMN frame_transition_id SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_session_runtime_commands_frame_transition'
    ) THEN
        ALTER TABLE session_runtime_commands
            ADD CONSTRAINT fk_session_runtime_commands_frame_transition
            FOREIGN KEY (frame_transition_id)
            REFERENCES agent_frame_transitions(id)
            ON DELETE CASCADE;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_session_runtime_commands_frame_transition
    ON session_runtime_commands(frame_transition_id);
