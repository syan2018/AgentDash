CREATE TABLE activity_execution_claims (
    claim_id text NOT NULL,
    run_id text NOT NULL,
    activity_key text NOT NULL,
    attempt integer NOT NULL,
    executor_kind text NOT NULL,
    status text NOT NULL,
    idempotency_key text NOT NULL,
    executor_run_ref text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    graph_instance_id text NOT NULL
);

CREATE TABLE agent_assignments (
    id text NOT NULL,
    run_id text NOT NULL,
    graph_instance_id text NOT NULL,
    activity_key text NOT NULL,
    attempt integer NOT NULL,
    agent_id text NOT NULL,
    frame_id text NOT NULL,
    lease_status text DEFAULT 'active'::text NOT NULL,
    assigned_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    released_at timestamp with time zone
);

CREATE TABLE agent_frame_transitions (
    id text NOT NULL,
    target_frame_id text NOT NULL,
    run_id text NOT NULL,
    lifecycle_key text NOT NULL,
    phase_node text NOT NULL,
    capability_keys_json text NOT NULL,
    transition_json text NOT NULL,
    source_turn_id text,
    created_at_ms bigint NOT NULL
);

CREATE TABLE agent_frames (
    id text NOT NULL,
    agent_id text NOT NULL,
    revision integer DEFAULT 1 NOT NULL,
    procedure_id text,
    graph_instance_id text,
    activity_key text,
    effective_capability_json text,
    context_slice_json text,
    vfs_surface_json text,
    mcp_surface_json text,
    created_by_kind text NOT NULL,
    created_by_id text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    execution_profile_json text,
    visible_canvas_mount_ids_json text
);

