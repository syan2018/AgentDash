-- Dash native history owns the exact surface accepted by the concrete Agent. Preserve each
-- materialized instruction's stable contribution key and channel so provider prompt assembly and
-- ContextFrame projection derive from the same source-owned fact.

CREATE FUNCTION pg_temp.migrate_dash_surface_instructions(
    value jsonb,
    canonical_digest text,
    canonical_instructions jsonb
)
RETURNS jsonb
LANGUAGE plpgsql
IMMUTABLE
AS $$
DECLARE
    migrated jsonb;
    prompt text;
BEGIN
    IF jsonb_typeof(value) = 'object' THEN
        IF jsonb_typeof(value -> 'revision') = 'number'
           AND jsonb_typeof(value -> 'digest') = 'string'
           AND jsonb_typeof(value -> 'tools') = 'array'
           AND value ->> 'digest' = canonical_digest
           AND jsonb_typeof(canonical_instructions) = 'array'
           AND jsonb_array_length(canonical_instructions) > 0
        THEN
            RETURN (value - 'system_prompt') || jsonb_build_object(
                'instructions',
                canonical_instructions
            );
        END IF;

        IF jsonb_typeof(value -> 'revision') = 'number'
           AND jsonb_typeof(value -> 'digest') = 'string'
           AND jsonb_typeof(value -> 'system_prompt') = 'string'
           AND jsonb_typeof(value -> 'tools') = 'array'
        THEN
            prompt := value ->> 'system_prompt';
            RETURN (value - 'system_prompt') || jsonb_build_object(
                'instructions',
                CASE
                    WHEN BTRIM(prompt) = '' THEN '[]'::jsonb
                    ELSE jsonb_build_array(jsonb_build_object(
                        'key', 'instruction:migrated:system_prompt',
                        'channel', 'system',
                        'text', prompt
                    ))
                END
            );
        END IF;

        SELECT COALESCE(
            jsonb_object_agg(
                item.key,
                pg_temp.migrate_dash_surface_instructions(
                    item.value,
                    canonical_digest,
                    canonical_instructions
                )
            ),
            '{}'::jsonb
        )
        INTO migrated
        FROM jsonb_each(value) AS item;
        RETURN migrated;
    END IF;

    IF jsonb_typeof(value) = 'array' THEN
        SELECT COALESCE(
            jsonb_agg(
                pg_temp.migrate_dash_surface_instructions(
                    item.value,
                    canonical_digest,
                    canonical_instructions
                )
                ORDER BY item.ordinality
            ),
            '[]'::jsonb
        )
        INTO migrated
        FROM jsonb_array_elements(value) WITH ORDINALITY AS item(value, ordinality);
        RETURN migrated;
    END IF;

    RETURN value;
END;
$$;

WITH source_surface AS (
    SELECT
        source.source_coordinate,
        source.document #>> '{metadata,callback_surface,digest}' AS canonical_digest,
        COALESCE((
            SELECT jsonb_agg(
                jsonb_build_object(
                    'key', contribution.value ->> 'key',
                    'channel', contribution.value #>> '{payload,channel}',
                    'text', contribution.value #>> '{payload,text}'
                )
                ORDER BY contribution.ordinality
            )
            FROM jsonb_array_elements(
                COALESCE(
                    source.document #> '{metadata,callback_surface,contributions}',
                    '[]'::jsonb
                )
            ) WITH ORDINALITY AS contribution(value, ordinality)
            WHERE contribution.value #>> '{payload,kind}' = 'instruction'
        ), '[]'::jsonb) AS canonical_instructions
    FROM dash_complete_source AS source
)
UPDATE dash_complete_source AS source
SET document = pg_temp.migrate_dash_surface_instructions(
    source.document,
    source_surface.canonical_digest,
    source_surface.canonical_instructions
)
FROM source_surface
WHERE source_surface.source_coordinate = source.source_coordinate
  AND (
      source.document @? '$.**.system_prompt'
      OR source.document @? '$.**.instructions'
  );
