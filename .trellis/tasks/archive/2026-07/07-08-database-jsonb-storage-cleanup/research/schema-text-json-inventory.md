# Research: schema-text-json-inventory

- Query: PostgreSQL migrations under `crates/agentdash-infrastructure/migrations` for live `TEXT` columns that store structured JSON, with repository cross-checks for JSON roundtrip semantics.
- Scope: internal
- Date: 2026-07-08

## Findings

### Files Found

- `crates/agentdash-infrastructure/migrations/0001_init.sql` - baseline schema for most candidate `TEXT` JSON columns.
- `crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql` - adds `lifecycle_runs.context/orchestrations/view_projection`; later migration drops `context/view_projection`.
- `crates/agentdash-infrastructure/migrations/0008_agent_frame_visible_workspace_modules.sql` - adds `agent_frames.visible_workspace_module_refs_json`.
- `crates/agentdash-infrastructure/migrations/0012_mcp_preset_runtime_binding.sql` - adds `mcp_presets.runtime_binding`.
- `crates/agentdash-infrastructure/migrations/0015_lifecycle_run_tasks_story_task_cleanup.sql` - adds `lifecycle_runs.tasks` and drops `stories.tasks/task_count`.
- `crates/agentdash-infrastructure/migrations/0026_agent_run_canvas_runtime_state.sql` - creates canvas runtime observation/snapshot payload tables.
- `crates/agentdash-infrastructure/migrations/0032_agent_run_mailbox_source_identity.sql` - adds `agent_run_mailbox_messages.source_metadata` and drops legacy `source`.
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql` - creates `agent_run_lineages.fork_point_ref/metadata`.
- `crates/agentdash-infrastructure/migrations/0040_session_events_envelope_only.sql` - removes flattened event columns, leaving `notification_json` as event envelope.
- `crates/agentdash-infrastructure/migrations/0041_drop_lifecycle_run_context_view_projection.sql` - drops `lifecycle_runs.context/view_projection`.
- `crates/agentdash-infrastructure/migrations/0045_runtime_session_trace_table_names.sql` - renames `session_*` tables to `runtime_session_*`.
- `crates/agentdash-infrastructure/migrations/0049_agent_frame_surface_document.sql` - adds and backfills `agent_frames.surface` from split JSON text projection columns.
- `crates/agentdash-infrastructure/migrations/0053_agent_run_control_effects.sql` - renames `runtime_session_terminal_effects` to `agent_run_control_effects`.
- `crates/agentdash-infrastructure/migrations/0057_lifecycle_run_channel_registry.sql` - confirms newer owner document pattern already uses `jsonb`.
- Repository cross-check files: `workflow_repository.rs`, `lifecycle_anchor_repository.rs`, `agent_run_lineage_repository.rs`, `agent_run_mailbox_repository.rs`, `session_repository.rs`, `project_backend_access_repository.rs`, `workspace_repository.rs`, `project_vfs_mount_repository.rs`, `routine_repository.rs`, `mcp_preset_repository.rs`, `settings_repository.rs`, `state_change_store.rs`, `story_repository.rs`, `backend_repository.rs`, `agent_repository.rs`, `project_repository.rs`, `llm_provider_repository.rs`, `canvas_repository.rs`, `canvas_runtime_state_repository.rs`, and `auth/session_service.rs`.

### Code Patterns

- Historical repository pattern is local string JSON helpers: `parse_json_column` / `serialize_json_column` in `agent_repository.rs:54`, `llm_provider_repository.rs:98`, `project_repository.rs:272`, `project_vfs_mount_repository.rs:45`, `routine_repository.rs:52`, `story_repository.rs:157`, `backend_repository.rs:357`, and `workflow_repository.rs:667`.
- AgentFrame currently parses optional JSON text and serializes surface projections in `lifecycle_anchor_repository.rs:198`, `lifecycle_anchor_repository.rs:226`, `lifecycle_anchor_repository.rs:255`, and `lifecycle_anchor_repository.rs:264`.
- Session persistence writes JSON strings through `json_string(...)` for event envelopes, transitions, delivery commands, lineage, compaction, and projection segments in `session_repository.rs:354`, `session_repository.rs:814`, `session_repository.rs:866`, `session_repository.rs:1283`, `session_repository.rs:1513`, and `session_repository.rs:1599`.
- Some queries cast text JSON in predicates today: `lifecycle_anchor_repository.rs:625` and `lifecycle_anchor_repository.rs:665` query `lifecycle_gates.payload_json::jsonb`; `routine_repository.rs:126` and `routine_repository.rs:174` query `routines.trigger_config::jsonb`.
- Newer pattern already exists with `sqlx::types::Json<T>` and direct `jsonb` columns, e.g. `extension_package_artifact_repository.rs:1`, `project_extension_installation_repository.rs:233`, and `runtime_health_repository.rs:63`.

### Live Inventory

Classification values used here:

- `convert_to_jsonb`: structured business document or structured payload; default target.
- `keep_text`: raw text, source file body, prompt/template text, or byte/text-preserving content.
- `convert_to_json`: none found; no column showed a key-order or duplicate-key requirement.
- `promote_scalar`: no whole-column promotion selected in this pass. Predicate-heavy JSON documents are noted for possible field-level scalar/index follow-up.
- `defer`: none selected for live structured JSON columns; not-live columns are listed under caveats.

#### Agent Frames And Frame Transitions

- `agent_frame_transitions.capability_keys_json` - current: `text NOT NULL`, no default, defined at `crates/agentdash-infrastructure/migrations/0001_init.sql:34`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: `ALTER TABLE agent_frame_transitions ALTER COLUMN capability_keys_json TYPE jsonb USING capability_keys_json::jsonb;`. Evidence: session transition insert serializes with `json_string` at `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:814`.
- `agent_frame_transitions.transition_json` - current: `text NOT NULL`, no default, defined at `crates/agentdash-infrastructure/migrations/0001_init.sql:35`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING transition_json::jsonb`. Evidence: `session_repository.rs:818`.
- `agent_frames.effective_capability_json` - current: nullable `text`, defined at `0001_init.sql:47`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable conversion should use `CASE WHEN effective_capability_json IS NULL OR NULLIF(BTRIM(effective_capability_json), '') IS NULL THEN NULL ELSE effective_capability_json::jsonb END`. Evidence: `lifecycle_anchor_repository.rs:227`.
- `agent_frames.context_slice_json` - current: nullable `text`, defined at `0001_init.sql:48`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: same nullable conversion. Evidence: `lifecycle_anchor_repository.rs:231`.
- `agent_frames.vfs_surface_json` - current: nullable `text`, defined at `0001_init.sql:49`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: same nullable conversion. Evidence: `lifecycle_anchor_repository.rs:232`.
- `agent_frames.mcp_surface_json` - current: nullable `text`, defined at `0001_init.sql:50`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: same nullable conversion. Evidence: `lifecycle_anchor_repository.rs:233`.
- `agent_frames.execution_profile_json` - current: nullable `text`, defined at `0001_init.sql:54`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: same nullable conversion. Evidence: `lifecycle_anchor_repository.rs:234`.
- `agent_frames.visible_canvas_mount_ids_json` - current: nullable `text`, defined at `0001_init.sql:55`; rewritten by mount convergence at `0022_canvas_mount_id_convergence.sql:8`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: run after 0022 rewrite; nullable `CASE` conversion. Evidence: `lifecycle_anchor_repository.rs:238`.
- `agent_frames.visible_workspace_module_refs_json` - current: nullable `text`, added at `0008_agent_frame_visible_workspace_modules.sql:4`; rewritten by mount convergence at `0022_canvas_mount_id_convergence.sql:21`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: run after 0022 rewrite; nullable `CASE` conversion. Evidence: `lifecycle_anchor_repository.rs:242`.
- `agent_frames.surface` - current: nullable `text`, added at `0049_agent_frame_surface_document.sql:2` and backfilled from split columns at `0049_agent_frame_surface_document.sql:4`. Classification: `convert_to_jsonb`. Target: nullable `jsonb` with no default unless a separate AgentFrame hardening step makes it `NOT NULL`. Migration notes: convert after 0049 backfill; nullable `CASE` conversion. Evidence: `lifecycle_anchor_repository.rs:226` and `lifecycle_anchor_repository.rs:264`.

