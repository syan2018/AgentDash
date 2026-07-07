# Research: product/config repository JSONB inventory

- Query: inspect the specified PostgreSQL product/config repositories for TEXT JSON columns and serde_json string roundtrips; classify each as convert_to_jsonb now, keep_text/raw, scalar enum/string, or defer, with exact code sites and migration target.
- Scope: internal
- Date: 2026-07-08

## Findings

### Context Read

- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/prd.md` - requires explicit inventory for live TEXT JSON columns, default target `jsonb`, and typed repository mapping.
- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/design.md` - defines selection rules: business document -> `jsonb`, raw text -> `TEXT`, rare JSON text-preservation -> `json`, high-frequency predicates -> scalar.
- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/implement.md` - requires inventory before migration and repository conversion.
- `.trellis/spec/backend/database-guidelines.md` - new structured document columns use business names plus `jsonb`; repository reads/writes typed documents via `sqlx::types::Json<T>`.
- `.trellis/spec/backend/repository-pattern.md` - PostgreSQL repositories assume schema is migration-owned; repository code should map aggregate facts, not own schema bootstrap.

### Files Found

- `crates/agentdash-infrastructure/src/persistence/postgres/project_repository.rs` - owns `projects.config` mapping.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs` - owns `project_agents.config` mapping.
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs` - owns `backends.device` and `views.backend_ids/filters` mappings.
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs` - owns `stories.tags/context` mapping and writes story state-change payload through another store.
- `crates/agentdash-infrastructure/src/persistence/postgres/workspace_repository.rs` - owns `workspaces.identity_payload/mount_capabilities` and `workspace_bindings.detected_facts`.
- `crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs` - owns `canvases.sandbox_config` and raw `canvas_files.content`.
- `crates/agentdash-infrastructure/src/persistence/postgres/canvas_runtime_state_repository.rs` - owns canvas runtime observation/snapshot `payload`.
- `crates/agentdash-infrastructure/src/persistence/postgres/settings_repository.rs` - owns JSON-valued `settings.value`.
- `crates/agentdash-infrastructure/src/persistence/postgres/llm_provider_repository.rs` - owns `llm_providers.models/blocked_models`.
- `crates/agentdash-infrastructure/src/persistence/postgres/mcp_preset_repository.rs` - owns `mcp_presets.transport/route_policy/runtime_binding`.
- `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs` - owns routine config and execution payload JSON.
- `crates/agentdash-infrastructure/src/persistence/postgres/project_vfs_mount_repository.rs` - owns `project_vfs_mounts.capabilities/installed_source/content`.
- `crates/agentdash-infrastructure/src/persistence/postgres/project_backend_access_repository.rs` - owns project backend access policies and backend workspace inventory JSON.
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - baseline schema for the candidate columns.
- `crates/agentdash-infrastructure/migrations/0012_mcp_preset_runtime_binding.sql` - adds `mcp_presets.runtime_binding text`.
- `crates/agentdash-infrastructure/migrations/0019_decouple_workspace_inventory_from_runtime_health.sql` - changes backend access/inventory scalar defaults.
- `crates/agentdash-infrastructure/migrations/0023_canvas_personal_shared_distribution.sql` - adds scalar canvas ownership/scope columns.
- `crates/agentdash-infrastructure/migrations/0025_backend_workspace_inventory_identity_discovery.sql` - updates backend inventory source scalar constraint.
- `crates/agentdash-infrastructure/migrations/0026_agent_run_canvas_runtime_state.sql` - creates canvas runtime state payload TEXT columns.
- `crates/agentdash-infrastructure/migrations/0033_migrate_user_preferences_to_settings.sql` - migrates boolean preference values into `settings.value` as JSON-valid scalar text.

### Code Patterns

