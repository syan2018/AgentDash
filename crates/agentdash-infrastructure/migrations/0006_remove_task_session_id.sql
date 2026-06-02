-- Phase 0A: 从 tasks 表移除 session_id 列
-- Session 归属统一通过 session_bindings 表管理 (owner_type='task', label='execution')
DROP INDEX IF EXISTS idx_tasks_session_id;
ALTER TABLE tasks DROP COLUMN IF EXISTS session_id;
