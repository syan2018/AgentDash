UPDATE canvases
SET mount_id = 'cvs-' || regexp_replace(mount_id, '^(cvs-)+', '')
WHERE mount_id IS NOT NULL
  AND mount_id <> ''
  AND mount_id IS DISTINCT FROM 'cvs-' || regexp_replace(mount_id, '^(cvs-)+', '');

UPDATE agent_frames
SET visible_canvas_mount_ids_json = (
    SELECT COALESCE(
        jsonb_agg(
            'cvs-' || regexp_replace(value #>> '{}', '^(cvs-)+', '')
        ),
        '[]'::jsonb
    )::text
    FROM jsonb_array_elements(visible_canvas_mount_ids_json::jsonb) AS value
)
WHERE visible_canvas_mount_ids_json IS NOT NULL
  AND jsonb_typeof(visible_canvas_mount_ids_json::jsonb) = 'array';

UPDATE agent_frames
SET visible_workspace_module_refs_json = (
    SELECT COALESCE(
        jsonb_agg(
            CASE
                WHEN value #>> '{}' LIKE 'canvas:%' THEN
                    'canvas:cvs-' || regexp_replace(
                        substring(value #>> '{}' FROM length('canvas:') + 1),
                        '^(cvs-)+',
                        ''
                    )
                ELSE value #>> '{}'
            END
        ),
        '[]'::jsonb
    )::text
    FROM jsonb_array_elements(visible_workspace_module_refs_json::jsonb) AS value
)
WHERE visible_workspace_module_refs_json IS NOT NULL
  AND jsonb_typeof(visible_workspace_module_refs_json::jsonb) = 'array';

WITH rewritten AS (
    SELECT
        id,
        jsonb_set(
            config::jsonb,
            '{visible_workspace_module_refs}',
            (
                SELECT COALESCE(
                    jsonb_agg(
                        CASE
                            WHEN value #>> '{}' LIKE 'canvas:%' THEN
                                'canvas:cvs-' || regexp_replace(
                                    substring(value #>> '{}' FROM length('canvas:') + 1),
                                    '^(cvs-)+',
                                    ''
                                )
                            ELSE value #>> '{}'
                        END
                    ),
                    '[]'::jsonb
                )
                FROM jsonb_array_elements(config::jsonb->'visible_workspace_module_refs') AS value
            ),
            false
        )::text AS next_config
    FROM project_agents
    WHERE config::jsonb ? 'visible_workspace_module_refs'
      AND jsonb_typeof(config::jsonb->'visible_workspace_module_refs') = 'array'
)
UPDATE project_agents
SET config = rewritten.next_config
FROM rewritten
WHERE project_agents.id = rewritten.id
  AND project_agents.config IS DISTINCT FROM rewritten.next_config;

CREATE UNIQUE INDEX IF NOT EXISTS canvases_project_mount_id_uidx
    ON canvases (project_id, mount_id);
