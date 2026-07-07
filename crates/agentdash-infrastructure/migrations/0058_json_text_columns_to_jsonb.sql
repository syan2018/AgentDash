-- Structured documents stored as text now use PostgreSQL jsonb at the persistence boundary.

ALTER TABLE agent_frame_transitions
    ALTER COLUMN capability_keys_json TYPE jsonb USING capability_keys_json::jsonb,
    ALTER COLUMN transition_json TYPE jsonb USING transition_json::jsonb;

ALTER TABLE agent_frames
    ALTER COLUMN effective_capability_json TYPE jsonb USING NULLIF(BTRIM(effective_capability_json), '')::jsonb,
    ALTER COLUMN context_slice_json TYPE jsonb USING NULLIF(BTRIM(context_slice_json), '')::jsonb,
    ALTER COLUMN vfs_surface_json TYPE jsonb USING NULLIF(BTRIM(vfs_surface_json), '')::jsonb,
    ALTER COLUMN mcp_surface_json TYPE jsonb USING NULLIF(BTRIM(mcp_surface_json), '')::jsonb,
    ALTER COLUMN execution_profile_json TYPE jsonb USING NULLIF(BTRIM(execution_profile_json), '')::jsonb,
    ALTER COLUMN visible_canvas_mount_ids_json TYPE jsonb USING NULLIF(BTRIM(visible_canvas_mount_ids_json), '')::jsonb,
    ALTER COLUMN visible_workspace_module_refs_json TYPE jsonb USING NULLIF(BTRIM(visible_workspace_module_refs_json), '')::jsonb,
    ALTER COLUMN surface TYPE jsonb USING NULLIF(BTRIM(surface), '')::jsonb;

ALTER TABLE agent_lineages
    ALTER COLUMN metadata_json TYPE jsonb USING NULLIF(BTRIM(metadata_json), '')::jsonb;

ALTER TABLE agent_procedures
    ALTER COLUMN contract TYPE jsonb USING contract::jsonb;

UPDATE agent_procedures
SET source = TRIM(BOTH '"' FROM source)
WHERE source LIKE '"%"';

ALTER TABLE auth_sessions
    ALTER COLUMN identity_json TYPE jsonb USING identity_json::jsonb;

ALTER TABLE backend_workspace_inventory
    ALTER COLUMN identity_payload DROP DEFAULT,
    ALTER COLUMN identity_payload TYPE jsonb USING identity_payload::jsonb,
    ALTER COLUMN identity_payload SET DEFAULT '{}'::jsonb,
    ALTER COLUMN detected_facts DROP DEFAULT,
    ALTER COLUMN detected_facts TYPE jsonb USING detected_facts::jsonb,
    ALTER COLUMN detected_facts SET DEFAULT '{}'::jsonb;

ALTER TABLE canvases
    ALTER COLUMN sandbox_config DROP DEFAULT,
    ALTER COLUMN sandbox_config TYPE jsonb USING sandbox_config::jsonb,
    ALTER COLUMN sandbox_config SET DEFAULT '{}'::jsonb;

ALTER TABLE lifecycle_gates
    ALTER COLUMN payload_json TYPE jsonb USING NULLIF(BTRIM(payload_json), '')::jsonb;

ALTER TABLE lifecycle_runs
    ALTER COLUMN orchestrations DROP DEFAULT,
    ALTER COLUMN orchestrations TYPE jsonb USING orchestrations::jsonb,
    ALTER COLUMN orchestrations SET DEFAULT '[]'::jsonb,
    ALTER COLUMN tasks DROP DEFAULT,
    ALTER COLUMN tasks TYPE jsonb USING tasks::jsonb,
    ALTER COLUMN tasks SET DEFAULT '[]'::jsonb,
    ALTER COLUMN execution_log DROP DEFAULT,
    ALTER COLUMN execution_log TYPE jsonb USING execution_log::jsonb,
    ALTER COLUMN execution_log SET DEFAULT '[]'::jsonb;

UPDATE lifecycle_runs
SET status = TRIM(BOTH '"' FROM status)
WHERE status LIKE '"%"';

ALTER TABLE lifecycle_subject_associations
    ALTER COLUMN metadata_json TYPE jsonb USING NULLIF(BTRIM(metadata_json), '')::jsonb;

