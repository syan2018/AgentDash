DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'runtime_health'
          AND column_name = 'accessible_roots'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'runtime_health'
          AND column_name = 'workspace_roots'
    ) THEN
        ALTER TABLE runtime_health
            RENAME COLUMN accessible_roots TO workspace_roots;
    END IF;
END $$;
