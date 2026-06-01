-- 0081: Drop deprecated binding_kinds column
-- WorkflowBindingKind replaced by LifecycleSubjectAssociation

ALTER TABLE agent_procedures DROP COLUMN IF EXISTS binding_kinds;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS binding_kinds;
