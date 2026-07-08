ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS channel_registry jsonb NOT NULL DEFAULT '{}'::jsonb;