ALTER TABLE IF EXISTS lifecycle_workflow_instances
    ALTER COLUMN activity_state_json TYPE jsonb USING NULLIF(BTRIM(activity_state_json), '')::jsonb;

ALTER TABLE llm_providers
    ALTER COLUMN models DROP DEFAULT,
    ALTER COLUMN models TYPE jsonb USING models::jsonb,
    ALTER COLUMN models SET DEFAULT '[]'::jsonb,
    ALTER COLUMN blocked_models DROP DEFAULT,
    ALTER COLUMN blocked_models TYPE jsonb USING blocked_models::jsonb,
    ALTER COLUMN blocked_models SET DEFAULT '[]'::jsonb;

ALTER TABLE mcp_presets
    ALTER COLUMN transport TYPE jsonb USING transport::jsonb,
    ALTER COLUMN runtime_binding TYPE jsonb USING NULLIF(BTRIM(runtime_binding), '')::jsonb;

UPDATE mcp_presets
SET route_policy = TRIM(BOTH '"' FROM route_policy)
WHERE route_policy LIKE '"%"';

ALTER TABLE project_agents
    ALTER COLUMN config DROP DEFAULT,
    ALTER COLUMN config TYPE jsonb USING config::jsonb,
    ALTER COLUMN config SET DEFAULT '{}'::jsonb;

ALTER TABLE project_backend_access
    ALTER COLUMN root_policy DROP DEFAULT,
    ALTER COLUMN root_policy TYPE jsonb USING root_policy::jsonb,
    ALTER COLUMN root_policy SET DEFAULT '{"kind":"workspace_registry"}'::jsonb,
    ALTER COLUMN capability_policy DROP DEFAULT,
    ALTER COLUMN capability_policy TYPE jsonb USING capability_policy::jsonb,
    ALTER COLUMN capability_policy SET DEFAULT '{}'::jsonb;

ALTER TABLE project_vfs_mounts
    ALTER COLUMN capabilities DROP DEFAULT,
    ALTER COLUMN capabilities TYPE jsonb USING capabilities::jsonb,
    ALTER COLUMN capabilities SET DEFAULT '[]'::jsonb,
    ALTER COLUMN installed_source TYPE jsonb USING NULLIF(BTRIM(installed_source), '')::jsonb,
    ALTER COLUMN content TYPE jsonb USING content::jsonb;

ALTER TABLE projects
    ALTER COLUMN config DROP DEFAULT,
    ALTER COLUMN config TYPE jsonb USING config::jsonb,
    ALTER COLUMN config SET DEFAULT '{}'::jsonb;

ALTER TABLE routine_executions
    ALTER COLUMN trigger_payload TYPE jsonb USING NULLIF(BTRIM(trigger_payload), '')::jsonb;

ALTER TABLE routines
    ALTER COLUMN trigger_config TYPE jsonb USING trigger_config::jsonb,
    ALTER COLUMN dispatch_strategy TYPE jsonb USING dispatch_strategy::jsonb;

ALTER TABLE settings
    ALTER COLUMN value TYPE jsonb USING value::jsonb;

ALTER TABLE state_changes
    ALTER COLUMN payload DROP DEFAULT,
    ALTER COLUMN payload TYPE jsonb USING payload::jsonb,
    ALTER COLUMN payload SET DEFAULT '{}'::jsonb;

ALTER TABLE stories
    ALTER COLUMN tags DROP DEFAULT,
    ALTER COLUMN tags TYPE jsonb USING tags::jsonb,
    ALTER COLUMN tags SET DEFAULT '[]'::jsonb,
    ALTER COLUMN context DROP DEFAULT,
    ALTER COLUMN context TYPE jsonb USING context::jsonb,
    ALTER COLUMN context SET DEFAULT '{}'::jsonb;

ALTER TABLE views
    ALTER COLUMN backend_ids DROP DEFAULT,
    ALTER COLUMN backend_ids TYPE jsonb USING backend_ids::jsonb,
    ALTER COLUMN backend_ids SET DEFAULT '[]'::jsonb,
    ALTER COLUMN filters DROP DEFAULT,
    ALTER COLUMN filters TYPE jsonb USING filters::jsonb,
    ALTER COLUMN filters SET DEFAULT '{}'::jsonb;

