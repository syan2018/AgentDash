DROP INDEX IF EXISTS idx_agent_run_lineages_parent_runtime;
DROP INDEX IF EXISTS idx_agent_run_lineages_child_runtime;

ALTER TABLE agent_run_lineages
    DROP COLUMN IF EXISTS parent_runtime_session_id,
    DROP COLUMN IF EXISTS child_runtime_session_id;
