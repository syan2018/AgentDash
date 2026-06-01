-- permission_grants: session_id → frame anchor 重构。
-- 主查询路径从 session_id 切换到 effect_frame_id（agent_frames.id），
-- session_id 重命名为 source_runtime_session_id 保留为审计追溯字段。

-- 1. 添加 effect_frame_id 列
ALTER TABLE permission_grants
  ADD COLUMN IF NOT EXISTS effect_frame_id TEXT;

-- 2. 重命名 session_id → source_runtime_session_id
ALTER TABLE permission_grants
  RENAME COLUMN session_id TO source_runtime_session_id;

-- 3. 删除旧索引
DROP INDEX IF EXISTS idx_permission_grants_session_active;

-- 4. 创建 frame anchor 索引
CREATE INDEX IF NOT EXISTS idx_permission_grants_frame_active
  ON permission_grants(effect_frame_id)
  WHERE status IN ('applied', 'scope_escalated');
