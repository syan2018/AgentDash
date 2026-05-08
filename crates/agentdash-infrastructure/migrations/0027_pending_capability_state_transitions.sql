ALTER TABLE sessions
ADD COLUMN IF NOT EXISTS pending_capability_state_transitions_json TEXT NOT NULL DEFAULT '[]';

UPDATE sessions
SET pending_capability_state_transitions_json = COALESCE(
    (
        SELECT jsonb_agg(
            CASE
                WHEN item.transition ? 'surface' THEN
                    jsonb_set(
                        item.transition - 'surface',
                        '{state}',
                        jsonb_build_object(
                            'capabilities', COALESCE(
                                item.transition->'surface'->'flowCapabilities'->'effective_capabilities',
                                '[]'::jsonb
                            ),
                            'tool_clusters', COALESCE(
                                item.transition->'surface'->'flowCapabilities'->'enabled_clusters',
                                '[]'::jsonb
                            ),
                            'tool_policy', COALESCE(
                                item.transition->'surface'->'flowCapabilities'->'tool_filters',
                                '{}'::jsonb
                            ),
                            'mcp_servers', COALESCE(
                                item.transition->'surface'->'mcpServers',
                                '[]'::jsonb
                            ),
                            'vfs',
                                item.transition->'surface'->'vfs'
                        )
                    )
                ELSE item.transition
            END
        )::text
        FROM jsonb_array_elements(pending_capability_surface_transitions_json::jsonb) AS item(transition)
    ),
    '[]'
)
WHERE pending_capability_state_transitions_json = '[]'
  AND pending_capability_surface_transitions_json IS NOT NULL
  AND pending_capability_surface_transitions_json <> '[]';

ALTER TABLE sessions
DROP COLUMN IF EXISTS pending_capability_surface_transitions_json;
