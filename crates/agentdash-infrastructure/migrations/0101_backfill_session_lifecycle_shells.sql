-- 为存量 runtime sessions 补齐当前 lifecycle 控制面锚点。
-- 这些 session 原本只有会话事件与 meta；补齐 shell 后可被 agent/frame-first 列表与投递链路重新定位。

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TEMP TABLE __session_lifecycle_shell_backfill ON COMMIT DROP AS
SELECT
    s.id AS session_id,
    CASE
        WHEN s.project_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
             AND EXISTS (SELECT 1 FROM projects p WHERE p.id = s.project_id)
            THEN s.project_id
        ELSE '00000000-0000-0000-0000-000000000101'
    END AS project_id,
    to_timestamp(s.created_at::double precision / 1000.0) AS created_at,
    to_timestamp(s.updated_at::double precision / 1000.0) AS updated_at,
    s.executor_config_json,
    gen_random_uuid()::text AS run_id,
    gen_random_uuid()::text AS graph_instance_id,
    gen_random_uuid()::text AS agent_id,
    gen_random_uuid()::text AS frame_id,
    gen_random_uuid()::text AS assignment_id
FROM sessions s
WHERE NOT EXISTS (
    SELECT 1
    FROM runtime_session_execution_anchors anchor
    WHERE anchor.runtime_session_id = s.id
);

INSERT INTO projects (
    id, name, description, config, created_by_user_id, updated_by_user_id,
    visibility, is_template, cloned_from_project_id, created_at, updated_at
)
SELECT
    '00000000-0000-0000-0000-000000000101',
    'Recovered Sessions',
    '存量自由会话恢复项目。',
    '{}',
    'system',
    'system',
    'private',
    FALSE,
    NULL,
    now(),
    now()
