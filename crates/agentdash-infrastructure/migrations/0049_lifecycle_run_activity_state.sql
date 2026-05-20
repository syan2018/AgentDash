ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS activity_state TEXT;
