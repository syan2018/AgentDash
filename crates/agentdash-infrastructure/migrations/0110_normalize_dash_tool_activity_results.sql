-- Dash source document同时保存native history与折叠后的item snapshot，因此所有位置的工具结果都使用
-- 同一种canonical typed content形态。迁移一次性解码既有callback envelope，让history replay与
-- AgentDash Card投影消费相同的文本/图片内容项。

CREATE FUNCTION pg_temp.try_parse_dash_tool_result(value text)
RETURNS jsonb
LANGUAGE plpgsql
IMMUTABLE
AS $$
BEGIN
    RETURN value::jsonb;
EXCEPTION
    WHEN invalid_text_representation THEN
        RETURN NULL;
END;
$$;

CREATE FUNCTION pg_temp.normalize_dash_tool_activity_result(value jsonb)
RETURNS jsonb
LANGUAGE plpgsql
IMMUTABLE
AS $$
DECLARE
    migrated jsonb;
    encoded_text text;
    envelope jsonb;
BEGIN
    IF jsonb_typeof(value) = 'object' THEN
        SELECT COALESCE(
            jsonb_object_agg(
                item.key,
                pg_temp.normalize_dash_tool_activity_result(item.value)
            ),
            '{}'::jsonb
        )
        INTO migrated
        FROM jsonb_each(value) AS item;
        value := migrated;

        IF jsonb_typeof(value -> 'is_error') = 'boolean' THEN
            IF jsonb_typeof(value -> 'content') = 'string' THEN
                encoded_text := value ->> 'content';
            ELSIF jsonb_typeof(value -> 'content') = 'array'
                  AND jsonb_array_length(value -> 'content') = 1
                  AND value #>> '{content,0,type}' = 'text'
                  AND jsonb_typeof(value #> '{content,0,text}') = 'string'
            THEN
                encoded_text := value #>> '{content,0,text}';
            END IF;

            IF encoded_text IS NOT NULL THEN
                envelope := pg_temp.try_parse_dash_tool_result(encoded_text);
            END IF;

            IF jsonb_typeof(envelope) = 'object'
               AND jsonb_typeof(envelope -> 'content') = 'array'
            THEN
                value := jsonb_set(value, '{content}', envelope -> 'content');
                IF jsonb_typeof(envelope -> 'is_error') = 'boolean' THEN
                    value := jsonb_set(value, '{is_error}', envelope -> 'is_error');
                END IF;
                IF envelope ? 'details' AND envelope -> 'details' <> 'null'::jsonb THEN
                    value := jsonb_set(value, '{details}', envelope -> 'details');
                END IF;
            ELSIF jsonb_typeof(value -> 'content') = 'string' THEN
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
        END IF;

        RETURN value;
    END IF;

    IF jsonb_typeof(value) = 'array' THEN
        SELECT COALESCE(
            jsonb_agg(
                pg_temp.normalize_dash_tool_activity_result(item.value)
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
SET document = pg_temp.normalize_dash_tool_activity_result(document);

UPDATE dash_complete_effect
SET record = pg_temp.normalize_dash_tool_activity_result(record);
