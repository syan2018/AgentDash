-- 0023: 修复 migration 0021 的 WHERE 条件 bug
--
-- 问题：workflow_repository.rs 使用 serde_json::to_string(&binding_kind) 存储，
-- 产生 JSON 编码字符串，即 "task"（含双引号，6字节）。
-- 但 0021 的 WHERE binding_kind = 'task' 匹配的是 4字节无引号字符串，
-- 导致 0021 的 UPDATE 命中空集，旧 task 记录未被迁移，读取时反序列化报错。
--
-- 本迁移用正确的 JSON 编码格式做匹配，将残留的 "task" 记录统一改为 "story"。
-- 同时补齐 0021 漏掉的 lifecycle_definitions.recommended_binding_roles 更新。
--
-- 幂等性：UPDATE ... WHERE 天然幂等；重跑只会命中空集。

-- 1. lifecycle_definitions.binding_kind（0021 漏补）
UPDATE lifecycle_definitions
SET binding_kind = '"story"'
WHERE binding_kind = '"task"';

-- 2. workflow_definitions.binding_kind（0021 漏补）
UPDATE workflow_definitions
SET binding_kind = '"story"'
WHERE binding_kind = '"task"';

-- 3. lifecycle_definitions.recommended_binding_roles（0021 完全漏处理）
--    数组元素里可能有 "task"，替换为 "story" 并去重。
UPDATE lifecycle_definitions
SET recommended_binding_roles = (
    SELECT jsonb_agg(DISTINCT role)::text
    FROM (
        SELECT CASE WHEN elem = '"task"'::jsonb THEN '"story"'::jsonb ELSE elem END AS role
        FROM jsonb_array_elements((recommended_binding_roles::jsonb)) elem
    ) roles
)
WHERE recommended_binding_roles IS NOT NULL
  AND recommended_binding_roles <> ''
  AND (recommended_binding_roles::jsonb) @> '["task"]'::jsonb;

-- 4. workflow_definitions.recommended_binding_roles（0021 已处理，本次幂等重跑兜底）
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
  AND (recommended_binding_roles::jsonb) @> '["task"]'::jsonb;
