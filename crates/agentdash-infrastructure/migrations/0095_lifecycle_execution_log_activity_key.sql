UPDATE lifecycle_runs AS lr
SET execution_log = COALESCE(
    (
        SELECT jsonb_agg(
            CASE
                WHEN elem ? 'activity_key' AND btrim(elem ->> 'activity_key') <> '' THEN elem
                ELSE elem || jsonb_build_object(
                    'activity_key',
                    COALESCE(
                        NULLIF(elem ->> 'node_key', ''),
                        NULLIF(elem ->> 'step_key', ''),
                        NULLIF(elem ->> 'activity', ''),
                        NULLIF(
                            CASE
                                WHEN active_key IS NULL THEN NULL
                                WHEN position(':' IN active_key) > 0 THEN split_part(active_key, ':', 2)
                                ELSE active_key
                            END,
                            ''
                        ),
                        'unknown'
                    )
                )
            END
            ORDER BY idx
        )
        FROM jsonb_array_elements(lr.execution_log::jsonb) WITH ORDINALITY AS t(elem, idx)
        CROSS JOIN LATERAL (
            SELECT NULLIF(lr.active_node_keys::jsonb ->> 0, '') AS active_key
        ) active
    ),
    '[]'::jsonb
)::text
WHERE lr.execution_log <> '[]'
  AND EXISTS (
      SELECT 1
      FROM jsonb_array_elements(lr.execution_log::jsonb) AS t(elem)
      WHERE NOT (elem ? 'activity_key')
         OR btrim(elem ->> 'activity_key') = ''
  );
