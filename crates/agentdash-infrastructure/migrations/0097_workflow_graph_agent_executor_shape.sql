UPDATE workflow_graphs AS wg
SET activities = COALESCE(
    (
        SELECT jsonb_agg(
            CASE
                WHEN elem -> 'executor' ->> 'kind' = 'agent' THEN
                    elem || jsonb_build_object(
                        'executor',
                        ((elem -> 'executor') - 'workflow_key' - 'session_policy') || jsonb_build_object(
                            'procedure_key',
                            COALESCE(
                                NULLIF(elem -> 'executor' ->> 'procedure_key', ''),
                                NULLIF(elem -> 'executor' ->> 'workflow_key', ''),
                                NULLIF(elem ->> 'procedure_key', ''),
                                ''
                            ),
                            'agent_reuse_policy',
                            COALESCE(
                                NULLIF(elem -> 'executor' ->> 'agent_reuse_policy', ''),
                                CASE
                                    WHEN elem -> 'executor' ->> 'session_policy' = 'continue_root'
                                        THEN 'continue_current_agent'
                                    ELSE 'create_activity_agent'
                                END
                            ),
                            'runtime_session_policy',
                            COALESCE(
                                NULLIF(elem -> 'executor' ->> 'runtime_session_policy', ''),
                                CASE
                                    WHEN elem -> 'executor' ->> 'session_policy' = 'continue_root'
                                        THEN 'deliver_to_current_trace'
                                    ELSE 'create_new'
                                END
                            )
                        )
                    )
                ELSE elem
            END
            ORDER BY idx
        )
        FROM jsonb_array_elements(wg.activities::jsonb) WITH ORDINALITY AS t(elem, idx)
    ),
    '[]'::jsonb
)::text
WHERE wg.activities <> '[]'
  AND EXISTS (
      SELECT 1
      FROM jsonb_array_elements(wg.activities::jsonb) AS t(elem)
      WHERE elem -> 'executor' ->> 'kind' = 'agent'
        AND (
            COALESCE(elem -> 'executor' ->> 'procedure_key', '') = ''
            OR COALESCE(elem -> 'executor' ->> 'agent_reuse_policy', '') = ''
            OR COALESCE(elem -> 'executor' ->> 'runtime_session_policy', '') = ''
            OR elem -> 'executor' ? 'workflow_key'
            OR elem -> 'executor' ? 'session_policy'
        )
  );
