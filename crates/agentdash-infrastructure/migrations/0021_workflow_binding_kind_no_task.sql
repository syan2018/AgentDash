-- 0021: Model C 收敛——WorkflowBindingKind 移除 "task" 变体
--
-- 背景：Task 不再作为独立 aggregate（见 Model C / 04-27-slim-runtime-layer-session-owner），
-- 合入 Story aggregate 后，task 级 lifecycle 统一由 Story-bound lifecycle 承载。
-- `WorkflowBindingKind` enum 从 {Project, Story, Task} 收敛为 {Project, Story}。
--
-- 本迁移职责：把现存 binding_kind='task' 的 workflow / lifecycle definition
-- 统一迁移到 binding_kind='story'，以便 Rust 侧 enum 能正常反序列化。
--
-- 幂等性：UPDATE ... WHERE 天然幂等；重跑只会命中空集。
--
-- 注意：这里 **不加** CHECK 约束——保留运行期 lifecycle_runs / 历史 JSON payload
-- 的灵活性；Rust 侧 enum 反序列化已是实际守门。

-- lifecycle_definitions.binding_kind 列（TEXT NOT NULL）
UPDATE lifecycle_definitions
SET binding_kind = 'story'
WHERE binding_kind = 'task';

-- workflow_definitions.binding_kind 列（TEXT NOT NULL）
UPDATE workflow_definitions
SET binding_kind = 'story'
WHERE binding_kind = 'task';

-- workflow_definitions.recommended_binding_roles 是 TEXT 存的 JSON 数组；
-- 将其中含 "task" 的元素替换为 "story"，并对已经存在 "story" 的做去重。
-- 风格：尽量用 jsonb 运算；本迁移假设字段内容是合法 JSON 数组。
-- 若历史数据里存在非法 JSON 文本，本段会失败，需要先手工修复坏数据再重跑迁移。
UPDATE workflow_definitions
SET recommended_binding_roles = (
    SELECT jsonb_agg(DISTINCT role)::text
    FROM (
        SELECT CASE WHEN elem = '"task"'::jsonb THEN '"story"'::jsonb ELSE elem END AS role
        FROM jsonb_array_elements((recommended_binding_roles::jsonb)) elem
    ) roles
)
WHERE recommended_binding_roles IS NOT NULL
  AND recommended_binding_roles <> ''
  AND (
      -- 仅处理包含 "task" 的记录，其余跳过
      (recommended_binding_roles::jsonb) @> '["task"]'::jsonb
  );
