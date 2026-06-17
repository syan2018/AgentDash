ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS tasks text DEFAULT '[]'::text NOT NULL;

ALTER TABLE stories
    DROP COLUMN IF EXISTS tasks,
    DROP COLUMN IF EXISTS task_count;