- Repeated local string JSON codecs exist in `project_repository.rs:311-316`, `agent_repository.rs:197-210`, `backend_repository.rs:396-401`, `story_repository.rs:247-252`, `routine_repository.rs:407-420`, `project_vfs_mount_repository.rs:183-215`, and `llm_provider_repository.rs:361-374`.
- Direct `serde_json::to_string` / `from_str` roundtrips are used in `settings_repository.rs:82` and `settings_repository.rs:139`, `canvas_repository.rs:121`, `canvas_repository.rs:329`, `canvas_repository.rs:457-459`, `canvas_runtime_state_repository.rs:39`, `canvas_runtime_state_repository.rs:98`, `canvas_runtime_state_repository.rs:163`, `canvas_runtime_state_repository.rs:167`, `workspace_repository.rs:48`, `workspace_repository.rs:103`, `workspace_repository.rs:107`, `workspace_repository.rs:201`, `workspace_repository.rs:205`, `workspace_repository.rs:307`, `workspace_repository.rs:379`, and `mcp_preset_repository.rs:37-39`, `mcp_preset_repository.rs:102-104`, `mcp_preset_repository.rs:207`, `mcp_preset_repository.rs:267-278`.
- Existing target pattern is already present elsewhere: `extension_package_artifact_repository.rs:1`, `extension_package_artifact_repository.rs:52`, `extension_package_artifact_repository.rs:120`, `project_extension_installation_repository.rs:1`, `project_extension_installation_repository.rs:56-57`, `project_extension_installation_repository.rs:212`, `project_extension_installation_repository.rs:233`, and `runtime_health_repository.rs:63-64`, `runtime_health_repository.rs:208-209` use `sqlx::types::Json<T>` with `jsonb`.

### Convert To JSONB Now

