ALTER TABLE lifecycle_runs
    DROP COLUMN IF EXISTS context,
    DROP COLUMN IF EXISTS view_projection;