WHERE EXISTS (
    SELECT 1
    FROM __session_lifecycle_shell_backfill b
    WHERE b.project_id = '00000000-0000-0000-0000-000000000101'
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO project_subject_grants (
    project_id, subject_type, subject_id, role, granted_by_user_id, created_at, updated_at
)
SELECT
    '00000000-0000-0000-0000-000000000101',
    'user',
    'local-user',
    'owner',
    'system',
    now(),
    now()
WHERE EXISTS (
    SELECT 1
    FROM __session_lifecycle_shell_backfill b
    WHERE b.project_id = '00000000-0000-0000-0000-000000000101'
)
ON CONFLICT (project_id, subject_type, subject_id) DO NOTHING;

UPDATE sessions s
SET project_id = b.project_id
FROM __session_lifecycle_shell_backfill b
WHERE s.id = b.session_id
  AND s.project_id IS DISTINCT FROM b.project_id;

INSERT INTO agent_procedures (
    id, project_id, key, name, description, source, version, contract,
    library_asset_id, source_ref, source_version, source_digest, installed_at,
    created_at, updated_at
)
SELECT
    gen_random_uuid()::text,
    p.project_id,
    'builtin.freeform_agent',
    'Freeform Agent',
    '普通自由会话的默认 Agent procedure。',
    '"builtin_seed"',
    1,
    '{}',
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    now(),
    now()
FROM (SELECT DISTINCT project_id FROM __session_lifecycle_shell_backfill) p
WHERE NOT EXISTS (
    SELECT 1
    FROM agent_procedures existing
    WHERE existing.project_id = p.project_id
      AND existing.key = 'builtin.freeform_agent'
);

INSERT INTO workflow_graphs (
    id, project_id, key, name, description, source, version, entry_activity_key,
    activities, transitions, library_asset_id, source_ref, source_version, source_digest,
    installed_at, created_at, updated_at
)
SELECT
    gen_random_uuid()::text,
    p.project_id,
    'builtin.freeform_session',
    'Freeform Session',
    '普通自由会话的无外围约束过程。',
    '"builtin_seed"',
    1,
    'main_conversation',
    jsonb_build_array(
        jsonb_build_object(
            'key', 'main_conversation',
            'description', '普通自由会话主对话。',
            'executor', jsonb_build_object(
                'kind', 'agent',
                'procedure_key', 'builtin.freeform_agent',
                'agent_reuse_policy', 'continue_current_agent',
                'runtime_session_policy', 'deliver_to_current_trace'
            ),
            'completion_policy', jsonb_build_object('kind', 'open_ended'),
            'iteration_policy', jsonb_build_object(
                'max_attempts', NULL,
                'artifact_alias', 'latest_and_history'
            ),
            'join_policy', 'all'
        )
    )::text,
    '[]',
    NULL,
    NULL,
    NULL,
    NULL,
    NULL,
    now(),
    now()
FROM (SELECT DISTINCT project_id FROM __session_lifecycle_shell_backfill) p
WHERE NOT EXISTS (
    SELECT 1
    FROM workflow_graphs existing
    WHERE existing.project_id = p.project_id
      AND existing.key = 'builtin.freeform_session'
);

INSERT INTO lifecycle_runs (
    id, project_id, root_graph_id, status, active_node_keys, record_artifacts,
    execution_log, created_at, updated_at, last_activity_at
)
SELECT
    b.run_id,
    b.project_id,
    graph.id,
    '"running"',
    '["main_conversation"]',
    '{}',
    '[]',
    b.created_at,
    b.updated_at,
    b.updated_at
FROM __session_lifecycle_shell_backfill b
JOIN workflow_graphs graph
  ON graph.project_id = b.project_id
 AND graph.key = 'builtin.freeform_session'
ON CONFLICT (id) DO NOTHING;

INSERT INTO lifecycle_workflow_instances (
    id, run_id, graph_id, role, status, activity_state_json, created_at, updated_at
)
SELECT
    b.graph_instance_id,
    b.run_id,
    graph.id,
    'root',
    'active',
    jsonb_build_object(
        'graph_instance_id', b.graph_instance_id,
        'status', 'running',
        'attempts', jsonb_build_array(
            jsonb_build_object(
                'activity_key', 'main_conversation',
                'attempt', 1,
                'status', 'running',
                'started_at', b.created_at
            )
        ),
        'outputs', '[]'::jsonb,
        'inputs', '[]'::jsonb
    )::text,
    b.created_at,
    b.updated_at
FROM __session_lifecycle_shell_backfill b
JOIN workflow_graphs graph
  ON graph.project_id = b.project_id
 AND graph.key = 'builtin.freeform_session'
ON CONFLICT (id) DO NOTHING;

INSERT INTO lifecycle_agents (
    id, run_id, project_id, agent_kind, agent_role, project_agent_id, status,
    bootstrap_status, current_frame_id, created_at, updated_at
)
SELECT
    b.agent_id,
    b.run_id,
    b.project_id,
    'project_agent',
    'primary',
    NULL,
    'active',
    'bootstrapped',
    b.frame_id,
    b.created_at,
    b.updated_at
FROM __session_lifecycle_shell_backfill b
ON CONFLICT (id) DO NOTHING;

INSERT INTO agent_frames (
    id, agent_id, revision, procedure_id, graph_instance_id, activity_key,
    effective_capability_json, context_slice_json, vfs_surface_json, mcp_surface_json,
    runtime_session_refs_json, execution_profile_json, visible_canvas_mount_ids_json,
    created_by_kind, created_by_id, created_at
)
SELECT
    b.frame_id,
    b.agent_id,
    1,
    procedure.id,
    b.graph_instance_id,
    'main_conversation',
    NULL,
    NULL,
    NULL,
    NULL,
    jsonb_build_array(
        jsonb_build_object(
            'kind', 'runtime_session',
            'session_id', b.session_id
        )
    )::text,
    b.executor_config_json,
    NULL,
    'session_lifecycle_shell_backfill',
    b.session_id,
    b.created_at
FROM __session_lifecycle_shell_backfill b
JOIN agent_procedures procedure
  ON procedure.project_id = b.project_id
 AND procedure.key = 'builtin.freeform_agent'
ON CONFLICT (id) DO NOTHING;

INSERT INTO agent_assignments (
    id, run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id,
    lease_status, assigned_at, released_at
)
SELECT
    b.assignment_id,
    b.run_id,
    b.graph_instance_id,
    'main_conversation',
    1,
    b.agent_id,
    b.frame_id,
    'active',
    b.created_at,
    NULL
FROM __session_lifecycle_shell_backfill b
ON CONFLICT (id) DO NOTHING;

INSERT INTO runtime_session_execution_anchors (
    runtime_session_id, run_id, launch_frame_id, agent_id, assignment_id,
    graph_instance_id, activity_key, attempt, created_by_kind, created_at, updated_at
)
SELECT
    b.session_id,
    b.run_id::uuid,
    b.frame_id::uuid,
    b.agent_id::uuid,
    b.assignment_id::uuid,
    b.graph_instance_id::uuid,
    'main_conversation',
    1,
    'session_lifecycle_shell_backfill',
    b.created_at,
    b.updated_at
FROM __session_lifecycle_shell_backfill b
ON CONFLICT (runtime_session_id) DO NOTHING;
