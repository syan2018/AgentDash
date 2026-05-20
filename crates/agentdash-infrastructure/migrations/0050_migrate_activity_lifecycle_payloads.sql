WITH migrated_activities AS (
    SELECT
        lifecycle_definitions.id,
        lifecycle_definitions.entry_step_key,
        jsonb_agg(
            jsonb_build_object(
                'key', step.value ->> 'key',
                'description', COALESCE(step.value ->> 'description', ''),
                'executor', jsonb_build_object(
                    'kind', 'agent',
                    'workflow_key', COALESCE(step.value ->> 'workflow_key', ''),
                    'session_policy', CASE
                        WHEN COALESCE(step.value ->> 'node_type', 'agent_node') = 'phase_node'
                            THEN 'continue_root'
                        ELSE 'spawn_child'
                    END
                ),
                'input_ports', COALESCE(step.value -> 'input_ports', '[]'::jsonb),
                'output_ports', COALESCE(step.value -> 'output_ports', '[]'::jsonb),
                'completion_policy', CASE
                    WHEN jsonb_array_length(COALESCE(step.value -> 'output_ports', '[]'::jsonb)) = 0
                        THEN jsonb_build_object('kind', 'executor_terminal')
                    ELSE jsonb_build_object(
                        'kind', 'output_ports',
                        'required_ports', (
                            SELECT COALESCE(jsonb_agg(port.value ->> 'key'), '[]'::jsonb)
                            FROM jsonb_array_elements(COALESCE(step.value -> 'output_ports', '[]'::jsonb)) AS port(value)
                        )
                    )
                END
            )
            ORDER BY step.ordinality
        ) AS activities
    FROM lifecycle_definitions
    CROSS JOIN LATERAL jsonb_array_elements(lifecycle_definitions.steps::jsonb)
        WITH ORDINALITY AS step(value, ordinality)
    WHERE lifecycle_definitions.entry_activity_key = ''
      AND lifecycle_definitions.steps <> '[]'
    GROUP BY lifecycle_definitions.id, lifecycle_definitions.entry_step_key
),
migrated_transitions AS (
    SELECT
        lifecycle_definitions.id,
        COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'from', edge.value ->> 'from_node',
                    'to', edge.value ->> 'to_node',
                    'kind', CASE
                        WHEN COALESCE(edge.value ->> 'kind', 'artifact') = 'artifact'
                            THEN 'artifact'
                        ELSE 'flow'
                    END,
                    'condition', jsonb_build_object('kind', 'always'),
                    'artifact_bindings', CASE
                        WHEN COALESCE(edge.value ->> 'kind', 'artifact') = 'artifact'
                            THEN jsonb_build_array(jsonb_build_object(
                                'from_port', edge.value ->> 'from_port',
                                'to_port', edge.value ->> 'to_port',
                                'alias', 'latest'
                            ))
                        ELSE '[]'::jsonb
                    END
                )
                ORDER BY edge.ordinality
            ),
            '[]'::jsonb
        ) AS transitions
    FROM lifecycle_definitions
    LEFT JOIN LATERAL jsonb_array_elements(lifecycle_definitions.edges::jsonb)
        WITH ORDINALITY AS edge(value, ordinality) ON true
    WHERE lifecycle_definitions.entry_activity_key = ''
      AND lifecycle_definitions.steps <> '[]'
    GROUP BY lifecycle_definitions.id
)
UPDATE lifecycle_definitions
SET
    entry_activity_key = migrated_activities.entry_step_key,
    activities = migrated_activities.activities::text,
    transitions = migrated_transitions.transitions::text,
    entry_step_key = migrated_activities.entry_step_key,
    steps = '[]',
    edges = '[]',
    updated_at = NOW()::text
FROM migrated_activities
JOIN migrated_transitions ON migrated_transitions.id = migrated_activities.id
WHERE lifecycle_definitions.id = migrated_activities.id;
