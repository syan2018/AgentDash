-- Procedure/graph selection is represented by LifecycleSubjectAssociation.

DO $$
BEGIN
    IF to_regclass('public.agent_procedures') IS NULL
       AND to_regclass('public.workflow_definitions') IS NOT NULL THEN
        ALTER TABLE workflow_definitions RENAME TO agent_procedures;
    END IF;

    IF to_regclass('public.workflow_graphs') IS NULL
       AND to_regclass('public.lifecycle_definitions') IS NOT NULL THEN
        ALTER TABLE lifecycle_definitions RENAME TO workflow_graphs;
    END IF;
END $$;

ALTER TABLE agent_procedures DROP COLUMN IF EXISTS binding_kinds;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS binding_kinds;
