-- Routine dispatch policy is part of the lifecycle control plane vocabulary.

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'routines'
          AND column_name = 'session_strategy'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'routines'
          AND column_name = 'dispatch_strategy'
    ) THEN
        ALTER TABLE routines RENAME COLUMN session_strategy TO dispatch_strategy;
    END IF;
END $$;
