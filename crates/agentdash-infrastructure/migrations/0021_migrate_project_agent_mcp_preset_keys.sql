-- ProjectAgent MCP preset 选择收束到 ToolCapabilityDirective。
--
-- ProjectAgent config 存为 JSON text；legacy mcp_preset_keys 迁移为
-- capability_directives 中的 { "add": "mcp:<key>" }。legacy add 前置，
-- 已有 explicit directives 后置，保持后来者胜语义。

WITH rewritten AS (
    SELECT
        id,
        jsonb_set(
            config::jsonb - 'mcp_preset_keys',
            '{capability_directives}',
            (
                COALESCE(
                    (
                        SELECT jsonb_agg(
                            jsonb_build_object('add', 'mcp:' || btrim(legacy.value #>> '{}'))
                            ORDER BY legacy.ord
                        )
                        FROM jsonb_array_elements(config::jsonb->'mcp_preset_keys')
                            WITH ORDINALITY AS legacy(value, ord)
                        WHERE jsonb_typeof(legacy.value) = 'string'
                          AND btrim(legacy.value #>> '{}') <> ''
                          AND position('::' in btrim(legacy.value #>> '{}')) = 0
                    ),
                    '[]'::jsonb
                )
                ||
                CASE
                    WHEN jsonb_typeof(config::jsonb->'capability_directives') = 'array'
                        THEN config::jsonb->'capability_directives'
                    ELSE '[]'::jsonb
                END
            ),
            true
        )::text AS next_config
    FROM project_agents
    WHERE config::jsonb ? 'mcp_preset_keys'
      AND jsonb_typeof(config::jsonb->'mcp_preset_keys') = 'array'
      AND NOT EXISTS (
          SELECT 1
          FROM jsonb_array_elements(config::jsonb->'mcp_preset_keys') AS invalid(value)
          WHERE jsonb_typeof(invalid.value) <> 'string'
             OR btrim(invalid.value #>> '{}') = ''
             OR position('::' in btrim(invalid.value #>> '{}')) <> 0
      )
)
UPDATE project_agents
SET config = rewritten.next_config
FROM rewritten
WHERE project_agents.id = rewritten.id
  AND project_agents.config IS DISTINCT FROM rewritten.next_config;
