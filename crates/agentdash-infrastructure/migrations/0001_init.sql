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
    session_id TEXT,
    agent_binding TEXT NOT NULL DEFAULT '{}',
    artifacts TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_bindings (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL DEFAULT '',
    session_id TEXT NOT NULL,
    owner_type TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
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

CREATE TABLE IF NOT EXISTS workflow_definitions (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    binding_kind TEXT NOT NULL,
    recommended_binding_roles TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL,
    status TEXT NOT NULL,
    version INTEGER NOT NULL,
    contract TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS lifecycle_definitions (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    binding_kind TEXT NOT NULL,
    recommended_binding_roles TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL,
    status TEXT NOT NULL,
    version INTEGER NOT NULL,
    entry_step_key TEXT NOT NULL,
    steps TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_assignments (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    lifecycle_id TEXT NOT NULL,
    role TEXT NOT NULL,
    enabled BOOLEAN NOT NULL,
    is_default BOOLEAN NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS lifecycle_runs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    lifecycle_id TEXT NOT NULL,
    binding_kind TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    status TEXT NOT NULL,
    current_step_key TEXT,
    step_states TEXT NOT NULL,
    record_artifacts TEXT NOT NULL,
    execution_log TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_activity_at TEXT NOT NULL
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
