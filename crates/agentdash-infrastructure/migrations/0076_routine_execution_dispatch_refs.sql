-- routine_executions: session_id → dispatch refs 迁移
-- Breaking-mode: 直接替换字段，不保留兼容

ALTER TABLE routine_executions ADD COLUMN dispatch_run_id TEXT;
ALTER TABLE routine_executions ADD COLUMN dispatch_agent_id TEXT;
ALTER TABLE routine_executions ADD COLUMN dispatch_frame_id TEXT;

-- 将旧 running/completed 状态统一迁移为 dispatched
UPDATE routine_executions SET status = 'dispatched' WHERE status IN ('running', 'completed');

ALTER TABLE routine_executions DROP COLUMN IF EXISTS session_id;

CREATE INDEX IF NOT EXISTS idx_routine_exec_dispatch_run
    ON routine_executions(dispatch_run_id) WHERE dispatch_run_id IS NOT NULL;
