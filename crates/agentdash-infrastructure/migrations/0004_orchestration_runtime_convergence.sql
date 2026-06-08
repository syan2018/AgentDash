ALTER TABLE runtime_session_execution_anchors
    ADD COLUMN IF NOT EXISTS orchestration_id text,
    ADD COLUMN IF NOT EXISTS node_path text,
    ADD COLUMN IF NOT EXISTS node_attempt integer;

ALTER TABLE lifecycle_runs
    DROP CONSTRAINT IF EXISTS lifecycle_runs_topology_root_graph_check,
    DROP COLUMN IF EXISTS root_graph_id;

ALTER TABLE agent_frames
    DROP COLUMN IF EXISTS procedure_id;

ALTER TABLE runtime_session_execution_anchors
    DROP COLUMN IF EXISTS assignment_id,
    DROP COLUMN IF EXISTS graph_instance_id,
    DROP COLUMN IF EXISTS activity_key,
    DROP COLUMN IF EXISTS attempt;

ALTER TABLE routine_executions
    ADD COLUMN IF NOT EXISTS dispatch_orchestration_id text,
    ADD COLUMN IF NOT EXISTS dispatch_node_path text;

DROP INDEX IF EXISTS idx_routine_exec_dispatch_assignment;

ALTER TABLE routine_executions
    DROP COLUMN IF EXISTS dispatch_assignment_id;

DROP TABLE IF EXISTS activity_execution_claims;
DROP TABLE IF EXISTS agent_assignments;
DROP TABLE IF EXISTS lifecycle_workflow_instances;
