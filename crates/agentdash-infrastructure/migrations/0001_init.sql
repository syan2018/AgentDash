CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    config TEXT NOT NULL DEFAULT '{}',
    created_by_user_id TEXT NOT NULL DEFAULT 'system',
    updated_by_user_id TEXT NOT NULL DEFAULT 'system',
    visibility TEXT NOT NULL DEFAULT 'private',
    is_template BOOLEAN NOT NULL DEFAULT FALSE,
    cloned_from_project_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS project_subject_grants (
    project_id TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    role TEXT NOT NULL,
    granted_by_user_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project_id, subject_type, subject_id)
);

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    identity_kind TEXT NOT NULL DEFAULT 'local_dir',
    identity_payload TEXT NOT NULL DEFAULT '{}',
    resolution_policy TEXT NOT NULL DEFAULT 'prefer_online',
    default_binding_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workspace_bindings (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    backend_id TEXT NOT NULL,
    root_ref TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    detected_facts TEXT NOT NULL DEFAULT '{}',
    last_verified_at TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS stories (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    default_workspace_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'created',
    priority TEXT NOT NULL DEFAULT 'p2',
    story_type TEXT NOT NULL DEFAULT 'feature',
    tags TEXT NOT NULL DEFAULT '[]',
    task_count INTEGER NOT NULL DEFAULT 0,
    context TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS state_changes (
    id BIGSERIAL PRIMARY KEY,
    project_id TEXT NOT NULL DEFAULT '',
    entity_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    backend_id TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    story_id TEXT NOT NULL,
    workspace_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    execution_mode TEXT NOT NULL DEFAULT 'standard',
    agent_binding TEXT NOT NULL DEFAULT '{}',
    artifacts TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    project_id TEXT,
    title TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    last_event_seq BIGINT NOT NULL DEFAULT 0,
    last_execution_status TEXT NOT NULL DEFAULT 'idle',
    last_turn_id TEXT,
    last_terminal_message TEXT,
    executor_config_json TEXT,
    executor_session_id TEXT,
    companion_context_json TEXT,
    visible_canvas_mount_ids_json TEXT
);

CREATE TABLE IF NOT EXISTS session_events (
    session_id TEXT NOT NULL,
    event_seq BIGINT NOT NULL,
    occurred_at_ms BIGINT NOT NULL,
    committed_at_ms BIGINT NOT NULL,
    session_update_type TEXT NOT NULL,
    turn_id TEXT,
    entry_index BIGINT,
    tool_call_id TEXT,
    notification_json TEXT NOT NULL,
    PRIMARY KEY (session_id, event_seq)
);

CREATE TABLE IF NOT EXISTS backends (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    auth_token TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    backend_type TEXT NOT NULL DEFAULT 'local',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS views (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    backend_ids TEXT NOT NULL DEFAULT '[]',
    filters TEXT NOT NULL DEFAULT '{}',
    sort_by TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS user_preferences (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    scope_kind TEXT NOT NULL,
    scope_id TEXT NOT NULL DEFAULT '',
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (scope_kind, scope_id, key)
);

CREATE TABLE IF NOT EXISTS users (
    user_id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    auth_mode TEXT NOT NULL,
    display_name TEXT,
    email TEXT,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    provider TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS groups (
    group_id TEXT PRIMARY KEY,
    display_name TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS group_memberships (
    user_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, group_id)
);

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    agent_type TEXT NOT NULL,
    base_config TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS project_agent_links (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    config_override TEXT,
    default_lifecycle_key TEXT,
    is_default_for_story BOOLEAN NOT NULL DEFAULT FALSE,
    is_default_for_task BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, agent_id)
);

CREATE TABLE IF NOT EXISTS auth_sessions (
    token_hash TEXT PRIMARY KEY,
    identity_json TEXT NOT NULL,
    expires_at BIGINT NULL,
    revoked_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_procedures (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL,
    version INTEGER NOT NULL,
    contract TEXT NOT NULL,
    library_asset_id TEXT,
    source_ref TEXT,
    source_version TEXT,
    source_digest TEXT,
    installed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(project_id, key)
);

CREATE INDEX IF NOT EXISTS idx_agent_procedures_library_asset_id
    ON agent_procedures(library_asset_id);

CREATE TABLE IF NOT EXISTS workflow_graphs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL,
    version INTEGER NOT NULL,
    entry_activity_key TEXT NOT NULL,
    activities TEXT NOT NULL DEFAULT '[]',
    transitions TEXT NOT NULL DEFAULT '[]',
    library_asset_id TEXT,
    source_ref TEXT,
    source_version TEXT,
    source_digest TEXT,
    installed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(project_id, key)
);

CREATE INDEX IF NOT EXISTS idx_workflow_graphs_library_asset_id
    ON workflow_graphs(library_asset_id);

CREATE TABLE IF NOT EXISTS lifecycle_runs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    lifecycle_id TEXT NOT NULL,
    status TEXT NOT NULL,
    active_node_keys TEXT NOT NULL DEFAULT '[]',
    record_artifacts TEXT NOT NULL DEFAULT '[]',
    execution_log TEXT NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS lifecycle_workflow_instances (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    graph_id TEXT NOT NULL,
    role TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    activity_state_json TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lwi_run_id
    ON lifecycle_workflow_instances(run_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_lwi_run_root
    ON lifecycle_workflow_instances(run_id, role)
    WHERE role = 'root';

CREATE TABLE IF NOT EXISTS lifecycle_agents (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,
    agent_kind TEXT NOT NULL,
    agent_role TEXT NOT NULL DEFAULT 'primary',
    project_agent_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    current_frame_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_agents_run_id
    ON lifecycle_agents(run_id);

CREATE INDEX IF NOT EXISTS idx_lifecycle_agents_project_id
    ON lifecycle_agents(project_id);

CREATE TABLE IF NOT EXISTS agent_frames (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL DEFAULT 1,
    procedure_id TEXT,
    graph_instance_id TEXT,
    activity_key TEXT,
    effective_capability_json TEXT,
    context_slice_json TEXT,
    vfs_surface_json TEXT,
    mcp_surface_json TEXT,
    runtime_session_refs_json TEXT,
    execution_profile_json TEXT,
    created_by_kind TEXT NOT NULL DEFAULT 'system',
    created_by_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_frames_agent_revision
    ON agent_frames(agent_id, revision);

CREATE INDEX IF NOT EXISTS idx_agent_frames_agent_id
    ON agent_frames(agent_id);

CREATE TABLE IF NOT EXISTS agent_assignments (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    graph_instance_id TEXT NOT NULL,
    activity_key TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    agent_id TEXT NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    frame_id TEXT NOT NULL REFERENCES agent_frames(id) ON DELETE CASCADE,
    lease_status TEXT NOT NULL DEFAULT 'active',
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    released_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_agent_assignments_run_id
    ON agent_assignments(run_id);

CREATE INDEX IF NOT EXISTS idx_agent_assignments_graph_activity
    ON agent_assignments(graph_instance_id, activity_key, attempt);

CREATE INDEX IF NOT EXISTS idx_agent_assignments_agent_id
    ON agent_assignments(agent_id);

CREATE TABLE IF NOT EXISTS lifecycle_subject_associations (
    id TEXT PRIMARY KEY,
    anchor_run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    anchor_agent_id TEXT,
    subject_kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    role TEXT NOT NULL,
    metadata_json TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lsa_anchor_run
    ON lifecycle_subject_associations(anchor_run_id);

CREATE INDEX IF NOT EXISTS idx_lsa_anchor_agent
    ON lifecycle_subject_associations(anchor_agent_id)
    WHERE anchor_agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_lsa_subject
    ON lifecycle_subject_associations(subject_kind, subject_id);

CREATE TABLE IF NOT EXISTS lifecycle_gates (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id TEXT,
    frame_id TEXT,
    gate_kind TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    payload_json TEXT,
    resolved_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_run_id
    ON lifecycle_gates(run_id);

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_agent_status
    ON lifecycle_gates(agent_id, status)
    WHERE agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_correlation
    ON lifecycle_gates(correlation_id);

CREATE TABLE IF NOT EXISTS agent_lineages (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    parent_agent_id TEXT,
    child_agent_id TEXT NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    relation_kind TEXT NOT NULL,
    source_frame_id TEXT,
    metadata_json TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS canvases (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    mount_id TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    entry_file TEXT NOT NULL,
    sandbox_config TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS canvas_files (
    canvas_id TEXT NOT NULL,
    path TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (canvas_id, path)
);

CREATE TABLE IF NOT EXISTS canvas_bindings (
    canvas_id TEXT NOT NULL,
    alias TEXT NOT NULL,
    source_uri TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/json',
    PRIMARY KEY (canvas_id, alias)
);
