ALTER TABLE routine_executions
    ADD COLUMN dispatch_mailbox jsonb;

CREATE INDEX idx_routine_exec_recoverable
    ON routine_executions (started_at)
    WHERE status = 'pending' AND dispatch_run_id IS NOT NULL;

CREATE INDEX idx_routine_exec_runtime_operation
    ON routine_executions ((dispatch_mailbox->>'runtime_operation_id'))
    WHERE dispatch_mailbox->>'runtime_operation_id' IS NOT NULL;
