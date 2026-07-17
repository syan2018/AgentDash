UPDATE agent_runtime_thread
SET projection = jsonb_set(
    projection,
    '{thread_name}',
    'null'::jsonb,
    true
)
WHERE NOT (projection ? 'thread_name');

UPDATE lifecycle_agents
SET workspace_title = NULL,
    workspace_title_source = NULL
WHERE workspace_title_source IN ('auto', 'codex');
