DO $$
DECLARE
    has_legacy_hook_plan boolean;
    has_hook_plan boolean;
BEGIN
    SELECT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'agent_frames'
          AND column_name = 'hook_plan_json'
    ) INTO has_legacy_hook_plan;

    SELECT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'agent_frames'
          AND column_name = 'hook_plan'
    ) INTO has_hook_plan;

    IF has_legacy_hook_plan AND has_hook_plan THEN
        RAISE EXCEPTION 'agent_frames has both hook_plan_json and hook_plan';
    ELSIF has_legacy_hook_plan THEN
        ALTER TABLE agent_frames RENAME COLUMN hook_plan_json TO hook_plan;
    ELSIF NOT has_hook_plan THEN
        RAISE EXCEPTION 'agent_frames HookPlan source column is missing';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'agent_frames'
          AND column_name = 'hook_plan'
          AND data_type = 'jsonb'
    ) THEN
        RAISE EXCEPTION 'agent_frames.hook_plan must use jsonb';
    END IF;
END $$;
