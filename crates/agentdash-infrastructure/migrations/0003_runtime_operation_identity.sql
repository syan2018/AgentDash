ALTER TABLE public.dash_complete_source
    ADD COLUMN repository_schema_version smallint;

UPDATE public.dash_complete_source
SET repository_schema_version = 1;

ALTER TABLE public.dash_complete_source
    ALTER COLUMN repository_schema_version SET DEFAULT 2,
    ALTER COLUMN repository_schema_version SET NOT NULL;

UPDATE public.routine_executions
SET dispatch_input_handoff = jsonb_set(
    dispatch_input_handoff,
    '{runtime_operation_id}',
    to_jsonb(
        'product-effect:v2:' ||
        substr(dispatch_input_handoff->>'runtime_operation_id', length('product-command:v2:') + 1)
    )
)
WHERE dispatch_input_handoff->>'runtime_operation_id' LIKE 'product-command:v2:%';

UPDATE public.lifecycle_gates
SET delivery = jsonb_set(
    delivery,
    '{marker,accepted_operation_id}',
    to_jsonb(
        'product-effect:v2:' ||
        substr(delivery#>>'{marker,accepted_operation_id}', length('product-command:v2:') + 1)
    )
)
WHERE delivery#>>'{marker,accepted_operation_id}' LIKE 'product-command:v2:%';

UPDATE public.lifecycle_runs AS run
SET channel_registry = jsonb_set(
    run.channel_registry,
    '{channels}',
    (
        SELECT COALESCE(
            jsonb_agg(
                jsonb_set(
                    channel_record.value,
                    '{delivery_state}',
                    (
                        SELECT COALESCE(
                            jsonb_agg(
                                CASE
                                    WHEN delivery.value#>>'{materialized_ref,kind}' = 'agent_input'
                                     AND delivery.value#>>'{materialized_ref,operation_id}'
                                         LIKE 'product-command:v2:%'
                                    THEN jsonb_set(
                                        delivery.value,
                                        '{materialized_ref,operation_id}',
                                        to_jsonb(
                                            'product-effect:v2:' ||
                                            substr(
                                                delivery.value#>>'{materialized_ref,operation_id}',
                                                length('product-command:v2:') + 1
                                            )
                                        )
                                    )
                                    ELSE delivery.value
                                END
                                ORDER BY delivery.ordinality
                            ),
                            '[]'::jsonb
                        )
                        FROM jsonb_array_elements(
                            COALESCE(channel_record.value->'delivery_state', '[]'::jsonb)
                        ) WITH ORDINALITY AS delivery(value, ordinality)
                    ),
                    true
                )
                ORDER BY channel_record.ordinality
            ),
            '[]'::jsonb
        )
        FROM jsonb_array_elements(
            COALESCE(run.channel_registry->'channels', '[]'::jsonb)
        ) WITH ORDINALITY AS channel_record(value, ordinality)
    ),
    true
)
WHERE EXISTS (
    SELECT 1
    FROM jsonb_array_elements(
        COALESCE(run.channel_registry->'channels', '[]'::jsonb)
    ) AS channel_record(value)
    CROSS JOIN LATERAL jsonb_array_elements(
        COALESCE(channel_record.value->'delivery_state', '[]'::jsonb)
    ) AS delivery(value)
    WHERE delivery.value#>>'{materialized_ref,kind}' = 'agent_input'
      AND delivery.value#>>'{materialized_ref,operation_id}' LIKE 'product-command:v2:%'
);
