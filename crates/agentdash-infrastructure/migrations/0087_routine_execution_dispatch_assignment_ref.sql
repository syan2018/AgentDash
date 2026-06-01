-- routine_executions: dispatch refs include AgentAssignment execution evidence

ALTER TABLE routine_executions ADD COLUMN dispatch_assignment_id TEXT;

CREATE INDEX IF NOT EXISTS idx_routine_exec_dispatch_assignment
    ON routine_executions(dispatch_assignment_id) WHERE dispatch_assignment_id IS NOT NULL;
