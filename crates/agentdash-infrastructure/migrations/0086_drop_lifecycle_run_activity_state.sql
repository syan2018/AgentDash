ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS active_node_keys TEXT NOT NULL DEFAULT '[]';

ALTER TABLE lifecycle_runs
    DROP COLUMN IF EXISTS activity_state;
