-- A concrete Agent surface must retain the semantic evidence required to project accepted
-- instructions and tools back into the AgentDash protocol. Existing development documents are
-- upgraded in place so repository decoding remains atomic with the schema rollout.

CREATE FUNCTION pg_temp.tool_protocol_projector(tool_name text)
RETURNS jsonb
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE tool_name
        WHEN 'shell_exec' THEN jsonb_build_object('kind', 'command')
        WHEN 'fs_apply_patch' THEN jsonb_build_object('kind', 'file_change')
        WHEN 'fs_read' THEN jsonb_build_object('kind', 'fs_read')
        WHEN 'fs_grep' THEN jsonb_build_object('kind', 'fs_grep')
        WHEN 'fs_glob' THEN jsonb_build_object('kind', 'fs_glob')
        ELSE jsonb_build_object(
            'kind', 'dynamic',
            'namespace', CASE
                WHEN tool_name LIKE 'mcp\_%' ESCAPE '\' THEN 'mcp'
                WHEN tool_name LIKE 'workspace\_module\_%' ESCAPE '\' THEN 'workspace_module'
                WHEN tool_name LIKE 'task\_%' ESCAPE '\' THEN 'task'
                ELSE 'agentdash'
            END
        )
    END
$$;

CREATE FUNCTION pg_temp.instruction_presentation(channel_name text)
RETURNS jsonb
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE channel_name
        WHEN 'system' THEN jsonb_build_object('kind', 'system_guidelines')
        WHEN 'developer' THEN jsonb_build_object('kind', 'system_guidelines')
        WHEN 'constraint' THEN jsonb_build_object('kind', 'system_guidelines')
        WHEN 'constraints' THEN jsonb_build_object('kind', 'system_guidelines')
        WHEN 'instruction' THEN jsonb_build_object('kind', 'system_guidelines')
        WHEN 'instruction_append' THEN jsonb_build_object('kind', 'system_guidelines')
        WHEN 'persona' THEN jsonb_build_object('kind', 'identity')
        WHEN 'agent_identity' THEN jsonb_build_object('kind', 'identity')
        WHEN 'workspace' THEN jsonb_build_object('kind', 'environment')
        WHEN 'vfs' THEN jsonb_build_object('kind', 'environment')
        WHEN 'runtime_policy' THEN jsonb_build_object('kind', 'environment')
        WHEN 'user_context' THEN jsonb_build_object('kind', 'user_context')
        ELSE jsonb_build_object('kind', 'assignment_context')
    END
$$;

CREATE FUNCTION pg_temp.migrate_surface_semantic_evidence(value jsonb)
RETURNS jsonb
LANGUAGE plpgsql
IMMUTABLE
AS $$
DECLARE
    migrated jsonb;
BEGIN
    IF jsonb_typeof(value) = 'object' THEN
        -- Complete Agent contribution payload.
        IF value ->> 'kind' = 'tool'
           AND jsonb_typeof(value -> 'name') = 'string'
           AND jsonb_typeof(value -> 'input_schema') IS NOT NULL
           AND NOT value ? 'protocol_projector'
        THEN
            value := value || jsonb_build_object(
                'protocol_projector',
                pg_temp.tool_protocol_projector(value ->> 'name')
            );
        END IF;

        IF value ->> 'kind' = 'instruction'
           AND jsonb_typeof(value -> 'channel') = 'string'
           AND jsonb_typeof(value -> 'text') = 'string'
           AND NOT value ? 'presentation'
        THEN
            value := value || jsonb_build_object(
                'presentation',
                pg_temp.instruction_presentation(value ->> 'channel')
            );
        END IF;

        -- Dash accepted tool definition.
        IF jsonb_typeof(value -> 'name') = 'string'
           AND jsonb_typeof(value -> 'description') = 'string'
           AND jsonb_typeof(value -> 'input_schema') IS NOT NULL
           AND NOT value ? 'kind'
           AND NOT value ? 'protocol_projector'
        THEN
            value := value || jsonb_build_object(
                'protocol_projector',
                pg_temp.tool_protocol_projector(value ->> 'name')
            );
        END IF;

        -- Dash accepted instruction definition.
        IF jsonb_typeof(value -> 'key') = 'string'
           AND jsonb_typeof(value -> 'channel') = 'string'
           AND jsonb_typeof(value -> 'text') = 'string'
           AND NOT value ? 'kind'
           AND NOT value ? 'presentation'
        THEN
            value := value || jsonb_build_object(
                'presentation',
                pg_temp.instruction_presentation(value ->> 'channel')
            );
        END IF;

        -- A tool history entry owns the projector accepted for that call. Historical cards can
        -- therefore be reconstructed after the active surface changes or is revoked.
        IF value ->> 'type' IN ('tool_call', 'tool_activity')
           AND jsonb_typeof(value -> 'name') = 'string'
           AND NOT value ? 'protocol_projector'
        THEN
            value := value || jsonb_build_object(
                'protocol_projector',
                pg_temp.tool_protocol_projector(value ->> 'name')
            );
        END IF;

        SELECT COALESCE(
            jsonb_object_agg(
                item.key,
                pg_temp.migrate_surface_semantic_evidence(item.value)
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
                pg_temp.migrate_surface_semantic_evidence(item.value)
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
SET document = pg_temp.migrate_surface_semantic_evidence(document);

UPDATE dash_complete_effect
SET record = pg_temp.migrate_surface_semantic_evidence(record);
