# TEXT JSON Column Inventory

日期：2026-07-08

目标：把 live schema 中承载结构化业务文档的 TEXT JSON 列迁移为 PostgreSQL `jsonb`，同时把 scalar enum/string 和 raw text 明确留在各自合适的存储类型上。

## 结论

- 默认结构化业务文档迁移为 `jsonb`，通过 repository 的 typed mapping / `json_document` helper 读写。
- 未发现需要 PostgreSQL `json` 的列；没有列需要保留 JSON key 顺序、重复 key 或原始 JSON 文本形态。
- Scalar enum/string 保持 `text`，迁移只规范化历史 JSON-string 值。
- Raw text、prompt、source body、文件内容、错误文本保持 `text`。

## Convert To JSONB

| Owner | Columns |
| --- | --- |
| Agent frame/session transition | `agent_frame_transitions.capability_keys_json`, `agent_frame_transitions.transition_json`, `agent_frames.surface`, `agent_frames.effective_capability_json`, `agent_frames.context_slice_json`, `agent_frames.vfs_surface_json`, `agent_frames.mcp_surface_json`, `agent_frames.execution_profile_json`, `agent_frames.visible_canvas_mount_ids_json`, `agent_frames.visible_workspace_module_refs_json` |
| Workflow/lifecycle | `agent_procedures.contract`, `workflow_graphs.activities`, `workflow_graphs.transitions`, `lifecycle_runs.orchestrations`, `lifecycle_runs.tasks`, `lifecycle_runs.execution_log` |
| Lifecycle anchors | `lifecycle_gates.payload_json`, `lifecycle_subject_associations.metadata_json`, `agent_lineages.metadata_json` |
| Project/config | `projects.config`, `project_agents.config`, `views.backend_ids`, `views.filters`, `stories.tags`, `stories.context`, `settings.value` |
| Backend/workspace | `project_backend_access.root_policy`, `project_backend_access.capability_policy`, `backend_workspace_inventory.identity_payload`, `backend_workspace_inventory.detected_facts`, `workspaces.identity_payload`, `workspaces.mount_capabilities`, `workspace_bindings.detected_facts` |
| VFS/canvas | `project_vfs_mounts.capabilities`, `project_vfs_mounts.installed_source`, `project_vfs_mounts.content`, `canvases.sandbox_config`, `agent_run_canvas_runtime_observations.payload`, `agent_run_canvas_interaction_snapshots.payload` |
| Routine/MCP/LLM | `routines.trigger_config`, `routines.dispatch_strategy`, `routine_executions.trigger_payload`, `mcp_presets.transport`, `mcp_presets.runtime_binding`, `llm_providers.models`, `llm_providers.blocked_models` |
| Auth/state/mailbox/lineage | `auth_sessions.identity_json`, `state_changes.payload`, `agent_run_mailbox_messages.source_metadata`, `agent_run_lineages.fork_point_ref`, `agent_run_lineages.metadata` |
| Runtime session persistence | `runtime_session_events.notification_json`, `runtime_session_compactions.replacement_projection_json`, `runtime_session_compactions.token_stats_json`, `runtime_session_compactions.diagnostics_json`, `runtime_session_lineage.fork_point_ref_json`, `runtime_session_lineage.metadata_json`, `runtime_session_projection_segments.source_refs_json`, `runtime_session_projection_segments.content_json`, `runtime_session_delivery_commands.payload_json`, `agent_run_control_effects.payload_json` |

## Scalar Text

| Columns | Reason |
| --- | --- |
| `lifecycle_runs.status` | Lifecycle state is filtered and reasoned about as a scalar status. Migration normalizes legacy JSON-string values to bare text. |
| `agent_procedures.source`, `workflow_graphs.source` | `DefinitionSource` is a scalar enum. Migration normalizes legacy JSON-string values to bare text. |
| `mcp_presets.route_policy` | `McpRoutePolicy` is an application routing enum. Migration normalizes legacy JSON-string values to bare text. |
| `state_changes.kind`, story status/priority/type, backend type, workspace status/kind/policy, project/backend access scalar fields | These are scalar business states or identifiers. Repository maps them with explicit string helpers. |

## Keep Text

| Columns / families | Reason |
| --- | --- |
| `canvas_files.content`, `inline_fs_files.text_content` | Raw file/source text. |
| `routines.prompt_template`, `routine_executions.resolved_prompt`, execution/error text | User-authored prompt and runtime text. |
| descriptions, notes, titles, labels, digest/source-ref strings, error messages | Human text or scalar identifiers. |

## Historical Migration Hits

`rg` over all migrations still reports `_json text` and `DEFAULT '{}'::text` in older migration files because migration history is append-only. The live target is expressed by `0058_json_text_columns_to_jsonb.sql`; historical files are not rewritten in this task.

Not-live historical candidates:

- `lifecycle_runs.context` and `lifecycle_runs.view_projection` were dropped by `0041_drop_lifecycle_run_context_view_projection.sql`.
- `lifecycle_workflow_instances.activity_state_json` belongs to a dropped table; `0058` keeps an `ALTER TABLE IF EXISTS` guard for old local databases that may still pass through that migration state.
- `user_preferences.value`, `stories.tasks`, `canvas_bindings` are no longer live schema owners.

## Validation Notes

- `cargo check -p agentdash-infrastructure` verifies typed repository mapping after conversion.
- `pnpm run migration:guard` verifies migration history ownership.
- Static grep for JSON string helpers should be empty in `crates/agentdash-infrastructure/src/persistence/postgres` and `session_core.rs` after this task.
