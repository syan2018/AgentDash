ALTER TABLE public.interaction_definition_revisions
    ADD COLUMN contract jsonb;

UPDATE public.interaction_definition_revisions
SET contract = jsonb_build_object(
    'kind', document -> 'kind',
    'definition_format_version', document -> 'definition_format_version',
    'interaction_contract_version', document -> 'interaction_contract_version',
    'authoring_mount_id', document -> 'authoring_mount_id',
    'title', document -> 'title',
    'description', document -> 'description',
    'initial_state', document -> 'initial_state',
    'state_schema', document -> 'state_schema',
    'agent_projection', document -> 'agent_projection',
    'command_definitions', document -> 'command_definitions',
    'component_bindings', document -> 'component_bindings',
    'action_bindings', COALESCE(document -> 'action_bindings', '[]'::jsonb),
    'resource_slots', document -> 'resource_slots',
    'created_by', document -> 'created_by'
);

ALTER TABLE public.interaction_definition_revisions
    ALTER COLUMN contract SET NOT NULL,
    ADD CONSTRAINT interaction_definition_revisions_contract_shape_check CHECK (
        jsonb_typeof(contract) = 'object'
        AND jsonb_typeof(contract -> 'command_definitions') = 'array'
        AND jsonb_typeof(contract -> 'component_bindings') = 'array'
        AND jsonb_typeof(contract -> 'action_bindings') = 'array'
        AND jsonb_typeof(contract -> 'resource_slots') = 'array'
    ),
    DROP COLUMN document;
