WITH generated_mounts AS (
    SELECT
        revision_id,
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
    WHERE contract ->> 'authoring_mount_id' = 'cvs-' || definition_id::text
)
UPDATE public.interaction_definition_revisions AS revisions
SET contract = jsonb_set(
    revisions.contract,
    '{authoring_mount_id}',
    to_jsonb(generated_mounts.mount_id),
    false
)
FROM generated_mounts
WHERE revisions.revision_id = generated_mounts.revision_id;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM (
            SELECT
                contract ->> 'authoring_mount_id' AS mount_id,
                count(DISTINCT definition_id) AS definition_count
            FROM public.interaction_definition_revisions
            GROUP BY contract ->> 'authoring_mount_id'
            HAVING count(DISTINCT definition_id) > 1
        ) AS collisions
    ) THEN
        RAISE EXCEPTION 'Canvas authoring mount identity collision';
    END IF;
END
$$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM public.interaction_definition_revisions
        WHERE jsonb_array_length(
            COALESCE(contract -> 'action_bindings', '[]'::jsonb)
        ) > 0
    ) THEN
        RAISE EXCEPTION
            'Canvas action_bindings must be materialized into SourceBundle canvas.json';
    END IF;
END
$$;

ALTER TABLE public.interaction_definition_revisions
    DROP CONSTRAINT interaction_definition_revisions_contract_shape_check;

UPDATE public.interaction_definition_revisions
SET contract = contract - 'action_bindings';

ALTER TABLE public.interaction_definition_revisions
    ADD CONSTRAINT interaction_definition_revisions_contract_shape_check CHECK (
        jsonb_typeof(contract) = 'object'
        AND jsonb_typeof(contract -> 'command_definitions') = 'array'
        AND jsonb_typeof(contract -> 'component_bindings') = 'array'
        AND jsonb_typeof(contract -> 'resource_slots') = 'array'
        AND NOT contract ? 'action_bindings'
    );
