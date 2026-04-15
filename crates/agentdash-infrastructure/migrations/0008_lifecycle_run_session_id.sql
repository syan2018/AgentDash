-- LifecycleRun: 从 binding_kind+binding_id 迁移到 session_id
-- lifecycle run 跟着父 session 走，不再直接绑定 Task/Story。

ALTER TABLE lifecycle_runs ADD COLUMN session_id TEXT NOT NULL DEFAULT '';

-- 回填：无法自动推导 session_id，旧数据标记为空串，新 run 正常填写。
-- 删掉旧列
ALTER TABLE lifecycle_runs DROP COLUMN binding_kind;
ALTER TABLE lifecycle_runs DROP COLUMN binding_id;