| Repository | Columns | Evidence | Migration target |
| --- | --- | --- | --- |
| `project_repository.rs` | `projects.config` | Schema is `config text DEFAULT '{}'::text NOT NULL` at `0001_init.sql:474`; repository writes string JSON at `project_repository.rs:41` and `project_repository.rs:111`, row keeps `config: String` at `project_repository.rs:240`, and reads typed `ProjectConfig` at `project_repository.rs:272`. Domain type is `Project.config: ProjectConfig` at `project/entity.rs:18` and `ProjectConfig` is a structured document at `project/value_objects.rs:8-22`. | `jsonb DEFAULT '{}'::jsonb NOT NULL`; map with `Json<ProjectConfig>`. |
| `agent_repository.rs` | `project_agents.config` | Schema is `config text DEFAULT '{}'::text NOT NULL` at `0001_init.sql:393`; repository parses at `agent_repository.rs:54` and serializes at `agent_repository.rs:77`, `agent_repository.rs:161`. Domain exposes `ProjectAgent.config: serde_json::Value` at `agent/entity.rs:21`, with typed access through agent config APIs. | `jsonb DEFAULT '{}'::jsonb NOT NULL`; map with `Json<serde_json::Value>` unless this slice also types the domain config. Update later migrations that conceptually rewrite `config::jsonb` not to cast back to text in new forward work. |
| `backend_repository.rs` | `views.backend_ids`, `views.filters` | Schema stores `backend_ids text DEFAULT '[]'::text NOT NULL` and `filters text DEFAULT '{}'::text NOT NULL` at `0001_init.sql:758-759`; repository serializes at `backend_repository.rs:281-282` and parses at `backend_repository.rs:357-358`. Domain has `ViewConfig.backend_ids: Vec<String>` and `filters: Value` at `backend/entity.rs:97-98`. | `backend_ids jsonb DEFAULT '[]'::jsonb NOT NULL`, `filters jsonb DEFAULT '{}'::jsonb NOT NULL`; map with `Json<Vec<String>>` and `Json<Value>`. |
| `story_repository.rs` | `stories.tags`, `stories.context` | Schema stores `tags text DEFAULT '[]'::text NOT NULL` and `context text DEFAULT '{}'::text NOT NULL` at `0001_init.sql:729` and `0001_init.sql:731`; repository serializes at `story_repository.rs:42-43`, `story_repository.rs:225-226` and parses at `story_repository.rs:157-158`. Domain `StoryContext` is structured at `story/value_objects.rs:49-62`. | `tags jsonb DEFAULT '[]'::jsonb NOT NULL`, `context jsonb DEFAULT '{}'::jsonb NOT NULL`; map with `Json<Vec<String>>` and `Json<StoryContext>`. |
| `workspace_repository.rs` | `workspaces.identity_payload`, `workspaces.mount_capabilities`, `workspace_bindings.detected_facts` | Schema stores `identity_payload text DEFAULT '{}'::text NOT NULL`, `mount_capabilities text DEFAULT '["read","write","list","search","exec"]'::text NOT NULL`, and `detected_facts text DEFAULT '{}'::text NOT NULL` at `0001_init.sql:802`, `0001_init.sql:806`, `0001_init.sql:790`; repository serializes at `workspace_repository.rs:48`, `workspace_repository.rs:103`, `workspace_repository.rs:107`, `workspace_repository.rs:201`, `workspace_repository.rs:205` and parses at `workspace_repository.rs:307`, `workspace_repository.rs:316`, `workspace_repository.rs:351-355`, `workspace_repository.rs:378-380`. Domain fields are `Value`, `Vec<MountCapability>`, and `Value` at `workspace/entity.rs:22`, `workspace/entity.rs:29`, `workspace/value_objects.rs:51`. | `identity_payload jsonb DEFAULT '{}'::jsonb NOT NULL`; `mount_capabilities jsonb DEFAULT '["read","write","list","search","exec"]'::jsonb NOT NULL`; `workspace_bindings.detected_facts jsonb DEFAULT '{}'::jsonb NOT NULL`; map with `Json<Value>` / `Json<Vec<MountCapability>>`. |
| `canvas_repository.rs` | `canvases.sandbox_config` | Schema stores `sandbox_config text DEFAULT '{}'::text NOT NULL` at `0001_init.sql:177`; repository serializes at `canvas_repository.rs:121` and `canvas_repository.rs:329`, row stores `String` at `canvas_repository.rs:394`, and parses at `canvas_repository.rs:416`, `canvas_repository.rs:457-459`. Domain `Canvas.sandbox_config` is typed at `canvas/entity.rs:18` and `CanvasSandboxConfig` is structured at `canvas/value_objects.rs:135`. | `jsonb DEFAULT '{}'::jsonb NOT NULL`; map with `Json<CanvasSandboxConfig>`. |
| `canvas_runtime_state_repository.rs` | `agent_run_canvas_runtime_observations.payload`, `agent_run_canvas_interaction_snapshots.payload` | Schema creates both payload columns as `text NOT NULL` at `0026_agent_run_canvas_runtime_state.sql:16` and `0026_agent_run_canvas_runtime_state.sql:37`; repository serializes full typed observations/snapshots at `canvas_runtime_state_repository.rs:39` and `canvas_runtime_state_repository.rs:98`, row stores `payload: String` at `canvas_runtime_state_repository.rs:154`, and parses at `canvas_runtime_state_repository.rs:163`, `canvas_runtime_state_repository.rs:167`. Domain types are `CanvasRuntimeObservation` and `CanvasInteractionSnapshot` at `canvas/runtime_state.rs:68` and `canvas/runtime_state.rs:98`. | Both `payload jsonb NOT NULL`; map with `Json<CanvasRuntimeObservation>` and `Json<CanvasInteractionSnapshot>`. Existing scalar columns already cover lookup/uniqueness. |
| `settings_repository.rs` | `settings.value` | Schema stores `value text NOT NULL` at `0001_init.sql:674`; repository API accepts `serde_json::Value` at `settings_repository.rs:80`, serializes at `settings_repository.rs:82`, row stores `String` at `settings_repository.rs:131`, and parses at `settings_repository.rs:139`. `0033_migrate_user_preferences_to_settings.sql:9` writes boolean values as JSON-valid scalar text. | `jsonb NOT NULL`; map with `Json<Value>`. This column may contain JSON scalars as well as objects/arrays, which is valid `jsonb`. |
| `llm_provider_repository.rs` | `llm_providers.models`, `llm_providers.blocked_models` | Schema stores both as `text DEFAULT '[]'::text NOT NULL` at `0001_init.sql:336-337`; repository parses at `llm_provider_repository.rs:98-99` and serializes at `llm_provider_repository.rs:113-115`, `llm_provider_repository.rs:174-176`. Domain fields are `serde_json::Value` at `llm_provider/entity.rs:186` and `llm_provider/entity.rs:189`. | `jsonb DEFAULT '[]'::jsonb NOT NULL`; map with `Json<Value>`. A later domain-typing task can narrow these to model-list value objects. |
| `mcp_preset_repository.rs` | `mcp_presets.transport`, `mcp_presets.runtime_binding` | Schema stores `transport text NOT NULL` at `0001_init.sql:358` and adds nullable `runtime_binding text` at `0012_mcp_preset_runtime_binding.sql:1-2`; repository serializes at `mcp_preset_repository.rs:37`, `mcp_preset_repository.rs:39`, `mcp_preset_repository.rs:102`, `mcp_preset_repository.rs:104`, `mcp_preset_repository.rs:202-209`, and parses at `mcp_preset_repository.rs:267-278`. Domain has `McpTransportConfig` and `Option<McpRuntimeBindingConfig>` at `mcp_preset/entity.rs:25`, `mcp_preset/entity.rs:29`; structured transport/binding types are defined at `mcp_preset/value_objects.rs:20-40` and `mcp_preset/value_objects.rs:57-98`. | `transport jsonb NOT NULL`, `runtime_binding jsonb NULL`; map with `Json<McpTransportConfig>` and `Option<Json<McpRuntimeBindingConfig>>`. |
| `routine_repository.rs` | `routines.trigger_config`, `routines.dispatch_strategy`, `routine_executions.trigger_payload` | Schema stores `trigger_config text NOT NULL`, `dispatch_strategy text NOT NULL`, and nullable `trigger_payload text` at `0001_init.sql:507-508`, `0001_init.sql:488`; repository parses at `routine_repository.rs:52-55`, `routine_repository.rs:260`, serializes at `routine_repository.rs:69-71`, `routine_repository.rs:136-139`, `routine_repository.rs:279`, `routine_repository.rs:331`, and already casts `trigger_config::jsonb` for queries at `routine_repository.rs:126`, `routine_repository.rs:174`. Domain has tagged documents at `routine/entity.rs:57-98` and JSON payload at `routine/entity.rs:170`. | `trigger_config jsonb NOT NULL`, `dispatch_strategy jsonb NOT NULL`, `trigger_payload jsonb NULL`; map with `Json<RoutineTriggerConfig>`, `Json<DispatchStrategy>`, `Option<Json<Value>>`. Consider a GIN or expression index for webhook endpoint lookup after conversion; do not keep TEXT plus `::jsonb` casts. |
| `project_vfs_mount_repository.rs` | `project_vfs_mounts.capabilities`, `project_vfs_mounts.installed_source`, `project_vfs_mounts.content` | Schema stores `capabilities text DEFAULT '[]'::text NOT NULL`, nullable `installed_source text`, and `content text NOT NULL` at `0001_init.sql:463-465`; repository parses at `project_vfs_mount_repository.rs:45-50` and serializes at `project_vfs_mount_repository.rs:71-81`, `project_vfs_mount_repository.rs:145-155`. Domain has `capabilities: Vec<MountCapability>`, `installed_source: Option<InstalledAssetSource>`, and tagged `ProjectVfsMountContent` at `project_vfs_mount/entity.rs:16-20`, with the content enum at `project_vfs_mount/entity.rs:83-88`. | `capabilities jsonb DEFAULT '[]'::jsonb NOT NULL`, `installed_source jsonb NULL`, `content jsonb NOT NULL`; map with `Json<Vec<MountCapability>>`, `Option<Json<InstalledAssetSource>>`, `Json<ProjectVfsMountContent>`. Despite the name, this `content` is not raw file text. |
| `project_backend_access_repository.rs` | `project_backend_access.root_policy`, `project_backend_access.capability_policy`, `backend_workspace_inventory.identity_payload`, `backend_workspace_inventory.detected_facts` | Schema stores backend access policies as TEXT JSON at `0001_init.sql:414-415`, with updated root default at `0019_decouple_workspace_inventory_from_runtime_health.sql:11-16`; backend inventory stores JSON at `0001_init.sql:125-126`. Repository serializes policies at `project_backend_access_repository.rs:35-39`, `project_backend_access_repository.rs:65-69`, serializes inventory facts at `project_backend_access_repository.rs:231-237`, parses policies at `project_backend_access_repository.rs:337-342`, parses inventory facts at `project_backend_access_repository.rs:368-377`, and uses shared `serialize_json` / `parse_json_col` at `project_backend_access_repository.rs:413-425`. Domain fields are `Value` at `backend/entity.rs:330-331` and `backend/entity.rs:404-405`. | `root_policy jsonb DEFAULT '{"kind":"workspace_registry"}'::jsonb NOT NULL`, `capability_policy jsonb DEFAULT '{}'::jsonb NOT NULL`, `identity_payload jsonb DEFAULT '{}'::jsonb NOT NULL`, `detected_facts jsonb DEFAULT '{}'::jsonb NOT NULL`; map with `Json<Value>` until policies are typed. |

