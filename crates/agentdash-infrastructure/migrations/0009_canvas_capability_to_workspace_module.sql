-- Canvas Agent 工具面收束到 workspace_module。
--
-- ProjectAgent config 存为 JSON text；这里只改 capability_directives 中的
-- canvas 能力意图，Canvas 资产/文件/绑定事实源不迁移。

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
                                        WHEN 'canvas' THEN 'workspace_module'
                                        WHEN 'canvas::canvases_list' THEN 'workspace_module::workspace_module_list'
                                        WHEN 'canvas::canvas_start' THEN 'workspace_module::workspace_module_create'
                                        WHEN 'canvas::bind_canvas_data' THEN 'workspace_module::workspace_module_invoke'
                                        WHEN 'canvas::present_canvas' THEN 'workspace_module::workspace_module_present'
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
                                        WHEN 'canvas' THEN 'workspace_module'
                                        WHEN 'canvas::canvases_list' THEN 'workspace_module::workspace_module_list'
                                        WHEN 'canvas::canvas_start' THEN 'workspace_module::workspace_module_create'
                                        WHEN 'canvas::bind_canvas_data' THEN 'workspace_module::workspace_module_invoke'
                                        WHEN 'canvas::present_canvas' THEN 'workspace_module::workspace_module_present'
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
