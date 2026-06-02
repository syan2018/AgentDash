-- Phase 2: 移除 SessionBinding 模块
-- Session 降级为纯 runtime event stream 容器，不再需要 binding 表表达归属关系。
-- 业务归属全部通过 lifecycle_run_links 表达。

-- 给 sessions 表加 project_id 列（替代通过 session_bindings 间接查找的路径）
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS project_id TEXT;

-- 从 session_bindings 回填 project_id 到 sessions（取第一条 binding 的 project_id）
UPDATE sessions s
SET project_id = (
    SELECT sb.project_id
    FROM session_bindings sb
    WHERE sb.session_id = s.id
    LIMIT 1
)
WHERE s.project_id IS NULL;

-- 删除 session_bindings 表
DROP TABLE IF EXISTS session_bindings;