### Keep TEXT / Raw

| Repository | Columns / sites | Reason |
| --- | --- | --- |
| `canvas_repository.rs` | `canvas_files.content text DEFAULT ''::text NOT NULL` at `0001_init.sql:167`; repository reads/writes file content at `canvas_repository.rs:38`, `canvas_repository.rs:74-78`, row field at `canvas_repository.rs:408`; domain `CanvasFile.content: String` at `canvas/value_objects.rs:170`. | Raw source/file text. Do not convert to `jsonb`. |
| `routine_repository.rs` | `routines.prompt_template text NOT NULL` at `0001_init.sql:505`, `routine_executions.resolved_prompt text` at `0001_init.sql:489`, and `routine_executions.error text` at `0001_init.sql:493`; repository binds prompt text at `routine_repository.rs:80`, `routine_repository.rs:147`, resolved prompt at `routine_repository.rs:290`, `routine_repository.rs:340`. | User-authored prompt/template and execution error text. Keep `TEXT`. |
| `project_backend_access_repository.rs` | `project_backend_access.note text` at `0001_init.sql:416`, `backend_workspace_inventory.last_error text` at `0001_init.sql:130`; repository maps note at `project_backend_access_repository.rs:54`, `project_backend_access_repository.rs:81`, `project_backend_access_repository.rs:343-345`, last error at `project_backend_access_repository.rs:259`, `project_backend_access_repository.rs:393-399`. | Human note and error message text. Keep `TEXT`. |
| `project_vfs_mount_repository.rs` | `project_vfs_mounts.description text` at `0001_init.sql:462`; repository maps description at `project_vfs_mount_repository.rs:27`, `project_vfs_mount_repository.rs:44`, `project_vfs_mount_repository.rs:70`, `project_vfs_mount_repository.rs:144`. | Human description text. Keep `TEXT`. |

