
SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

SET default_tablespace = '';

SET default_table_access_method = heap;

CREATE TABLE public.agent_lineages (
    id text NOT NULL,
    run_id text NOT NULL,
    parent_agent_id text,
    child_agent_id text NOT NULL,
    relation_kind text NOT NULL,
    source_frame_id text,
    metadata_json jsonb,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE public.agent_procedures (
    id text NOT NULL,
    key text NOT NULL,
    name text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    source text NOT NULL,
    version integer NOT NULL,
    contract jsonb NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    project_id text NOT NULL,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone
);

CREATE TABLE public.agent_run_lineages (
    id text NOT NULL,
    parent_run_id text NOT NULL,
    parent_agent_id text NOT NULL,
    child_run_id text NOT NULL,
    child_agent_id text NOT NULL,
    relation_kind text NOT NULL,
    fork_point_event_seq bigint,
    fork_point_ref jsonb,
    forked_by_user_id text NOT NULL,
    metadata jsonb,
    created_at timestamp with time zone NOT NULL,
    parent_frame_id text,
    parent_frame_revision integer,
    child_frame_id text,
    child_frame_revision integer,
    CONSTRAINT agent_run_lineages_distinct_run_check CHECK ((parent_run_id <> child_run_id)),
    CONSTRAINT agent_run_lineages_relation_kind_check CHECK ((relation_kind = 'fork'::text))
);

CREATE TABLE public.agent_run_terminal_projection (
    terminal_id text NOT NULL,
    target_run_id text NOT NULL,
    target_agent_id text NOT NULL,
    project_id text NOT NULL,
    terminal_owner_epoch_id text NOT NULL,
    runtime_thread_id text NOT NULL,
    source_ref text NOT NULL,
    source_committed_revision bigint CONSTRAINT agent_run_terminal_projectio_source_committed_revision_not_null NOT NULL,
    source_applied_surface_revision bigint CONSTRAINT agent_run_terminal_projecti_source_applied_surface_rev_not_null NOT NULL,
    source_activated_revision bigint,
    backend_id text NOT NULL,
    process_state text NOT NULL,
    availability text NOT NULL,
    latest_source_sequence bigint NOT NULL,
    next_output_sequence bigint NOT NULL,
    max_output_bytes bigint NOT NULL,
    projection jsonb NOT NULL,
    CONSTRAINT agent_run_terminal_projectio_source_applied_surface_revis_check CHECK ((source_applied_surface_revision >= 0)),
    CONSTRAINT agent_run_terminal_projection_availability_check CHECK ((availability = ANY (ARRAY['online'::text, 'offline'::text, 'reconciling'::text]))),
    CONSTRAINT agent_run_terminal_projection_backend_id_check CHECK ((btrim(backend_id) <> ''::text)),
    CONSTRAINT agent_run_terminal_projection_latest_source_sequence_check CHECK ((latest_source_sequence >= 0)),
    CONSTRAINT agent_run_terminal_projection_max_output_bytes_check CHECK ((max_output_bytes >= 0)),
    CONSTRAINT agent_run_terminal_projection_next_output_sequence_check CHECK ((next_output_sequence >= 0)),
    CONSTRAINT agent_run_terminal_projection_process_state_check CHECK ((process_state = ANY (ARRAY['starting'::text, 'running'::text, 'exited'::text, 'killed'::text, 'lost'::text]))),
    CONSTRAINT agent_run_terminal_projection_projection_check CHECK ((jsonb_typeof(projection) = 'object'::text)),
    CONSTRAINT agent_run_terminal_projection_runtime_thread_id_check CHECK ((btrim(runtime_thread_id) <> ''::text)),
    CONSTRAINT agent_run_terminal_projection_source_activated_revision_check CHECK (((source_activated_revision IS NULL) OR (source_activated_revision >= 0))),
    CONSTRAINT agent_run_terminal_projection_source_committed_revision_check CHECK ((source_committed_revision >= 0)),
    CONSTRAINT agent_run_terminal_projection_source_ref_check CHECK ((btrim(source_ref) <> ''::text)),
    CONSTRAINT agent_run_terminal_projection_terminal_id_check CHECK ((btrim(terminal_id) <> ''::text)),
    CONSTRAINT agent_run_terminal_projection_terminal_owner_epoch_id_check CHECK ((btrim(terminal_owner_epoch_id) <> ''::text))
);

CREATE TABLE public.agent_run_terminal_projection_change (
    target_run_id text NOT NULL,
    target_agent_id text NOT NULL,
    project_id text NOT NULL,
    revision bigint NOT NULL,
    change_sequence bigint NOT NULL,
    change_id text NOT NULL,
    terminal_id text NOT NULL,
    terminal_owner_epoch_id text CONSTRAINT agent_run_terminal_projection__terminal_owner_epoch_id_not_null NOT NULL,
    source_sequence bigint,
    output_sequence bigint,
    payload_digest text NOT NULL,
    delta_kind text NOT NULL,
    change jsonb NOT NULL,
    CONSTRAINT agent_run_terminal_projection_cha_terminal_owner_epoch_id_check CHECK ((btrim(terminal_owner_epoch_id) <> ''::text)),
    CONSTRAINT agent_run_terminal_projection_change_change_check CHECK ((jsonb_typeof(change) = 'object'::text)),
    CONSTRAINT agent_run_terminal_projection_change_change_id_check CHECK ((btrim(change_id) <> ''::text)),
    CONSTRAINT agent_run_terminal_projection_change_change_sequence_check CHECK ((change_sequence > 0)),
    CONSTRAINT agent_run_terminal_projection_change_delta_kind_check CHECK ((btrim(delta_kind) <> ''::text)),
    CONSTRAINT agent_run_terminal_projection_change_output_sequence_check CHECK (((output_sequence IS NULL) OR (output_sequence >= 0))),
    CONSTRAINT agent_run_terminal_projection_change_payload_digest_check CHECK ((btrim(payload_digest) <> ''::text)),
    CONSTRAINT agent_run_terminal_projection_change_revision_check CHECK ((revision > 0)),
    CONSTRAINT agent_run_terminal_projection_change_source_sequence_check CHECK (((source_sequence IS NULL) OR (source_sequence > 0))),
    CONSTRAINT agent_run_terminal_projection_change_terminal_id_check CHECK ((btrim(terminal_id) <> ''::text))
);

CREATE TABLE public.agent_run_terminal_projection_head (
    target_run_id text NOT NULL,
    target_agent_id text NOT NULL,
    project_id text NOT NULL,
    revision bigint NOT NULL,
    latest_change_sequence bigint CONSTRAINT agent_run_terminal_projection_h_latest_change_sequence_not_null NOT NULL,
    CONSTRAINT agent_run_terminal_projection_head_latest_change_sequence_check CHECK ((latest_change_sequence >= 0)),
    CONSTRAINT agent_run_terminal_projection_head_revision_check CHECK ((revision >= 0))
);

CREATE TABLE public.auth_sessions (
    token_hash text NOT NULL,
    identity_json jsonb NOT NULL,
    expires_at bigint,
    revoked_at bigint,
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL
);

CREATE TABLE public.backend_execution_leases (
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

CREATE TABLE public.backend_workspace_inventory (
    id text NOT NULL,
    backend_id text NOT NULL,
    root_ref text NOT NULL,
    identity_kind text NOT NULL,
    identity_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    detected_facts jsonb DEFAULT '{}'::jsonb NOT NULL,
    status text DEFAULT 'available'::text NOT NULL,
    source text DEFAULT 'manual_register'::text NOT NULL,
    last_seen_at timestamp with time zone NOT NULL,
    last_error text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    CONSTRAINT backend_workspace_inventory_source_check CHECK ((source = ANY (ARRAY['manual_register'::text, 'identity_discovery'::text])))
);

CREATE TABLE public.backends (
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
    visibility text DEFAULT 'private'::text NOT NULL,
    share_scope_kind text DEFAULT 'user'::text NOT NULL,
    share_scope_id text,
    capability_slot text DEFAULT 'default'::text NOT NULL
);

CREATE TABLE public.dash_complete_effect (
    effect_id text NOT NULL,
    record jsonb NOT NULL,
    CONSTRAINT dash_complete_effect_effect_id_check CHECK ((btrim(effect_id) <> ''::text)),
    CONSTRAINT dash_complete_effect_record_check CHECK ((jsonb_typeof(record) = 'object'::text))
);

CREATE TABLE public.dash_complete_source (
    source_coordinate text NOT NULL,
    repository jsonb NOT NULL,
    metadata jsonb NOT NULL,
    observation jsonb NOT NULL,
    CONSTRAINT dash_complete_source_metadata_object CHECK ((jsonb_typeof(metadata) = 'object'::text)),
    CONSTRAINT dash_complete_source_observation_object CHECK ((jsonb_typeof(observation) = 'object'::text)),
    CONSTRAINT dash_complete_source_repository_object CHECK ((jsonb_typeof(repository) = 'object'::text)),
    CONSTRAINT dash_complete_source_source_coordinate_check CHECK ((btrim(source_coordinate) <> ''::text))
);

CREATE TABLE public.extension_package_artifacts (
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

CREATE TABLE public.group_memberships (
    user_id text NOT NULL,
    group_id text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.groups (
    group_id text NOT NULL,
    display_name text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.inline_fs_files (
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

CREATE TABLE public.library_assets (
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
    CONSTRAINT library_assets_source_check CHECK ((source = ANY (ARRAY['builtin'::text, 'user_authored'::text, 'remote_imported'::text, 'integration_embedded'::text]))),
    CONSTRAINT library_assets_type_check CHECK ((asset_type = ANY (ARRAY['agent_template'::text, 'mcp_server_template'::text, 'workflow_template'::text, 'skill_template'::text, 'vfs_mount_template'::text, 'extension_template'::text])))
);

CREATE TABLE public.lifecycle_agents (
    id text NOT NULL,
    run_id text NOT NULL,
    project_id text NOT NULL,
    source text CONSTRAINT lifecycle_agents_agent_kind_not_null NOT NULL,
    project_agent_id text,
    status text DEFAULT 'active'::text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    bootstrap_status text DEFAULT 'not_applicable'::text NOT NULL,
    created_by_user_id text DEFAULT 'system'::text NOT NULL,
    workspace_title text,
    workspace_title_source text,
    frames jsonb DEFAULT '[]'::jsonb NOT NULL,
    runtime_binding jsonb,
    CONSTRAINT lifecycle_agents_frames_check CHECK ((jsonb_typeof(frames) = 'array'::text)),
    CONSTRAINT lifecycle_agents_runtime_binding_check CHECK (((runtime_binding IS NULL) OR (jsonb_typeof(runtime_binding) = 'object'::text)))
);

CREATE TABLE public.lifecycle_gates (
    id text NOT NULL,
    run_id text NOT NULL,
    agent_id text,
    frame_id text,
    gate_kind text NOT NULL,
    correlation_id text NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    payload_json jsonb,
    resolved_by text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    resolved_at timestamp with time zone,
    delivery jsonb DEFAULT '{}'::jsonb NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE public.lifecycle_runs (
    id text NOT NULL,
    project_id text NOT NULL,
    topology text NOT NULL,
    status text NOT NULL,
    execution_log jsonb DEFAULT '[]'::jsonb NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    last_activity_at timestamp with time zone NOT NULL,
    orchestrations jsonb DEFAULT '[]'::jsonb NOT NULL,
    tasks jsonb DEFAULT '[]'::jsonb NOT NULL,
    created_by_user_id text DEFAULT 'system'::text NOT NULL,
    channel_registry jsonb DEFAULT '{"schema_version":2,"channels":[]}'::jsonb NOT NULL,
    revision bigint DEFAULT 0 NOT NULL,
    CONSTRAINT lifecycle_runs_revision_check CHECK ((revision >= 0))
);

CREATE TABLE public.lifecycle_subject_associations (
    id text NOT NULL,
    anchor_run_id text NOT NULL,
    anchor_agent_id text,
    subject_kind text NOT NULL,
    subject_id text NOT NULL,
    role text NOT NULL,
    metadata_json jsonb,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE public.llm_provider_user_credentials (
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

CREATE TABLE public.llm_providers (
    id text NOT NULL,
    name text NOT NULL,
    slug text NOT NULL,
    protocol text NOT NULL,
    base_url text DEFAULT ''::text NOT NULL,
    wire_api text DEFAULT ''::text NOT NULL,
    default_model text DEFAULT ''::text NOT NULL,
    models jsonb DEFAULT '[]'::jsonb NOT NULL,
    blocked_models jsonb DEFAULT '[]'::jsonb NOT NULL,
    env_api_key text DEFAULT ''::text NOT NULL,
    discovery_url text DEFAULT ''::text NOT NULL,
    sort_order integer DEFAULT 0 NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    credential_mode text DEFAULT 'global_only'::text NOT NULL,
    global_api_key_ciphertext text DEFAULT ''::text NOT NULL
);

CREATE TABLE public.mcp_presets (
    id text NOT NULL,
    project_id text NOT NULL,
    description text,
    source text NOT NULL,
    builtin_key text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    key text NOT NULL,
    display_name text NOT NULL,
    transport jsonb NOT NULL,
    route_policy text NOT NULL,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone,
    runtime_binding jsonb,
    CONSTRAINT mcp_presets_builtin_key_consistency CHECK ((((source = 'builtin'::text) AND (builtin_key IS NOT NULL)) OR ((source = 'user'::text) AND (builtin_key IS NULL)))),
    CONSTRAINT mcp_presets_source_check CHECK ((source = ANY (ARRAY['builtin'::text, 'user'::text])))
);

CREATE TABLE public.project_agents (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    agent_type text NOT NULL,
    config jsonb DEFAULT '{}'::jsonb NOT NULL,
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

CREATE TABLE public.project_backend_access (
    id text NOT NULL,
    project_id text NOT NULL,
    backend_id text NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    access_mode text DEFAULT 'explicit_grant'::text NOT NULL,
    priority integer DEFAULT 0 NOT NULL,
    root_policy jsonb DEFAULT '{"kind": "workspace_registry"}'::jsonb NOT NULL,
    capability_policy jsonb DEFAULT '{}'::jsonb NOT NULL,
    note text,
    created_by text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.project_extension_installations (
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

CREATE TABLE public.project_subject_grants (
    project_id text NOT NULL,
    subject_type text NOT NULL,
    subject_id text NOT NULL,
    role text NOT NULL,
    granted_by_user_id text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.project_vfs_mounts (
    id text NOT NULL,
    project_id text NOT NULL,
    mount_id text NOT NULL,
    display_name text NOT NULL,
    description text,
    capabilities jsonb DEFAULT '[]'::jsonb NOT NULL,
    installed_source jsonb,
    content jsonb NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.projects (
    id text NOT NULL,
    name text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    config jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_by_user_id text DEFAULT 'system'::text NOT NULL,
    updated_by_user_id text DEFAULT 'system'::text NOT NULL,
    visibility text DEFAULT 'private'::text NOT NULL,
    is_template boolean DEFAULT false NOT NULL,
    cloned_from_project_id text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.routine_executions (
    id text NOT NULL,
    routine_id text NOT NULL,
    trigger_source text NOT NULL,
    trigger_payload jsonb,
    resolved_prompt text,
    status text DEFAULT 'pending'::text NOT NULL,
    started_at timestamp with time zone NOT NULL,
    completed_at timestamp with time zone,
    error text,
    entity_key text,
    dispatch_run_id text,
    dispatch_agent_id text,
    dispatch_frame_id text,
    dispatch_orchestration_id text,
    dispatch_node_path text,
    dispatch_input_handoff jsonb
);

CREATE TABLE public.routines (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    prompt_template text NOT NULL,
    project_agent_id text CONSTRAINT routines_agent_id_not_null NOT NULL,
    trigger_config jsonb NOT NULL,
    dispatch_strategy jsonb NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    last_fired_at timestamp with time zone
);

CREATE TABLE public.runner_registration_tokens (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    token_secret_hash text NOT NULL,
    token_prefix text NOT NULL,
    created_by_user_id text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    last_used_at timestamp with time zone,
    last_claimed_backend_id text,
    default_capability_slot text DEFAULT 'default'::text NOT NULL,
    machine_policy jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.runtime_health (
    backend_id text NOT NULL,
    profile_id text,
    name text NOT NULL,
    status text NOT NULL,
    version text,
    capabilities jsonb DEFAULT '{}'::jsonb NOT NULL,
    device jsonb DEFAULT '{}'::jsonb NOT NULL,
    connected_at timestamp with time zone,
    last_seen_at timestamp with time zone,
    disconnected_at timestamp with time zone,
    disconnect_reason text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT runtime_health_status_check CHECK ((status = ANY (ARRAY['online'::text, 'offline'::text, 'starting'::text, 'degraded'::text, 'stopping'::text, 'error'::text])))
);

CREATE TABLE public.settings (
    scope_kind text NOT NULL,
    scope_id text DEFAULT ''::text NOT NULL,
    key text NOT NULL,
    value jsonb NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE public.skill_assets (
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

CREATE TABLE public.state_changes (
    id bigint NOT NULL,
    project_id text NOT NULL,
    entity_id text NOT NULL,
    kind text NOT NULL,
    payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    backend_id text,
    created_at timestamp with time zone NOT NULL
);

CREATE SEQUENCE public.state_changes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.state_changes_id_seq OWNED BY public.state_changes.id;

CREATE TABLE public.stories (
    id text NOT NULL,
    project_id text NOT NULL,
    default_workspace_id text,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    status text DEFAULT 'created'::text NOT NULL,
    priority text DEFAULT 'p2'::text NOT NULL,
    story_type text DEFAULT 'feature'::text NOT NULL,
    tags jsonb DEFAULT '[]'::jsonb NOT NULL,
    context jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.users (
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

CREATE TABLE public.workflow_executor_effects (
    effect_id text NOT NULL,
    effect_kind text NOT NULL,
    lifecycle_run_id text NOT NULL,
    orchestration_id text NOT NULL,
    node_path text NOT NULL,
    attempt bigint NOT NULL,
    payload_digest text NOT NULL,
    request jsonb,
    state text NOT NULL,
    gate_id text,
    runner_state text DEFAULT 'not_applied'::text NOT NULL,
    runner_claim_id text,
    runner_claim_owner text,
    runner_lease_expires_at timestamp with time zone,
    runner_evidence jsonb,
    runner_late_evidence jsonb,
    runner_receipt jsonb,
    receipt jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workflow_executor_effects_attempt_check CHECK ((attempt > 0)),
    CONSTRAINT workflow_executor_effects_check CHECK ((((effect_kind = 'function'::text) AND (gate_id IS NULL) AND (request IS NOT NULL)) OR ((effect_kind = ANY (ARRAY['human_gate_open'::text, 'human_gate_resolution'::text])) AND (gate_id IS NOT NULL)))),
    CONSTRAINT workflow_executor_effects_check1 CHECK ((((state = 'prepared'::text) AND (receipt IS NULL)) OR ((state = 'terminal'::text) AND (receipt IS NOT NULL)))),
    CONSTRAINT workflow_executor_effects_check2 CHECK ((((runner_state = 'not_applied'::text) AND (runner_claim_id IS NULL) AND (runner_claim_owner IS NULL) AND (runner_lease_expires_at IS NULL) AND (runner_evidence IS NULL) AND (runner_receipt IS NULL)) OR ((runner_state = ANY (ARRAY['accepted'::text, 'in_flight'::text])) AND (runner_claim_id IS NOT NULL) AND (runner_claim_owner IS NOT NULL) AND (runner_lease_expires_at IS NOT NULL) AND (runner_evidence IS NOT NULL) AND (runner_receipt IS NOT NULL)) OR ((runner_state = ANY (ARRAY['succeeded'::text, 'failed'::text])) AND (runner_claim_id IS NOT NULL) AND (runner_claim_owner IS NOT NULL) AND (runner_lease_expires_at IS NOT NULL) AND (runner_evidence IS NOT NULL) AND (runner_receipt IS NOT NULL)) OR ((runner_state = 'lost'::text) AND (runner_claim_id IS NOT NULL) AND (runner_claim_owner IS NOT NULL) AND (runner_lease_expires_at IS NOT NULL) AND (runner_evidence IS NOT NULL) AND (runner_receipt IS NULL)))),
    CONSTRAINT workflow_executor_effects_effect_id_check CHECK ((btrim(effect_id) <> ''::text)),
    CONSTRAINT workflow_executor_effects_effect_kind_check CHECK ((effect_kind = ANY (ARRAY['function'::text, 'human_gate_open'::text, 'human_gate_resolution'::text]))),
    CONSTRAINT workflow_executor_effects_node_path_check CHECK ((btrim(node_path) <> ''::text)),
    CONSTRAINT workflow_executor_effects_orchestration_id_check CHECK ((btrim(orchestration_id) <> ''::text)),
    CONSTRAINT workflow_executor_effects_payload_digest_check CHECK ((btrim(payload_digest) <> ''::text)),
    CONSTRAINT workflow_executor_effects_receipt_check CHECK (((receipt IS NULL) OR (jsonb_typeof(receipt) = 'object'::text))),
    CONSTRAINT workflow_executor_effects_request_check CHECK (((request IS NULL) OR (jsonb_typeof(request) = 'object'::text))),
    CONSTRAINT workflow_executor_effects_runner_claim_id_check CHECK (((runner_claim_id IS NULL) OR (btrim(runner_claim_id) <> ''::text))),
    CONSTRAINT workflow_executor_effects_runner_claim_owner_check CHECK (((runner_claim_owner IS NULL) OR (btrim(runner_claim_owner) <> ''::text))),
    CONSTRAINT workflow_executor_effects_runner_evidence_check CHECK (((runner_evidence IS NULL) OR (jsonb_typeof(runner_evidence) = 'object'::text))),
    CONSTRAINT workflow_executor_effects_runner_late_evidence_check CHECK (((runner_late_evidence IS NULL) OR (jsonb_typeof(runner_late_evidence) = 'object'::text))),
    CONSTRAINT workflow_executor_effects_runner_receipt_check CHECK (((runner_receipt IS NULL) OR (jsonb_typeof(runner_receipt) = 'object'::text))),
    CONSTRAINT workflow_executor_effects_runner_state_check CHECK ((runner_state = ANY (ARRAY['not_applied'::text, 'accepted'::text, 'in_flight'::text, 'succeeded'::text, 'failed'::text, 'lost'::text]))),
    CONSTRAINT workflow_executor_effects_state_check CHECK ((state = ANY (ARRAY['prepared'::text, 'terminal'::text])))
);

CREATE TABLE public.workflow_graphs (
    id text NOT NULL,
    key text NOT NULL,
    name text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    source text NOT NULL,
    version integer NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    project_id text NOT NULL,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone,
    entry_activity_key text DEFAULT ''::text NOT NULL,
    activities jsonb DEFAULT '[]'::jsonb NOT NULL,
    transitions jsonb DEFAULT '[]'::jsonb NOT NULL
);

CREATE TABLE public.workspace_bindings (
    id text NOT NULL,
    workspace_id text NOT NULL,
    backend_id text NOT NULL,
    root_ref text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    detected_facts jsonb DEFAULT '{}'::jsonb NOT NULL,
    last_verified_at timestamp with time zone,
    priority integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

CREATE TABLE public.workspaces (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    identity_kind text DEFAULT 'local_dir'::text NOT NULL,
    identity_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    resolution_policy text DEFAULT 'prefer_online'::text NOT NULL,
    default_binding_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    mount_capabilities jsonb DEFAULT '["read", "write", "list", "search", "exec"]'::jsonb NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);

ALTER TABLE ONLY public.state_changes ALTER COLUMN id SET DEFAULT nextval('public.state_changes_id_seq'::regclass);

ALTER TABLE ONLY public.agent_lineages
    ADD CONSTRAINT agent_lineages_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.agent_procedures
    ADD CONSTRAINT agent_procedures_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.agent_run_lineages
    ADD CONSTRAINT agent_run_lineages_child_unique UNIQUE (child_run_id, child_agent_id);

ALTER TABLE ONLY public.agent_run_lineages
    ADD CONSTRAINT agent_run_lineages_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.agent_run_terminal_projection_change
    ADD CONSTRAINT agent_run_terminal_projection_change_change_id_key UNIQUE (change_id);

ALTER TABLE ONLY public.agent_run_terminal_projection_change
    ADD CONSTRAINT agent_run_terminal_projection_change_pkey PRIMARY KEY (target_run_id, target_agent_id, change_sequence);

ALTER TABLE ONLY public.agent_run_terminal_projection_head
    ADD CONSTRAINT agent_run_terminal_projection_head_pkey PRIMARY KEY (target_run_id, target_agent_id);

ALTER TABLE ONLY public.agent_run_terminal_projection
    ADD CONSTRAINT agent_run_terminal_projection_pkey PRIMARY KEY (terminal_id);

ALTER TABLE ONLY public.agent_run_terminal_projection_change
    ADD CONSTRAINT agent_run_terminal_projection_target_run_id_target_agent_id_key UNIQUE (target_run_id, target_agent_id, revision);

ALTER TABLE ONLY public.agent_run_terminal_projection
    ADD CONSTRAINT agent_run_terminal_projection_terminal_id_target_run_id_tar_key UNIQUE (terminal_id, target_run_id, target_agent_id);

ALTER TABLE ONLY public.agent_run_terminal_projection
    ADD CONSTRAINT agent_run_terminal_projection_terminal_owner_epoch_id_termi_key UNIQUE (terminal_owner_epoch_id, terminal_id);

ALTER TABLE ONLY public.auth_sessions
    ADD CONSTRAINT auth_sessions_pkey PRIMARY KEY (token_hash);

ALTER TABLE ONLY public.backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_session_id_turn_id_key UNIQUE (session_id, turn_id);

ALTER TABLE ONLY public.backend_workspace_inventory
    ADD CONSTRAINT backend_workspace_inventory_backend_id_root_ref_key UNIQUE (backend_id, root_ref);

ALTER TABLE ONLY public.backend_workspace_inventory
    ADD CONSTRAINT backend_workspace_inventory_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.backends
    ADD CONSTRAINT backends_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.dash_complete_effect
    ADD CONSTRAINT dash_complete_effect_pkey PRIMARY KEY (effect_id);

ALTER TABLE ONLY public.dash_complete_source
    ADD CONSTRAINT dash_complete_source_pkey PRIMARY KEY (source_coordinate);

ALTER TABLE ONLY public.extension_package_artifacts
    ADD CONSTRAINT extension_package_artifacts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.group_memberships
    ADD CONSTRAINT group_memberships_pkey PRIMARY KEY (user_id, group_id);

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (group_id);

ALTER TABLE ONLY public.inline_fs_files
    ADD CONSTRAINT inline_fs_files_owner_kind_owner_id_container_id_path_key UNIQUE (owner_kind, owner_id, container_id, path);

ALTER TABLE ONLY public.inline_fs_files
    ADD CONSTRAINT inline_fs_files_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.library_assets
    ADD CONSTRAINT library_assets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_id_run_project_key UNIQUE (id, run_id, project_id);

ALTER TABLE ONLY public.lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.lifecycle_gates
    ADD CONSTRAINT lifecycle_gates_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.lifecycle_runs
    ADD CONSTRAINT lifecycle_runs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.lifecycle_subject_associations
    ADD CONSTRAINT lifecycle_subject_associations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_provider_id_user_id_key UNIQUE (provider_id, user_id);

ALTER TABLE ONLY public.llm_providers
    ADD CONSTRAINT llm_providers_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.llm_providers
    ADD CONSTRAINT llm_providers_slug_key UNIQUE (slug);

ALTER TABLE ONLY public.mcp_presets
    ADD CONSTRAINT mcp_presets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.project_agents
    ADD CONSTRAINT project_agents_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.project_agents
    ADD CONSTRAINT project_agents_project_id_name_key UNIQUE (project_id, name);

ALTER TABLE ONLY public.project_backend_access
    ADD CONSTRAINT project_backend_access_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.project_backend_access
    ADD CONSTRAINT project_backend_access_project_id_backend_id_key UNIQUE (project_id, backend_id);

ALTER TABLE ONLY public.project_extension_installations
    ADD CONSTRAINT project_extension_installations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.project_extension_installations
    ADD CONSTRAINT project_extension_installations_unique_key UNIQUE (project_id, extension_key);

ALTER TABLE ONLY public.project_subject_grants
    ADD CONSTRAINT project_subject_grants_pkey PRIMARY KEY (project_id, subject_type, subject_id);

ALTER TABLE ONLY public.project_vfs_mounts
    ADD CONSTRAINT project_vfs_mounts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.project_vfs_mounts
    ADD CONSTRAINT project_vfs_mounts_project_id_mount_id_key UNIQUE (project_id, mount_id);

ALTER TABLE ONLY public.projects
    ADD CONSTRAINT projects_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.routine_executions
    ADD CONSTRAINT routine_executions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.routines
    ADD CONSTRAINT routines_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.routines
    ADD CONSTRAINT routines_project_id_name_key UNIQUE (project_id, name);

ALTER TABLE ONLY public.runner_registration_tokens
    ADD CONSTRAINT runner_registration_tokens_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.runtime_health
    ADD CONSTRAINT runtime_health_pkey PRIMARY KEY (backend_id);

ALTER TABLE ONLY public.settings
    ADD CONSTRAINT settings_pkey PRIMARY KEY (scope_kind, scope_id, key);

ALTER TABLE ONLY public.skill_assets
    ADD CONSTRAINT skill_assets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.state_changes
    ADD CONSTRAINT state_changes_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.stories
    ADD CONSTRAINT stories_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (user_id);

ALTER TABLE ONLY public.workflow_executor_effects
    ADD CONSTRAINT workflow_executor_effects_gate_id_effect_kind_key UNIQUE (gate_id, effect_kind);

ALTER TABLE ONLY public.workflow_executor_effects
    ADD CONSTRAINT workflow_executor_effects_lifecycle_run_id_orchestration_id_key UNIQUE (lifecycle_run_id, orchestration_id, node_path, attempt, effect_kind);

ALTER TABLE ONLY public.workflow_executor_effects
    ADD CONSTRAINT workflow_executor_effects_pkey PRIMARY KEY (effect_id);

ALTER TABLE ONLY public.workflow_graphs
    ADD CONSTRAINT workflow_graphs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.workspace_bindings
    ADD CONSTRAINT workspace_bindings_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_pkey PRIMARY KEY (id);

CREATE UNIQUE INDEX agent_run_terminal_projection_change_output_key ON public.agent_run_terminal_projection_change USING btree (terminal_id, output_sequence) WHERE (output_sequence IS NOT NULL);

CREATE UNIQUE INDEX agent_run_terminal_projection_change_source_key ON public.agent_run_terminal_projection_change USING btree (terminal_owner_epoch_id, source_sequence) WHERE (source_sequence IS NOT NULL);

CREATE INDEX idx_agent_lineages_child ON public.agent_lineages USING btree (child_agent_id);

CREATE INDEX idx_agent_lineages_parent ON public.agent_lineages USING btree (parent_agent_id) WHERE (parent_agent_id IS NOT NULL);

CREATE INDEX idx_agent_lineages_run_id ON public.agent_lineages USING btree (run_id);

CREATE INDEX idx_agent_procedures_library_asset_id ON public.agent_procedures USING btree (library_asset_id);

CREATE UNIQUE INDEX idx_agent_procedures_project_key ON public.agent_procedures USING btree (project_id, key);

CREATE INDEX idx_agent_run_lineages_child ON public.agent_run_lineages USING btree (child_run_id, child_agent_id);

CREATE INDEX idx_agent_run_lineages_parent ON public.agent_run_lineages USING btree (parent_run_id, parent_agent_id, created_at DESC);

CREATE INDEX idx_backend_execution_leases_active_backend ON public.backend_execution_leases USING btree (backend_id) WHERE (state = ANY (ARRAY['claimed'::text, 'running'::text]));

CREATE INDEX idx_backend_execution_leases_backend_state ON public.backend_execution_leases USING btree (backend_id, state);

CREATE INDEX idx_backend_execution_leases_session ON public.backend_execution_leases USING btree (session_id);

CREATE INDEX idx_backend_workspace_inventory_backend ON public.backend_workspace_inventory USING btree (backend_id);

CREATE INDEX idx_backend_workspace_inventory_status ON public.backend_workspace_inventory USING btree (status);

CREATE UNIQUE INDEX idx_backends_local_machine_scope_slot ON public.backends USING btree (machine_id, share_scope_kind, COALESCE(share_scope_id, ''::text), capability_slot) WHERE ((backend_type = 'local'::text) AND (machine_id IS NOT NULL) AND (share_scope_kind IS NOT NULL) AND (capability_slot IS NOT NULL));

CREATE INDEX idx_extension_package_artifacts_owner ON public.extension_package_artifacts USING btree (owner_kind, owner_id);

CREATE UNIQUE INDEX idx_extension_package_artifacts_owner_digest ON public.extension_package_artifacts USING btree (owner_kind, owner_id, archive_digest);

CREATE INDEX idx_extension_package_artifacts_owner_extension ON public.extension_package_artifacts USING btree (owner_kind, owner_id, extension_id);

CREATE INDEX idx_inline_fs_files_owner ON public.inline_fs_files USING btree (owner_kind, owner_id, container_id);

CREATE INDEX idx_library_assets_asset_type ON public.library_assets USING btree (asset_type);

CREATE UNIQUE INDEX idx_library_assets_identity ON public.library_assets USING btree (asset_type, scope, COALESCE(owner_id, ''::text), key);

CREATE INDEX idx_library_assets_scope_owner ON public.library_assets USING btree (scope, owner_id);

CREATE INDEX idx_library_assets_source_ref ON public.library_assets USING btree (source_ref);

CREATE INDEX idx_lifecycle_agents_project_id ON public.lifecycle_agents USING btree (project_id);

CREATE INDEX idx_lifecycle_agents_run_id ON public.lifecycle_agents USING btree (run_id);

CREATE INDEX idx_lifecycle_gates_agent_status ON public.lifecycle_gates USING btree (agent_id, status) WHERE (agent_id IS NOT NULL);

CREATE INDEX idx_lifecycle_gates_correlation ON public.lifecycle_gates USING btree (correlation_id);

CREATE INDEX idx_lifecycle_gates_run_id ON public.lifecycle_gates USING btree (run_id);

CREATE INDEX idx_llm_provider_user_credentials_provider ON public.llm_provider_user_credentials USING btree (provider_id);

CREATE INDEX idx_llm_provider_user_credentials_user ON public.llm_provider_user_credentials USING btree (user_id);

CREATE INDEX idx_lsa_anchor_agent ON public.lifecycle_subject_associations USING btree (anchor_agent_id) WHERE (anchor_agent_id IS NOT NULL);

CREATE INDEX idx_lsa_anchor_run ON public.lifecycle_subject_associations USING btree (anchor_run_id);

CREATE INDEX idx_lsa_subject ON public.lifecycle_subject_associations USING btree (subject_kind, subject_id);

CREATE INDEX idx_mcp_presets_library_asset_id ON public.mcp_presets USING btree (library_asset_id);

CREATE UNIQUE INDEX idx_mcp_presets_project_builtin_key ON public.mcp_presets USING btree (project_id, builtin_key) WHERE (builtin_key IS NOT NULL);

CREATE INDEX idx_mcp_presets_project_id ON public.mcp_presets USING btree (project_id);

CREATE UNIQUE INDEX idx_mcp_presets_project_key ON public.mcp_presets USING btree (project_id, key);

CREATE INDEX idx_project_agents_project ON public.project_agents USING btree (project_id);

CREATE INDEX idx_project_backend_access_backend ON public.project_backend_access USING btree (backend_id);

CREATE INDEX idx_project_backend_access_project ON public.project_backend_access USING btree (project_id);

CREATE INDEX idx_project_backend_access_status ON public.project_backend_access USING btree (status);

CREATE INDEX idx_project_extension_installations_artifact ON public.project_extension_installations USING btree (package_artifact_id);

CREATE INDEX idx_project_extension_installations_project ON public.project_extension_installations USING btree (project_id);

CREATE INDEX idx_project_extension_installations_source ON public.project_extension_installations USING btree (installed_library_asset_id);

CREATE INDEX idx_project_vfs_mounts_project ON public.project_vfs_mounts USING btree (project_id);

CREATE INDEX idx_routine_exec_dispatch_run ON public.routine_executions USING btree (dispatch_run_id) WHERE (dispatch_run_id IS NOT NULL);

CREATE INDEX idx_routine_exec_entity ON public.routine_executions USING btree (routine_id, entity_key);

CREATE INDEX idx_routine_exec_recoverable ON public.routine_executions USING btree (started_at) WHERE ((status = 'pending'::text) AND (dispatch_run_id IS NOT NULL));

CREATE INDEX idx_routine_exec_routine ON public.routine_executions USING btree (routine_id);

CREATE INDEX idx_routine_exec_status ON public.routine_executions USING btree (routine_id, status);

CREATE INDEX idx_routines_enabled ON public.routines USING btree (enabled);

CREATE INDEX idx_routines_project ON public.routines USING btree (project_id);

CREATE INDEX idx_runner_registration_tokens_active_project ON public.runner_registration_tokens USING btree (project_id, expires_at) WHERE (revoked_at IS NULL);

CREATE INDEX idx_runner_registration_tokens_expires_active ON public.runner_registration_tokens USING btree (expires_at) WHERE (revoked_at IS NULL);

CREATE INDEX idx_runner_registration_tokens_last_claimed_backend ON public.runner_registration_tokens USING btree (last_claimed_backend_id);

CREATE INDEX idx_runner_registration_tokens_last_used ON public.runner_registration_tokens USING btree (last_used_at);

CREATE INDEX idx_runner_registration_tokens_project ON public.runner_registration_tokens USING btree (project_id);

CREATE INDEX idx_runtime_health_last_seen_at ON public.runtime_health USING btree (last_seen_at);

CREATE INDEX idx_runtime_health_status ON public.runtime_health USING btree (status);

CREATE INDEX idx_skill_assets_library_asset_id ON public.skill_assets USING btree (library_asset_id);

CREATE UNIQUE INDEX idx_skill_assets_project_builtin_key ON public.skill_assets USING btree (project_id, builtin_key) WHERE (builtin_key IS NOT NULL);

CREATE INDEX idx_skill_assets_project_id ON public.skill_assets USING btree (project_id);

CREATE UNIQUE INDEX idx_skill_assets_project_key ON public.skill_assets USING btree (project_id, key);

CREATE INDEX idx_state_changes_project_id_id ON public.state_changes USING btree (project_id, id);

CREATE INDEX idx_workflow_graphs_library_asset_id ON public.workflow_graphs USING btree (library_asset_id);

CREATE UNIQUE INDEX idx_workflow_graphs_project_key ON public.workflow_graphs USING btree (project_id, key);

CREATE UNIQUE INDEX lifecycle_agents_runtime_thread_id_unique ON public.lifecycle_agents USING btree (((runtime_binding ->> 'runtime_thread_id'::text))) WHERE (runtime_binding IS NOT NULL);

CREATE INDEX routine_executions_input_operation_id_idx ON public.routine_executions USING btree (((dispatch_input_handoff ->> 'runtime_operation_id'::text))) WHERE ((dispatch_input_handoff ->> 'runtime_operation_id'::text) IS NOT NULL);

ALTER TABLE ONLY public.agent_lineages
    ADD CONSTRAINT agent_lineages_child_agent_id_fkey FOREIGN KEY (child_agent_id) REFERENCES public.lifecycle_agents(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.agent_lineages
    ADD CONSTRAINT agent_lineages_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.agent_run_lineages
    ADD CONSTRAINT agent_run_lineages_child_agent_id_fkey FOREIGN KEY (child_agent_id) REFERENCES public.lifecycle_agents(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.agent_run_lineages
    ADD CONSTRAINT agent_run_lineages_child_run_id_fkey FOREIGN KEY (child_run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.agent_run_lineages
    ADD CONSTRAINT agent_run_lineages_parent_agent_id_fkey FOREIGN KEY (parent_agent_id) REFERENCES public.lifecycle_agents(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.agent_run_lineages
    ADD CONSTRAINT agent_run_lineages_parent_run_id_fkey FOREIGN KEY (parent_run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.agent_run_terminal_projection
    ADD CONSTRAINT agent_run_terminal_projectio_target_agent_id_target_run_i_fkey1 FOREIGN KEY (target_agent_id, target_run_id, project_id) REFERENCES public.lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE;

ALTER TABLE ONLY public.agent_run_terminal_projection_change
    ADD CONSTRAINT agent_run_terminal_projectio_target_agent_id_target_run_i_fkey2 FOREIGN KEY (target_agent_id, target_run_id, project_id) REFERENCES public.lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE;

ALTER TABLE ONLY public.agent_run_terminal_projection_head
    ADD CONSTRAINT agent_run_terminal_projection_target_agent_id_target_run_i_fkey FOREIGN KEY (target_agent_id, target_run_id, project_id) REFERENCES public.lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE;

ALTER TABLE ONLY public.backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_backend_id_fkey FOREIGN KEY (backend_id) REFERENCES public.backends(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.lifecycle_gates
    ADD CONSTRAINT lifecycle_gates_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.lifecycle_subject_associations
    ADD CONSTRAINT lifecycle_subject_associations_anchor_run_id_fkey FOREIGN KEY (anchor_run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES public.llm_providers(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.project_backend_access
    ADD CONSTRAINT project_backend_access_project_id_fkey FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.runner_registration_tokens
    ADD CONSTRAINT runner_registration_tokens_project_id_fkey FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.runtime_health
    ADD CONSTRAINT runtime_health_backend_id_fkey FOREIGN KEY (backend_id) REFERENCES public.backends(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.workflow_executor_effects
    ADD CONSTRAINT workflow_executor_effects_gate_id_fkey FOREIGN KEY (gate_id) REFERENCES public.lifecycle_gates(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.workflow_executor_effects
    ADD CONSTRAINT workflow_executor_effects_lifecycle_run_id_fkey FOREIGN KEY (lifecycle_run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;

CREATE TABLE public.interaction_definitions (
    id uuid PRIMARY KEY,
    project_id text NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    owner_kind text NOT NULL CHECK (owner_kind IN ('user', 'project')),
    owner_id text NOT NULL,
    kind text NOT NULL CHECK (kind = 'canvas'),
    current_revision_id uuid NOT NULL,
    status text NOT NULL CHECK (status IN ('active', 'archived')),
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    CHECK (owner_kind <> 'project' OR owner_id = project_id)
);

CREATE INDEX idx_interaction_definitions_project_catalog
    ON public.interaction_definitions (project_id, kind, status, updated_at DESC, id);
CREATE INDEX idx_interaction_definitions_owner
    ON public.interaction_definitions (owner_kind, owner_id, updated_at DESC, id);

CREATE TABLE public.interaction_source_bundles (
    digest text PRIMARY KEY CHECK (digest ~ '^sha256:[0-9a-fA-F]{64}$'),
    format_version smallint NOT NULL CHECK (format_version = 1),
    entry_file text NOT NULL,
    sandbox jsonb NOT NULL,
    created_at timestamptz NOT NULL
);

CREATE TABLE public.interaction_source_files (
    source_bundle_digest text NOT NULL REFERENCES public.interaction_source_bundles(digest) ON DELETE RESTRICT,
    path text NOT NULL,
    content text NOT NULL,
    media_type text,
    PRIMARY KEY (source_bundle_digest, path)
);

CREATE TABLE public.interaction_definition_revisions (
    revision_id uuid PRIMARY KEY,
    definition_id uuid NOT NULL REFERENCES public.interaction_definitions(id) ON DELETE RESTRICT,
    revision_number bigint NOT NULL CHECK (revision_number > 0),
    project_id text NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    owner_kind text NOT NULL CHECK (owner_kind IN ('user', 'project')),
    owner_id text NOT NULL,
    source_bundle_digest text NOT NULL REFERENCES public.interaction_source_bundles(digest) ON DELETE RESTRICT,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (definition_id, revision_number),
    UNIQUE (revision_id, definition_id),
    CHECK (owner_kind <> 'project' OR owner_id = project_id)
);

ALTER TABLE public.interaction_definitions
    ADD CONSTRAINT interaction_definitions_current_revision_fkey
    FOREIGN KEY (current_revision_id, id)
    REFERENCES public.interaction_definition_revisions(revision_id, definition_id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX idx_interaction_definition_revisions_definition
    ON public.interaction_definition_revisions (definition_id, revision_number DESC);

CREATE TABLE public.interaction_definition_lineage (
    definition_revision_id uuid PRIMARY KEY REFERENCES public.interaction_definition_revisions(revision_id) ON DELETE CASCADE,
    lineage_kind text NOT NULL CHECK (lineage_kind IN ('published_from', 'copied_from')),
    source_definition_id uuid NOT NULL REFERENCES public.interaction_definitions(id) ON DELETE RESTRICT,
    source_revision_id uuid NOT NULL REFERENCES public.interaction_definition_revisions(revision_id) ON DELETE RESTRICT,
    source_bundle_digest text NOT NULL REFERENCES public.interaction_source_bundles(digest) ON DELETE RESTRICT
);

CREATE INDEX idx_interaction_definition_lineage_source
    ON public.interaction_definition_lineage (source_definition_id, lineage_kind, definition_revision_id);

CREATE TABLE public.interaction_instances (
    id uuid PRIMARY KEY,
    owner_kind text NOT NULL CHECK (owner_kind IN ('user', 'project')),
    owner_id text NOT NULL,
    definition_id uuid NOT NULL REFERENCES public.interaction_definitions(id) ON DELETE RESTRICT,
    definition_revision_id uuid NOT NULL REFERENCES public.interaction_definition_revisions(revision_id) ON DELETE RESTRICT,
    contract_version smallint NOT NULL CHECK (contract_version = 1),
    state_revision bigint NOT NULL CHECK (state_revision >= 0),
    status text NOT NULL CHECK (status IN ('open', 'closed')),
    state jsonb NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    closed_at timestamptz,
    CHECK ((status = 'open' AND closed_at IS NULL) OR (status = 'closed' AND closed_at IS NOT NULL))
);

CREATE INDEX idx_interaction_instances_owner
    ON public.interaction_instances (owner_kind, owner_id, status, updated_at DESC, id);
CREATE INDEX idx_interaction_instances_definition_revision
    ON public.interaction_instances (definition_revision_id, status, id);

CREATE TABLE public.interaction_state_revisions (
    instance_id uuid NOT NULL REFERENCES public.interaction_instances(id) ON DELETE CASCADE,
    state_revision bigint NOT NULL CHECK (state_revision >= 0),
    source_event_id uuid,
    state jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (instance_id, state_revision),
    UNIQUE (source_event_id)
);

CREATE TABLE public.interaction_attachments (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES public.interaction_instances(id) ON DELETE CASCADE,
    subject_kind text NOT NULL CHECK (subject_kind IN ('agent_run', 'user_workshop', 'workflow_run')),
    subject_id text NOT NULL,
    role text NOT NULL CHECK (role IN ('editor', 'observer', 'renderer', 'automation')),
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    detached_at timestamptz
);

CREATE UNIQUE INDEX interaction_attachments_active_subject_unique
    ON public.interaction_attachments (instance_id, subject_kind, subject_id)
    WHERE detached_at IS NULL;
CREATE INDEX idx_interaction_attachments_subject
    ON public.interaction_attachments (subject_kind, subject_id, detached_at, instance_id);

CREATE TABLE public.interaction_runtime_bindings (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES public.interaction_instances(id) ON DELETE CASCADE,
    attachment_id uuid REFERENCES public.interaction_attachments(id) ON DELETE CASCADE,
    attachment_scope text GENERATED ALWAYS AS (COALESCE(attachment_id::text, '')) STORED,
    slot_key text NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (instance_id, attachment_scope, slot_key)
);

CREATE INDEX idx_interaction_runtime_bindings_instance
    ON public.interaction_runtime_bindings (instance_id, attachment_scope, slot_key);

CREATE TABLE public.interaction_presentation_states (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES public.interaction_instances(id) ON DELETE CASCADE,
    user_id text NOT NULL,
    presentation_key text NOT NULL,
    revision bigint NOT NULL CHECK (revision > 0),
    value jsonb NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (instance_id, user_id, presentation_key)
);

CREATE TABLE public.interaction_renderer_leases (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES public.interaction_instances(id) ON DELETE CASCADE,
    renderer_key text NOT NULL,
    user_id text NOT NULL,
    revision bigint NOT NULL CHECK (revision > 0),
    acquired_at timestamptz NOT NULL,
    renewed_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    CHECK (renewed_at >= acquired_at),
    CHECK (expires_at > renewed_at),
    CHECK (expires_at <= renewed_at + interval '5 minutes'),
    UNIQUE (instance_id, renderer_key)
);

CREATE INDEX idx_interaction_renderer_leases_active
    ON public.interaction_renderer_leases (instance_id, expires_at);

CREATE TABLE public.interaction_events (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES public.interaction_instances(id) ON DELETE CASCADE,
    sequence bigint NOT NULL CHECK (sequence > 0),
    command_id uuid NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (instance_id, sequence),
    UNIQUE (instance_id, command_id)
);

ALTER TABLE public.interaction_state_revisions
    ADD CONSTRAINT interaction_state_revisions_source_event_fkey
    FOREIGN KEY (source_event_id) REFERENCES public.interaction_events(id) ON DELETE RESTRICT;

CREATE TABLE public.interaction_operation_effect_intents (
    effect_id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES public.interaction_instances(id) ON DELETE CASCADE,
    source_event_id uuid NOT NULL UNIQUE REFERENCES public.interaction_events(id) ON DELETE RESTRICT,
    status text NOT NULL CHECK (status IN ('pending', 'claimed', 'succeeded', 'retry_scheduled', 'terminal_failed')),
    next_attempt_at timestamptz NOT NULL,
    claim_token uuid,
    claim_expires_at timestamptz,
    document jsonb NOT NULL,
    CHECK ((status = 'claimed' AND claim_token IS NOT NULL AND claim_expires_at IS NOT NULL)
        OR (status <> 'claimed' AND claim_token IS NULL AND claim_expires_at IS NULL))
);

CREATE INDEX idx_interaction_effect_intents_claim
    ON public.interaction_operation_effect_intents (status, next_attempt_at, claim_expires_at, effect_id);

CREATE TABLE public.interaction_command_receipts (
    instance_id uuid NOT NULL REFERENCES public.interaction_instances(id) ON DELETE CASCADE,
    command_id uuid NOT NULL,
    request_digest text NOT NULL,
    event_id uuid NOT NULL UNIQUE REFERENCES public.interaction_events(id) ON DELETE RESTRICT,
    effect_id uuid REFERENCES public.interaction_operation_effect_intents(effect_id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (instance_id, command_id)
);
