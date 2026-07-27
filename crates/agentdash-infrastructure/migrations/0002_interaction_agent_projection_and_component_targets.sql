UPDATE public.interaction_definition_revisions AS revision
SET document = jsonb_set(
    jsonb_set(
        revision.document,
        '{agent_projection}',
        '{"version":1,"allowed_state_paths":[]}'::jsonb,
        true
    ),
    '{component_bindings}',
    COALESCE(
        (
            SELECT jsonb_agg(
                (binding - 'event_commands')
                || jsonb_build_object(
                    'event_bindings',
                    COALESCE(
                        (
                            SELECT jsonb_agg(
                                (event_binding - 'command_key')
                                || jsonb_build_object(
                                    'target',
                                    jsonb_build_object(
                                        'kind', 'platform_command',
                                        'command_key', event_binding ->> 'command_key'
                                    )
                                )
                            )
                            FROM jsonb_array_elements(
                                COALESCE(binding -> 'event_commands', '[]'::jsonb)
                            ) AS event_binding
                        ),
                        '[]'::jsonb
                    )
                )
            )
            FROM jsonb_array_elements(
                COALESCE(revision.document -> 'component_bindings', '[]'::jsonb)
            ) AS binding
        ),
        '[]'::jsonb
    ),
    true
);
