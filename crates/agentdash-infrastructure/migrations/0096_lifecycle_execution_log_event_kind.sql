UPDATE lifecycle_runs AS lr
SET execution_log = COALESCE(
    (
        SELECT jsonb_agg(
            CASE elem ->> 'event_kind'
                WHEN 'step_activated' THEN elem || jsonb_build_object('event_kind', 'activity_activated')
                WHEN 'step_completed' THEN elem || jsonb_build_object('event_kind', 'activity_completed')
                ELSE elem
            END
            ORDER BY idx
        )
        FROM jsonb_array_elements(lr.execution_log::jsonb) WITH ORDINALITY AS t(elem, idx)
    ),
    '[]'::jsonb
)::text
WHERE lr.execution_log <> '[]'
  AND EXISTS (
      SELECT 1
      FROM jsonb_array_elements(lr.execution_log::jsonb) AS t(elem)
      WHERE elem ->> 'event_kind' IN ('step_activated', 'step_completed')
  );
