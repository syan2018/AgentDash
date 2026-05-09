-- Migration: tool_clusters → capability_directives
--
-- 将 agents.base_config 和 project_agent_links.config_override 中的
-- "tool_clusters": ["read", "write", ...] 转为
-- "capability_directives": [{"add":"file_read"}, {"add":"file_write"}, ...]
--
-- 映射关系：
--   read         → file_read
--   write        → file_write
--   execute      → shell_execute
--   workflow     → workflow
--   collaboration → collaboration
--   canvas       → canvas

UPDATE agents
SET base_config = (
    jsonb_set(
        base_config::jsonb,
        '{capability_directives}',
        (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'add',
                    CASE value
                        WHEN 'read' THEN 'file_read'
                        WHEN 'write' THEN 'file_write'
                        WHEN 'execute' THEN 'shell_execute'
                        WHEN 'workflow' THEN 'workflow'
                        WHEN 'collaboration' THEN 'collaboration'
                        WHEN 'canvas' THEN 'canvas'
                        ELSE value
                    END
                )
            )
            FROM jsonb_array_elements_text(base_config::jsonb -> 'tool_clusters') AS tool_cluster(value)
        ),
        true
    ) - 'tool_clusters'
)::text
WHERE jsonb_typeof(base_config::jsonb -> 'tool_clusters') = 'array'
  AND jsonb_array_length(base_config::jsonb -> 'tool_clusters') > 0;

-- agents 中仅有 tool_clusters 键但值为空数组的行：仅删除键
UPDATE agents
SET base_config = ((base_config::jsonb) - 'tool_clusters')::text
WHERE jsonb_typeof(base_config::jsonb -> 'tool_clusters') = 'array'
  AND jsonb_array_length(base_config::jsonb -> 'tool_clusters') = 0;

-- project_agent_links.config_override 同理
UPDATE project_agent_links
SET config_override = (
    jsonb_set(
        config_override::jsonb,
        '{capability_directives}',
        (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'add',
                    CASE value
                        WHEN 'read' THEN 'file_read'
                        WHEN 'write' THEN 'file_write'
                        WHEN 'execute' THEN 'shell_execute'
                        WHEN 'workflow' THEN 'workflow'
                        WHEN 'collaboration' THEN 'collaboration'
                        WHEN 'canvas' THEN 'canvas'
                        ELSE value
                    END
                )
            )
            FROM jsonb_array_elements_text(config_override::jsonb -> 'tool_clusters') AS tool_cluster(value)
        ),
        true
    ) - 'tool_clusters'
)::text
WHERE config_override IS NOT NULL
  AND jsonb_typeof(config_override::jsonb -> 'tool_clusters') = 'array'
  AND jsonb_array_length(config_override::jsonb -> 'tool_clusters') > 0;

UPDATE project_agent_links
SET config_override = ((config_override::jsonb) - 'tool_clusters')::text
WHERE config_override IS NOT NULL
  AND jsonb_typeof(config_override::jsonb -> 'tool_clusters') = 'array'
  AND jsonb_array_length(config_override::jsonb -> 'tool_clusters') = 0;
