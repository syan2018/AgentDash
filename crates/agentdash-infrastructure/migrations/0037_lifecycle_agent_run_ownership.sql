ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS created_by_user_id text DEFAULT 'system'::text NOT NULL;

ALTER TABLE lifecycle_agents
    ADD COLUMN IF NOT EXISTS created_by_user_id text DEFAULT 'system'::text NOT NULL;