CREATE TABLE agent_lineages (
    id text NOT NULL,
    run_id text NOT NULL,
    parent_agent_id text,
    child_agent_id text NOT NULL,
    relation_kind text NOT NULL,
    source_frame_id text,
    metadata_json text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE agent_procedures (
    id text CONSTRAINT agent_procedures_id_not_null NOT NULL,
    key text CONSTRAINT agent_procedures_key_not_null NOT NULL,
    name text CONSTRAINT agent_procedures_name_not_null NOT NULL,
    description text DEFAULT ''::text CONSTRAINT agent_procedures_description_not_null NOT NULL,
    source text CONSTRAINT agent_procedures_source_not_null NOT NULL,
    version integer CONSTRAINT agent_procedures_version_not_null NOT NULL,
    contract text CONSTRAINT agent_procedures_contract_not_null NOT NULL,
    created_at timestamp with time zone CONSTRAINT agent_procedures_created_at_not_null NOT NULL,
    updated_at timestamp with time zone CONSTRAINT agent_procedures_updated_at_not_null NOT NULL,
    project_id text CONSTRAINT agent_procedures_project_id_not_null NOT NULL,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone
);

CREATE TABLE auth_sessions (
    token_hash text NOT NULL,
    identity_json text NOT NULL,
    expires_at bigint,
    revoked_at bigint,
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL
);

CREATE TABLE backend_execution_leases (
    id text NOT NULL,
    backend_id text NOT NULL,
    session_id text NOT NULL,
    turn_id text NOT NULL,
    executor_id text NOT NULL,
    workspace_id text,
    root_ref text,
    selection_mode text NOT NULL,
    state text NOT NULL,
    claim_reason text,
    terminal_kind text,
    release_reason text,
    claimed_at timestamp with time zone NOT NULL,
    activated_at timestamp with time zone,
    released_at timestamp with time zone,
    last_seen_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    CONSTRAINT backend_execution_leases_selection_mode_check CHECK ((selection_mode = ANY (ARRAY['explicit'::text, 'auto_idle'::text, 'workspace_binding'::text]))),
    CONSTRAINT backend_execution_leases_state_check CHECK ((state = ANY (ARRAY['claimed'::text, 'running'::text, 'released'::text, 'lost'::text, 'failed'::text]))),
    CONSTRAINT backend_execution_leases_terminal_kind_check CHECK (((terminal_kind IS NULL) OR (terminal_kind = ANY (ARRAY['completed'::text, 'failed'::text, 'interrupted'::text]))))
);

CREATE TABLE backend_workspace_inventory (
    id text NOT NULL,
    backend_id text NOT NULL,
    root_ref text NOT NULL,
    identity_kind text NOT NULL,
    identity_payload text DEFAULT '{}'::text NOT NULL,
    detected_facts text DEFAULT '{}'::text NOT NULL,
    status text DEFAULT 'available'::text NOT NULL,
    source text DEFAULT 'manual_refresh'::text NOT NULL,
    last_seen_at timestamp with time zone NOT NULL,
    last_error text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE backends (
    id text NOT NULL,
    name text NOT NULL,
    endpoint text NOT NULL,
    auth_token text,
    enabled boolean DEFAULT true NOT NULL,
    backend_type text DEFAULT 'local'::text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    owner_user_id text,
    profile_id text,
    device_id text,
    device jsonb DEFAULT '{}'::jsonb NOT NULL,
    last_claimed_at timestamp with time zone,
    machine_id text,
    machine_label text,
    legacy_machine_ids jsonb DEFAULT '[]'::jsonb NOT NULL,
    visibility text DEFAULT 'private'::text NOT NULL,
    share_scope_kind text DEFAULT 'user'::text NOT NULL,
    share_scope_id text,
    capability_slot text DEFAULT 'default'::text NOT NULL
);

CREATE TABLE canvas_bindings (
    canvas_id text NOT NULL,
    alias text NOT NULL,
    source_uri text NOT NULL,
    content_type text DEFAULT 'application/json'::text NOT NULL
);

CREATE TABLE canvas_files (
    canvas_id text NOT NULL,
    path text NOT NULL,
    content text DEFAULT ''::text NOT NULL
);

CREATE TABLE canvases (
    id text NOT NULL,
    project_id text NOT NULL,
    mount_id text DEFAULT ''::text NOT NULL,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    entry_file text NOT NULL,
    sandbox_config text DEFAULT '{}'::text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE extension_package_artifacts (
    id text NOT NULL,
    extension_id text NOT NULL,
    package_name text NOT NULL,
    package_version text NOT NULL,
    asset_version text NOT NULL,
    source_version text NOT NULL,
    storage_ref text NOT NULL,
    archive_digest text NOT NULL,
    manifest_digest text NOT NULL,
    manifest jsonb NOT NULL,
    byte_size bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    owner_kind text NOT NULL,
    owner_id text NOT NULL,
    CONSTRAINT extension_package_artifacts_digest_format CHECK ((archive_digest ~~ 'sha256:%'::text)),
    CONSTRAINT extension_package_artifacts_manifest_digest_format CHECK ((manifest_digest ~~ 'sha256:%'::text)),
    CONSTRAINT extension_package_artifacts_owner_kind_check CHECK ((owner_kind = ANY (ARRAY['project'::text, 'library_asset'::text])))
);

CREATE TABLE group_memberships (
    user_id text NOT NULL,
    group_id text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE groups (
    group_id text NOT NULL,
    display_name text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE inline_fs_files (
    id text NOT NULL,
    owner_kind text NOT NULL,
    owner_id text NOT NULL,
    container_id text NOT NULL,
    path text NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    content_kind text NOT NULL,
    mime_type text,
    text_content text,
    binary_content bytea,
    size_bytes bigint NOT NULL,
    CONSTRAINT chk_inline_fs_files_content_kind CHECK ((content_kind = ANY (ARRAY['text'::text, 'binary'::text]))),
    CONSTRAINT chk_inline_fs_files_content_payload CHECK ((((content_kind = 'text'::text) AND (text_content IS NOT NULL) AND (binary_content IS NULL)) OR ((content_kind = 'binary'::text) AND (binary_content IS NOT NULL) AND (text_content IS NULL) AND (mime_type IS NOT NULL))))
);

CREATE TABLE library_assets (
    id text NOT NULL,
    asset_type text NOT NULL,
    scope text NOT NULL,
    owner_id text,
    key text NOT NULL,
    display_name text NOT NULL,
    description text,
    version text NOT NULL,
    source text NOT NULL,
    source_ref text,
    payload_digest text NOT NULL,
    deprecated boolean DEFAULT false NOT NULL,
    payload jsonb NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    CONSTRAINT library_assets_scope_check CHECK ((scope = ANY (ARRAY['builtin'::text, 'system'::text, 'org'::text, 'user'::text]))),
    CONSTRAINT library_assets_source_check CHECK ((source = ANY (ARRAY['builtin'::text, 'user_authored'::text, 'remote_imported'::text, 'plugin_embedded'::text]))),
    CONSTRAINT library_assets_type_check CHECK ((asset_type = ANY (ARRAY['agent_template'::text, 'mcp_server_template'::text, 'workflow_template'::text, 'skill_template'::text, 'vfs_mount_template'::text, 'extension_template'::text])))
);

CREATE TABLE lifecycle_agents (
    id text NOT NULL,
    run_id text NOT NULL,
    project_id text NOT NULL,
    agent_kind text NOT NULL,
    agent_role text DEFAULT 'primary'::text NOT NULL,
    project_agent_id text,
    status text DEFAULT 'active'::text NOT NULL,
    current_frame_id text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    bootstrap_status text DEFAULT 'not_applicable'::text NOT NULL
);

CREATE TABLE lifecycle_gates (
    id text NOT NULL,
    run_id text NOT NULL,
    agent_id text,
    frame_id text,
    gate_kind text NOT NULL,
    correlation_id text NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    payload_json text,
    resolved_by text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    resolved_at timestamp with time zone
);

CREATE TABLE lifecycle_runs (
    id text NOT NULL,
    project_id text NOT NULL,
    topology text NOT NULL,
    root_graph_id text,
    status text NOT NULL,
    execution_log text DEFAULT '[]'::text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    last_activity_at timestamp with time zone NOT NULL
);

CREATE TABLE lifecycle_subject_associations (
    id text NOT NULL,
    anchor_run_id text NOT NULL,
    anchor_agent_id text,
    subject_kind text NOT NULL,
    subject_id text NOT NULL,
    role text NOT NULL,
    metadata_json text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE lifecycle_workflow_instances (
    id text NOT NULL,
    run_id text NOT NULL,
    graph_id text NOT NULL,
    role text NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    activity_state_json text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE llm_provider_user_credentials (
    id text NOT NULL,
    provider_id text NOT NULL,
    user_id text NOT NULL,
    api_key_ciphertext text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    verification_status text DEFAULT 'unverified'::text NOT NULL,
    verification_message text DEFAULT ''::text NOT NULL,
    verified_at timestamp with time zone
);

CREATE TABLE llm_providers (
    id text NOT NULL,
    name text NOT NULL,
    slug text NOT NULL,
    protocol text NOT NULL,
    base_url text DEFAULT ''::text NOT NULL,
    wire_api text DEFAULT ''::text NOT NULL,
    default_model text DEFAULT ''::text NOT NULL,
    models text DEFAULT '[]'::text NOT NULL,
    blocked_models text DEFAULT '[]'::text NOT NULL,
    env_api_key text DEFAULT ''::text NOT NULL,
    discovery_url text DEFAULT ''::text NOT NULL,
    sort_order integer DEFAULT 0 NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    credential_mode text DEFAULT 'global_only'::text NOT NULL,
    global_api_key_ciphertext text DEFAULT ''::text NOT NULL
);

CREATE TABLE mcp_presets (
    id text NOT NULL,
    project_id text NOT NULL,
    description text,
    source text NOT NULL,
    builtin_key text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    key text NOT NULL,
    display_name text NOT NULL,
    transport text NOT NULL,
    route_policy text NOT NULL,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone,
    CONSTRAINT mcp_presets_builtin_key_consistency CHECK ((((source = 'builtin'::text) AND (builtin_key IS NOT NULL)) OR ((source = 'user'::text) AND (builtin_key IS NULL)))),
    CONSTRAINT mcp_presets_source_check CHECK ((source = ANY (ARRAY['builtin'::text, 'user'::text])))
);

CREATE TABLE permission_grants (
    id text NOT NULL,
    run_id text NOT NULL,
    source_runtime_session_id text CONSTRAINT permission_grants_source_runtime_session_id_not_null NOT NULL,
    source_turn_id text,
    source_tool_call_id text,
    requested_paths jsonb NOT NULL,
    reason text NOT NULL,
    grant_scope text NOT NULL,
    expires_at timestamp with time zone,
    scope_escalation_intent jsonb,
    status text DEFAULT 'created'::text NOT NULL,
    policy_decision jsonb,
    approved_by text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    effect_frame_id text
);

CREATE TABLE project_agents (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    agent_type text NOT NULL,
    config text DEFAULT '{}'::text NOT NULL,
    installed_library_asset_id text,
    installed_source_ref text,
    installed_source_version text,
    installed_source_digest text,
    installed_at timestamp with time zone,
    default_lifecycle_key text,
    is_default_for_story boolean DEFAULT false NOT NULL,
    is_default_for_task boolean DEFAULT false NOT NULL,
    knowledge_enabled boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE project_backend_access (
    id text NOT NULL,
    project_id text NOT NULL,
    backend_id text NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    access_mode text DEFAULT 'use_inventory'::text NOT NULL,
    priority integer DEFAULT 0 NOT NULL,
    root_policy text DEFAULT '{"kind":"backend_inventory"}'::text NOT NULL,
    capability_policy text DEFAULT '{}'::text NOT NULL,
    note text,
    created_by text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE project_extension_installations (
    id text NOT NULL,
    project_id text NOT NULL,
    extension_key text NOT NULL,
    display_name text NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    config jsonb DEFAULT '{}'::jsonb NOT NULL,
    manifest jsonb NOT NULL,
    installed_library_asset_id text,
    installed_source_ref text,
    installed_source_version text,
    installed_source_digest text,
    installed_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    package_artifact_id text,
    package_name text,
    package_version text,
    package_asset_version text,
    package_source_version text,
    artifact_storage_ref text,
    artifact_archive_digest text,
    artifact_manifest_digest text
);

CREATE TABLE project_subject_grants (
    project_id text NOT NULL,
    subject_type text NOT NULL,
    subject_id text NOT NULL,
    role text NOT NULL,
    granted_by_user_id text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE project_vfs_mounts (
    id text NOT NULL,
    project_id text NOT NULL,
    mount_id text NOT NULL,
    display_name text NOT NULL,
    description text,
    capabilities text DEFAULT '[]'::text NOT NULL,
    installed_source text,
    content text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE projects (
    id text NOT NULL,
    name text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    config text DEFAULT '{}'::text NOT NULL,
    created_by_user_id text DEFAULT 'system'::text NOT NULL,
    updated_by_user_id text DEFAULT 'system'::text NOT NULL,
    visibility text DEFAULT 'private'::text NOT NULL,
    is_template boolean DEFAULT false NOT NULL,
    cloned_from_project_id text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE routine_executions (
    id text NOT NULL,
    routine_id text NOT NULL,
    trigger_source text NOT NULL,
    trigger_payload text,
    resolved_prompt text,
    status text DEFAULT 'pending'::text NOT NULL,
    started_at timestamp with time zone NOT NULL,
    completed_at timestamp with time zone,
    error text,
    entity_key text,
    dispatch_run_id text,
    dispatch_agent_id text,
    dispatch_frame_id text,
    dispatch_assignment_id text
);

CREATE TABLE routines (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    prompt_template text NOT NULL,
    project_agent_id text CONSTRAINT routines_agent_id_not_null NOT NULL,
    trigger_config text NOT NULL,
    dispatch_strategy text CONSTRAINT routines_dispatch_strategy_not_null NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    last_fired_at timestamp with time zone
);

CREATE TABLE runtime_health (
    backend_id text NOT NULL,
    profile_id text,
    name text NOT NULL,
    status text NOT NULL,
    version text,
    capabilities jsonb DEFAULT '{}'::jsonb NOT NULL,
    workspace_roots jsonb DEFAULT '[]'::jsonb CONSTRAINT runtime_health_workspace_roots_not_null NOT NULL,
    device jsonb DEFAULT '{}'::jsonb NOT NULL,
    connected_at timestamp with time zone,
    last_seen_at timestamp with time zone,
    disconnected_at timestamp with time zone,
    disconnect_reason text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT runtime_health_status_check CHECK ((status = ANY (ARRAY['online'::text, 'offline'::text, 'starting'::text, 'degraded'::text, 'stopping'::text, 'error'::text])))
);

CREATE TABLE runtime_session_execution_anchors (
    runtime_session_id text NOT NULL,
    run_id text NOT NULL,
    launch_frame_id text NOT NULL,
    agent_id text NOT NULL,
    assignment_id text,
    graph_instance_id text,
    activity_key text,
    attempt integer,
    created_by_kind text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE session_compactions (
    id text NOT NULL,
    session_id text NOT NULL,
    projection_kind text NOT NULL,
    projection_version bigint NOT NULL,
    lifecycle_item_id text NOT NULL,
    start_event_seq bigint NOT NULL,
    completed_event_seq bigint,
    failed_event_seq bigint,
    status text NOT NULL,
    trigger text NOT NULL,
    reason text,
    phase text,
    strategy text NOT NULL,
    budget_scope text,
    base_head_event_seq bigint,
    source_start_event_seq bigint,
    source_end_event_seq bigint,
    first_kept_event_seq bigint,
    summary text DEFAULT ''::text NOT NULL,
    replacement_projection_json text DEFAULT '{}'::text NOT NULL,
    token_stats_json text DEFAULT '{}'::text NOT NULL,
    diagnostics_json text DEFAULT '{}'::text NOT NULL,
    created_by text,
    created_at_ms bigint NOT NULL,
    completed_at_ms bigint
);

CREATE TABLE session_events (
    session_id text NOT NULL,
    event_seq bigint NOT NULL,
    occurred_at_ms bigint NOT NULL,
    committed_at_ms bigint NOT NULL,
    session_update_type text NOT NULL,
    turn_id text,
    entry_index bigint,
    tool_call_id text,
    notification_json text NOT NULL
);

CREATE TABLE session_lineage (
    child_session_id text NOT NULL,
    parent_session_id text NOT NULL,
    relation_kind text NOT NULL,
    fork_point_event_seq bigint,
    fork_point_ref_json text DEFAULT '{}'::text NOT NULL,
    fork_point_compaction_id text,
    status text NOT NULL,
    created_at_ms bigint NOT NULL,
    updated_at_ms bigint NOT NULL,
    metadata_json text DEFAULT '{}'::text NOT NULL,
    CONSTRAINT session_lineage_check CHECK ((child_session_id <> parent_session_id))
);

CREATE TABLE session_projection_heads (
    session_id text NOT NULL,
    projection_kind text NOT NULL,
    projection_version bigint NOT NULL,
    head_event_seq bigint NOT NULL,
    active_compaction_id text,
    updated_by_event_seq bigint,
    updated_at_ms bigint NOT NULL
);

CREATE TABLE session_projection_segments (
    id text NOT NULL,
    session_id text NOT NULL,
    projection_kind text NOT NULL,
    projection_version bigint NOT NULL,
    sort_order bigint NOT NULL,
    segment_type text NOT NULL,
    origin text NOT NULL,
    synthetic boolean DEFAULT false NOT NULL,
    source_start_event_seq bigint,
    source_end_event_seq bigint,
    source_refs_json text DEFAULT '[]'::text NOT NULL,
    generated_by_compaction_id text,
    content_json text NOT NULL,
    token_estimate bigint,
    created_at_ms bigint NOT NULL
);

CREATE TABLE session_runtime_commands (
    id text NOT NULL,
    session_id text NOT NULL,
    phase_node text NOT NULL,
    status text NOT NULL,
    payload_json text NOT NULL,
    created_at_ms bigint NOT NULL,
    updated_at_ms bigint NOT NULL,
    applied_at_ms bigint,
    failed_at_ms bigint,
    last_error text,
    frame_transition_id text NOT NULL
);

CREATE TABLE session_terminal_effects (
    id text NOT NULL,
    session_id text NOT NULL,
    turn_id text NOT NULL,
    terminal_event_seq bigint NOT NULL,
    effect_type text NOT NULL,
    payload_json text NOT NULL,
    status text NOT NULL,
    attempt_count bigint DEFAULT 0 NOT NULL,
    created_at_ms bigint NOT NULL,
    updated_at_ms bigint NOT NULL,
    last_error text
);

CREATE TABLE sessions (
    id text NOT NULL,
    title text NOT NULL,
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL,
    last_event_seq bigint DEFAULT 0 NOT NULL,
    last_delivery_status text DEFAULT 'idle'::text NOT NULL,
    last_turn_id text,
    last_terminal_message text,
    executor_session_id text,
    title_source text DEFAULT 'auto'::text NOT NULL
);

CREATE TABLE settings (
    scope_kind text NOT NULL,
    scope_id text DEFAULT ''::text NOT NULL,
    key text NOT NULL,
    value text NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE skill_assets (
    id text NOT NULL,
    project_id text NOT NULL,
    key text NOT NULL,
    display_name text NOT NULL,
    description text NOT NULL,
    source text NOT NULL,
    builtin_key text,
    disable_model_invocation boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    remote_source_url text,
    remote_imported_at timestamp with time zone,
    remote_digest text,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone,
    CONSTRAINT skill_assets_builtin_key_consistency CHECK ((((source = 'builtin_seed'::text) AND (builtin_key IS NOT NULL)) OR ((source <> 'builtin_seed'::text) AND (builtin_key IS NULL)))),
    CONSTRAINT skill_assets_source_check CHECK ((source = ANY (ARRAY['builtin_seed'::text, 'user'::text, 'github'::text, 'clawhub'::text, 'skills_sh'::text])))
);

CREATE TABLE state_changes (
    id bigint NOT NULL,
    project_id text NOT NULL,
    entity_id text NOT NULL,
    kind text NOT NULL,
    payload text DEFAULT '{}'::text NOT NULL,
    backend_id text,
    created_at timestamp with time zone NOT NULL
);

CREATE SEQUENCE state_changes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE state_changes_id_seq OWNED BY state_changes.id;

CREATE TABLE stories (
    id text NOT NULL,
    project_id text NOT NULL,
    default_workspace_id text,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    status text DEFAULT 'created'::text NOT NULL,
    priority text DEFAULT 'p2'::text NOT NULL,
    story_type text DEFAULT 'feature'::text NOT NULL,
    tags text DEFAULT '[]'::text NOT NULL,
    task_count integer DEFAULT 0 NOT NULL,
    context text DEFAULT '{}'::text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    tasks jsonb DEFAULT '[]'::jsonb NOT NULL
);

CREATE TABLE user_preferences (
    key text NOT NULL,
    value text NOT NULL
);

CREATE TABLE users (
    user_id text NOT NULL,
    subject text NOT NULL,
    auth_mode text NOT NULL,
    display_name text,
    email text,
    is_admin boolean DEFAULT false NOT NULL,
    provider text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    avatar_url text
);

CREATE TABLE views (
    id text NOT NULL,
    name text NOT NULL,
    backend_ids text DEFAULT '[]'::text NOT NULL,
    filters text DEFAULT '{}'::text NOT NULL,
    sort_by text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE workflow_graphs (
    id text CONSTRAINT workflow_graphs_id_not_null NOT NULL,
    key text CONSTRAINT workflow_graphs_key_not_null NOT NULL,
    name text CONSTRAINT workflow_graphs_name_not_null NOT NULL,
    description text DEFAULT ''::text CONSTRAINT workflow_graphs_description_not_null NOT NULL,
    source text CONSTRAINT workflow_graphs_source_not_null NOT NULL,
    version integer CONSTRAINT workflow_graphs_version_not_null NOT NULL,
    created_at timestamp with time zone CONSTRAINT workflow_graphs_created_at_not_null NOT NULL,
    updated_at timestamp with time zone CONSTRAINT workflow_graphs_updated_at_not_null NOT NULL,
    project_id text CONSTRAINT workflow_graphs_project_id_not_null NOT NULL,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone,
    entry_activity_key text DEFAULT ''::text CONSTRAINT workflow_graphs_entry_activity_key_not_null NOT NULL,
    activities text DEFAULT '[]'::text CONSTRAINT workflow_graphs_activities_not_null NOT NULL,
    transitions text DEFAULT '[]'::text CONSTRAINT workflow_graphs_transitions_not_null NOT NULL
);

CREATE TABLE workspace_bindings (
    id text NOT NULL,
    workspace_id text NOT NULL,
    backend_id text NOT NULL,
    root_ref text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    detected_facts text DEFAULT '{}'::text NOT NULL,
    last_verified_at timestamp with time zone,
    priority integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE workspaces (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    identity_kind text DEFAULT 'local_dir'::text NOT NULL,
    identity_payload text DEFAULT '{}'::text NOT NULL,
    resolution_policy text DEFAULT 'prefer_online'::text NOT NULL,
    default_binding_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    mount_capabilities text DEFAULT '["read","write","list","search","exec"]'::text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

ALTER TABLE ONLY state_changes ALTER COLUMN id SET DEFAULT nextval('state_changes_id_seq'::regclass);

ALTER TABLE ONLY activity_execution_claims
    ADD CONSTRAINT activity_execution_claims_idempotency_key_key UNIQUE (idempotency_key);

ALTER TABLE ONLY activity_execution_claims
    ADD CONSTRAINT activity_execution_claims_pkey PRIMARY KEY (claim_id);

ALTER TABLE ONLY agent_assignments
    ADD CONSTRAINT agent_assignments_pkey PRIMARY KEY (id);

ALTER TABLE ONLY agent_frame_transitions
    ADD CONSTRAINT agent_frame_transitions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY agent_frames
    ADD CONSTRAINT agent_frames_pkey PRIMARY KEY (id);

ALTER TABLE ONLY agent_lineages
    ADD CONSTRAINT agent_lineages_pkey PRIMARY KEY (id);

ALTER TABLE ONLY auth_sessions
    ADD CONSTRAINT auth_sessions_pkey PRIMARY KEY (token_hash);

ALTER TABLE ONLY backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_pkey PRIMARY KEY (id);

ALTER TABLE ONLY backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_session_id_turn_id_key UNIQUE (session_id, turn_id);

ALTER TABLE ONLY backend_workspace_inventory
    ADD CONSTRAINT backend_workspace_inventory_backend_id_root_ref_key UNIQUE (backend_id, root_ref);

ALTER TABLE ONLY backend_workspace_inventory
    ADD CONSTRAINT backend_workspace_inventory_pkey PRIMARY KEY (id);

ALTER TABLE ONLY backends
    ADD CONSTRAINT backends_pkey PRIMARY KEY (id);

ALTER TABLE ONLY canvas_bindings
    ADD CONSTRAINT canvas_bindings_pkey PRIMARY KEY (canvas_id, alias);

ALTER TABLE ONLY canvas_files
    ADD CONSTRAINT canvas_files_pkey PRIMARY KEY (canvas_id, path);

ALTER TABLE ONLY canvases
    ADD CONSTRAINT canvases_pkey PRIMARY KEY (id);

ALTER TABLE ONLY extension_package_artifacts
    ADD CONSTRAINT extension_package_artifacts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY group_memberships
    ADD CONSTRAINT group_memberships_pkey PRIMARY KEY (user_id, group_id);

ALTER TABLE ONLY groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (group_id);

ALTER TABLE ONLY inline_fs_files
    ADD CONSTRAINT inline_fs_files_owner_kind_owner_id_container_id_path_key UNIQUE (owner_kind, owner_id, container_id, path);

ALTER TABLE ONLY inline_fs_files
    ADD CONSTRAINT inline_fs_files_pkey PRIMARY KEY (id);

ALTER TABLE ONLY library_assets
    ADD CONSTRAINT library_assets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_pkey PRIMARY KEY (id);

ALTER TABLE ONLY workflow_graphs
    ADD CONSTRAINT workflow_graphs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY lifecycle_gates
    ADD CONSTRAINT lifecycle_gates_pkey PRIMARY KEY (id);

ALTER TABLE ONLY lifecycle_runs
    ADD CONSTRAINT lifecycle_runs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY lifecycle_runs
    ADD CONSTRAINT lifecycle_runs_topology_root_graph_check CHECK (
        (topology = 'graphless' AND root_graph_id IS NULL)
        OR (topology = 'workflow_graph' AND root_graph_id IS NOT NULL)
    );

ALTER TABLE ONLY lifecycle_subject_associations
    ADD CONSTRAINT lifecycle_subject_associations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY lifecycle_workflow_instances
    ADD CONSTRAINT lifecycle_workflow_instances_pkey PRIMARY KEY (id);

ALTER TABLE ONLY llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_pkey PRIMARY KEY (id);

ALTER TABLE ONLY llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_provider_id_user_id_key UNIQUE (provider_id, user_id);

ALTER TABLE ONLY llm_providers
    ADD CONSTRAINT llm_providers_pkey PRIMARY KEY (id);

ALTER TABLE ONLY llm_providers
    ADD CONSTRAINT llm_providers_slug_key UNIQUE (slug);

ALTER TABLE ONLY mcp_presets
    ADD CONSTRAINT mcp_presets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY permission_grants
    ADD CONSTRAINT permission_grants_pkey PRIMARY KEY (id);

ALTER TABLE ONLY project_agents
    ADD CONSTRAINT project_agents_pkey PRIMARY KEY (id);

ALTER TABLE ONLY project_agents
    ADD CONSTRAINT project_agents_project_id_name_key UNIQUE (project_id, name);

ALTER TABLE ONLY project_backend_access
    ADD CONSTRAINT project_backend_access_pkey PRIMARY KEY (id);

ALTER TABLE ONLY project_backend_access
    ADD CONSTRAINT project_backend_access_project_id_backend_id_key UNIQUE (project_id, backend_id);

ALTER TABLE ONLY project_extension_installations
    ADD CONSTRAINT project_extension_installations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY project_extension_installations
    ADD CONSTRAINT project_extension_installations_unique_key UNIQUE (project_id, extension_key);

ALTER TABLE ONLY project_subject_grants
    ADD CONSTRAINT project_subject_grants_pkey PRIMARY KEY (project_id, subject_type, subject_id);

ALTER TABLE ONLY project_vfs_mounts
    ADD CONSTRAINT project_vfs_mounts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY project_vfs_mounts
    ADD CONSTRAINT project_vfs_mounts_project_id_mount_id_key UNIQUE (project_id, mount_id);

ALTER TABLE ONLY projects
    ADD CONSTRAINT projects_pkey PRIMARY KEY (id);

ALTER TABLE ONLY routine_executions
    ADD CONSTRAINT routine_executions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY routines
    ADD CONSTRAINT routines_pkey PRIMARY KEY (id);

ALTER TABLE ONLY routines
    ADD CONSTRAINT routines_project_id_name_key UNIQUE (project_id, name);

ALTER TABLE ONLY runtime_health
    ADD CONSTRAINT runtime_health_pkey PRIMARY KEY (backend_id);

ALTER TABLE ONLY runtime_session_execution_anchors
    ADD CONSTRAINT runtime_session_execution_anchors_pkey PRIMARY KEY (runtime_session_id);

ALTER TABLE ONLY session_compactions
    ADD CONSTRAINT session_compactions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY session_events
    ADD CONSTRAINT session_events_pkey PRIMARY KEY (session_id, event_seq);

ALTER TABLE ONLY session_lineage
    ADD CONSTRAINT session_lineage_pkey PRIMARY KEY (child_session_id);

ALTER TABLE ONLY session_projection_heads
    ADD CONSTRAINT session_projection_heads_pkey PRIMARY KEY (session_id, projection_kind);

ALTER TABLE ONLY session_projection_segments
    ADD CONSTRAINT session_projection_segments_pkey PRIMARY KEY (id);

ALTER TABLE ONLY session_projection_segments
    ADD CONSTRAINT session_projection_segments_session_kind_version_order_key UNIQUE (session_id, projection_kind, projection_version, sort_order);

ALTER TABLE ONLY session_runtime_commands
    ADD CONSTRAINT session_runtime_commands_pkey PRIMARY KEY (id);

ALTER TABLE ONLY session_terminal_effects
    ADD CONSTRAINT session_terminal_effects_pkey PRIMARY KEY (id);

ALTER TABLE ONLY sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY settings
    ADD CONSTRAINT settings_pkey PRIMARY KEY (scope_kind, scope_id, key);

ALTER TABLE ONLY skill_assets
    ADD CONSTRAINT skill_assets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY state_changes
    ADD CONSTRAINT state_changes_pkey PRIMARY KEY (id);

ALTER TABLE ONLY stories
    ADD CONSTRAINT stories_pkey PRIMARY KEY (id);

ALTER TABLE ONLY user_preferences
    ADD CONSTRAINT user_preferences_pkey PRIMARY KEY (key);

ALTER TABLE ONLY users
    ADD CONSTRAINT users_pkey PRIMARY KEY (user_id);

ALTER TABLE ONLY views
    ADD CONSTRAINT views_pkey PRIMARY KEY (id);

ALTER TABLE ONLY agent_procedures
    ADD CONSTRAINT agent_procedures_pkey PRIMARY KEY (id);

ALTER TABLE ONLY workspace_bindings
    ADD CONSTRAINT workspace_bindings_pkey PRIMARY KEY (id);

ALTER TABLE ONLY workspaces
    ADD CONSTRAINT workspaces_pkey PRIMARY KEY (id);

CREATE INDEX idx_activity_execution_claims_run_id ON activity_execution_claims USING btree (run_id);

CREATE INDEX idx_agent_assignments_active_agent ON agent_assignments USING btree (agent_id, lease_status) WHERE (lease_status = 'active'::text);

CREATE INDEX idx_agent_assignments_agent_id ON agent_assignments USING btree (agent_id);

CREATE INDEX idx_agent_assignments_graph_activity ON agent_assignments USING btree (graph_instance_id, activity_key, attempt);

CREATE INDEX idx_agent_assignments_run_id ON agent_assignments USING btree (run_id);

CREATE INDEX idx_agent_frame_transitions_run_phase ON agent_frame_transitions USING btree (run_id, lifecycle_key, phase_node);

CREATE INDEX idx_agent_frame_transitions_target_frame ON agent_frame_transitions USING btree (target_frame_id, created_at_ms);

CREATE INDEX idx_agent_frames_agent_id ON agent_frames USING btree (agent_id);

CREATE UNIQUE INDEX idx_agent_frames_agent_revision ON agent_frames USING btree (agent_id, revision);

CREATE INDEX idx_agent_lineages_child ON agent_lineages USING btree (child_agent_id);

CREATE INDEX idx_agent_lineages_parent ON agent_lineages USING btree (parent_agent_id) WHERE (parent_agent_id IS NOT NULL);

CREATE INDEX idx_agent_lineages_run_id ON agent_lineages USING btree (run_id);

CREATE INDEX idx_backend_execution_leases_active_backend ON backend_execution_leases USING btree (backend_id) WHERE (state = ANY (ARRAY['claimed'::text, 'running'::text]));

CREATE INDEX idx_backend_execution_leases_backend_state ON backend_execution_leases USING btree (backend_id, state);

CREATE INDEX idx_backend_execution_leases_session ON backend_execution_leases USING btree (session_id);

CREATE INDEX idx_backend_workspace_inventory_backend ON backend_workspace_inventory USING btree (backend_id);

CREATE INDEX idx_backend_workspace_inventory_status ON backend_workspace_inventory USING btree (status);

CREATE UNIQUE INDEX idx_backends_local_machine_scope_slot ON backends USING btree (machine_id, share_scope_kind, COALESCE(share_scope_id, ''::text), capability_slot) WHERE ((backend_type = 'local'::text) AND (machine_id IS NOT NULL) AND (share_scope_kind IS NOT NULL) AND (capability_slot IS NOT NULL));

CREATE INDEX idx_extension_package_artifacts_owner ON extension_package_artifacts USING btree (owner_kind, owner_id);

CREATE UNIQUE INDEX idx_extension_package_artifacts_owner_digest ON extension_package_artifacts USING btree (owner_kind, owner_id, archive_digest);

CREATE INDEX idx_extension_package_artifacts_owner_extension ON extension_package_artifacts USING btree (owner_kind, owner_id, extension_id);

CREATE INDEX idx_inline_fs_files_owner ON inline_fs_files USING btree (owner_kind, owner_id, container_id);

CREATE INDEX idx_library_assets_asset_type ON library_assets USING btree (asset_type);

CREATE UNIQUE INDEX idx_library_assets_identity ON library_assets USING btree (asset_type, scope, COALESCE(owner_id, ''::text), key);

CREATE INDEX idx_library_assets_scope_owner ON library_assets USING btree (scope, owner_id);

CREATE INDEX idx_library_assets_source_ref ON library_assets USING btree (source_ref);

CREATE INDEX idx_lifecycle_agents_project_id ON lifecycle_agents USING btree (project_id);

CREATE INDEX idx_lifecycle_agents_run_id ON lifecycle_agents USING btree (run_id);

CREATE INDEX idx_workflow_graphs_library_asset_id ON workflow_graphs USING btree (library_asset_id);

CREATE UNIQUE INDEX idx_workflow_graphs_project_key ON workflow_graphs USING btree (project_id, key);

CREATE INDEX idx_lifecycle_gates_agent_status ON lifecycle_gates USING btree (agent_id, status) WHERE (agent_id IS NOT NULL);

CREATE INDEX idx_lifecycle_gates_correlation ON lifecycle_gates USING btree (correlation_id);

CREATE INDEX idx_lifecycle_gates_run_id ON lifecycle_gates USING btree (run_id);

CREATE INDEX idx_llm_provider_user_credentials_provider ON llm_provider_user_credentials USING btree (provider_id);

CREATE INDEX idx_llm_provider_user_credentials_user ON llm_provider_user_credentials USING btree (user_id);

CREATE INDEX idx_lsa_anchor_agent ON lifecycle_subject_associations USING btree (anchor_agent_id) WHERE (anchor_agent_id IS NOT NULL);

CREATE INDEX idx_lsa_anchor_run ON lifecycle_subject_associations USING btree (anchor_run_id);

CREATE INDEX idx_lsa_subject ON lifecycle_subject_associations USING btree (subject_kind, subject_id);

CREATE INDEX idx_lwi_run_id ON lifecycle_workflow_instances USING btree (run_id);

CREATE UNIQUE INDEX idx_lwi_run_root ON lifecycle_workflow_instances USING btree (run_id, role) WHERE (role = 'root'::text);

CREATE INDEX idx_mcp_presets_library_asset_id ON mcp_presets USING btree (library_asset_id);

CREATE UNIQUE INDEX idx_mcp_presets_project_builtin_key ON mcp_presets USING btree (project_id, builtin_key) WHERE (builtin_key IS NOT NULL);

CREATE INDEX idx_mcp_presets_project_id ON mcp_presets USING btree (project_id);

CREATE UNIQUE INDEX idx_mcp_presets_project_key ON mcp_presets USING btree (project_id, key);

CREATE INDEX idx_permission_grants_frame_active ON permission_grants USING btree (effect_frame_id) WHERE (status = ANY (ARRAY['applied'::text, 'scope_escalated'::text]));

CREATE INDEX idx_permission_grants_run ON permission_grants USING btree (run_id);

CREATE INDEX idx_permission_grants_status ON permission_grants USING btree (status) WHERE (status = ANY (ARRAY['applied'::text, 'scope_escalated'::text, 'pending_user_approval'::text]));

CREATE INDEX idx_project_agents_project ON project_agents USING btree (project_id);

CREATE INDEX idx_project_backend_access_backend ON project_backend_access USING btree (backend_id);

CREATE INDEX idx_project_backend_access_project ON project_backend_access USING btree (project_id);

CREATE INDEX idx_project_backend_access_status ON project_backend_access USING btree (status);

CREATE INDEX idx_project_extension_installations_artifact ON project_extension_installations USING btree (package_artifact_id);

CREATE INDEX idx_project_extension_installations_project ON project_extension_installations USING btree (project_id);

CREATE INDEX idx_project_extension_installations_source ON project_extension_installations USING btree (installed_library_asset_id);

CREATE INDEX idx_project_vfs_mounts_project ON project_vfs_mounts USING btree (project_id);

CREATE INDEX idx_routine_exec_dispatch_assignment ON routine_executions USING btree (dispatch_assignment_id) WHERE (dispatch_assignment_id IS NOT NULL);

CREATE INDEX idx_routine_exec_dispatch_run ON routine_executions USING btree (dispatch_run_id) WHERE (dispatch_run_id IS NOT NULL);

CREATE INDEX idx_routine_exec_entity ON routine_executions USING btree (routine_id, entity_key);

CREATE INDEX idx_routine_exec_routine ON routine_executions USING btree (routine_id);

CREATE INDEX idx_routine_exec_status ON routine_executions USING btree (routine_id, status);

CREATE INDEX idx_routines_enabled ON routines USING btree (enabled);

CREATE INDEX idx_routines_project ON routines USING btree (project_id);

CREATE INDEX idx_rsea_agent ON runtime_session_execution_anchors USING btree (agent_id);

CREATE INDEX idx_rsea_launch_frame ON runtime_session_execution_anchors USING btree (launch_frame_id);

CREATE INDEX idx_rsea_run ON runtime_session_execution_anchors USING btree (run_id);

CREATE INDEX idx_rsea_run_agent ON runtime_session_execution_anchors USING btree (run_id, agent_id);

CREATE INDEX idx_runtime_health_last_seen_at ON runtime_health USING btree (last_seen_at);

CREATE INDEX idx_runtime_health_status ON runtime_health USING btree (status);

CREATE INDEX idx_session_compactions_lifecycle_item ON session_compactions USING btree (session_id, lifecycle_item_id);

CREATE INDEX idx_session_compactions_session_kind_status ON session_compactions USING btree (session_id, projection_kind, status, projection_version);

CREATE INDEX idx_session_compactions_source_range ON session_compactions USING btree (session_id, source_start_event_seq, source_end_event_seq);

CREATE INDEX idx_session_lineage_fork_point ON session_lineage USING btree (parent_session_id, fork_point_event_seq, fork_point_compaction_id);

CREATE INDEX idx_session_lineage_parent_status_kind ON session_lineage USING btree (parent_session_id, status, relation_kind, created_at_ms, child_session_id);

CREATE INDEX idx_session_projection_heads_active_compaction ON session_projection_heads USING btree (session_id, active_compaction_id);

CREATE INDEX idx_session_projection_segments_projection ON session_projection_segments USING btree (session_id, projection_kind, projection_version, sort_order);

CREATE INDEX idx_session_projection_segments_source_range ON session_projection_segments USING btree (session_id, source_start_event_seq, source_end_event_seq);

CREATE INDEX idx_session_runtime_commands_frame_transition ON session_runtime_commands USING btree (frame_transition_id);

CREATE INDEX idx_session_runtime_commands_session_status ON session_runtime_commands USING btree (session_id, status);

CREATE INDEX idx_session_runtime_commands_status_updated ON session_runtime_commands USING btree (status, updated_at_ms);

CREATE INDEX idx_session_terminal_effects_session_turn ON session_terminal_effects USING btree (session_id, turn_id);

CREATE INDEX idx_session_terminal_effects_status_updated ON session_terminal_effects USING btree (status, updated_at_ms);

CREATE INDEX idx_session_terminal_effects_terminal_event ON session_terminal_effects USING btree (session_id, terminal_event_seq);

CREATE INDEX idx_skill_assets_library_asset_id ON skill_assets USING btree (library_asset_id);

CREATE UNIQUE INDEX idx_skill_assets_project_builtin_key ON skill_assets USING btree (project_id, builtin_key) WHERE (builtin_key IS NOT NULL);

CREATE INDEX idx_skill_assets_project_id ON skill_assets USING btree (project_id);

CREATE UNIQUE INDEX idx_skill_assets_project_key ON skill_assets USING btree (project_id, key);

CREATE INDEX idx_state_changes_project_id_id ON state_changes USING btree (project_id, id);

CREATE INDEX idx_agent_procedures_library_asset_id ON agent_procedures USING btree (library_asset_id);

CREATE UNIQUE INDEX idx_agent_procedures_project_key ON agent_procedures USING btree (project_id, key);

CREATE UNIQUE INDEX ux_activity_execution_claims_active_attempt ON activity_execution_claims USING btree (run_id, graph_instance_id, activity_key, attempt) WHERE (status = ANY (ARRAY['claiming'::text, 'running'::text]));

ALTER TABLE ONLY agent_assignments
    ADD CONSTRAINT agent_assignments_agent_id_fkey FOREIGN KEY (agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;

ALTER TABLE ONLY agent_assignments
    ADD CONSTRAINT agent_assignments_frame_id_fkey FOREIGN KEY (frame_id) REFERENCES agent_frames(id) ON DELETE CASCADE;

ALTER TABLE ONLY agent_assignments
    ADD CONSTRAINT agent_assignments_run_id_fkey FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY agent_frame_transitions
    ADD CONSTRAINT agent_frame_transitions_target_frame_id_fkey FOREIGN KEY (target_frame_id) REFERENCES agent_frames(id) ON DELETE CASCADE;

ALTER TABLE ONLY agent_frames
    ADD CONSTRAINT agent_frames_agent_id_fkey FOREIGN KEY (agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;

ALTER TABLE ONLY agent_lineages
    ADD CONSTRAINT agent_lineages_child_agent_id_fkey FOREIGN KEY (child_agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;

ALTER TABLE ONLY agent_lineages
    ADD CONSTRAINT agent_lineages_run_id_fkey FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_backend_id_fkey FOREIGN KEY (backend_id) REFERENCES backends(id) ON DELETE CASCADE;

ALTER TABLE ONLY session_runtime_commands
    ADD CONSTRAINT fk_session_runtime_commands_frame_transition FOREIGN KEY (frame_transition_id) REFERENCES agent_frame_transitions(id) ON DELETE CASCADE;

ALTER TABLE ONLY lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_run_id_fkey FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY lifecycle_gates
    ADD CONSTRAINT lifecycle_gates_run_id_fkey FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY lifecycle_subject_associations
    ADD CONSTRAINT lifecycle_subject_associations_anchor_run_id_fkey FOREIGN KEY (anchor_run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY lifecycle_workflow_instances
    ADD CONSTRAINT lifecycle_workflow_instances_run_id_fkey FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES llm_providers(id) ON DELETE CASCADE;

ALTER TABLE ONLY project_backend_access
    ADD CONSTRAINT project_backend_access_project_id_fkey FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE;

ALTER TABLE ONLY runtime_health
    ADD CONSTRAINT runtime_health_backend_id_fkey FOREIGN KEY (backend_id) REFERENCES backends(id) ON DELETE CASCADE;

ALTER TABLE ONLY session_compactions
    ADD CONSTRAINT session_compactions_session_id_fkey FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE;

ALTER TABLE ONLY session_lineage
    ADD CONSTRAINT session_lineage_child_session_id_fkey FOREIGN KEY (child_session_id) REFERENCES sessions(id) ON DELETE CASCADE;

ALTER TABLE ONLY session_lineage
    ADD CONSTRAINT session_lineage_fork_point_compaction_id_fkey FOREIGN KEY (fork_point_compaction_id) REFERENCES session_compactions(id) ON DELETE SET NULL;

ALTER TABLE ONLY session_lineage
    ADD CONSTRAINT session_lineage_parent_session_id_fkey FOREIGN KEY (parent_session_id) REFERENCES sessions(id) ON DELETE CASCADE;

ALTER TABLE ONLY session_projection_heads
    ADD CONSTRAINT session_projection_heads_active_compaction_id_fkey FOREIGN KEY (active_compaction_id) REFERENCES session_compactions(id) ON DELETE SET NULL;

ALTER TABLE ONLY session_projection_heads
    ADD CONSTRAINT session_projection_heads_session_id_fkey FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE;

ALTER TABLE ONLY session_projection_segments
    ADD CONSTRAINT session_projection_segments_generated_by_compaction_id_fkey FOREIGN KEY (generated_by_compaction_id) REFERENCES session_compactions(id) ON DELETE SET NULL;

ALTER TABLE ONLY session_projection_segments
    ADD CONSTRAINT session_projection_segments_session_id_fkey FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE;

ALTER TABLE ONLY session_runtime_commands
    ADD CONSTRAINT session_runtime_commands_session_id_fkey FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE;

ALTER TABLE ONLY session_terminal_effects
    ADD CONSTRAINT session_terminal_effects_session_id_fkey FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE;