### Scalar Enum / String

These sites use JSON serialization only to turn enum values into strings, or are scalar strings already parsed manually. They should not become `jsonb`.

| Repository | Sites | Target |
| --- | --- | --- |
| `backend_repository.rs` | `backend_type` is stored as TEXT in `0001_init.sql:141`; repository writes it by `serde_json::to_string(...).trim_matches('"')` at `backend_repository.rs:56`. | Keep scalar `text`; replace the serde_json string roundtrip with enum/string helper when touching this file. |
| `story_repository.rs` | `status`, `priority`, and `story_type` are TEXT at `0001_init.sql:726-728`; repository writes them via serde JSON strings at `story_repository.rs:39-41` and `story_repository.rs:222-224`, then parses manually at `story_repository.rs:255-293`. | Keep scalar `text`; optionally add as_str helpers. |
| `mcp_preset_repository.rs` | `route_policy text NOT NULL` at `0001_init.sql:359`; domain is scalar enum `McpRoutePolicy` at `mcp_preset/value_objects.rs:103-110`; repository currently stores JSON string at `mcp_preset_repository.rs:38`, `mcp_preset_repository.rs:103` and parses via `serde_json::from_str` at `mcp_preset_repository.rs:270-272`. | Normalize to scalar TEXT values (`auto`, `relay`, `direct`) in migration or repository cleanup; do not convert to `jsonb`. Existing rows likely contain JSON string literals such as `"direct"`, so migration should unwrap JSON strings. |
| `project_repository.rs` | `visibility`, grant `role`, and `subject_type` are scalar text parsed by helper functions at `project_repository.rs:275`, `project_repository.rs:319-346`. | Keep scalar `text`. |
| `workspace_repository.rs` | `identity_kind`, `resolution_policy`, workspace/binding `status` are scalar text parsed by helper functions around `workspace_repository.rs:315-321` and binding mapping around `workspace_repository.rs:349-356`. | Keep scalar `text`. |
| `project_backend_access_repository.rs` | access/inventory `status`, `access_mode`, `source`, and `identity_kind` are scalar text parsed at `project_backend_access_repository.rs:328-333`, `project_backend_access_repository.rs:378-387`, `project_backend_access_repository.rs:455-531`. | Keep scalar `text`; no JSONB migration. |