#### Lifecycle And Workflow

- `lifecycle_runs.orchestrations` - current: `text DEFAULT '[]'::text NOT NULL`, added at `0003_lifecycle_orchestration_contract.sql:3`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: drop default, `TYPE jsonb USING orchestrations::jsonb`, set typed default. Evidence: `workflow_repository.rs:699`.
- `lifecycle_runs.tasks` - current: `text DEFAULT '[]'::text NOT NULL`, added at `0015_lifecycle_run_tasks_story_task_cleanup.sql:2`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: same array-default conversion. Evidence: `workflow_repository.rs:703`.
- `lifecycle_runs.execution_log` - current: `text DEFAULT '[]'::text NOT NULL`, defined at `0001_init.sql:288`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: same array-default conversion. Evidence: `workflow_repository.rs:708`.
- `workflow_graphs.source` - current: `text NOT NULL`, no default, defined at `0001_init.sql:769`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING source::jsonb`; this is typed definition source, not raw source text. Evidence: `workflow_repository.rs:162` and `workflow_repository.rs:657`.
- `workflow_graphs.activities` - current: `text DEFAULT '[]'::text NOT NULL`, defined at `0001_init.sql:780`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: array-default conversion. Evidence: `workflow_repository.rs:165` and `workflow_repository.rs:667`.
- `workflow_graphs.transitions` - current: `text DEFAULT '[]'::text NOT NULL`, defined at `0001_init.sql:781`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: array-default conversion. Evidence: `workflow_repository.rs:166` and `workflow_repository.rs:668`.
- `agent_procedures.source` - current: `text NOT NULL`, no default, defined at `0001_init.sql:74`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING source::jsonb`; repository treats it as typed definition source. Evidence: `workflow_repository.rs:44` and `workflow_repository.rs:611`.
- `agent_procedures.contract` - current: `text NOT NULL`, no default, defined at `0001_init.sql:76`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING contract::jsonb`. Evidence: `workflow_repository.rs:45` and `workflow_repository.rs:620`.

#### Lifecycle Anchors, Gates, And Lineage

- `lifecycle_gates.payload_json` - current: nullable `text`, defined at `0001_init.sql:276`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion; replace repository predicates from `payload_json::jsonb` to direct `payload_json`, then consider GIN/expression indexes or scalar fields for wait-policy selectors if this path is hot. Evidence: `lifecycle_anchor_repository.rs:542`, `lifecycle_anchor_repository.rs:558`, `lifecycle_anchor_repository.rs:625`, and `lifecycle_anchor_repository.rs:665`.
- `lifecycle_subject_associations.metadata_json` - current: nullable `text`, defined at `0001_init.sql:301`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion. Evidence: `lifecycle_anchor_repository.rs:405` and `lifecycle_anchor_repository.rs:419`.
- `agent_lineages.metadata_json` - current: nullable `text`, defined at `0001_init.sql:65`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion. Evidence: `lifecycle_anchor_repository.rs:1085` and `lifecycle_anchor_repository.rs:1099`.
- `agent_run_lineages.fork_point_ref` - current: nullable `text`, defined at `0038_agent_run_lineages.sql:9`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion; current column name is business-ish but repository field is `fork_point_ref_json`, so naming cleanup can be follow-up. Evidence: `agent_run_lineage_repository.rs:297` and `agent_run_lineage_repository.rs:619`.
- `agent_run_lineages.metadata` - current: nullable `text`, defined at `0038_agent_run_lineages.sql:13`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion; optional rename to `metadata_json` is naming cleanup, not required for type truth. Evidence: `agent_run_lineage_repository.rs:299` and `agent_run_lineage_repository.rs:621`.

#### Project, Agent, Backend, Workspace

- `projects.config` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:474`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `project_repository.rs:41`, `project_repository.rs:111`, and `project_repository.rs:272`.
- `project_agents.config` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:393`; rewritten as text JSON by `0009_canvas_capability_to_workspace_module.sql:10`, `0021_migrate_project_agent_mcp_preset_keys.sql:11`, `0022_canvas_mount_id_convergence.sql:45`, and `0028_workspace_module_operate_tool_name.sql:10`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: new conversion must run after those text rewrites; object-default conversion. Evidence: `agent_repository.rs:54`, `agent_repository.rs:77`, and `agent_repository.rs:161`.
- `project_backend_access.root_policy` - current: `text NOT NULL`; initial default `DEFAULT '{"kind":"backend_inventory"}'::text` is defined at `0001_init.sql:414`, then changed to workspace registry at `0019_decouple_workspace_inventory_from_runtime_health.sql:16`. Classification: `convert_to_jsonb`. Target: current semantic default as `jsonb` (`'{"kind":"workspace_registry"}'::jsonb`) and `NOT NULL`. Migration notes: drop current text default, cast, set typed default. Evidence: `project_backend_access_repository.rs:337`.
- `project_backend_access.capability_policy` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:415`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `project_backend_access_repository.rs:338`.
- `backend_workspace_inventory.identity_payload` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:125`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `project_backend_access_repository.rs:368`.
- `backend_workspace_inventory.detected_facts` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:126`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `project_backend_access_repository.rs:373`.
- `workspace_bindings.detected_facts` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:790`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `workspace_repository.rs:351`.
- `workspaces.identity_payload` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:802`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `workspace_repository.rs:103` and `workspace_repository.rs:316`.
- `workspaces.mount_capabilities` - current: `text DEFAULT '["read","write","list","search","exec"]'::text NOT NULL`, defined at `0001_init.sql:806`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '["read","write","list","search","exec"]'::jsonb NOT NULL`. Migration notes: array-default conversion with exact capability list preserved. Evidence: `workspace_repository.rs:107` and `workspace_repository.rs:303`.

#### VFS, Canvas, And Canvas Runtime

- `project_vfs_mounts.capabilities` - current: `text DEFAULT '[]'::text NOT NULL`, defined at `0001_init.sql:463`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: array-default conversion. Evidence: `project_vfs_mount_repository.rs:45`.
- `project_vfs_mounts.installed_source` - current: nullable `text`, defined at `0001_init.sql:464`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion. Evidence: `project_vfs_mount_repository.rs:46` and `project_vfs_mount_repository.rs:75`.
- `project_vfs_mounts.content` - current: `text NOT NULL`, no default, defined at `0001_init.sql:465`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING content::jsonb`; this is typed VFS mount content, not `canvas_files.content` raw file text. Evidence: `project_vfs_mount_repository.rs:50` and `project_vfs_mount_repository.rs:79`.
- `canvases.sandbox_config` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:177`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `canvas_repository.rs:121`, `canvas_repository.rs:329`, and `canvas_repository.rs:458`.
- `agent_run_canvas_observations.payload` - current: `text NOT NULL`, no default, defined at `0026_agent_run_canvas_runtime_state.sql:16`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING payload::jsonb`. Evidence: `canvas_runtime_state_repository.rs:39` and `canvas_runtime_state_repository.rs:163`.
- `agent_run_canvas_snapshots.payload` - current: `text NOT NULL`, no default, defined at `0026_agent_run_canvas_runtime_state.sql:37`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING payload::jsonb`. Evidence: `canvas_runtime_state_repository.rs:98` and `canvas_runtime_state_repository.rs:167`.

#### Routines And Settings

- `routines.trigger_config` - current: `text NOT NULL`, no default, defined at `0001_init.sql:507`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING trigger_config::jsonb`; update queries to remove `::jsonb` casts and consider GIN/expression index or field-level scalar promotion for trigger lookup. Evidence: `routine_repository.rs:52`, `routine_repository.rs:126`, and `routine_repository.rs:174`.
- `routines.dispatch_strategy` - current: `text NOT NULL`, no default, defined at `0001_init.sql:508`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING dispatch_strategy::jsonb`. Evidence: `routine_repository.rs:53` and `routine_repository.rs:70`.
- `routine_executions.trigger_payload` - current: nullable `text`, defined at `0001_init.sql:488`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion. Evidence: `routine_repository.rs:257`, `routine_repository.rs:276`, and `routine_repository.rs:328`.
- `settings.value` - current: `text NOT NULL`, no default, defined at `0001_init.sql:674`; legacy preferences migrated to boolean JSON scalar text at `0033_migrate_user_preferences_to_settings.sql:9`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING value::jsonb`; `jsonb` accepts object, array, string, boolean, number, and null values, but invalid raw legacy strings should fail. Evidence: `settings_repository.rs:82` and `settings_repository.rs:139`.

