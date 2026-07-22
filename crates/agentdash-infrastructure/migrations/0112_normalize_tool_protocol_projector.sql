-- Tool projector provenance selects an AgentDash card family. Dynamic tools use their callable
-- name as the complete presentation identity, matching the canonical main-thread item shape.
-- Normalize the accepted surface and history evidence in place so persisted owner documents use
-- the same enum shape as newly admitted tool calls.

CREATE FUNCTION pg_temp.normalized_tool_protocol_projector(value jsonb)
RETURNS jsonb
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE
        WHEN jsonb_typeof(value) <> 'object' THEN value
        WHEN value ? 'family' THEN
            CASE
                WHEN value ->> 'family' = 'dynamic' THEN value - 'namespace'
                ELSE value
            END
        WHEN value ? 'kind' THEN
            (CASE
                WHEN value ->> 'kind' = 'dynamic' THEN value - 'namespace'
                ELSE value
            END - 'kind') || jsonb_build_object('family', value ->> 'kind')
        ELSE value
    END
$$;

CREATE FUNCTION pg_temp.normalize_tool_protocol_projectors(value jsonb)
RETURNS jsonb
LANGUAGE plpgsql
IMMUTABLE
AS $$
DECLARE
    migrated jsonb;
BEGIN
    IF jsonb_typeof(value) = 'object' THEN
        SELECT COALESCE(
            jsonb_object_agg(
                item.key,
                CASE
                    WHEN item.key = 'protocol_projector'
                    THEN pg_temp.normalized_tool_protocol_projector(item.value)
                    ELSE pg_temp.normalize_tool_protocol_projectors(item.value)
                END
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
                pg_temp.normalize_tool_protocol_projectors(item.value)
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
SET document = pg_temp.normalize_tool_protocol_projectors(document);

UPDATE dash_complete_effect
SET record = pg_temp.normalize_tool_protocol_projectors(record);