### Already JSONB / No TEXT JSON Action

- `backends.device` is already `jsonb DEFAULT '{}'::jsonb NOT NULL` at `0001_init.sql:146`; repository binds/reads `serde_json::Value` without string roundtrip at `backend_repository.rs:66`, `backend_repository.rs:80`, `backend_repository.rs:311`, `backend_repository.rs:335`. No migration target needed for this column in this slice.
- `project_extension_installations.config/manifest`, runtime health capability/device, extension package manifests, and shared library payloads are outside the requested repository list but provide local `Json<T>` examples.

### Defer / Out Of Scope For This Repository Inventory

- `state_changes.payload` is `text DEFAULT '{}'::text NOT NULL` at `0001_init.sql:706`, and `story_repository.rs:242-244` builds a `serde_json::Value` payload for `append_state_change_in_tx`; the repository owner is `state_change_store`, not `story_repository.rs`. Defer to the broader `state_change_store` inventory slice.
- `lifecycle_runs.*`, `agent_frames.*`, workflow graph JSON, mailbox source metadata, and session persistence JSON are named in task-level planning but are not in the product/config repository list supplied for this research worker. They should be covered by separate repository inventories.
- `canvas_bindings` appears in `0001_init.sql:157-162` but is dropped by `0029_drop_canvas_bindings.sql:1`; no live migration target.

### Migration Target Notes

- Non-null object/array TEXT JSON columns should drop the text default, convert with `USING column::jsonb`, then set the matching jsonb default (`'{}'::jsonb` or `'[]'::jsonb`) and keep `NOT NULL`.
- Nullable TEXT JSON columns should convert with a nullable-safe expression: empty string should become `NULL` only if current code can produce or tolerate it; otherwise prefer strict `column::jsonb` to expose bad data.
- `settings.value` should use `value::jsonb`; JSON scalar text such as `true` from `0033_migrate_user_preferences_to_settings.sql:9` remains valid.
- `mcp_presets.route_policy` should be treated as scalar text, not JSONB. A forward migration should normalize JSON string values to bare scalar values before repository reads stop calling `serde_json::from_str`.
- `routines.trigger_config` already uses `trigger_config::jsonb @> $1::jsonb` at `routine_repository.rs:126` and `routine_repository.rs:174`; after type conversion those casts should be removed and an index decision should be made for endpoint/type lookup.

### Recommended Implementation Slices

1. **Core product config JSONB**: convert `projects.config`, `project_agents.config`, `settings.value`, `llm_providers.models`, `llm_providers.blocked_models`; replace their repository helpers with `Json<T>`.
2. **MCP and routine configs**: convert `mcp_presets.transport/runtime_binding`, normalize scalar `mcp_presets.route_policy`, convert `routines.trigger_config/dispatch_strategy` and `routine_executions.trigger_payload`; remove `trigger_config::jsonb` casts and decide endpoint/type indexing.
3. **Workspace/backend inventory documents**: convert `workspaces.identity_payload/mount_capabilities`, `workspace_bindings.detected_facts`, `project_backend_access.root_policy/capability_policy`, and `backend_workspace_inventory.identity_payload/detected_facts`.
4. **Story/canvas/VFS documents**: convert `stories.tags/context`, `canvases.sandbox_config`, canvas runtime payloads, and `project_vfs_mounts.capabilities/installed_source/content`; explicitly leave `canvas_files.content` and prompt/error text as `TEXT`.
5. **Post-cleanup grep pass**: rerun `rg -n "parse_json_column|serialize_json_column|serde_json::from_str|serde_json::to_string" crates/agentdash-infrastructure/src/persistence/postgres` and classify any remaining hits as scalar enum/string, raw text, defer, or test-only.

### Related Specs

- `.trellis/spec/backend/database-guidelines.md` - JSONB document column naming and typed repository mapping contracts.
- `.trellis/spec/backend/repository-pattern.md` - schema ownership and repository transaction boundaries.

### External References

- No web or third-party documentation was needed. The required target mapping pattern is already present in local `sqlx::types::Json<T>` repository implementations listed under Code Patterns.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task; the user-supplied task path was used as the output root.
- This research only covers the repository files explicitly named in the prompt. It does not claim the global TEXT JSON inventory is complete.
- No code or migration files were edited.
