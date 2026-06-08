ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS context text DEFAULT '{}'::text NOT NULL,
    ADD COLUMN IF NOT EXISTS orchestrations text DEFAULT '[]'::text NOT NULL,
    ADD COLUMN IF NOT EXISTS view_projection text;
