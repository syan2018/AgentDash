-- Procedure/graph selection is represented by LifecycleSubjectAssociation.

ALTER TABLE agent_procedures DROP COLUMN IF EXISTS binding_kinds;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS binding_kinds;