#### LLM, MCP, Views, Stories

- `llm_providers.models` - current: `text DEFAULT '[]'::text NOT NULL`, defined at `0001_init.sql:336`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: array-default conversion. Evidence: `llm_provider_repository.rs:98` and `llm_provider_repository.rs:113`.
- `llm_providers.blocked_models` - current: `text DEFAULT '[]'::text NOT NULL`, defined at `0001_init.sql:337`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: array-default conversion. Evidence: `llm_provider_repository.rs:99` and `llm_provider_repository.rs:115`.
- `mcp_presets.transport` - current: `text NOT NULL`, no default, defined at `0001_init.sql:358`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING transport::jsonb`. Evidence: `mcp_preset_repository.rs:37` and `mcp_preset_repository.rs:267`.
- `mcp_presets.route_policy` - current: `text NOT NULL`, no default, defined at `0001_init.sql:359`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING route_policy::jsonb`. Evidence: `mcp_preset_repository.rs:38` and `mcp_preset_repository.rs:270`.
- `mcp_presets.runtime_binding` - current: nullable `text`, added at `0012_mcp_preset_runtime_binding.sql:2`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion. Evidence: `mcp_preset_repository.rs:202` and `mcp_preset_repository.rs:274`.
- `views.backend_ids` - current: `text DEFAULT '[]'::text NOT NULL`, defined at `0001_init.sql:758`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: array-default conversion. Evidence: `backend_repository.rs:281` and `backend_repository.rs:357`.
- `views.filters` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:759`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `backend_repository.rs:282` and `backend_repository.rs:358`.
- `stories.tags` - current: `text DEFAULT '[]'::text NOT NULL`, defined at `0001_init.sql:729`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: array-default conversion. Evidence: `story_repository.rs:42` and `story_repository.rs:158`.
- `stories.context` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:731`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `story_repository.rs:43` and `story_repository.rs:157`.

