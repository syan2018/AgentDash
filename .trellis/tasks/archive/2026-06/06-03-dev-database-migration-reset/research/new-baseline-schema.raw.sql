--
-- PostgreSQL database dump
--

\restrict tmvVrJiLr4fri51Bg6eGiWC704CeaoiVDLKohjpjvN2iYMz8Dc9OH24Prtcx097

-- Dumped from database version 18.3
-- Dumped by pg_dump version 18.3

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: pgcrypto; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;


--
-- Name: EXTENSION pgcrypto; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pgcrypto IS 'cryptographic functions';


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: activity_execution_claims; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.activity_execution_claims (
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
    graph_instance_id text DEFAULT '00000000-0000-0000-0000-000000000000'::text NOT NULL
);


--
-- Name: agent_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.agent_assignments (
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


--
-- Name: agent_frame_transitions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.agent_frame_transitions (
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


--
-- Name: agent_frames; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.agent_frames (
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
    runtime_session_refs_json text,
    created_by_kind text DEFAULT 'backfill'::text NOT NULL,
    created_by_id text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    execution_profile_json text,
    visible_canvas_mount_ids_json text
);


--
-- Name: agent_lineages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.agent_lineages (
    id text NOT NULL,
    run_id text NOT NULL,
    parent_agent_id text,
    child_agent_id text NOT NULL,
    relation_kind text NOT NULL,
    source_frame_id text,
    metadata_json text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: agent_procedures; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.agent_procedures (
    id text CONSTRAINT workflow_definitions_id_not_null NOT NULL,
    key text CONSTRAINT workflow_definitions_key_not_null NOT NULL,
    name text CONSTRAINT workflow_definitions_name_not_null NOT NULL,
    description text DEFAULT ''::text CONSTRAINT workflow_definitions_description_not_null NOT NULL,
    source text CONSTRAINT workflow_definitions_source_not_null NOT NULL,
    version integer CONSTRAINT workflow_definitions_version_not_null NOT NULL,
    contract text CONSTRAINT workflow_definitions_contract_not_null NOT NULL,
    created_at timestamp with time zone CONSTRAINT workflow_definitions_created_at_not_null NOT NULL,
    updated_at timestamp with time zone CONSTRAINT workflow_definitions_updated_at_not_null NOT NULL,
    project_id text DEFAULT '00000000-0000-0000-0000-000000000000'::text CONSTRAINT workflow_definitions_project_id_not_null NOT NULL,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone
);


--
-- Name: auth_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.auth_sessions (
    token_hash text NOT NULL,
    identity_json text NOT NULL,
    expires_at bigint,
    revoked_at bigint,
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL
);


--
-- Name: backend_execution_leases; Type: TABLE; Schema: public; Owner: -
--

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


--
-- Name: backend_workspace_inventory; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.backend_workspace_inventory (
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


--
-- Name: backends; Type: TABLE; Schema: public; Owner: -
--

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
    legacy_machine_ids jsonb DEFAULT '[]'::jsonb NOT NULL,
    visibility text DEFAULT 'private'::text NOT NULL,
    share_scope_kind text DEFAULT 'user'::text NOT NULL,
    share_scope_id text,
    capability_slot text DEFAULT 'default'::text NOT NULL
);


--
-- Name: canvas_bindings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.canvas_bindings (
    canvas_id text NOT NULL,
    alias text NOT NULL,
    source_uri text NOT NULL,
    content_type text DEFAULT 'application/json'::text NOT NULL
);


--
-- Name: canvas_files; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.canvas_files (
    canvas_id text NOT NULL,
    path text NOT NULL,
    content text DEFAULT ''::text NOT NULL
);


--
-- Name: canvases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.canvases (
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


--
-- Name: extension_package_artifacts; Type: TABLE; Schema: public; Owner: -
--

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


--
-- Name: group_memberships; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.group_memberships (
    user_id text NOT NULL,
    group_id text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.groups (
    group_id text NOT NULL,
    display_name text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: inline_fs_files; Type: TABLE; Schema: public; Owner: -
--

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


--
-- Name: library_assets; Type: TABLE; Schema: public; Owner: -
--

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
    CONSTRAINT library_assets_source_check CHECK ((source = ANY (ARRAY['builtin'::text, 'user_authored'::text, 'remote_imported'::text, 'plugin_embedded'::text]))),
    CONSTRAINT library_assets_type_check CHECK ((asset_type = ANY (ARRAY['agent_template'::text, 'mcp_server_template'::text, 'workflow_template'::text, 'skill_template'::text, 'vfs_mount_template'::text, 'extension_template'::text])))
);


--
-- Name: lifecycle_agents; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.lifecycle_agents (
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


--
-- Name: lifecycle_gates; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.lifecycle_gates (
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


--
-- Name: lifecycle_runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.lifecycle_runs (
    id text NOT NULL,
    project_id text NOT NULL,
    root_graph_id text CONSTRAINT lifecycle_runs_lifecycle_id_not_null NOT NULL,
    status text NOT NULL,
    record_artifacts text NOT NULL,
    execution_log text DEFAULT '[]'::text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    last_activity_at timestamp with time zone NOT NULL,
    active_node_keys text DEFAULT '[]'::text NOT NULL
);


--
-- Name: lifecycle_subject_associations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.lifecycle_subject_associations (
    id text NOT NULL,
    anchor_run_id text NOT NULL,
    anchor_agent_id text,
    subject_kind text NOT NULL,
    subject_id text NOT NULL,
    role text NOT NULL,
    metadata_json text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: lifecycle_workflow_instances; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.lifecycle_workflow_instances (
    id text NOT NULL,
    run_id text NOT NULL,
    graph_id text NOT NULL,
    role text NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    activity_state_json text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: llm_provider_user_credentials; Type: TABLE; Schema: public; Owner: -
--

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


--
-- Name: llm_providers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.llm_providers (
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


--
-- Name: mcp_presets; Type: TABLE; Schema: public; Owner: -
--

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


--
-- Name: permission_grants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.permission_grants (
    id text NOT NULL,
    run_id text NOT NULL,
    source_runtime_session_id text CONSTRAINT permission_grants_session_id_not_null NOT NULL,
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


--
-- Name: project_agents; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_agents (
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


--
-- Name: project_backend_access; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_backend_access (
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


--
-- Name: project_extension_installations; Type: TABLE; Schema: public; Owner: -
--

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


--
-- Name: project_subject_grants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_subject_grants (
    project_id text NOT NULL,
    subject_type text NOT NULL,
    subject_id text NOT NULL,
    role text NOT NULL,
    granted_by_user_id text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: project_vfs_mounts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_vfs_mounts (
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


--
-- Name: projects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.projects (
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


--
-- Name: routine_executions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.routine_executions (
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


--
-- Name: routines; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.routines (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    prompt_template text NOT NULL,
    project_agent_id text CONSTRAINT routines_agent_id_not_null NOT NULL,
    trigger_config text NOT NULL,
    dispatch_strategy text CONSTRAINT routines_session_strategy_not_null NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    last_fired_at timestamp with time zone
);


--
-- Name: runtime_health; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.runtime_health (
    backend_id text NOT NULL,
    profile_id text,
    name text NOT NULL,
    status text NOT NULL,
    version text,
    capabilities jsonb DEFAULT '{}'::jsonb NOT NULL,
    workspace_roots jsonb DEFAULT '[]'::jsonb CONSTRAINT runtime_health_accessible_roots_not_null NOT NULL,
    device jsonb DEFAULT '{}'::jsonb NOT NULL,
    connected_at timestamp with time zone,
    last_seen_at timestamp with time zone,
    disconnected_at timestamp with time zone,
    disconnect_reason text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT runtime_health_status_check CHECK ((status = ANY (ARRAY['online'::text, 'offline'::text, 'starting'::text, 'degraded'::text, 'stopping'::text, 'error'::text])))
);


--
-- Name: runtime_session_execution_anchors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.runtime_session_execution_anchors (
    runtime_session_id text NOT NULL,
    run_id uuid NOT NULL,
    launch_frame_id uuid NOT NULL,
    agent_id uuid NOT NULL,
    assignment_id uuid,
    graph_instance_id uuid,
    activity_key text,
    attempt integer,
    created_by_kind text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: session_compactions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.session_compactions (
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


--
-- Name: session_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.session_events (
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


--
-- Name: session_lineage; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.session_lineage (
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


--
-- Name: session_projection_heads; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.session_projection_heads (
    session_id text NOT NULL,
    projection_kind text NOT NULL,
    projection_version bigint NOT NULL,
    head_event_seq bigint NOT NULL,
    active_compaction_id text,
    updated_by_event_seq bigint,
    updated_at_ms bigint NOT NULL
);


--
-- Name: session_projection_segments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.session_projection_segments (
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


--
-- Name: session_runtime_commands; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.session_runtime_commands (
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


--
-- Name: session_terminal_effects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.session_terminal_effects (
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


--
-- Name: sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.sessions (
    id text NOT NULL,
    title text NOT NULL,
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL,
    last_event_seq bigint DEFAULT 0 NOT NULL,
    last_execution_status text DEFAULT 'idle'::text NOT NULL,
    last_turn_id text,
    last_terminal_message text,
    executor_config_json text,
    executor_session_id text,
    title_source text DEFAULT 'auto'::text NOT NULL,
    tab_layout_json text,
    project_id text
);


--
-- Name: settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.settings (
    scope_kind text NOT NULL,
    scope_id text DEFAULT ''::text NOT NULL,
    key text NOT NULL,
    value text NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: skill_assets; Type: TABLE; Schema: public; Owner: -
--

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


--
-- Name: state_changes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.state_changes (
    id bigint NOT NULL,
    project_id text DEFAULT ''::text NOT NULL,
    entity_id text NOT NULL,
    kind text NOT NULL,
    payload text DEFAULT '{}'::text NOT NULL,
    backend_id text,
    created_at timestamp with time zone NOT NULL
);


--
-- Name: state_changes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.state_changes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: state_changes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.state_changes_id_seq OWNED BY public.state_changes.id;


--
-- Name: stories; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.stories (
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


--
-- Name: user_preferences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_preferences (
    key text NOT NULL,
    value text NOT NULL
);


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

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


--
-- Name: views; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.views (
    id text NOT NULL,
    name text NOT NULL,
    backend_ids text DEFAULT '[]'::text NOT NULL,
    filters text DEFAULT '{}'::text NOT NULL,
    sort_by text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: workflow_graphs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.workflow_graphs (
    id text CONSTRAINT lifecycle_definitions_id_not_null NOT NULL,
    key text CONSTRAINT lifecycle_definitions_key_not_null NOT NULL,
    name text CONSTRAINT lifecycle_definitions_name_not_null NOT NULL,
    description text DEFAULT ''::text CONSTRAINT lifecycle_definitions_description_not_null NOT NULL,
    source text CONSTRAINT lifecycle_definitions_source_not_null NOT NULL,
    version integer CONSTRAINT lifecycle_definitions_version_not_null NOT NULL,
    created_at timestamp with time zone CONSTRAINT lifecycle_definitions_created_at_not_null NOT NULL,
    updated_at timestamp with time zone CONSTRAINT lifecycle_definitions_updated_at_not_null NOT NULL,
    project_id text DEFAULT '00000000-0000-0000-0000-000000000000'::text CONSTRAINT lifecycle_definitions_project_id_not_null NOT NULL,
    library_asset_id text,
    source_ref text,
    source_version text,
    source_digest text,
    installed_at timestamp with time zone,
    entry_activity_key text DEFAULT ''::text CONSTRAINT lifecycle_definitions_entry_activity_key_not_null NOT NULL,
    activities text DEFAULT '[]'::text CONSTRAINT lifecycle_definitions_activities_not_null NOT NULL,
    transitions text DEFAULT '[]'::text CONSTRAINT lifecycle_definitions_transitions_not_null NOT NULL
);


--
-- Name: workspace_bindings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.workspace_bindings (
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


--
-- Name: workspaces; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.workspaces (
    id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    identity_kind text DEFAULT 'local_dir'::text NOT NULL,
    identity_payload text DEFAULT '{}'::text NOT NULL,
    resolution_policy text DEFAULT 'prefer_online'::text NOT NULL,
    default_binding_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: state_changes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.state_changes ALTER COLUMN id SET DEFAULT nextval('public.state_changes_id_seq'::regclass);


--
-- Name: activity_execution_claims activity_execution_claims_idempotency_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.activity_execution_claims
    ADD CONSTRAINT activity_execution_claims_idempotency_key_key UNIQUE (idempotency_key);


--
-- Name: activity_execution_claims activity_execution_claims_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.activity_execution_claims
    ADD CONSTRAINT activity_execution_claims_pkey PRIMARY KEY (claim_id);


--
-- Name: agent_assignments agent_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_assignments
    ADD CONSTRAINT agent_assignments_pkey PRIMARY KEY (id);


--
-- Name: agent_frame_transitions agent_frame_transitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_frame_transitions
    ADD CONSTRAINT agent_frame_transitions_pkey PRIMARY KEY (id);


--
-- Name: agent_frames agent_frames_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_frames
    ADD CONSTRAINT agent_frames_pkey PRIMARY KEY (id);


--
-- Name: agent_lineages agent_lineages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_lineages
    ADD CONSTRAINT agent_lineages_pkey PRIMARY KEY (id);


--
-- Name: auth_sessions auth_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_sessions
    ADD CONSTRAINT auth_sessions_pkey PRIMARY KEY (token_hash);


--
-- Name: backend_execution_leases backend_execution_leases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_pkey PRIMARY KEY (id);


--
-- Name: backend_execution_leases backend_execution_leases_session_id_turn_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_session_id_turn_id_key UNIQUE (session_id, turn_id);


--
-- Name: backend_workspace_inventory backend_workspace_inventory_backend_id_root_ref_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backend_workspace_inventory
    ADD CONSTRAINT backend_workspace_inventory_backend_id_root_ref_key UNIQUE (backend_id, root_ref);


--
-- Name: backend_workspace_inventory backend_workspace_inventory_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backend_workspace_inventory
    ADD CONSTRAINT backend_workspace_inventory_pkey PRIMARY KEY (id);


--
-- Name: backends backends_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backends
    ADD CONSTRAINT backends_pkey PRIMARY KEY (id);


--
-- Name: canvas_bindings canvas_bindings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.canvas_bindings
    ADD CONSTRAINT canvas_bindings_pkey PRIMARY KEY (canvas_id, alias);


--
-- Name: canvas_files canvas_files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.canvas_files
    ADD CONSTRAINT canvas_files_pkey PRIMARY KEY (canvas_id, path);


--
-- Name: canvases canvases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.canvases
    ADD CONSTRAINT canvases_pkey PRIMARY KEY (id);


--
-- Name: extension_package_artifacts extension_package_artifacts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.extension_package_artifacts
    ADD CONSTRAINT extension_package_artifacts_pkey PRIMARY KEY (id);


--
-- Name: group_memberships group_memberships_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_memberships
    ADD CONSTRAINT group_memberships_pkey PRIMARY KEY (user_id, group_id);


--
-- Name: groups groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (group_id);


--
-- Name: inline_fs_files inline_fs_files_owner_kind_owner_id_container_id_path_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.inline_fs_files
    ADD CONSTRAINT inline_fs_files_owner_kind_owner_id_container_id_path_key UNIQUE (owner_kind, owner_id, container_id, path);


--
-- Name: inline_fs_files inline_fs_files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.inline_fs_files
    ADD CONSTRAINT inline_fs_files_pkey PRIMARY KEY (id);


--
-- Name: library_assets library_assets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.library_assets
    ADD CONSTRAINT library_assets_pkey PRIMARY KEY (id);


--
-- Name: lifecycle_agents lifecycle_agents_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_pkey PRIMARY KEY (id);


--
-- Name: workflow_graphs lifecycle_definitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflow_graphs
    ADD CONSTRAINT lifecycle_definitions_pkey PRIMARY KEY (id);


--
-- Name: lifecycle_gates lifecycle_gates_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_gates
    ADD CONSTRAINT lifecycle_gates_pkey PRIMARY KEY (id);


--
-- Name: lifecycle_runs lifecycle_runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_runs
    ADD CONSTRAINT lifecycle_runs_pkey PRIMARY KEY (id);


--
-- Name: lifecycle_subject_associations lifecycle_subject_associations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_subject_associations
    ADD CONSTRAINT lifecycle_subject_associations_pkey PRIMARY KEY (id);


--
-- Name: lifecycle_workflow_instances lifecycle_workflow_instances_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_workflow_instances
    ADD CONSTRAINT lifecycle_workflow_instances_pkey PRIMARY KEY (id);


--
-- Name: llm_provider_user_credentials llm_provider_user_credentials_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_pkey PRIMARY KEY (id);


--
-- Name: llm_provider_user_credentials llm_provider_user_credentials_provider_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_provider_id_user_id_key UNIQUE (provider_id, user_id);


--
-- Name: llm_providers llm_providers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_providers
    ADD CONSTRAINT llm_providers_pkey PRIMARY KEY (id);


--
-- Name: llm_providers llm_providers_slug_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_providers
    ADD CONSTRAINT llm_providers_slug_key UNIQUE (slug);


--
-- Name: mcp_presets mcp_presets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mcp_presets
    ADD CONSTRAINT mcp_presets_pkey PRIMARY KEY (id);


--
-- Name: permission_grants permission_grants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_grants
    ADD CONSTRAINT permission_grants_pkey PRIMARY KEY (id);


--
-- Name: project_agents project_agents_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_agents
    ADD CONSTRAINT project_agents_pkey PRIMARY KEY (id);


--
-- Name: project_agents project_agents_project_id_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_agents
    ADD CONSTRAINT project_agents_project_id_name_key UNIQUE (project_id, name);


--
-- Name: project_backend_access project_backend_access_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_backend_access
    ADD CONSTRAINT project_backend_access_pkey PRIMARY KEY (id);


--
-- Name: project_backend_access project_backend_access_project_id_backend_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_backend_access
    ADD CONSTRAINT project_backend_access_project_id_backend_id_key UNIQUE (project_id, backend_id);


--
-- Name: project_extension_installations project_extension_installations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_extension_installations
    ADD CONSTRAINT project_extension_installations_pkey PRIMARY KEY (id);


--
-- Name: project_extension_installations project_extension_installations_unique_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_extension_installations
    ADD CONSTRAINT project_extension_installations_unique_key UNIQUE (project_id, extension_key);


--
-- Name: project_subject_grants project_subject_grants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_subject_grants
    ADD CONSTRAINT project_subject_grants_pkey PRIMARY KEY (project_id, subject_type, subject_id);


--
-- Name: project_vfs_mounts project_vfs_mounts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_vfs_mounts
    ADD CONSTRAINT project_vfs_mounts_pkey PRIMARY KEY (id);


--
-- Name: project_vfs_mounts project_vfs_mounts_project_id_mount_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_vfs_mounts
    ADD CONSTRAINT project_vfs_mounts_project_id_mount_id_key UNIQUE (project_id, mount_id);


--
-- Name: projects projects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.projects
    ADD CONSTRAINT projects_pkey PRIMARY KEY (id);


--
-- Name: routine_executions routine_executions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.routine_executions
    ADD CONSTRAINT routine_executions_pkey PRIMARY KEY (id);


--
-- Name: routines routines_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.routines
    ADD CONSTRAINT routines_pkey PRIMARY KEY (id);


--
-- Name: routines routines_project_id_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.routines
    ADD CONSTRAINT routines_project_id_name_key UNIQUE (project_id, name);


--
-- Name: runtime_health runtime_health_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.runtime_health
    ADD CONSTRAINT runtime_health_pkey PRIMARY KEY (backend_id);


--
-- Name: runtime_session_execution_anchors runtime_session_execution_anchors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.runtime_session_execution_anchors
    ADD CONSTRAINT runtime_session_execution_anchors_pkey PRIMARY KEY (runtime_session_id);


--
-- Name: session_compactions session_compactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_compactions
    ADD CONSTRAINT session_compactions_pkey PRIMARY KEY (id);


--
-- Name: session_events session_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_events
    ADD CONSTRAINT session_events_pkey PRIMARY KEY (session_id, event_seq);


--
-- Name: session_lineage session_lineage_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_lineage
    ADD CONSTRAINT session_lineage_pkey PRIMARY KEY (child_session_id);


--
-- Name: session_projection_heads session_projection_heads_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_projection_heads
    ADD CONSTRAINT session_projection_heads_pkey PRIMARY KEY (session_id, projection_kind);


--
-- Name: session_projection_segments session_projection_segments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_projection_segments
    ADD CONSTRAINT session_projection_segments_pkey PRIMARY KEY (id);


--
-- Name: session_projection_segments session_projection_segments_session_kind_version_order_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_projection_segments
    ADD CONSTRAINT session_projection_segments_session_kind_version_order_key UNIQUE (session_id, projection_kind, projection_version, sort_order);


--
-- Name: session_runtime_commands session_runtime_commands_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_runtime_commands
    ADD CONSTRAINT session_runtime_commands_pkey PRIMARY KEY (id);


--
-- Name: session_terminal_effects session_terminal_effects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_terminal_effects
    ADD CONSTRAINT session_terminal_effects_pkey PRIMARY KEY (id);


--
-- Name: sessions sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (id);


--
-- Name: settings settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.settings
    ADD CONSTRAINT settings_pkey PRIMARY KEY (scope_kind, scope_id, key);


--
-- Name: skill_assets skill_assets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.skill_assets
    ADD CONSTRAINT skill_assets_pkey PRIMARY KEY (id);


--
-- Name: state_changes state_changes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.state_changes
    ADD CONSTRAINT state_changes_pkey PRIMARY KEY (id);


--
-- Name: stories stories_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stories
    ADD CONSTRAINT stories_pkey PRIMARY KEY (id);


--
-- Name: user_preferences user_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT user_preferences_pkey PRIMARY KEY (key);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (user_id);


--
-- Name: views views_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.views
    ADD CONSTRAINT views_pkey PRIMARY KEY (id);


--
-- Name: agent_procedures workflow_definitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_procedures
    ADD CONSTRAINT workflow_definitions_pkey PRIMARY KEY (id);


--
-- Name: workspace_bindings workspace_bindings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workspace_bindings
    ADD CONSTRAINT workspace_bindings_pkey PRIMARY KEY (id);


--
-- Name: workspaces workspaces_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_pkey PRIMARY KEY (id);


--
-- Name: idx_activity_execution_claims_run_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_activity_execution_claims_run_id ON public.activity_execution_claims USING btree (run_id);


--
-- Name: idx_agent_assignments_active_agent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_assignments_active_agent ON public.agent_assignments USING btree (agent_id, lease_status) WHERE (lease_status = 'active'::text);


--
-- Name: idx_agent_assignments_agent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_assignments_agent_id ON public.agent_assignments USING btree (agent_id);


--
-- Name: idx_agent_assignments_graph_activity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_assignments_graph_activity ON public.agent_assignments USING btree (graph_instance_id, activity_key, attempt);


--
-- Name: idx_agent_assignments_run_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_assignments_run_id ON public.agent_assignments USING btree (run_id);


--
-- Name: idx_agent_frame_transitions_run_phase; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_frame_transitions_run_phase ON public.agent_frame_transitions USING btree (run_id, lifecycle_key, phase_node);


--
-- Name: idx_agent_frame_transitions_target_frame; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_frame_transitions_target_frame ON public.agent_frame_transitions USING btree (target_frame_id, created_at_ms);


--
-- Name: idx_agent_frames_agent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_frames_agent_id ON public.agent_frames USING btree (agent_id);


--
-- Name: idx_agent_frames_agent_revision; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_agent_frames_agent_revision ON public.agent_frames USING btree (agent_id, revision);


--
-- Name: idx_agent_lineages_child; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_lineages_child ON public.agent_lineages USING btree (child_agent_id);


--
-- Name: idx_agent_lineages_parent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_lineages_parent ON public.agent_lineages USING btree (parent_agent_id) WHERE (parent_agent_id IS NOT NULL);


--
-- Name: idx_agent_lineages_run_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_agent_lineages_run_id ON public.agent_lineages USING btree (run_id);


--
-- Name: idx_backend_execution_leases_active_backend; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_backend_execution_leases_active_backend ON public.backend_execution_leases USING btree (backend_id) WHERE (state = ANY (ARRAY['claimed'::text, 'running'::text]));


--
-- Name: idx_backend_execution_leases_backend_state; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_backend_execution_leases_backend_state ON public.backend_execution_leases USING btree (backend_id, state);


--
-- Name: idx_backend_execution_leases_session; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_backend_execution_leases_session ON public.backend_execution_leases USING btree (session_id);


--
-- Name: idx_backend_workspace_inventory_backend; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_backend_workspace_inventory_backend ON public.backend_workspace_inventory USING btree (backend_id);


--
-- Name: idx_backend_workspace_inventory_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_backend_workspace_inventory_status ON public.backend_workspace_inventory USING btree (status);


--
-- Name: idx_backends_local_machine_scope_slot; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_backends_local_machine_scope_slot ON public.backends USING btree (machine_id, share_scope_kind, COALESCE(share_scope_id, ''::text), capability_slot) WHERE ((backend_type = 'local'::text) AND (machine_id IS NOT NULL) AND (share_scope_kind IS NOT NULL) AND (capability_slot IS NOT NULL));


--
-- Name: idx_extension_package_artifacts_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_extension_package_artifacts_owner ON public.extension_package_artifacts USING btree (owner_kind, owner_id);


--
-- Name: idx_extension_package_artifacts_owner_digest; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_extension_package_artifacts_owner_digest ON public.extension_package_artifacts USING btree (owner_kind, owner_id, archive_digest);


--
-- Name: idx_extension_package_artifacts_owner_extension; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_extension_package_artifacts_owner_extension ON public.extension_package_artifacts USING btree (owner_kind, owner_id, extension_id);


--
-- Name: idx_inline_fs_files_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_inline_fs_files_owner ON public.inline_fs_files USING btree (owner_kind, owner_id, container_id);


--
-- Name: idx_library_assets_asset_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_library_assets_asset_type ON public.library_assets USING btree (asset_type);


--
-- Name: idx_library_assets_identity; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_library_assets_identity ON public.library_assets USING btree (asset_type, scope, COALESCE(owner_id, ''::text), key);


--
-- Name: idx_library_assets_scope_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_library_assets_scope_owner ON public.library_assets USING btree (scope, owner_id);


--
-- Name: idx_library_assets_source_ref; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_library_assets_source_ref ON public.library_assets USING btree (source_ref);


--
-- Name: idx_lifecycle_agents_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lifecycle_agents_project_id ON public.lifecycle_agents USING btree (project_id);


--
-- Name: idx_lifecycle_agents_run_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lifecycle_agents_run_id ON public.lifecycle_agents USING btree (run_id);


--
-- Name: idx_lifecycle_definitions_library_asset_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lifecycle_definitions_library_asset_id ON public.workflow_graphs USING btree (library_asset_id);


--
-- Name: idx_lifecycle_definitions_project_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_lifecycle_definitions_project_key ON public.workflow_graphs USING btree (project_id, key);


--
-- Name: idx_lifecycle_gates_agent_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lifecycle_gates_agent_status ON public.lifecycle_gates USING btree (agent_id, status) WHERE (agent_id IS NOT NULL);


--
-- Name: idx_lifecycle_gates_correlation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lifecycle_gates_correlation ON public.lifecycle_gates USING btree (correlation_id);


--
-- Name: idx_lifecycle_gates_run_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lifecycle_gates_run_id ON public.lifecycle_gates USING btree (run_id);


--
-- Name: idx_llm_provider_user_credentials_provider; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_llm_provider_user_credentials_provider ON public.llm_provider_user_credentials USING btree (provider_id);


--
-- Name: idx_llm_provider_user_credentials_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_llm_provider_user_credentials_user ON public.llm_provider_user_credentials USING btree (user_id);


--
-- Name: idx_lsa_anchor_agent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lsa_anchor_agent ON public.lifecycle_subject_associations USING btree (anchor_agent_id) WHERE (anchor_agent_id IS NOT NULL);


--
-- Name: idx_lsa_anchor_run; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lsa_anchor_run ON public.lifecycle_subject_associations USING btree (anchor_run_id);


--
-- Name: idx_lsa_subject; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lsa_subject ON public.lifecycle_subject_associations USING btree (subject_kind, subject_id);


--
-- Name: idx_lwi_run_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_lwi_run_id ON public.lifecycle_workflow_instances USING btree (run_id);


--
-- Name: idx_lwi_run_root; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_lwi_run_root ON public.lifecycle_workflow_instances USING btree (run_id, role) WHERE (role = 'root'::text);


--
-- Name: idx_mcp_presets_library_asset_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mcp_presets_library_asset_id ON public.mcp_presets USING btree (library_asset_id);


--
-- Name: idx_mcp_presets_project_builtin_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_mcp_presets_project_builtin_key ON public.mcp_presets USING btree (project_id, builtin_key) WHERE (builtin_key IS NOT NULL);


--
-- Name: idx_mcp_presets_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mcp_presets_project_id ON public.mcp_presets USING btree (project_id);


--
-- Name: idx_mcp_presets_project_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_mcp_presets_project_key ON public.mcp_presets USING btree (project_id, key);


--
-- Name: idx_permission_grants_frame_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_permission_grants_frame_active ON public.permission_grants USING btree (effect_frame_id) WHERE (status = ANY (ARRAY['applied'::text, 'scope_escalated'::text]));


--
-- Name: idx_permission_grants_run; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_permission_grants_run ON public.permission_grants USING btree (run_id);


--
-- Name: idx_permission_grants_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_permission_grants_status ON public.permission_grants USING btree (status) WHERE (status = ANY (ARRAY['applied'::text, 'scope_escalated'::text, 'pending_user_approval'::text]));


--
-- Name: idx_project_agents_project; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_project_agents_project ON public.project_agents USING btree (project_id);


--
-- Name: idx_project_backend_access_backend; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_project_backend_access_backend ON public.project_backend_access USING btree (backend_id);


--
-- Name: idx_project_backend_access_project; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_project_backend_access_project ON public.project_backend_access USING btree (project_id);


--
-- Name: idx_project_backend_access_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_project_backend_access_status ON public.project_backend_access USING btree (status);


--
-- Name: idx_project_extension_installations_artifact; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_project_extension_installations_artifact ON public.project_extension_installations USING btree (package_artifact_id);


--
-- Name: idx_project_extension_installations_project; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_project_extension_installations_project ON public.project_extension_installations USING btree (project_id);


--
-- Name: idx_project_extension_installations_source; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_project_extension_installations_source ON public.project_extension_installations USING btree (installed_library_asset_id);


--
-- Name: idx_project_vfs_mounts_project; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_project_vfs_mounts_project ON public.project_vfs_mounts USING btree (project_id);


--
-- Name: idx_routine_exec_dispatch_assignment; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_routine_exec_dispatch_assignment ON public.routine_executions USING btree (dispatch_assignment_id) WHERE (dispatch_assignment_id IS NOT NULL);


--
-- Name: idx_routine_exec_dispatch_run; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_routine_exec_dispatch_run ON public.routine_executions USING btree (dispatch_run_id) WHERE (dispatch_run_id IS NOT NULL);


--
-- Name: idx_routine_exec_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_routine_exec_entity ON public.routine_executions USING btree (routine_id, entity_key);


--
-- Name: idx_routine_exec_routine; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_routine_exec_routine ON public.routine_executions USING btree (routine_id);


--
-- Name: idx_routine_exec_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_routine_exec_status ON public.routine_executions USING btree (routine_id, status);


--
-- Name: idx_routines_enabled; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_routines_enabled ON public.routines USING btree (enabled);


--
-- Name: idx_routines_project; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_routines_project ON public.routines USING btree (project_id);


--
-- Name: idx_rsea_agent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_rsea_agent ON public.runtime_session_execution_anchors USING btree (agent_id);


--
-- Name: idx_rsea_run; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_rsea_run ON public.runtime_session_execution_anchors USING btree (run_id);


--
-- Name: idx_runtime_health_last_seen_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_runtime_health_last_seen_at ON public.runtime_health USING btree (last_seen_at);


--
-- Name: idx_runtime_health_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_runtime_health_status ON public.runtime_health USING btree (status);


--
-- Name: idx_session_compactions_lifecycle_item; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_compactions_lifecycle_item ON public.session_compactions USING btree (session_id, lifecycle_item_id);


--
-- Name: idx_session_compactions_session_kind_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_compactions_session_kind_status ON public.session_compactions USING btree (session_id, projection_kind, status, projection_version);


--
-- Name: idx_session_compactions_source_range; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_compactions_source_range ON public.session_compactions USING btree (session_id, source_start_event_seq, source_end_event_seq);


--
-- Name: idx_session_lineage_fork_point; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_lineage_fork_point ON public.session_lineage USING btree (parent_session_id, fork_point_event_seq, fork_point_compaction_id);


--
-- Name: idx_session_lineage_parent_status_kind; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_lineage_parent_status_kind ON public.session_lineage USING btree (parent_session_id, status, relation_kind, created_at_ms, child_session_id);


--
-- Name: idx_session_projection_heads_active_compaction; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_projection_heads_active_compaction ON public.session_projection_heads USING btree (session_id, active_compaction_id);


--
-- Name: idx_session_projection_segments_projection; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_projection_segments_projection ON public.session_projection_segments USING btree (session_id, projection_kind, projection_version, sort_order);


--
-- Name: idx_session_projection_segments_source_range; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_projection_segments_source_range ON public.session_projection_segments USING btree (session_id, source_start_event_seq, source_end_event_seq);


--
-- Name: idx_session_runtime_commands_frame_transition; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_runtime_commands_frame_transition ON public.session_runtime_commands USING btree (frame_transition_id);


--
-- Name: idx_session_runtime_commands_session_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_runtime_commands_session_status ON public.session_runtime_commands USING btree (session_id, status);


--
-- Name: idx_session_runtime_commands_status_updated; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_runtime_commands_status_updated ON public.session_runtime_commands USING btree (status, updated_at_ms);


--
-- Name: idx_session_terminal_effects_session_turn; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_terminal_effects_session_turn ON public.session_terminal_effects USING btree (session_id, turn_id);


--
-- Name: idx_session_terminal_effects_status_updated; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_terminal_effects_status_updated ON public.session_terminal_effects USING btree (status, updated_at_ms);


--
-- Name: idx_session_terminal_effects_terminal_event; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_session_terminal_effects_terminal_event ON public.session_terminal_effects USING btree (session_id, terminal_event_seq);


--
-- Name: idx_skill_assets_library_asset_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_skill_assets_library_asset_id ON public.skill_assets USING btree (library_asset_id);


--
-- Name: idx_skill_assets_project_builtin_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_skill_assets_project_builtin_key ON public.skill_assets USING btree (project_id, builtin_key) WHERE (builtin_key IS NOT NULL);


--
-- Name: idx_skill_assets_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_skill_assets_project_id ON public.skill_assets USING btree (project_id);


--
-- Name: idx_skill_assets_project_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_skill_assets_project_key ON public.skill_assets USING btree (project_id, key);


--
-- Name: idx_workflow_definitions_library_asset_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_workflow_definitions_library_asset_id ON public.agent_procedures USING btree (library_asset_id);


--
-- Name: idx_workflow_definitions_project_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_workflow_definitions_project_key ON public.agent_procedures USING btree (project_id, key);


--
-- Name: ux_activity_execution_claims_active_attempt; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX ux_activity_execution_claims_active_attempt ON public.activity_execution_claims USING btree (run_id, graph_instance_id, activity_key, attempt) WHERE (status = ANY (ARRAY['claiming'::text, 'running'::text]));


--
-- Name: agent_assignments agent_assignments_agent_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_assignments
    ADD CONSTRAINT agent_assignments_agent_id_fkey FOREIGN KEY (agent_id) REFERENCES public.lifecycle_agents(id) ON DELETE CASCADE;


--
-- Name: agent_assignments agent_assignments_frame_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_assignments
    ADD CONSTRAINT agent_assignments_frame_id_fkey FOREIGN KEY (frame_id) REFERENCES public.agent_frames(id) ON DELETE CASCADE;


--
-- Name: agent_assignments agent_assignments_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_assignments
    ADD CONSTRAINT agent_assignments_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;


--
-- Name: agent_frame_transitions agent_frame_transitions_target_frame_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_frame_transitions
    ADD CONSTRAINT agent_frame_transitions_target_frame_id_fkey FOREIGN KEY (target_frame_id) REFERENCES public.agent_frames(id) ON DELETE CASCADE;


--
-- Name: agent_frames agent_frames_agent_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_frames
    ADD CONSTRAINT agent_frames_agent_id_fkey FOREIGN KEY (agent_id) REFERENCES public.lifecycle_agents(id) ON DELETE CASCADE;


--
-- Name: agent_lineages agent_lineages_child_agent_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_lineages
    ADD CONSTRAINT agent_lineages_child_agent_id_fkey FOREIGN KEY (child_agent_id) REFERENCES public.lifecycle_agents(id) ON DELETE CASCADE;


--
-- Name: agent_lineages agent_lineages_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.agent_lineages
    ADD CONSTRAINT agent_lineages_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;


--
-- Name: backend_execution_leases backend_execution_leases_backend_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backend_execution_leases
    ADD CONSTRAINT backend_execution_leases_backend_id_fkey FOREIGN KEY (backend_id) REFERENCES public.backends(id) ON DELETE CASCADE;


--
-- Name: session_runtime_commands fk_session_runtime_commands_frame_transition; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_runtime_commands
    ADD CONSTRAINT fk_session_runtime_commands_frame_transition FOREIGN KEY (frame_transition_id) REFERENCES public.agent_frame_transitions(id) ON DELETE CASCADE;


--
-- Name: lifecycle_agents lifecycle_agents_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;


--
-- Name: lifecycle_gates lifecycle_gates_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_gates
    ADD CONSTRAINT lifecycle_gates_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;


--
-- Name: lifecycle_subject_associations lifecycle_subject_associations_anchor_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_subject_associations
    ADD CONSTRAINT lifecycle_subject_associations_anchor_run_id_fkey FOREIGN KEY (anchor_run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;


--
-- Name: lifecycle_workflow_instances lifecycle_workflow_instances_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lifecycle_workflow_instances
    ADD CONSTRAINT lifecycle_workflow_instances_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.lifecycle_runs(id) ON DELETE CASCADE;


--
-- Name: llm_provider_user_credentials llm_provider_user_credentials_provider_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_provider_user_credentials
    ADD CONSTRAINT llm_provider_user_credentials_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES public.llm_providers(id) ON DELETE CASCADE;


--
-- Name: project_backend_access project_backend_access_project_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_backend_access
    ADD CONSTRAINT project_backend_access_project_id_fkey FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: runtime_health runtime_health_backend_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.runtime_health
    ADD CONSTRAINT runtime_health_backend_id_fkey FOREIGN KEY (backend_id) REFERENCES public.backends(id) ON DELETE CASCADE;


--
-- Name: session_compactions session_compactions_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_compactions
    ADD CONSTRAINT session_compactions_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;


--
-- Name: session_lineage session_lineage_child_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_lineage
    ADD CONSTRAINT session_lineage_child_session_id_fkey FOREIGN KEY (child_session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;


--
-- Name: session_lineage session_lineage_fork_point_compaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_lineage
    ADD CONSTRAINT session_lineage_fork_point_compaction_id_fkey FOREIGN KEY (fork_point_compaction_id) REFERENCES public.session_compactions(id) ON DELETE SET NULL;


--
-- Name: session_lineage session_lineage_parent_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_lineage
    ADD CONSTRAINT session_lineage_parent_session_id_fkey FOREIGN KEY (parent_session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;


--
-- Name: session_projection_heads session_projection_heads_active_compaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_projection_heads
    ADD CONSTRAINT session_projection_heads_active_compaction_id_fkey FOREIGN KEY (active_compaction_id) REFERENCES public.session_compactions(id) ON DELETE SET NULL;


--
-- Name: session_projection_heads session_projection_heads_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_projection_heads
    ADD CONSTRAINT session_projection_heads_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;


--
-- Name: session_projection_segments session_projection_segments_generated_by_compaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_projection_segments
    ADD CONSTRAINT session_projection_segments_generated_by_compaction_id_fkey FOREIGN KEY (generated_by_compaction_id) REFERENCES public.session_compactions(id) ON DELETE SET NULL;


--
-- Name: session_projection_segments session_projection_segments_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_projection_segments
    ADD CONSTRAINT session_projection_segments_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;


--
-- Name: session_runtime_commands session_runtime_commands_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_runtime_commands
    ADD CONSTRAINT session_runtime_commands_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;


--
-- Name: session_terminal_effects session_terminal_effects_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_terminal_effects
    ADD CONSTRAINT session_terminal_effects_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--

\unrestrict tmvVrJiLr4fri51Bg6eGiWC704CeaoiVDLKohjpjvN2iYMz8Dc9OH24Prtcx097