ALTER TABLE workflow_graphs
    ALTER COLUMN activities DROP DEFAULT,
    ALTER COLUMN activities TYPE jsonb USING activities::jsonb,
    ALTER COLUMN activities SET DEFAULT '[]'::jsonb,
    ALTER COLUMN transitions DROP DEFAULT,
    ALTER COLUMN transitions TYPE jsonb USING transitions::jsonb,
    ALTER COLUMN transitions SET DEFAULT '[]'::jsonb;

UPDATE workflow_graphs
SET source = TRIM(BOTH '"' FROM source)
WHERE source LIKE '"%"';

ALTER TABLE workspace_bindings
    ALTER COLUMN detected_facts DROP DEFAULT,
    ALTER COLUMN detected_facts TYPE jsonb USING detected_facts::jsonb,
    ALTER COLUMN detected_facts SET DEFAULT '{}'::jsonb;

ALTER TABLE workspaces
    ALTER COLUMN identity_payload DROP DEFAULT,
    ALTER COLUMN identity_payload TYPE jsonb USING identity_payload::jsonb,
    ALTER COLUMN identity_payload SET DEFAULT '{}'::jsonb,
    ALTER COLUMN mount_capabilities DROP DEFAULT,
    ALTER COLUMN mount_capabilities TYPE jsonb USING mount_capabilities::jsonb,
    ALTER COLUMN mount_capabilities SET DEFAULT '["read","write","list","search","exec"]'::jsonb;

ALTER TABLE agent_run_canvas_runtime_observations
    ALTER COLUMN payload TYPE jsonb USING payload::jsonb;

ALTER TABLE agent_run_canvas_interaction_snapshots
    ALTER COLUMN payload TYPE jsonb USING payload::jsonb;

ALTER TABLE agent_run_mailbox_messages
    ALTER COLUMN source_metadata TYPE jsonb USING NULLIF(BTRIM(source_metadata), '')::jsonb;

ALTER TABLE agent_run_lineages
    ALTER COLUMN fork_point_ref TYPE jsonb USING NULLIF(BTRIM(fork_point_ref), '')::jsonb,
    ALTER COLUMN metadata TYPE jsonb USING NULLIF(BTRIM(metadata), '')::jsonb;

ALTER TABLE runtime_session_compactions
    ALTER COLUMN replacement_projection_json DROP DEFAULT,
    ALTER COLUMN replacement_projection_json TYPE jsonb USING replacement_projection_json::jsonb,
    ALTER COLUMN replacement_projection_json SET DEFAULT '{}'::jsonb,
    ALTER COLUMN token_stats_json DROP DEFAULT,
    ALTER COLUMN token_stats_json TYPE jsonb USING token_stats_json::jsonb,
    ALTER COLUMN token_stats_json SET DEFAULT '{}'::jsonb,
    ALTER COLUMN diagnostics_json DROP DEFAULT,
    ALTER COLUMN diagnostics_json TYPE jsonb USING diagnostics_json::jsonb,
    ALTER COLUMN diagnostics_json SET DEFAULT '{}'::jsonb;

ALTER TABLE runtime_session_events
    ALTER COLUMN notification_json TYPE jsonb USING notification_json::jsonb;

ALTER TABLE runtime_session_lineage
    ALTER COLUMN fork_point_ref_json DROP DEFAULT,
    ALTER COLUMN fork_point_ref_json TYPE jsonb USING fork_point_ref_json::jsonb,
    ALTER COLUMN fork_point_ref_json SET DEFAULT '{}'::jsonb,
    ALTER COLUMN metadata_json DROP DEFAULT,
    ALTER COLUMN metadata_json TYPE jsonb USING metadata_json::jsonb,
    ALTER COLUMN metadata_json SET DEFAULT '{}'::jsonb;

ALTER TABLE runtime_session_projection_segments
    ALTER COLUMN source_refs_json DROP DEFAULT,
    ALTER COLUMN source_refs_json TYPE jsonb USING source_refs_json::jsonb,
    ALTER COLUMN source_refs_json SET DEFAULT '[]'::jsonb,
    ALTER COLUMN content_json TYPE jsonb USING content_json::jsonb;

ALTER TABLE runtime_session_delivery_commands
    ALTER COLUMN payload_json TYPE jsonb USING payload_json::jsonb;

ALTER TABLE agent_run_control_effects
    ALTER COLUMN payload_json TYPE jsonb USING payload_json::jsonb;
