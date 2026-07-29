-- Canvas authoring mount identity is stable for the lifetime of a definition.
-- Revision 1 owns the original title used to derive that identity; every later
-- revision and every immutable AgentFrame projection must carry the same value.

CREATE TEMP TABLE canvas_mount_identity_migration ON COMMIT DROP AS
SELECT
    definition_id,
    format(
        'cvs-%s-%s',
        COALESCE(
            NULLIF(
                trim(
                    BOTH '-' FROM left(
                        regexp_replace(
                            lower(contract ->> 'title'),
                            '[^a-z0-9]+',
                            '-',
                            'g'
                        ),
                        24
                    )
                ),
                ''
            ),
            'canvas'
        ),
        left(replace(definition_id::text, '-', ''), 8)
    ) AS mount_id
FROM public.interaction_definition_revisions
WHERE revision_number = 1;

UPDATE public.interaction_definition_revisions AS revisions
SET contract = jsonb_set(
    revisions.contract,
    '{authoring_mount_id}',
    to_jsonb(mapping.mount_id),
    false
)
FROM canvas_mount_identity_migration AS mapping
WHERE revisions.definition_id = mapping.definition_id
  AND revisions.contract ->> 'authoring_mount_id' <> mapping.mount_id;

WITH migrated_agents AS (
    SELECT
        agent.id,
        jsonb_agg(
        CASE
            WHEN frame #> '{surface,vfs_surface,mounts}' IS NULL THEN frame
            ELSE jsonb_set(
                frame,
                '{surface,vfs_surface,mounts}',
                COALESCE(
                    (
                        SELECT jsonb_agg(
                            CASE
                                WHEN mount ->> 'provider' = 'canvas_fs'
                                     AND mapping.mount_id IS NOT NULL
                                THEN mount
                                    || jsonb_build_object('id', mapping.mount_id)
                                    || jsonb_build_object(
                                        'metadata',
                                        COALESCE(mount -> 'metadata', '{}'::jsonb)
                                            || jsonb_build_object(
                                                'authoring_mount_id',
                                                mapping.mount_id
                                            )
                                    )
                                ELSE mount
                            END
                            ORDER BY mount_ordinality
                        )
                        FROM jsonb_array_elements(
                            frame #> '{surface,vfs_surface,mounts}'
                        ) WITH ORDINALITY AS mounts(mount, mount_ordinality)
                        LEFT JOIN canvas_mount_identity_migration AS mapping
                          ON mapping.definition_id::text =
                             COALESCE(
                                 mount -> 'metadata' ->> 'definition_id',
                                 mount ->> 'backend_id'
                             )
                    ),
                    '[]'::jsonb
                ),
                false
            )
        END
        ORDER BY frame_ordinality
        ) AS frames
    FROM public.lifecycle_agents AS agent
    CROSS JOIN LATERAL jsonb_array_elements(agent.frames)
         WITH ORDINALITY AS source_frames(frame, frame_ordinality)
    GROUP BY agent.id
)
UPDATE public.lifecycle_agents AS agent
SET frames = migrated.frames
FROM migrated_agents AS migrated
WHERE agent.id = migrated.id
  AND agent.frames IS DISTINCT FROM migrated.frames;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM public.interaction_definition_revisions AS revisions
        JOIN canvas_mount_identity_migration AS mapping
          ON mapping.definition_id = revisions.definition_id
        WHERE revisions.contract ->> 'authoring_mount_id' <> mapping.mount_id
    ) THEN
        RAISE EXCEPTION 'Canvas definition revisions do not share one mount identity';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.lifecycle_agents AS agent
        CROSS JOIN LATERAL jsonb_array_elements(agent.frames) AS frames(frame)
        CROSS JOIN LATERAL jsonb_array_elements(
            COALESCE(frame #> '{surface,vfs_surface,mounts}', '[]'::jsonb)
        ) AS mounts(mount)
        LEFT JOIN canvas_mount_identity_migration AS mapping
          ON mapping.definition_id::text =
             COALESCE(
                 mount -> 'metadata' ->> 'definition_id',
                 mount ->> 'backend_id'
             )
        WHERE mount ->> 'provider' = 'canvas_fs'
          AND (
              mapping.mount_id IS NULL
              OR mount ->> 'id' <> mapping.mount_id
              OR mount -> 'metadata' ->> 'authoring_mount_id' <> mapping.mount_id
          )
    ) THEN
        RAISE EXCEPTION 'AgentFrame Canvas VFS projection is not converged';
    END IF;
END
$$;
