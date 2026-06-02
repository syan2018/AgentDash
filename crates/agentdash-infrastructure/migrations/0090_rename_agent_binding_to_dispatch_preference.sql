-- Task.agent_binding -> Task.dispatch_preference
-- stories.tasks JSONB 内嵌 task 数组中的 key 重命名
UPDATE stories
SET tasks = (
    SELECT COALESCE(
        jsonb_agg(
            CASE
                WHEN elem ? 'agent_binding' THEN
                    (elem - 'agent_binding') || jsonb_build_object('dispatch_preference', elem -> 'agent_binding')
                ELSE elem
            END
            ORDER BY idx
        ),
        '[]'::jsonb
    )
    FROM jsonb_array_elements(tasks) WITH ORDINALITY AS t(elem, idx)
)
WHERE tasks IS NOT NULL
  AND jsonb_typeof(tasks) = 'array';
