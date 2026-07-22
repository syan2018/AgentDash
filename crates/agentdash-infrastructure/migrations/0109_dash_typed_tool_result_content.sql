-- Dash persists the exact typed tool content accepted from Host callbacks. Keeping text and image
-- parts structured lets provider transcript projection and AgentDash ThreadItem presentation read
-- the same source-owned result without serializing the callback envelope into user-visible text.

CREATE FUNCTION pg_temp.migrate_dash_typed_tool_result(value jsonb)
RETURNS jsonb
LANGUAGE plpgsql
IMMUTABLE
AS $$
DECLARE
    migrated jsonb;
BEGIN
    IF jsonb_typeof(value) = 'object' THEN
        IF value ->> 'channel' IN ('memory', 'memory_context')
           AND value -> 'presentation' = jsonb_build_object('kind', 'assignment_context')
        THEN
            value := jsonb_set(
                value,
                '{presentation}',
                jsonb_build_object('kind', 'memory_context')
            );
        END IF;

        IF value ->> 'type' = 'tool_result'
           AND jsonb_typeof(value -> 'content') = 'string'
        THEN
            value := jsonb_set(
                value,
                '{content}',
                jsonb_build_array(
                    jsonb_build_object(
                        'type', 'text',
                        'text', value ->> 'content'
                    )
                )
            );
        END IF;

        SELECT COALESCE(
            jsonb_object_agg(
                item.key,
                pg_temp.migrate_dash_typed_tool_result(item.value)
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
                pg_temp.migrate_dash_typed_tool_result(item.value)
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

UPDATE dash_complete_source
SET document = pg_temp.migrate_dash_typed_tool_result(document);

UPDATE dash_complete_effect
SET record = pg_temp.migrate_dash_typed_tool_result(record);