#### Auth, State Changes, Mailbox, Session Trace

- `auth_sessions.identity_json` - current: `text NOT NULL`, no default, defined at `0001_init.sql:89`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING identity_json::jsonb`; repository stores string today, but application serializes/deserializes typed auth identity. Evidence: `auth/session_service.rs:44`, `auth/session_service.rs:80`, and `auth_session_repository.rs:39`.
- `state_changes.payload` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:706`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `state_change_store.rs:148` and `state_change_store.rs:178`.
- `agent_run_mailbox_messages.source_metadata` - current: nullable `text`, added at `0032_agent_run_mailbox_source_identity.sql:9`. Classification: `convert_to_jsonb`. Target: nullable `jsonb`. Migration notes: nullable `CASE` conversion; adjacent `delivery_json`, `payload_json`, `executor_config_json`, and `launch_planning_input` are already `jsonb`. Evidence: `agent_run_mailbox_repository.rs:115` and `agent_run_mailbox_repository.rs:743`.
- `runtime_session_events.notification_json` - current: `text NOT NULL`, no default, defined as `session_events.notification_json` at `0001_init.sql:584`; flattened columns dropped at `0040_session_events_envelope_only.sql:1`; table renamed at `0045_runtime_session_trace_table_names.sql:9`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: use current table name `runtime_session_events`; direct `USING notification_json::jsonb`. Evidence: `session_repository.rs:354` and `session_repository.rs:1170`.
- `runtime_session_compactions.replacement_projection_json` - current: `text DEFAULT '{}'::text NOT NULL`, defined as `session_compactions.replacement_projection_json` at `0001_init.sql:567`; table renamed at `0045_runtime_session_trace_table_names.sql:15`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: use current table name; object-default conversion. Evidence: `session_repository.rs:1513`.
- `runtime_session_compactions.token_stats_json` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:568`; table renamed at `0045_runtime_session_trace_table_names.sql:15`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `session_repository.rs:1517`.
- `runtime_session_compactions.diagnostics_json` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:569`; table renamed at `0045_runtime_session_trace_table_names.sql:15`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `session_repository.rs:1521`.
- `runtime_session_lineage.fork_point_ref_json` - current: `text DEFAULT '{}'::text NOT NULL`, defined as `session_lineage.fork_point_ref_json` at `0001_init.sql:592`; table renamed at `0045_runtime_session_trace_table_names.sql:39`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: use current table name; object-default conversion. Evidence: `session_repository.rs:1283`.
- `runtime_session_lineage.metadata_json` - current: `text DEFAULT '{}'::text NOT NULL`, defined at `0001_init.sql:597`; table renamed at `0045_runtime_session_trace_table_names.sql:39`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '{}'::jsonb NOT NULL`. Migration notes: object-default conversion. Evidence: `session_repository.rs:1287`.
- `runtime_session_projection_segments.source_refs_json` - current: `text DEFAULT '[]'::text NOT NULL`, defined as `session_projection_segments.source_refs_json` at `0001_init.sql:622`; table renamed at `0045_runtime_session_trace_table_names.sql:27`. Classification: `convert_to_jsonb`. Target: `jsonb DEFAULT '[]'::jsonb NOT NULL`. Migration notes: use current table name; array-default conversion. Evidence: `session_repository.rs:1599`.
- `runtime_session_projection_segments.content_json` - current: `text NOT NULL`, no default, defined at `0001_init.sql:624`; table renamed at `0045_runtime_session_trace_table_names.sql:27`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: direct `USING content_json::jsonb`. Evidence: `session_repository.rs:1603`.
- `runtime_session_delivery_commands.payload_json` - current: `text NOT NULL`, no default, defined as `session_runtime_commands.payload_json` at `0001_init.sql:634`; table renamed at `0045_runtime_session_trace_table_names.sql:45`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: use current table name; direct `USING payload_json::jsonb`. Evidence: `session_repository.rs:866`.
- `agent_run_control_effects.payload_json` - current: `text NOT NULL`, no default, defined as `session_terminal_effects.payload_json` at `0001_init.sql:649`, renamed to `runtime_session_terminal_effects` at `0045_runtime_session_trace_table_names.sql:33`, then to `agent_run_control_effects` at `0053_agent_run_control_effects.sql:4`. Classification: `convert_to_jsonb`. Target: `jsonb NOT NULL`, no default. Migration notes: use current table name `agent_run_control_effects`; direct `USING payload_json::jsonb`. Evidence: `session_repository.rs:570`.

### Migration Ordering Notes

- Add a new forward migration after `0057_lifecycle_run_channel_registry.sql`. Do not edit historical migrations unless this task is later explicitly changed into a baseline rewrite.
- Use current table names for renamed session tables:
  - `runtime_session_events`, not `session_events`.
  - `runtime_session_compactions`, not `session_compactions`.
  - `runtime_session_lineage`, not `session_lineage`.
  - `runtime_session_projection_segments`, not `session_projection_segments`.
  - `runtime_session_delivery_commands`, not `session_runtime_commands`.
  - `agent_run_control_effects`, not `session_terminal_effects` or `runtime_session_terminal_effects`.
- For `NOT NULL` object defaults: drop old `text` default, `TYPE jsonb USING col::jsonb`, set `DEFAULT '{}'::jsonb`, preserve/set `NOT NULL`.
- For `NOT NULL` array defaults: drop old `text` default, `TYPE jsonb USING col::jsonb`, set `DEFAULT '[]'::jsonb`, preserve/set `NOT NULL`.
- For nullable document columns: prefer `USING CASE WHEN col IS NULL OR NULLIF(BTRIM(col), '') IS NULL THEN NULL ELSE col::jsonb END` so historical blank strings become null rather than invalid JSON. This applies only to nullable columns.
- For no-default `NOT NULL` document columns: direct `USING col::jsonb` is the intended pre-release behavior. Invalid historical data should fail the migration rather than be silently rewritten.
- For text JSON currently used in SQL predicates (`lifecycle_gates.payload_json`, `routines.trigger_config`), convert the column first, then update repository SQL to remove casts; add `jsonb` indexes or scalar columns only when the query contract warrants it.
- `_json` suffix cleanup is separate from type truth. Many columns currently encode projection/debug/payload semantics in the suffix; this inventory does not choose business rename targets except where noted.

### Related Specs

- `.trellis/spec/backend/database-guidelines.md` - documents migrations as schema source of truth, `jsonb` as default structured document storage, and typed repository mapping using `sqlx::types::Json<T>`.
- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/prd.md` - requires inventory and explicit classification for structured JSON in `TEXT`.
- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/design.md` - defines JSON vs JSONB rules, raw text exceptions, scalar promotion criteria, and migration patterns.
- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/implement.md` - places inventory before migration and repository conversion.

