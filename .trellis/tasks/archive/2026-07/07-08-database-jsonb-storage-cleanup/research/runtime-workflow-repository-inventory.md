# Research: runtime/workflow repository inventory

- Query: Inspect runtime/workflow PostgreSQL repositories for `serde_json::from_str` / `serde_json::to_string` / JSON parse-serialize helpers tied to PostgreSQL `TEXT` JSON columns; classify each as `convert_to_jsonb_now`, `keep_text/raw`, `scalar enum/string`, or `defer`; include owner, row fields, bind/read sites, tests, and suggested `sqlx::types::Json<T>` shape.
- Scope: internal
- Date: 2026-07-08

## Findings

### Files Found

- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/prd.md` - task goal: live structured JSON-in-TEXT columns should converge to `jsonb` by default, with raw text/scalar/defer exceptions documented.
- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/design.md` - design rule: repository semantics, not column suffixes, decide `jsonb` vs `json` vs scalar vs raw `TEXT`.
- `.trellis/tasks/07-08-database-jsonb-storage-cleanup/implement.md` - execution plan: inventory first, migration design second, repository conversion third, tests and spec cleanup after.
- `.trellis/spec/backend/database-guidelines.md` - current contract: new structured document columns use `jsonb`, repository uses typed value objects and `sqlx::types::Json<T>` or narrow shared codec; deserialization failures include `table.column`.
- `.trellis/spec/backend/repository-pattern.md` - repository contract: schema is owned by migrations; repositories persist aggregate facts and do not create/migrate schema.
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` - workflow procedure/graph/run repository with most legacy TEXT JSON roundtrips and existing `channel_registry` jsonb path.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - mixed repository file for lifecycle agents, agent frames, subject associations, gates, agent lineages, and runtime session execution anchors.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs` - agent-run fork lineage repository plus fork materialization transaction writer.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs` - mailbox repository; mostly jsonb already, with one legacy `source_metadata` TEXT JSON column.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` - PostgreSQL adapter for session SPI stores; writes JSON strings through `session_core::json_string`.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` - shared session row mappers and JSON parse/serialize helpers used by `session_repository.rs`.
- `crates/agentdash-infrastructure/src/persistence/postgres/state_change_store.rs` - state-change table writer/reader with `payload` as TEXT JSON.
- `crates/agentdash-infrastructure/src/persistence/postgres/state_change_repository.rs` - thin repository wrapper around `state_change_store`.
- `crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs` - internal reference for `sqlx::types::Json` bind/read shape.
- `crates/agentdash-infrastructure/src/persistence/postgres/runtime_health_repository.rs` - internal reference for `sqlx::types::Json<&T>` bind and `Json<Value>` row fields.

### Internal JSONB Mapping References

- `shared_library_repository.rs` imports `sqlx::types::Json` and binds a JSONB payload with `.bind(Json(asset.payload.clone()))`; its row struct reads `payload: Json<serde_json::Value>` (`shared_library_repository.rs:2`, `shared_library_repository.rs:47`, `shared_library_repository.rs:184`).
- `runtime_health_repository.rs` binds references with `sqlx::types::Json(&update.capabilities)` / `Json(&update.device)` and reads `capabilities: Json<Value>`, `device: Json<Value>` (`runtime_health_repository.rs:63`, `runtime_health_repository.rs:64`, `runtime_health_repository.rs:208`, `runtime_health_repository.rs:209`).
- `owner_document.rs` is an existing jsonb owner-document helper, but it currently reads/writes `serde_json::Value` and `from_value` / `to_value` rather than direct `Json<T>` row fields (`owner_document.rs:23`, `owner_document.rs:35`, `owner_document.rs:38`, `owner_document.rs:41`).

### Workflow Repository

Owner: `PostgresWorkflowRepository`, implementing `AgentProcedureRepository`, `WorkflowGraphRepository`, `WorkflowTemplateInstallRepository`, and `LifecycleRunRepository`.

Relevant schema:

- `agent_procedures.source text`, `agent_procedures.contract text` (`0001_init.sql:74`, `0001_init.sql:76`).
- `workflow_graphs.source text`, `workflow_graphs.activities text DEFAULT '[]'::text NOT NULL`, `workflow_graphs.transitions text DEFAULT '[]'::text NOT NULL` (`0001_init.sql:769`, `0001_init.sql:780`, `0001_init.sql:781`).
- `lifecycle_runs.status text`, `lifecycle_runs.execution_log text DEFAULT '[]'::text NOT NULL`; `lifecycle_runs.orchestrations text DEFAULT '[]'::text NOT NULL`; `lifecycle_runs.tasks text DEFAULT '[]'::text NOT NULL`; deleted columns `context` / `view_projection` should not be inventoried as live columns (`0001_init.sql:287`, `0001_init.sql:288`, `0003_lifecycle_orchestration_contract.sql:2`, `0003_lifecycle_orchestration_contract.sql:3`, `0015_lifecycle_run_tasks_story_task_cleanup.sql:2`, `0041_drop_lifecycle_run_context_view_projection.sql:1`).
- `lifecycle_runs.channel_registry jsonb NOT NULL DEFAULT '{}'::jsonb` is already jsonb and not a TEXT JSON conversion candidate (`0057_lifecycle_run_channel_registry.sql:2`).

Code patterns:

- Procedure writes serialize `source` and `contract` on create/update/install (`workflow_repository.rs:44`, `workflow_repository.rs:45`, `workflow_repository.rs:129`, `workflow_repository.rs:130`, `workflow_repository.rs:309`, `workflow_repository.rs:311`, `workflow_repository.rs:329`, `workflow_repository.rs:331`).
- Procedure row fields are `source: String`, `contract: String`, then read with `serde_json::from_str` (`workflow_repository.rs:590`, `workflow_repository.rs:592`, `workflow_repository.rs:611`, `workflow_repository.rs:620`).
- Graph writes serialize `source`, `activities`, `transitions` in direct create/update and template install paths (`workflow_repository.rs:162`, `workflow_repository.rs:165`, `workflow_repository.rs:166`, `workflow_repository.rs:231`, `workflow_repository.rs:234`, `workflow_repository.rs:235`, `workflow_repository.rs:372`, `workflow_repository.rs:375`, `workflow_repository.rs:376`, `workflow_repository.rs:396`, `workflow_repository.rs:399`, `workflow_repository.rs:400`).
- Graph row fields are `source: String`, `activities: String`, `transitions: String`; `source` is parsed with `from_str`, activities/transitions through `parse_json_column` (`workflow_repository.rs:634`, `workflow_repository.rs:637`, `workflow_repository.rs:638`, `workflow_repository.rs:657`, `workflow_repository.rs:667`, `workflow_repository.rs:668`).
- Lifecycle run writes serialize `orchestrations`, `tasks`, `status`, `execution_log`; `channel_registry` uses `serde_json::to_value` because it is already jsonb (`workflow_repository.rs:431`, `workflow_repository.rs:432`, `workflow_repository.rs:433`, `workflow_repository.rs:434`, `workflow_repository.rs:435`, `workflow_repository.rs:504`, `workflow_repository.rs:505`, `workflow_repository.rs:506`, `workflow_repository.rs:507`).
- Lifecycle run row fields are `orchestrations: String`, `tasks: String`, `status: String`, `execution_log: String`, `channel_registry: Value`; run mapper parses arrays/docs via `parse_json_column`, status via `serde_json::from_str`, and channel registry via `from_value` (`workflow_repository.rs:681`, `workflow_repository.rs:682`, `workflow_repository.rs:683`, `workflow_repository.rs:684`, `workflow_repository.rs:685`, `workflow_repository.rs:699`, `workflow_repository.rs:703`, `workflow_repository.rs:707`, `workflow_repository.rs:708`, `workflow_repository.rs:709`).
- Local helper `parse_json_column<T>` is exactly the old TEXT JSON parser for graph/run array/document columns (`workflow_repository.rs:744`, `workflow_repository.rs:748`).

Classification and target shape:

| Column | Current row field | Classification | Suggested shape |
| --- | --- | --- | --- |
| `agent_procedures.source` | `String` | `scalar enum/string` | keep `text`; replace JSON string protocol with typed scalar mapping such as `DefinitionSource::as_str()` / `try_from`. No `Json<T>`. |
| `agent_procedures.contract` | `String` | `convert_to_jsonb_now` | `contract: Json<AgentProcedureContract>`; bind `Json(&procedure.contract)` or `Json(procedure.contract.clone())`; domain gets `.0`. |
| `workflow_graphs.source` | `String` | `scalar enum/string` | keep `text`; typed scalar mapping. No `Json<T>`. |
| `workflow_graphs.activities` | `String` | `convert_to_jsonb_now` | `activities: Json<Vec<ActivityDefinition>>`; bind `Json(&lifecycle.activities)`. |
| `workflow_graphs.transitions` | `String` | `convert_to_jsonb_now` | `transitions: Json<Vec<ActivityTransition>>`; bind `Json(&lifecycle.transitions)`. |
| `lifecycle_runs.orchestrations` | `String` | `convert_to_jsonb_now` | `orchestrations: Json<Vec<OrchestrationInstance>>`; default `'[]'::jsonb`. |
| `lifecycle_runs.tasks` | `String` | `convert_to_jsonb_now` | `tasks: Json<Vec<LifecycleTaskPlanItem>>`; default `'[]'::jsonb`. |
| `lifecycle_runs.status` | `String` | `scalar enum/string` | keep `text`; store scalar status (`ready`, etc.) instead of JSON-quoted enum. No `Json<T>`. |
| `lifecycle_runs.execution_log` | `String` | `convert_to_jsonb_now` | `execution_log: Json<Vec<LifecycleExecutionEntry>>`; default `'[]'::jsonb`. |
| `lifecycle_runs.channel_registry` | `Value` | already jsonb / no TEXT action | Optional follow-up to row `Json<ChannelRegistryDocument>`; not part of TEXT JSON cleanup. |

Tests likely affected:

- `workflow_repository_lifecycle_run_row_parses_empty_orchestration_contract` constructs row strings and should move to `Json` row values (`workflow_repository.rs:829`, `workflow_repository.rs:836`, `workflow_repository.rs:837`, `workflow_repository.rs:839`).
- `workflow_repository_lifecycle_run_row_reports_bad_orchestration_column` and `workflow_repository_lifecycle_run_row_reports_bad_tasks_column` currently inject `"not-json"` into string row fields; shape-mismatch tests need either SQL fixture rows or `Json<T>` decode error coverage (`workflow_repository.rs:856`, `workflow_repository.rs:859`, `workflow_repository.rs:868`, `workflow_repository.rs:871`).
- Template install and lifecycle orchestration roundtrip tests exercise procedure/graph/run JSON columns (`workflow_repository.rs:1015`, `workflow_repository.rs:1099`).
- Channel registry tests should remain mostly unchanged but protect broad aggregate update preserving separate jsonb document (`workflow_repository.rs:1166`, `workflow_repository.rs:1212`).

### Lifecycle Anchor Repository

Owner file contains several repository owners. `PostgresRuntimeSessionExecutionAnchorRepository` has no JSON TEXT helper and can be left out of conversion.

#### AgentFrameRepository

Relevant schema:

- Initial split fields are `agent_frames.effective_capability_json`, `context_slice_json`, `vfs_surface_json`, `mcp_surface_json`, `execution_profile_json`, `visible_canvas_mount_ids_json` as `text` (`0001_init.sql:47`, `0001_init.sql:48`, `0001_init.sql:49`, `0001_init.sql:50`, `0001_init.sql:54`, `0001_init.sql:55`).
- `visible_workspace_module_refs_json` was added as `text` (`0008_agent_frame_visible_workspace_modules.sql:4`).
- `surface text` was added later and backfilled from split columns using `::jsonb`, confirming structured JSON semantics (`0049_agent_frame_surface_document.sql:2`, `0049_agent_frame_surface_document.sql:5`, `0049_agent_frame_surface_document.sql:10`, `0049_agent_frame_surface_document.sql:15`, `0049_agent_frame_surface_document.sql:20`).

Code patterns:

- `FrameRow` stores `surface` and all split projection columns as `Option<String>` (`lifecycle_anchor_repository.rs:181`, `lifecycle_anchor_repository.rs:185`, `lifecycle_anchor_repository.rs:186`, `lifecycle_anchor_repository.rs:187`, `lifecycle_anchor_repository.rs:188`, `lifecycle_anchor_repository.rs:189`, `lifecycle_anchor_repository.rs:190`, `lifecycle_anchor_repository.rs:191`, `lifecycle_anchor_repository.rs:192`).
- `parse_opt_json` parses optional `Value`; `parse_opt_surface` parses optional `AgentFrameSurfaceDocument`; mapper then applies surface projection (`lifecycle_anchor_repository.rs:198`, `lifecycle_anchor_repository.rs:200`, `lifecycle_anchor_repository.rs:207`, `lifecycle_anchor_repository.rs:212`, `lifecycle_anchor_repository.rs:226`, `lifecycle_anchor_repository.rs:227`, `lifecycle_anchor_repository.rs:231`, `lifecycle_anchor_repository.rs:232`, `lifecycle_anchor_repository.rs:233`, `lifecycle_anchor_repository.rs:234`, `lifecycle_anchor_repository.rs:238`, `lifecycle_anchor_repository.rs:242`, `lifecycle_anchor_repository.rs:250`).
- `opt_json_str` and `surface_json_str` serialize split projections and canonical surface; insert binds them as strings (`lifecycle_anchor_repository.rs:255`, `lifecycle_anchor_repository.rs:257`, `lifecycle_anchor_repository.rs:264`, `lifecycle_anchor_repository.rs:265`, `lifecycle_anchor_repository.rs:287`, `lifecycle_anchor_repository.rs:288`, `lifecycle_anchor_repository.rs:289`, `lifecycle_anchor_repository.rs:290`, `lifecycle_anchor_repository.rs:291`, `lifecycle_anchor_repository.rs:292`, `lifecycle_anchor_repository.rs:293`, `lifecycle_anchor_repository.rs:294`).

Classification and target shape:

| Column | Current row field | Classification | Suggested shape |
| --- | --- | --- | --- |
| `agent_frames.surface` | `Option<String>` | `convert_to_jsonb_now` | `surface: Option<Json<AgentFrameSurfaceDocument>>`; bind `Some(Json(frame.surface_document()))` or a helper for nullable document. |
| `agent_frames.effective_capability_json` | `Option<String>` | `convert_to_jsonb_now` | `Option<Json<Value>>`; still projection, derived from surface. |
| `agent_frames.context_slice_json` | `Option<String>` | `convert_to_jsonb_now` | `Option<Json<Value>>`. |
| `agent_frames.vfs_surface_json` | `Option<String>` | `convert_to_jsonb_now` | `Option<Json<Value>>`. |
| `agent_frames.mcp_surface_json` | `Option<String>` | `convert_to_jsonb_now` | `Option<Json<Value>>`. |
| `agent_frames.execution_profile_json` | `Option<String>` | `convert_to_jsonb_now` | `Option<Json<Value>>`. |
| `agent_frames.visible_canvas_mount_ids_json` | `Option<String>` | `convert_to_jsonb_now` | `Option<Json<Value>>` or a typed `Vec` if domain narrows it in the same slice. |
| `agent_frames.visible_workspace_module_refs_json` | `Option<String>` | `convert_to_jsonb_now` | `Option<Json<Value>>` or a typed `Vec` if domain narrows it in the same slice. |

Tests likely affected:

- `frame_row_projects_surface_document_as_canonical_source` and `frame_row_without_surface_derives_document_from_split_projection` build stringified JSON row fixtures and should move to `Json` fixtures or database roundtrip tests (`lifecycle_anchor_repository.rs:1378`, `lifecycle_anchor_repository.rs:1385`, `lifecycle_anchor_repository.rs:1386`, `lifecycle_anchor_repository.rs:1387`, `lifecycle_anchor_repository.rs:1405`, `lifecycle_anchor_repository.rs:1408`, `lifecycle_anchor_repository.rs:1409`).
- `agent_run_lineage_repository.rs` also inserts `agent_frames` through fork materialization and must be updated in the same AgentFrame schema slice (`agent_run_lineage_repository.rs:441`, `agent_run_lineage_repository.rs:456`, `agent_run_lineage_repository.rs:457`, `agent_run_lineage_repository.rs:458`, `agent_run_lineage_repository.rs:459`, `agent_run_lineage_repository.rs:460`, `agent_run_lineage_repository.rs:461`, `agent_run_lineage_repository.rs:462`, `agent_run_lineage_repository.rs:463`).

#### LifecycleSubjectAssociationRepository

Relevant schema:

- `lifecycle_subject_associations.metadata_json text` (`0001_init.sql:294`, `0001_init.sql:301`).

Code patterns:

- `AssocRow.metadata_json: Option<String>` is parsed with `serde_json::from_str`; create serializes `assoc.metadata_json` with `serde_json::to_string` (`lifecycle_anchor_repository.rs:385`, `lifecycle_anchor_repository.rs:405`, `lifecycle_anchor_repository.rs:407`, `lifecycle_anchor_repository.rs:418`, `lifecycle_anchor_repository.rs:421`, `lifecycle_anchor_repository.rs:435`).

Classification and target shape:

| Column | Current row field | Classification | Suggested shape |
| --- | --- | --- | --- |
| `lifecycle_subject_associations.metadata_json` | `Option<String>` | `convert_to_jsonb_now` | `metadata_json: Option<Json<Value>>`; bind nullable `Json(&assoc.metadata_json_value)`. Consider column rename later; type conversion can happen now. |

Tests likely affected:

- No focused repository test found in this file for subject association metadata. Application query paths use association metadata for labels; add a repository roundtrip test for non-empty/null metadata and shape mismatch.

#### LifecycleGateRepository

Relevant schema:

- `lifecycle_gates.payload_json text` (`0001_init.sql:268`, `0001_init.sql:276`).

Code patterns:

- `GateRow.payload_json: Option<String>` is parsed with `serde_json::from_str` (`lifecycle_anchor_repository.rs:525`, `lifecycle_anchor_repository.rs:542`, `lifecycle_anchor_repository.rs:544`).
- Create/update serialize `payload_json` with `serde_json::to_string` (`lifecycle_anchor_repository.rs:557`, `lifecycle_anchor_repository.rs:560`, `lifecycle_anchor_repository.rs:575`, `lifecycle_anchor_repository.rs:715`, `lifecycle_anchor_repository.rs:718`, `lifecycle_anchor_repository.rs:725`).
- Query paths already cast `payload_json::jsonb` for wait-policy discovery and producer filtering; converting the column removes repeated casts and makes operators native (`lifecycle_anchor_repository.rs:620`, `lifecycle_anchor_repository.rs:626`, `lifecycle_anchor_repository.rs:627`, `lifecycle_anchor_repository.rs:628`, `lifecycle_anchor_repository.rs:629`, `lifecycle_anchor_repository.rs:630`, `lifecycle_anchor_repository.rs:631`, `lifecycle_anchor_repository.rs:661`, `lifecycle_anchor_repository.rs:666`, `lifecycle_anchor_repository.rs:667`, `lifecycle_anchor_repository.rs:668`, `lifecycle_anchor_repository.rs:669`, `lifecycle_anchor_repository.rs:672`).

Classification and target shape:

| Column | Current row field | Classification | Suggested shape |
| --- | --- | --- | --- |
| `lifecycle_gates.payload_json` | `Option<String>` | `convert_to_jsonb_now` | `payload_json: Option<Json<Value>>`; bind nullable `Json<Value>`. Query SQL should use `payload_json -> ...` instead of `payload_json::jsonb -> ...`. |

Tests likely affected:

- Gate delivery marker tests seed and mutate `LifecycleGate` payload JSON and should cover jsonb query predicates after conversion (`lifecycle_anchor_repository.rs:1420`, `lifecycle_anchor_repository.rs:1447`, `lifecycle_anchor_repository.rs:1453`, `lifecycle_anchor_repository.rs:1525`).
- Add a focused shape-mismatch test for `lifecycle_gates.payload_json` because current errors only say `payload_json`, not `lifecycle_gates.payload_json`.

#### AgentLineageRepository

Relevant schema:

- `agent_lineages.metadata_json text` (`0001_init.sql:58`, `0001_init.sql:65`).

Code patterns:

- `LineageRow.metadata_json: Option<String>` is parsed with `serde_json::from_str`; create serializes metadata with `serde_json::to_string` (`lifecycle_anchor_repository.rs:1065`, `lifecycle_anchor_repository.rs:1085`, `lifecycle_anchor_repository.rs:1087`, `lifecycle_anchor_repository.rs:1098`, `lifecycle_anchor_repository.rs:1101`, `lifecycle_anchor_repository.rs:1115`).

Classification and target shape:

| Column | Current row field | Classification | Suggested shape |
| --- | --- | --- | --- |
| `agent_lineages.metadata_json` | `Option<String>` | `convert_to_jsonb_now` | `metadata_json: Option<Json<Value>>`; nullable jsonb. |

Tests likely affected:

- No focused PostgreSQL test found for this metadata field. Add a repository roundtrip test or include in existing lineage test coverage.

### Agent Run Lineage Repository

Owners: `PostgresAgentRunLineageRepository` for `agent_run_lineages`, plus `PostgresAgentRunForkMaterialization` which writes `agent_run_lineages`, child `lifecycle_runs`, child `lifecycle_agents`, child `agent_frames`, delivery bindings, and execution anchors inside one transaction.

Relevant schema:

- `agent_run_lineages.fork_point_ref text`, `agent_run_lineages.metadata text`; runtime-session refs were later dropped, baseline frame refs added, but these JSON columns remain text (`0038_agent_run_lineages.sql:1`, `0038_agent_run_lineages.sql:9`, `0038_agent_run_lineages.sql:13`, `0046_agent_run_lineage_product_refs.sql:4`, `0048_agent_run_lineage_baseline_refs.sql:1`).

Code patterns:

- `insert_agent_run_lineage` and `insert_agent_run_lineage_tx` bind `fork_point_ref_json` and `metadata_json` through `opt_json_str` (`agent_run_lineage_repository.rs:281`, `agent_run_lineage_repository.rs:297`, `agent_run_lineage_repository.rs:299`, `agent_run_lineage_repository.rs:307`, `agent_run_lineage_repository.rs:323`, `agent_run_lineage_repository.rs:325`).
- Insert SQL writes `fork_point_ref` and `metadata` (`agent_run_lineage_repository.rs:333`, `agent_run_lineage_repository.rs:337`).
- `AgentRunLineageRow` fields are `fork_point_ref: Option<String>`, `metadata: Option<String>` and mapper parses both through `parse_optional_json` (`agent_run_lineage_repository.rs:576`, `agent_run_lineage_repository.rs:588`, `agent_run_lineage_repository.rs:590`, `agent_run_lineage_repository.rs:619`, `agent_run_lineage_repository.rs:621`, `agent_run_lineage_repository.rs:650`, `agent_run_lineage_repository.rs:652`).
- Fork materialization also writes child `lifecycle_runs` with old string JSON columns (`agent_run_lineage_repository.rs:341`, `agent_run_lineage_repository.rs:358`, `agent_run_lineage_repository.rs:359`, `agent_run_lineage_repository.rs:360`, `agent_run_lineage_repository.rs:361`) and child `agent_frames` with old string JSON columns (`agent_run_lineage_repository.rs:441`, `agent_run_lineage_repository.rs:456`, `agent_run_lineage_repository.rs:457`, `agent_run_lineage_repository.rs:458`, `agent_run_lineage_repository.rs:459`, `agent_run_lineage_repository.rs:460`, `agent_run_lineage_repository.rs:461`, `agent_run_lineage_repository.rs:462`, `agent_run_lineage_repository.rs:463`).

Classification and target shape:

| Column | Current row field | Classification | Suggested shape |
| --- | --- | --- | --- |
| `agent_run_lineages.fork_point_ref` | `Option<String>` | `convert_to_jsonb_now` | `fork_point_ref: Option<Json<Value>>`; semantic column name can remain because it is a structured ref document, not just storage suffix. |
| `agent_run_lineages.metadata` | `Option<String>` | `convert_to_jsonb_now` | `metadata: Option<Json<Value>>`; semantic name can remain. |
| child `lifecycle_runs.orchestrations/tasks/execution_log` in fork materialization | string bind helper | `convert_to_jsonb_now` with workflow slice | Use same `Json<Vec<...>>` as `PostgresWorkflowRepository`; status remains scalar enum/string. |
| child `agent_frames.surface` and split projection columns in fork materialization | string bind helper | `convert_to_jsonb_now` with AgentFrame slice | Use same `Json<AgentFrameSurfaceDocument>` / `Json<Value>` shapes as `PostgresAgentFrameRepository`. |

Tests likely affected:

- `agent_run_lineage_row_maps_json_and_refs` constructs stringified JSON row values and should move to `Json` or DB roundtrip coverage (`agent_run_lineage_repository.rs:688`, `agent_run_lineage_repository.rs:710`, `agent_run_lineage_repository.rs:712`).
- Application fork tests exercise materialization inputs, but no PostgreSQL materialization integration test was found in this file. Add a Postgres materialization roundtrip or extend existing infra tests so child run/frame JSONB inserts are covered.

### Agent Run Mailbox Repository

Owner: `PostgresAgentRunMailboxRepository`.

Relevant schema:

- `agent_run_mailbox_messages.delivery_json jsonb`, `payload_json jsonb`, `executor_config_json jsonb` are already jsonb (`0013_agent_run_mailbox.sql:67`, `0013_agent_run_mailbox.sql:83`, `0013_agent_run_mailbox.sql:84`).
- `agent_run_mailbox_messages.source_metadata text` is the legacy TEXT JSON column added with source identity (`0032_agent_run_mailbox_source_identity.sql:1`, `0032_agent_run_mailbox_source_identity.sql:9`).
- `agent_run_mailbox_messages.launch_planning_input jsonb` and `agent_run_mailbox_states.backend_selection_preference jsonb` are already jsonb (`0035_agent_run_mailbox_backend_selection.sql:2`, `0035_agent_run_mailbox_backend_selection.sql:5`).

Code patterns:

- Create path serializes only `source_metadata` through `serialize_json_column`; `delivery_json`, `payload_json`, `executor_config_json`, and `launch_planning_input` bind `Value` directly (`agent_run_mailbox_repository.rs:115`, `agent_run_mailbox_repository.rs:141`, `agent_run_mailbox_repository.rs:143`, `agent_run_mailbox_repository.rs:153`, `agent_run_mailbox_repository.rs:154`, `agent_run_mailbox_repository.rs:155`).
- Row field `source_metadata: Option<String>` is parsed through `parse_json_column`; other JSONB fields are already `Value` / `Option<Value>` (`agent_run_mailbox_repository.rs:693`, `agent_run_mailbox_repository.rs:695`, `agent_run_mailbox_repository.rs:711`, `agent_run_mailbox_repository.rs:712`, `agent_run_mailbox_repository.rs:713`, `agent_run_mailbox_repository.rs:743`).
- `AgentRunMailboxStateRow.backend_selection_preference: Option<Value>` is already jsonb; setter binds `preference: Value` directly (`agent_run_mailbox_repository.rs:796`, `agent_run_mailbox_repository.rs:524`, `agent_run_mailbox_repository.rs:545`).
- Local `serialize_json_column` / `parse_json_column` are only for `source_metadata` (`agent_run_mailbox_repository.rs:817`, `agent_run_mailbox_repository.rs:823`, `agent_run_mailbox_repository.rs:830`, `agent_run_mailbox_repository.rs:835`).

Classification and target shape:

| Column | Current row field | Classification | Suggested shape |
| --- | --- | --- | --- |
| `agent_run_mailbox_messages.source_metadata` | `Option<String>` | `convert_to_jsonb_now` | `source_metadata: Option<Json<Value>>`; bind nullable `Json<Value>`. |
| `agent_run_mailbox_messages.delivery_json` | `Value` | already jsonb / no TEXT action | Optional consistency follow-up: row `Json<Value>` or typed `Json<MailboxDeliveryDocument>` if domain has one. |
| `agent_run_mailbox_messages.payload_json` | `Option<Value>` | already jsonb / no TEXT action | Optional row `Option<Json<Value>>`. |
| `agent_run_mailbox_messages.executor_config_json` | `Option<Value>` | already jsonb / no TEXT action | Optional row `Option<Json<Value>>`. |
| `agent_run_mailbox_messages.launch_planning_input` | `Option<Value>` | already jsonb / no TEXT action | Optional row `Option<Json<Value>>`. |
| `agent_run_mailbox_states.backend_selection_preference` | `Option<Value>` | already jsonb / no TEXT action | Optional row `Option<Json<Value>>`. |

Tests likely affected:

- `source_identity_roundtrips_through_message_rows` is the focused test for `source_metadata` roundtrip (`agent_run_mailbox_repository.rs:920`, `agent_run_mailbox_repository.rs:938`, `agent_run_mailbox_repository.rs:952`, `agent_run_mailbox_repository.rs:963`, `agent_run_mailbox_repository.rs:979`).
- Payload cleanup and claim tests should remain behaviorally stable but may need row type adjustment because `MAILBOX_COLS` includes JSONB fields (`agent_run_mailbox_repository.rs:401`, `agent_run_mailbox_repository.rs:982`).

### Session Repository

Owner: `PostgresSessionRepository`, implementing several session SPI stores. JSON parse/serialize helpers live in `crates/agentdash-infrastructure/src/persistence/session_core.rs`, but the bind/read sites are in `postgres/session_repository.rs`.

Relevant schema:

- `agent_frame_transitions.capability_keys_json text`, `transition_json text` (`0001_init.sql:28`, `0001_init.sql:34`, `0001_init.sql:35`).
- Runtime session tables were renamed from `session_*` to `runtime_session_*` in migration 0045, but the JSON column types remain from the original definitions (`0045_runtime_session_trace_table_names.sql:9`, `0045_runtime_session_trace_table_names.sql:12`, `0045_runtime_session_trace_table_names.sql:15`, `0045_runtime_session_trace_table_names.sql:18`, `0045_runtime_session_trace_table_names.sql:27`, `0045_runtime_session_trace_table_names.sql:30`, `0045_runtime_session_trace_table_names.sql:39`, `0045_runtime_session_trace_table_names.sql:42`, `0045_runtime_session_trace_table_names.sql:45`, `0045_runtime_session_trace_table_names.sql:48`).
- `runtime_session_compactions.replacement_projection_json`, `token_stats_json`, `diagnostics_json` are TEXT defaults (`0001_init.sql:567`, `0001_init.sql:568`, `0001_init.sql:569`).
- `runtime_session_events.notification_json text NOT NULL` (`0001_init.sql:575`, `0001_init.sql:584`).
- `runtime_session_lineage.fork_point_ref_json`, `metadata_json` are TEXT defaults (`0001_init.sql:587`, `0001_init.sql:592`, `0001_init.sql:597`).
- `runtime_session_projection_segments.source_refs_json`, `content_json` are TEXT JSON columns (`0001_init.sql:611`, `0001_init.sql:622`, `0001_init.sql:624`).
- `runtime_session_delivery_commands.payload_json text NOT NULL` (`0001_init.sql:629`, `0001_init.sql:634`).
- `agent_run_control_effects` was renamed from terminal effects in migration 0053; inherited `payload_json text NOT NULL` remains a structured JSON payload (`0001_init.sql:643`, `0001_init.sql:649`, `0053_agent_run_control_effects.sql:4`, `0053_agent_run_control_effects.sql:6`).

Code patterns:

- `json_string<T>` serializes all session JSON writes to `String` (`session_core.rs:444`, `session_core.rs:448`).
- `parse_json_column` parses generic `serde_json::Value` from a `String` (`session_core.rs:551`, `session_core.rs:555`).
- Event writes serialize `BackboneEnvelope` into `notification_json` in both append-event paths; read paths select `notification_json` and `persisted_event_from_row` parses `BackboneEnvelope` (`session_repository.rs:354`, `session_repository.rs:368`, `session_repository.rs:416`, `session_repository.rs:431`, `session_repository.rs:454`, `session_repository.rs:471`, `session_repository.rs:492`, `session_repository.rs:505`, `session_repository.rs:519`, `session_repository.rs:533`, `session_repository.rs:1170`, `session_repository.rs:1183`, `session_core.rs:66`, `session_core.rs:67`).
- Control effects serialize `record.payload` to `agent_run_control_effects.payload_json`; mapper parses `Value` (`session_repository.rs:570`, `session_repository.rs:599`, `session_core.rs:122`, `session_core.rs:123`).
- Runtime delivery command writes `agent_frame_transitions.capability_keys_json` and `transition_json`; mapper parses `BTreeSet<String>` and `RuntimeCapabilityTransition` (`session_repository.rs:814`, `session_repository.rs:818`, `session_repository.rs:844`, `session_repository.rs:845`, `session_repository.rs:909`, `session_repository.rs:910`, `session_core.rs:221`, `session_core.rs:222`, `session_core.rs:230`, `session_core.rs:231`).
- Runtime delivery command writes `runtime_session_delivery_commands.payload_json` as `RuntimeDeliveryCommand`; mapper parses it and checks consistency with joined frame transition (`session_repository.rs:866`, `session_repository.rs:883`, `session_repository.rs:902`, `session_repository.rs:956`, `session_core.rs:170`, `session_core.rs:171`, `session_core.rs:173`, `session_core.rs:175`).
- Compaction rows write and read `replacement_projection_json`, `token_stats_json`, `diagnostics_json` as `Value` (`session_repository.rs:1513`, `session_repository.rs:1517`, `session_repository.rs:1521`, `session_repository.rs:1563`, `session_repository.rs:1564`, `session_repository.rs:1565`, `session_core.rs:306`, `session_core.rs:310`, `session_core.rs:314`).
- Projection segments write/read `source_refs_json` and `content_json` as `Value` (`session_repository.rs:1599`, `session_repository.rs:1603`, `session_repository.rs:1632`, `session_repository.rs:1634`, `session_core.rs:358`, `session_core.rs:363`).
- Runtime session lineage writes/reads `fork_point_ref_json` and `metadata_json` as `Value` (`session_repository.rs:1283`, `session_repository.rs:1287`, `session_repository.rs:1314`, `session_repository.rs:1319`, `session_core.rs:426`, `session_core.rs:437`).

Classification and target shape:

| Column | Current helper/row shape | Classification | Suggested shape |
| --- | --- | --- | --- |
| `runtime_session_events.notification_json` | `json_string(BackboneEnvelope)` / `String -> BackboneEnvelope` | `convert_to_jsonb_now` | `notification_json: Json<BackboneEnvelope>`; bind `Json(&persisted.notification)`. |
| `agent_run_control_effects.payload_json` | `json_string(Value)` / `String -> Value` | `convert_to_jsonb_now` | `payload_json: Json<Value>`; bind `Json(&record.payload)`. |
| `agent_frame_transitions.capability_keys_json` | `json_string(BTreeSet<String>)` / `String -> BTreeSet<String>` | `convert_to_jsonb_now` | `capability_keys_json: Json<BTreeSet<String>>`; bind `Json(&frame_transition.capability_keys)`. |
| `agent_frame_transitions.transition_json` | `json_string(RuntimeCapabilityTransition)` / `String -> RuntimeCapabilityTransition` | `convert_to_jsonb_now` | `transition_json: Json<RuntimeCapabilityTransition>`. |
| `runtime_session_delivery_commands.payload_json` | `json_string(RuntimeDeliveryCommand)` / `String -> RuntimeDeliveryCommand` | `convert_to_jsonb_now` | `payload_json: Json<RuntimeDeliveryCommand>`. |
| `runtime_session_compactions.replacement_projection_json` | `json_string(Value)` / `parse_json_column` | `convert_to_jsonb_now` | `replacement_projection_json: Json<Value>`; default `'{}'::jsonb`. |
| `runtime_session_compactions.token_stats_json` | `json_string(Value)` / `parse_json_column` | `convert_to_jsonb_now` | `token_stats_json: Json<Value>`; default `'{}'::jsonb`. |
| `runtime_session_compactions.diagnostics_json` | `json_string(Value)` / `parse_json_column` | `convert_to_jsonb_now` | `diagnostics_json: Json<Value>`; default `'{}'::jsonb`. |
| `runtime_session_projection_segments.source_refs_json` | `json_string(Value)` / `parse_json_column` | `convert_to_jsonb_now` | `source_refs_json: Json<Value>`; default `'[]'::jsonb`. |
| `runtime_session_projection_segments.content_json` | `json_string(Value)` / `parse_json_column` | `convert_to_jsonb_now` | `content_json: Json<Value>`. |
| `runtime_session_lineage.fork_point_ref_json` | `json_string(Value)` / `parse_json_column` | `convert_to_jsonb_now` | `fork_point_ref_json: Json<Value>`; default `'{}'::jsonb`. |
| `runtime_session_lineage.metadata_json` | `json_string(Value)` / `parse_json_column` | `convert_to_jsonb_now` | `metadata_json: Json<Value>`; default `'{}'::jsonb`. |

Tests likely affected:

- `runtime_session_events_persist_envelope_without_flattened_fact_columns` asserts exact runtime event schema and will need expected type/name decision updates if column renaming happens; type-only conversion should preserve the column list (`session_repository.rs:1886`, `session_repository.rs:1906`, `session_repository.rs:1913`).
- `append_event_assigns_monotonic_event_seq` and `stale_save_session_meta_does_not_roll_back_event_projection` exercise event writes/reads (`session_repository.rs:1918`, `session_repository.rs:1971`).
- `compaction_projection_commit_persists_checkpoint_segments_and_head` exercises compaction JSON, projection segment JSON, and completion event JSON (`session_repository.rs:2029`, `session_repository.rs:2060`, `session_repository.rs:2068`, `session_repository.rs:2075`).
- `runtime_session_lineage_queries_are_stable_and_filterable` exercises runtime session lineage JSON fields (`session_repository.rs:2089`).
- No focused test name found for `agent_run_control_effects` / runtime delivery command JSON shape in `session_repository.rs`; add tests or include them in session repository roundtrip coverage.

### State Change Store / Repository

Owner: `PostgresStateChangeRepository` delegates to `state_change_store`.

Relevant schema:

- `state_changes.payload text DEFAULT '{}'::text NOT NULL`; `kind text` is scalar (`0001_init.sql:701`, `0001_init.sql:705`, `0001_init.sql:706`).

Code patterns:

- `append_state_change` and `append_state_change_in_tx` bind `payload.to_string()`; `StateChangeRow.payload: String` is parsed through `parse_json_payload` (`state_change_store.rs:15`, `state_change_store.rs:21`, `state_change_store.rs:40`, `state_change_store.rs:46`, `state_change_store.rs:128`, `state_change_store.rs:148`, `state_change_store.rs:178`, `state_change_store.rs:179`).
- `kind_to_db_value` uses `serde_json::to_string(kind)?.trim_matches('"')`, but the stored column is a scalar enum string and `parse_change_kind` already treats it as scalar (`state_change_store.rs:118`, `state_change_store.rs:119`, `state_change_store.rs:147`, `state_change_store.rs:162`).
- `state_change_repository.rs` has no JSON helper itself; it passes `serde_json::Value` to the store (`state_change_repository.rs:52`, `state_change_repository.rs:57`, `state_change_repository.rs:60`).

Classification and target shape:

| Column | Current row field | Classification | Suggested shape |
| --- | --- | --- | --- |
| `state_changes.payload` | `String` | `convert_to_jsonb_now` | `payload: Json<Value>`; bind `Json(payload)` or `Json(&payload)`. Default `'{}'::jsonb`. |
| `state_changes.kind` | `String` | `scalar enum/string` | keep `text`; replace JSON stringification with explicit scalar mapping. No `Json<T>`. |

Tests likely affected:

- No focused state-change PostgreSQL test was found in the inspected target files. Story repository tests may indirectly append state changes; add state change payload roundtrip and bad-shape migration/mapper coverage.

### Keep Text / Raw Findings

No `serde_json::from_str` / `to_string` helper in the inspected target set appears to be preserving raw text, source code, markdown, provider body bytes, or user-authored plain text. Raw text columns such as summaries, previews, errors, labels, messages, and IDs are not JSON parsed by these helpers and were excluded from conversion candidates.

### Defer Findings

- `lifecycle_runs.context` and `lifecycle_runs.view_projection` were initial candidates in early task notes but are not live columns in the current schema after migration 0041, so they are not deferred; they are not found in current target repository mapping.
- `_json` suffix cleanup should be deferred as a naming slice unless a column has an obvious business name. Type conversion can proceed now for structured document columns; renaming `notification_json`, `payload_json`, `metadata_json`, etc. would multiply SQL/test churn and should be a separate explicit naming pass.
- `agent_frames` split projection columns might eventually be dropped or renamed if surface becomes the only needed document, but current repository still reads/writes them as projection columns. Classification is `convert_to_jsonb_now`; defer only the removal/renaming decision.

## Recommended Implementation Slices

1. Workflow core JSONB slice:
   Convert `agent_procedures.contract`, `workflow_graphs.activities/transitions`, and `lifecycle_runs.orchestrations/tasks/execution_log`; keep `source` and `status` as scalar text; update `PostgresWorkflowRepository` and fork materialization lifecycle-run insert path.

2. AgentFrame surface/projection slice:
   Convert `agent_frames.surface` and split projection JSON columns; update `PostgresAgentFrameRepository`, fork materialization `insert_agent_frame_tx`, and frame row tests.

3. Lifecycle anchor metadata/gate slice:
   Convert `lifecycle_subject_associations.metadata_json`, `lifecycle_gates.payload_json`, and `agent_lineages.metadata_json`; update gate JSONB query operators to remove `::jsonb` casts; add/adjust metadata and gate payload roundtrip tests.

4. Agent-run lineage slice:
   Convert `agent_run_lineages.fork_point_ref` and `metadata`; update row fields and insert helpers to nullable `Json<Value>`; adjust unit tests and add a Postgres materialization roundtrip if feasible.

5. Mailbox narrow slice:
   Convert only `agent_run_mailbox_messages.source_metadata`; leave already-jsonb mailbox fields as no-op or optional consistency cleanup; update `source_identity_roundtrips_through_message_rows`.

6. Session persistence slice:
   Convert runtime session event/control/delivery/compaction/projection/lineage JSON columns together because `session_core::json_string` and `parse_json_column` are shared; update row mappers to `Json<T>` / `Json<Value>` and adjust session repository tests.

7. State change slice:
   Convert `state_changes.payload`; keep `kind` scalar; update `state_change_store` and add focused payload roundtrip/error-context coverage.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task, but the user provided the target task path and output file explicitly; research was written to that path.
- No external web documentation was used. Internal `sqlx::types::Json` patterns already exist in `shared_library_repository.rs` and `runtime_health_repository.rs`.
- This inventory does not edit code or migrations. It also does not cover other PostgreSQL repositories outside the user-specified runtime/workflow file set.
- Some suggested row shapes use `Json<Value>` because the current domain/SPI field type is `serde_json::Value`; narrowing those to typed value objects is a separate domain modeling decision unless the type already exists.
