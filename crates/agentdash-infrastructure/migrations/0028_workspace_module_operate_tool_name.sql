-- Workspace module 平台操作工具收束为 operate 入口。
--
-- ProjectAgent config 存为 JSON text；这里延续 0009 的结构化改写方式，
-- 只迁移 capability_directives 中已经写入的 workspace_module create 工具 ID。

WITH rewritten AS (
    SELECT
        id,
        jsonb_set(
            config::jsonb,
            '{capability_directives}',
            (
                SELECT COALESCE(jsonb_agg(
                    CASE
                        WHEN directive ? 'add' THEN
                            jsonb_set(
                                directive,
                                '{add}',
                                to_jsonb(
                                    CASE directive->>'add'
                                        WHEN 'workspace_module::workspace_module_create' THEN 'workspace_module::workspace_module_operate'
                                        ELSE directive->>'add'
                                    END
                                ),
                                false
                            )
                        WHEN directive ? 'remove' THEN
                            jsonb_set(
                                directive,
                                '{remove}',
                                to_jsonb(
                                    CASE directive->>'remove'
                                        WHEN 'workspace_module::workspace_module_create' THEN 'workspace_module::workspace_module_operate'
                                        ELSE directive->>'remove'
                                    END
                                ),
                                false
                            )
                        ELSE directive
                    END
                ), '[]'::jsonb)
                FROM jsonb_array_elements(config::jsonb->'capability_directives') AS directive
            ),
            false
        )::text AS next_config
    FROM project_agents
    WHERE config::jsonb ? 'capability_directives'
      AND jsonb_typeof(config::jsonb->'capability_directives') = 'array'
)
UPDATE project_agents
SET config = rewritten.next_config
FROM rewritten
WHERE project_agents.id = rewritten.id
  AND project_agents.config IS DISTINCT FROM rewritten.next_config;