### External References

- None. This inventory is based on repository migrations, repository/application code, and local Trellis specs.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this worker session. The output path was taken from the user-provided active task path.
- I did not run a live PostgreSQL schema introspection. The inventory is source-of-truth based on migration ordering plus repository grep.
- No `convert_to_json` candidates were found. I did not find a business requirement to preserve object key order, duplicate keys, or raw JSON text shape.
- No whole-column `promote_scalar` candidate was selected. Existing claim/status/order fields are already scalar; `lifecycle_gates.payload_json` and `routines.trigger_config` have JSON predicates and may need expression indexes or field-level scalar promotion after type conversion.
- Not-live structured text columns were excluded:
  - `lifecycle_runs.context` and `lifecycle_runs.view_projection` were added in `0003_lifecycle_orchestration_contract.sql:2` and dropped by `0041_drop_lifecycle_run_context_view_projection.sql:1`.
  - `lifecycle_workflow_instances.activity_state_json` was defined at `0001_init.sql:311`, but `lifecycle_workflow_instances` was dropped by `0004_orchestration_runtime_convergence.sql:30`.
  - `user_preferences.value` was defined at `0001_init.sql:739`, migrated to `settings`, and dropped by `0033_migrate_user_preferences_to_settings.sql:22`.
  - `stories.tasks` is already `jsonb` at `0001_init.sql:734` and was dropped by `0015_lifecycle_run_tasks_story_task_cleanup.sql:4`.
  - `canvas_bindings` was dropped by `0029_drop_canvas_bindings.sql:1`.
- `keep_text` examples reviewed:
  - `canvas_files.content` at `0001_init.sql:167` is raw canvas file text, not a system JSON document.
  - `inline_fs_files.text_content` at `0001_init.sql:226` is raw inline file text.
  - `routines.prompt_template` and `routine_executions.resolved_prompt` at `0001_init.sql:505` and `0001_init.sql:489` are prompt/template text.
  - Description, error, summary, source-ref, digest, and route/id columns are scalar/raw text even when adjacent to JSON documents.
