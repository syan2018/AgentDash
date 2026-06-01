-- Final hard cutover cleanup: the control plane starts from LifecycleRun →
-- LifecycleAgent → AgentFrame → RuntimeSession, with business subjects attached
-- through LifecycleSubjectAssociation.

DROP TABLE IF EXISTS lifecycle_run_links;
DROP TABLE IF EXISTS session_bindings;
DROP TABLE IF EXISTS workflow_assignments;

ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS session_id;
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS binding_kind;
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS binding_id;
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS current_step_key;
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS step_states;
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS port_outputs;

ALTER TABLE tasks DROP COLUMN IF EXISTS session_id;
ALTER TABLE tasks DROP COLUMN IF EXISTS executor_session_id;

ALTER TABLE agent_procedures DROP COLUMN IF EXISTS binding_kind;
ALTER TABLE agent_procedures DROP COLUMN IF EXISTS binding_kinds;
ALTER TABLE agent_procedures DROP COLUMN IF EXISTS recommended_binding_roles;
ALTER TABLE agent_procedures DROP COLUMN IF EXISTS status;

ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS binding_kind;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS binding_kinds;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS recommended_binding_roles;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS status;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS entry_step_key;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS steps;
ALTER TABLE workflow_graphs DROP COLUMN IF EXISTS edges;
