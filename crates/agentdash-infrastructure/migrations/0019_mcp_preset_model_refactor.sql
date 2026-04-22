ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS key TEXT;

ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS display_name TEXT;

ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS transport TEXT;

ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS route_policy TEXT;

UPDATE mcp_presets
SET
    key = COALESCE(key, name),
    display_name = COALESCE(display_name, name),
    transport = COALESCE(
        transport,
        CASE
            WHEN server_decl IS NULL THEN NULL
            ELSE (
                CASE
                    WHEN (server_decl::jsonb ->> 'type') IN ('http', 'sse') THEN
                        jsonb_build_object(
                            'type', server_decl::jsonb ->> 'type',
                            'url', server_decl::jsonb ->> 'url',
                            'headers', COALESCE(server_decl::jsonb -> 'headers', '[]'::jsonb)
                        )::text
                    WHEN (server_decl::jsonb ->> 'type') = 'stdio' THEN
                        jsonb_build_object(
                            'type', 'stdio',
                            'command', server_decl::jsonb ->> 'command',
                            'args', COALESCE(server_decl::jsonb -> 'args', '[]'::jsonb),
                            'env', COALESCE(server_decl::jsonb -> 'env', '[]'::jsonb)
                        )::text
                    ELSE server_decl
                END
            )
        END
    ),
    route_policy = COALESCE(
        route_policy,
        CASE
            WHEN server_decl IS NULL THEN 'auto'
            WHEN (server_decl::jsonb ? 'relay') IS FALSE THEN 'auto'
            WHEN (server_decl::jsonb ->> 'relay') = 'true' THEN 'relay'
            WHEN (server_decl::jsonb ->> 'relay') = 'false' THEN 'direct'
            ELSE 'auto'
        END
    );

ALTER TABLE mcp_presets
ALTER COLUMN key SET NOT NULL;

ALTER TABLE mcp_presets
ALTER COLUMN display_name SET NOT NULL;

ALTER TABLE mcp_presets
ALTER COLUMN transport SET NOT NULL;

ALTER TABLE mcp_presets
ALTER COLUMN route_policy SET NOT NULL;

DROP INDEX IF EXISTS idx_mcp_presets_project_name;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mcp_presets_project_key
    ON mcp_presets(project_id, key);

ALTER TABLE mcp_presets
DROP COLUMN IF EXISTS name;

ALTER TABLE mcp_presets
DROP COLUMN IF EXISTS server_decl;
