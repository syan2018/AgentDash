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
    SELECT json_remove(
        json_set(base_config, '$.capability_directives',
            json_group_array(
                json_object('add',
                    CASE value
                        WHEN 'read'          THEN 'file_read'
                        WHEN 'write'         THEN 'file_write'
                        WHEN 'execute'       THEN 'shell_execute'
                        WHEN 'workflow'      THEN 'workflow'
                        WHEN 'collaboration' THEN 'collaboration'
                        WHEN 'canvas'        THEN 'canvas'
                        ELSE value
                    END
                )
            )
        ),
        '$.tool_clusters'
    )
    FROM json_each(json_extract(base_config, '$.tool_clusters'))
)
WHERE json_extract(base_config, '$.tool_clusters') IS NOT NULL
  AND json_type(json_extract(base_config, '$.tool_clusters')) = 'array';

-- agents 中仅有 tool_clusters 键但值为空数组的行：仅删除键
UPDATE agents
SET base_config = json_remove(base_config, '$.tool_clusters')
WHERE json_extract(base_config, '$.tool_clusters') IS NOT NULL
  AND json_type(json_extract(base_config, '$.tool_clusters')) = 'array'
  AND json_array_length(json_extract(base_config, '$.tool_clusters')) = 0;

-- project_agent_links.config_override 同理
UPDATE project_agent_links
SET config_override = (
    SELECT json_remove(
        json_set(config_override, '$.capability_directives',
            json_group_array(
                json_object('add',
                    CASE value
                        WHEN 'read'          THEN 'file_read'
                        WHEN 'write'         THEN 'file_write'
                        WHEN 'execute'       THEN 'shell_execute'
                        WHEN 'workflow'      THEN 'workflow'
                        WHEN 'collaboration' THEN 'collaboration'
                        WHEN 'canvas'        THEN 'canvas'
                        ELSE value
                    END
                )
            )
        ),
        '$.tool_clusters'
    )
    FROM json_each(json_extract(config_override, '$.tool_clusters'))
)
WHERE config_override IS NOT NULL
  AND json_extract(config_override, '$.tool_clusters') IS NOT NULL
  AND json_type(json_extract(config_override, '$.tool_clusters')) = 'array'
  AND json_array_length(json_extract(config_override, '$.tool_clusters')) > 0;

UPDATE project_agent_links
SET config_override = json_remove(config_override, '$.tool_clusters')
WHERE config_override IS NOT NULL
  AND json_extract(config_override, '$.tool_clusters') IS NOT NULL
  AND json_type(json_extract(config_override, '$.tool_clusters')) = 'array'
  AND json_array_length(json_extract(config_override, '$.tool_clusters')) = 0;
