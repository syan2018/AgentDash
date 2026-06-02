-- Task.agent_binding -> Task.dispatch_preference
-- stories_snapshot JSONB 内嵌 tasks 数组中的 key 重命名
UPDATE stories
SET stories_snapshot = (
    SELECT jsonb_set(
        stories_snapshot,
        '{tasks}',
        COALESCE(
            (
                SELECT jsonb_agg(
                    CASE
                        WHEN elem ? 'agent_binding' THEN
                            (elem - 'agent_binding') || jsonb_build_object('dispatch_preference', elem -> 'agent_binding')
                        ELSE elem
                    END
                    ORDER BY idx
                )
                FROM jsonb_array_elements(stories_snapshot -> 'tasks') WITH ORDINALITY AS t(elem, idx)
            ),
            '[]'::jsonb
        )
    )
)
WHERE stories_snapshot -> 'tasks' IS NOT NULL;
