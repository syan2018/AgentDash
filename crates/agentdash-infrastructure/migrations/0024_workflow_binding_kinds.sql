ALTER TABLE workflow_definitions
ADD COLUMN IF NOT EXISTS binding_kinds TEXT NOT NULL DEFAULT '["story"]';

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'workflow_definitions'
          AND column_name = 'binding_kind'
    ) THEN
        UPDATE workflow_definitions
        SET binding_kinds = CASE
            WHEN binding_kind IN ('project', '"project"') THEN '["project"]'
            WHEN binding_kind IN ('story', '"story"', 'task', '"task"') THEN '["story"]'
            ELSE '["story"]'
        END;
    END IF;
END $$;

ALTER TABLE workflow_definitions DROP COLUMN IF EXISTS recommended_binding_roles;
ALTER TABLE workflow_definitions DROP COLUMN IF EXISTS binding_kind;

ALTER TABLE lifecycle_definitions
ADD COLUMN IF NOT EXISTS binding_kinds TEXT NOT NULL DEFAULT '["story"]';

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'lifecycle_definitions'
          AND column_name = 'binding_kind'
    ) THEN
        UPDATE lifecycle_definitions
        SET binding_kinds = CASE
            WHEN binding_kind IN ('project', '"project"') THEN '["project"]'
            WHEN binding_kind IN ('story', '"story"', 'task', '"task"') THEN '["story"]'
            ELSE '["story"]'
        END;
    END IF;
END $$;

ALTER TABLE lifecycle_definitions DROP COLUMN IF EXISTS recommended_binding_roles;
ALTER TABLE lifecycle_definitions DROP COLUMN IF EXISTS binding_kind;
