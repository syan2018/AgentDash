# Research: Session runtime schema

- Query: 正式评估 sessions / session_events / session_terminal_effects / session_runtime_commands / agent_frame_transitions / session_compactions / session_projection_heads / session_projection_segments / session_lineage 的字段语义正确性、归属和 baseline 收敛建议。
- Scope: internal
- Date: 2026-06-03

## Findings

### Files found

- `.trellis/tasks/06-03-database-semantic-baseline-audit/prd.md` - 要求正式评估 baseline 语义，区分 business fact、runtime/audit、projection/cache、historical residue 等类别。
- `.trellis/tasks/06-03-database-semantic-baseline-audit/design.md` - 定义本任务评估模型和输出形态，明确不直接修改 schema/code。
- `.trellis/tasks/06-03-database-semantic-baseline-audit/implement.md` - 将 Session runtime 列为独立研究分区。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - 当前 PostgreSQL baseline，目标表定义在 `agent_frame_transitions`、`session_compactions`、`session_events`、`session_lineage`、`session_projection_heads`、`session_projection_segments`、`session_runtime_commands`、`session_terminal_effects`、`sessions`。
- `crates/agentdash-spi/src/session_persistence.rs` - Session runtime persistence SPI，拆出 meta/event/effect/runtime-command/compaction/projection/lineage store。
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` - PostgreSQL session repository SQL，直接决定目标表读写语义。
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` - mapper 与 event-to-session projection 逻辑。
- `crates/agentdash-application/src/session/core.rs` - session meta 创建、running session 查询和 session core 服务入口。
- `crates/agentdash-application/src/session/types.rs` - launch runtime trace state 与冷启动恢复判断。
- `crates/agentdash-application/src/session/launch/commit.rs` - turn accepted 后写回 session meta 和 runtime command applied。
- `crates/agentdash-application/src/session/construction_planner.rs` - 当前仍从 `SessionMeta.executor_config` 读取 executor/provider 行为。
- `crates/agentdash-api/src/routes/sessions.rs` - HTTP session meta、fork、lineage、projection rollback、title/tab layout 更新路由。
- `crates/agentdash-api/src/dto/session.rs` - `UpdateSessionMetaRequest` 暴露 `tab_layout`。
- `crates/agentdash-contracts/src/session.rs` - generated session DTO contract 暴露 session runtime list/detail 字段。
- `.trellis/spec/backend/session/architecture.md` - Session 目标语义：当前 Session 是 RuntimeSession，不拥有业务归属、permission scope、Lifecycle progress 或 Agent effective surface。
- `.trellis/spec/backend/session/runtime-execution-state.md` - Session runtime 持久化 store 分类、runtime command outbox 与 frame transition fact 语义。
- `.trellis/spec/backend/session/session-startup-pipeline.md` - pending runtime command、AgentFrameTransitionRecord 与 SessionRuntimeCommandStore 的控制面分层。
- `.trellis/spec/backend/session/context-compaction-projection.md` - compaction/projection store 是模型上下文 checkpoint/projection 面。
- `.trellis/spec/backend/session/session-lineage-projection.md` - session lineage、projection head、fork/rollback 语义。
- `.trellis/spec/backend/database-guidelines.md` - init migration 只表达 schema/约束/索引；runtime facts 由 runtime repository 写入。
- `.trellis/spec/backend/repository-pattern.md` - Session runtime persistence 不通过 `RepositorySet`，而由 `agentdash-spi::session_persistence` 表达。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - Session event/projection/branch DTO 从 Rust contract 生成到前端。

### Current semantic baseline

- Session spec 明确当前 `Session` 语义上是 `RuntimeSession`，只拥有 turn/tool/event/resume/debug/projection/trace lineage，不拥有业务归属、permission scope、Lifecycle progress 或 Agent effective surface（`.trellis/spec/backend/session/architecture.md:5`）。
- 通过 runtime session 反查业务上下文只能走 `RuntimeSession -> AgentFrame -> LifecycleAgent -> LifecycleRun -> LifecycleSubjectAssociation`，不能把 runtime session head 当业务上下文源（`.trellis/spec/backend/session/architecture.md:31`）。
- runtime map、active turn、connector live session 是三个不同问题，不能用一个状态互相推断（`.trellis/spec/backend/session/architecture.md:32`）。
- `SessionMetaStore` 现在仍被定义成 “session meta CRUD 与投影字段合并写回”，因此 `sessions` 目前既是 runtime trace head，也是若干读模型缓存的落点（`.trellis/spec/backend/session/runtime-execution-state.md:111`）。
- `SessionEventStore` 是 append/read/list session events（`.trellis/spec/backend/session/runtime-execution-state.md:112`），`session_events` 是事实日志。
- `SessionTerminalEffectStore` 是 terminal effect outbox 写入、状态迁移和查询（`.trellis/spec/backend/session/runtime-execution-state.md:113`），`session_terminal_effects` 是 outbox/audit。
- `SessionRuntimeCommandStore` 是 runtime delivery command request upsert、requested 查询、applied/failed 状态迁移（`.trellis/spec/backend/session/runtime-execution-state.md:114`），它是 delivery outbox，不是 capability 事实源。
- Runtime context / capability transition 的事实源是 `AgentFrameTransitionRecord` / `agent_frame_transitions`；runtime command store 是 delivery outbox（`.trellis/spec/backend/session/runtime-execution-state.md:121`，`.trellis/spec/backend/session/session-startup-pipeline.md:132`）。
- 上下文压缩采用 lifecycle + AgentDash-owned projection store，原因是 compact 同时是可观察 lifecycle 和模型上下文 checkpoint（`.trellis/spec/backend/session/architecture.md:57`）。
- Fork 默认把 parent fork point 的模型可见 projection 固化为 child session 自己的 initial compaction，避免 child 继续执行依赖 parent live projection（`.trellis/spec/backend/session/architecture.md:58`）。
- `session_lineage` 只解释 runtime branch topology 与 restore provenance；业务可见性经 `LifecycleSubjectAssociation` 与 `AgentLineage` 投影（`.trellis/spec/backend/session/session-lineage-projection.md:43`）。
- Init migration 应只表达 schema、约束、索引和必要扩展，runtime facts 由 runtime repository 写入；因此 `created_by_kind DEFAULT 'backfill'` 一类历史回填默认值不应出现在新 init（`.trellis/spec/backend/database-guidelines.md:46`）。

### Table-by-table audit

#### sessions

Schema fields: `id`, `title`, `created_at`, `updated_at`, `last_event_seq DEFAULT 0`, `last_execution_status DEFAULT 'idle'`, `last_turn_id`, `last_terminal_message`, `executor_config_json`, `executor_session_id`, `title_source DEFAULT 'auto'`, `tab_layout_json`, `project_id` (`crates/agentdash-infrastructure/migrations/0001_init.sql:888`-`902`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `id` | runtime fact | Keep. Runtime session identity and FK target for session runtime tables. |
| `title` | projection/cache with user override | Keep for list UX, but it is runtime trace display metadata rather than business fact. `title_source` makes the projection/user override boundary explicit. |
| `created_at`, `updated_at` | runtime fact / list projection | Keep. `updated_at` is updated on event append and meta edits; it is also list ordering cache. |
| `last_event_seq` | runtime fact + projection cursor | Keep but normalize constraints. It is the authoritative event sequence allocator: repository increments it before inserting `session_events` (`session_repository.rs:351`-`365`). It is also used by cold-start decisions and fork bounds (`session/types.rs:153`-`156`, `session/branching.rs:178`-`183`). |
| `last_execution_status` | projection/cache | Requires code-change candidate to move out of `sessions` or rename as a meta projection. It is derived from appended events by `projection_from_envelope` (`session_core.rs:689`-`755`) and written back after `session_events` insert (`session_repository.rs:404`-`424`). It should not be interpreted as active turn/live connector truth because spec says runtime map/active turn/connector live session are distinct (`architecture.md:32`). |
| `last_turn_id` | projection/cache | Same as `last_execution_status`: derived from event trace, useful for list/resume UI, but not a primary runtime fact. |
| `last_terminal_message` | projection/cache | Same. It is derived from terminal/error events, not a durable terminal fact; terminal facts live in event log and terminal effect outbox. |
| `executor_config_json` | business/provider leakage | Highest-priority code-change candidate. Session launch writes executor config into meta (`session/launch/commit.rs:141`-`149`), construction planner reads it as `session.meta.executor_config` (`session/construction_planner.rs:186`，`:277`-`:278`). Spec says `RuntimeSession` does not own Agent effective surface or provider behavior (`architecture.md:5`，`:29`-`:31`). This belongs to AgentFrame execution profile / launch construction source, not `sessions`. |
| `executor_session_id` | runtime fact / connector private trace pointer | Keep for now, but rename/constraint consideration. It is used to detect executor follow-up and avoid repository rehydrate (`session/types.rs:107`-`126`, `:153`-`:156`) and is set from `ExecutorSessionBound` event projection (`session_core.rs:739`-`741`). It is a connector runtime trace pointer, not provider config. |
| `title_source` | projection/cache metadata | Keep but normalize default/constraint. `TitleSource` is `auto/source/user` in SPI (`session_persistence.rs:267`-`276`); API sets `user` on manual title edit (`routes/sessions.rs:464`). New init should add a CHECK. |
| `tab_layout_json` | business/UI leakage | Requires code-change candidate. API allows `PATCH /sessions/{id}/meta` to write `tab_layout` (`routes/sessions.rs:445`-`468`), DTO exposes it (`dto/session.rs:29`). This is UI preference/layout state, not runtime trace fact. Move to user preference/workspace view/tab state table keyed by session if the product needs persistence. |
| `project_id` | business leakage / temporary permission shortcut | Highest-priority code-change candidate. SPI comment says session-owned project is fixed at creation (`session_persistence.rs:281`-`284`), but spec says runtime session does not own business归属 and reverse lookup must go through AgentFrame/LifecycleRun/SubjectAssociation (`architecture.md:5`，`:31`). Move permission/project resolution to runtime_session_execution_anchors / AgentFrame path and remove from sessions. |

Recommended action:

- Keep `sessions` as a minimal runtime session head: `id`, display title metadata, timestamps, event sequence allocator, connector runtime trace pointer if still needed.
- Move/remove after code change: `project_id`, `executor_config_json`, `tab_layout_json`.
- Treat `last_execution_status`, `last_turn_id`, `last_terminal_message` as projection/cache. Either move to a dedicated session list projection table or rename/document them as `latest_*_projection` style if kept in `sessions`.
- Normalize defaults/checks: add CHECKs for `last_execution_status` and `title_source`; keep `last_event_seq DEFAULT 0` because creation path starts at zero (`session/core.rs:57`-`68`), but enforce non-negative.

#### session_events

Schema fields: `session_id`, `event_seq`, `occurred_at_ms`, `committed_at_ms`, `session_update_type`, `turn_id`, `entry_index`, `tool_call_id`, `notification_json` (`0001_init.sql:776`-`786`). Primary key `(session_id, event_seq)` (`0001_init.sql:1505`-`1509`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `session_id`, `event_seq` | runtime fact | Keep. Append-only timeline identity. |
| `occurred_at_ms`, `committed_at_ms` | runtime/audit fact | Keep. Current repository sets both to commit time, but separate fields are semantically valid for future relay/local event ingestion (`session_repository.rs:379`-`399`). |
| `session_update_type` | projection/index hint over event fact | Keep. It stores backend event type for stream/page filtering and frontend event mapping; not a business fact. Add CHECK only if event type set is stable enough. |
| `turn_id`, `entry_index`, `tool_call_id` | runtime trace indexes | Keep. Derived from envelope trace/tool payload in mapper and used for query/UI correlation (`session_core.rs:700`-`710`). |
| `notification_json` | runtime fact payload | Keep. This is the canonical BackboneEnvelope persisted event payload. |

Recommended action:

- Retain as baseline core table. No direct removal candidate.
- Consider indexes for `(session_id, turn_id)` or `(session_id, tool_call_id)` only if query code needs them; current init only has PK for event paging.
- Add FK to `sessions(id)` if absent in full init. Current migration has session FK for most session tables; verify `session_events` FK in final report because the targeted rg output did not show it near other session FKs.

#### session_terminal_effects

Schema fields: `id`, `session_id`, `turn_id`, `terminal_event_seq`, `effect_type`, `payload_json`, `status`, `attempt_count DEFAULT 0`, `created_at_ms`, `updated_at_ms`, `last_error` (`0001_init.sql:869`-`880`). Indexes cover session/turn, status/updated, terminal_event (`0001_init.sql:2188`-`2205`), FK to sessions (`0001_init.sql:2458`-`2462`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `id` | outbox/audit fact | Keep. Effect identity. |
| `session_id`, `turn_id`, `terminal_event_seq` | runtime/audit fact | Keep. Ties terminal side effect to the terminal event and turn. |
| `effect_type`, `payload_json` | outbox fact | Keep. Payload is delivery body. |
| `status`, `attempt_count`, `created_at_ms`, `updated_at_ms`, `last_error` | outbox lifecycle/audit | Keep. `attempt_count DEFAULT 0` is correct for newly requested outbox rows. Add CHECKs for status/effect_type if enums are stable. |

Recommended action:

- Retain as baseline outbox table.
- Normalize status constraints/defaults. No historical residue found.

#### agent_frame_transitions

Schema fields: `id`, `target_frame_id`, `run_id`, `lifecycle_key`, `phase_node`, `capability_keys_json`, `transition_json`, `source_turn_id`, `created_at_ms` (`0001_init.sql:47`-`57`). Primary key at `0001_init.sql:1137`-`1141`; indexes on `(run_id, lifecycle_key, phase_node)` and `(target_frame_id, created_at_ms)` (`0001_init.sql:1684`-`1694`); FK to `agent_frames(id)` (`0001_init.sql:2282`-`2286`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `id` | runtime/control-plane fact | Keep. Referenced by `session_runtime_commands.frame_transition_id`. |
| `target_frame_id` | control-plane fact | Keep. Transition applies to AgentFrame surface. |
| `run_id`, `lifecycle_key`, `phase_node` | workflow/control-plane fact | Keep, but consider FK/type normalization later. They locate the lifecycle phase for replay/query. |
| `capability_keys_json` | control-plane fact / compact transition summary | Keep. Spec says frame transition stores replayable transition records, not full `CapabilityState` projection (`architecture.md:34`). |
| `transition_json` | control-plane fact | Keep. Repository validates it as `RuntimeCapabilityTransition` (`session_core.rs:216`-`223`). |
| `source_turn_id` | runtime trace provenance | Keep. Optional provenance. |
| `created_at_ms` | audit fact | Keep. |

Recommended action:

- Retain as baseline. It is not residue despite living beside session runtime tables.
- Potential normalization: `run_id`/`target_frame_id` are stored as text but parsed as UUID in mapper (`session_core.rs:203`-`208`); new init should prefer UUID columns where surrounding tables already use UUID.
- Add FK for `run_id` if lifecycle run table ownership is stable; current init only shows target frame FK.

#### session_runtime_commands

Schema fields: `id`, `session_id`, `phase_node`, `status`, `payload_json`, `created_at_ms`, `updated_at_ms`, `applied_at_ms`, `failed_at_ms`, `last_error`, `frame_transition_id` (`0001_init.sql:850`-`862`). Indexes by frame transition, session/status, status/updated (`0001_init.sql:2167`-`2184`); FKs to `agent_frame_transitions` and `sessions` (`0001_init.sql:2322`-`2326`, `:2450`-`:2454`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `id` | outbox/audit fact | Keep. |
| `session_id` | delivery target fact | Keep. It is the runtime session receiving the command. |
| `phase_node` | duplicate/cache from frame transition | Keep for query convenience or remove after code-change. It duplicates `agent_frame_transitions.phase_node`; repository stores both and joins to transition (`session_repository.rs:724`-`782`, `:810`-`:865`). Because command status queries use session/status, phase may be redundant in the outbox. |
| `status`, `created_at_ms`, `updated_at_ms`, `applied_at_ms`, `failed_at_ms`, `last_error` | outbox lifecycle/audit | Keep. Status set is `requested/applied/failed` in SPI (`session_persistence.rs:306`-`321`). |
| `payload_json` | delivery outbox payload | Keep, but verify it does not duplicate transition fact beyond delivery envelope. Mapper validates payload/frame transition consistency (`session_core.rs:169`-`180`). |
| `frame_transition_id` | FK to control-plane fact | Keep. This is the key semantic split: command is delivery outbox, transition is fact. |

Recommended action:

- Retain table as baseline outbox.
- Normalize CHECK for status; avoid historical `pending` terminology in init. Current enum is already `requested/applied/failed`.
- Consider removing or renaming `phase_node` from command after confirming all queries can read it from transition; at minimum document it as denormalized query key.

#### session_compactions

Schema fields: `id`, `session_id`, `projection_kind`, `projection_version`, `lifecycle_item_id`, event seq range fields, `status`, `trigger`, `reason`, `phase`, `strategy`, `budget_scope`, `base_head_event_seq`, `source_*`, `first_kept_event_seq`, `summary`, `replacement_projection_json DEFAULT '{}'`, `token_stats_json DEFAULT '{}'`, `diagnostics_json DEFAULT '{}'`, `created_by`, timestamps (`0001_init.sql:743`-`769`). Indexes on lifecycle item, session/kind/status/version, source range (`0001_init.sql:2111`-`2128`); FK to sessions (`0001_init.sql:2386`-`2390`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `id`, `session_id` | runtime projection/checkpoint fact | Keep. |
| `projection_kind`, `projection_version` | projection/cache fact | Keep. Projection store is baseline because fork/rollback/restore depend on durable projection heads and segments. |
| `lifecycle_item_id` | runtime/audit fact with naming concern | Keep but review naming. It links compaction to observable lifecycle item; if this is not a DB FK, name should make provenance clear. |
| `start_event_seq`, `completed_event_seq`, `failed_event_seq` | runtime/audit fact | Keep. Compaction lifecycle is observable in event timeline. |
| `status`, `trigger`, `reason`, `phase`, `strategy`, `budget_scope` | runtime/audit/config fact | Keep with CHECK/default normalization. |
| `base_head_event_seq`, `source_start_event_seq`, `source_end_event_seq`, `first_kept_event_seq` | projection checkpoint fact | Keep. Needed to define model-visible boundary and retained range. |
| `summary` | projection payload/cache | Keep. Empty-string default is acceptable only if service treats summary as required display payload; otherwise prefer nullable. |
| `replacement_projection_json`, `token_stats_json`, `diagnostics_json` | projection/audit payload | Keep. Defaults are schema convenience, not historical backfill. |
| `created_by` | audit fact with weak naming | Keep but normalize. It should identify actor/system source; consider `created_by_kind` + `created_by_id` or a documented string enum. |
| `created_at_ms`, `completed_at_ms` | audit fact | Keep. |

Recommended action:

- Retain as baseline. Projection/compaction tables are not historical residue; specs explicitly require them for checkpoint, fork, rollback, branch-aware restore (`session-lineage-projection.md:11`-`25`).
- Normalize CHECKs for `status`, `projection_kind` if closed, and event sequence non-negative.
- Review whether `lifecycle_item_id` should be a FK or renamed to `lifecycle_event_id`/`compaction_lifecycle_item_id` to avoid implying business lifecycle progress ownership.

#### session_projection_heads

Schema fields: `session_id`, `projection_kind`, `projection_version`, `head_event_seq`, `active_compaction_id`, `updated_by_event_seq`, `updated_at_ms` (`0001_init.sql:812`-`820`). Primary key `(session_id, projection_kind)` (`0001_init.sql:1521`-`1525`); FK to sessions and optional compaction (`0001_init.sql:2418`-`2430`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `session_id`, `projection_kind` | projection identity | Keep. |
| `projection_version` | projection version/cache | Keep. |
| `head_event_seq` | projection cursor | Keep. It is the active model-visible cursor, not append-only event fact. |
| `active_compaction_id` | projection checkpoint reference | Keep. |
| `updated_by_event_seq`, `updated_at_ms` | audit/projection metadata | Keep. |

Recommended action:

- Retain as baseline. Spec says projection head is distinct from lineage edge and lets rollback move model-visible cursor without rewriting append-only events (`session-lineage-projection.md:11`-`22`).
- Consider PK `(session_id, projection_kind)` vs `(session_id, projection_kind, projection_version)`: current design stores only active head per kind and updates version in-place, which matches rollback semantics. Keep unless multi-head history becomes a product need.

#### session_projection_segments

Schema fields: `id`, `session_id`, `projection_kind`, `projection_version`, `sort_order`, `segment_type`, `origin`, `synthetic DEFAULT false`, `source_start_event_seq`, `source_end_event_seq`, `source_refs_json DEFAULT '[]'`, `generated_by_compaction_id`, `content_json`, `token_estimate`, `created_at_ms` (`0001_init.sql:827`-`843`). Unique key `(session_id, projection_kind, projection_version, sort_order)` (`0001_init.sql:1537`-`1541`); indexes by projection and source range (`0001_init.sql:2153`-`2163`); FKs to sessions and generated compaction (`0001_init.sql:2434`-`2446`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `id` | projection segment identity | Keep. |
| `session_id`, `projection_kind`, `projection_version`, `sort_order` | projection/cache fact | Keep. Defines ordered materialized model input. |
| `segment_type`, `origin`, `synthetic` | projection provenance | Keep. Frontend contract exposes provenance fields for projection view (`frontend-backend-contracts.md:86`). |
| `source_start_event_seq`, `source_end_event_seq`, `source_refs_json` | provenance/audit | Keep. Required to inspect where a synthetic segment came from. |
| `generated_by_compaction_id` | projection checkpoint reference | Keep. |
| `content_json` | projection payload | Keep. |
| `token_estimate` | projection/cache | Keep. It is cache-like but useful for context budgeting. |
| `created_at_ms` | audit fact | Keep. |

Recommended action:

- Retain as baseline. It is materialized projection, not source business fact, but is intentionally durable because restore/fork/rollback need it.
- Add CHECKs for non-negative seq/order/token and enum-like `segment_type`/`origin` if stable.

#### session_lineage

Schema fields: `child_session_id`, `parent_session_id`, `relation_kind`, `fork_point_event_seq`, `fork_point_ref_json DEFAULT '{}'`, `fork_point_compaction_id`, `status`, `created_at_ms`, `updated_at_ms`, `metadata_json DEFAULT '{}'`, self-cycle CHECK (`0001_init.sql:793`-`805`). Primary key `child_session_id` (`0001_init.sql:1513`-`1517`); indexes by fork point and parent/status/kind (`0001_init.sql:2132`-`2142`); FKs to child/parent sessions and compaction (`0001_init.sql:2394`-`2414`).

Classification:

| Field | Classification | Assessment |
| --- | --- | --- |
| `child_session_id`, `parent_session_id` | runtime trace lineage fact | Keep. Child has one primary parent edge. |
| `relation_kind` | runtime trace lineage fact | Keep. Current user fork uses `Fork`; other relation kinds remain trace facts, not lifecycle policy (`session-lineage-projection.md:39`). |
| `fork_point_event_seq`, `fork_point_ref_json`, `fork_point_compaction_id` | restore/provenance fact | Keep. Needed for fork materialization and compaction-bound fork validation. |
| `status` | lineage lifecycle/audit | Keep. Add CHECK for known statuses. |
| `created_at_ms`, `updated_at_ms`, `metadata_json` | audit/provenance | Keep. |

Recommended action:

- Retain as baseline. It is not business ownership; it is branch topology/provenance.
- Keep self-cycle CHECK; repository additionally prevents recursive cycles before upsert (`session_repository.rs:1180`-`1190` region).
- Add CHECK for `relation_kind`/`status` and consider explicit FK action policy: `ON DELETE CASCADE` is coherent for deleting a runtime trace, while `fork_point_compaction_id ON DELETE SET NULL` preserves lineage if checkpoint row is deleted.

### Removal and migration candidates

#### Can remove directly from new init

- No target table in this slice is safe to drop directly from `0001_init.sql` without code/spec changes. Every target table is referenced by SPI/repository and has current runtime semantics.
- Direct field removal without code change is also not safe among the requested fields. Some fields are semantically misplaced, but code currently reads/writes them.

#### Remove after code change

- `sessions.project_id`: move project/permission resolution to AgentFrame/LifecycleRun/LifecycleSubjectAssociation path. This is business leakage into RuntimeSession.
- `sessions.executor_config_json`: move provider/executor behavior to AgentFrame execution profile / launch construction source. This is provider behavior leakage into RuntimeSession.
- `sessions.tab_layout_json`: move to user/workspace/session UI preference table or view state. This is UI leakage into RuntimeSession.
- Possibly `session_runtime_commands.phase_node`: remove if all query/use sites can join through `agent_frame_transitions.phase_node`; otherwise retain as denormalized query key with explicit comment/spec.

#### Field location wrong / should migrate

- `sessions.last_execution_status`, `sessions.last_turn_id`, `sessions.last_terminal_message`: they are event-derived projection/cache fields. They can remain for list performance only if renamed/documented as latest event projection. Better target is a dedicated `session_runtime_summaries` / `session_list_projection` table keyed by `session_id`, leaving `sessions` as identity + sequence allocator.
- `sessions.project_id` is not merely misplaced but conflicts with target control-plane lookup rules. `runtime_session_execution_anchors` already exists in the broader schema (`0001_init.sql:724`-`735`) and should be part of the replacement path together with AgentFrame/LifecycleSubjectAssociation.

#### Keep but rename/default/constraint cleanup

- `sessions.title_source DEFAULT 'auto'`: keep but add CHECK (`auto/source/user`) and ensure default is product semantics, not dump residue.
- `sessions.last_execution_status DEFAULT 'idle'`: keep only if this remains a list projection; add CHECK (`idle/running/completed/failed/interrupted`) and non-authoritative status wording.
- `sessions.last_event_seq DEFAULT 0`: keep; it is the event sequence allocator. Add non-negative CHECK.
- `session_terminal_effects.attempt_count DEFAULT 0`: keep; correct new-row outbox default.
- `session_compactions.summary DEFAULT ''`: review nullable vs empty-string semantics.
- `session_compactions.created_by`: keep but consider `created_by_kind`/`created_by_id` or a documented enum string.
- JSON payload columns currently use `text`; project-wide schema style may prefer `jsonb` for queryable payloads, but if code treats them as opaque serialized contracts, `text` is acceptable. Decide globally in the main report.
- UUID-like text columns in `agent_frame_transitions` (`target_frame_id`, `run_id`) are parsed as UUID by mapper; new baseline can use UUID columns if surrounding repository and domain are ready.

#### Looks odd but should keep

- `session_compactions`, `session_projection_heads`, `session_projection_segments`: These are projection/cache tables, but they belong in baseline because model context restore, compaction checkpoint, fork initial projection and rollback require durable materialized projections.
- `session_lineage`: This is not business ownership. It should remain as runtime trace branch topology and restore provenance.
- `agent_frame_transitions`: Although adjacent to session runtime commands, it is the fact source for capability/runtime context transitions; command table only delivers it to a runtime session.
- `executor_session_id`: It is a connector private trace pointer and currently required for follow-up vs repository rehydrate decisions. It should not be conflated with `executor_config_json`.

### `created_by_kind DEFAULT 'backfill'` note

- The requested `created_by_kind DEFAULT 'backfill'` pattern appears on `agent_frames.created_by_kind` (`0001_init.sql:64`-`79`), not in the nine target session runtime tables.
- It should not remain in a new init baseline: `backfill` is a historical migration/default concept, while init migration should express fresh schema semantics and runtime/use case writes should supply creation source (`database-guidelines.md:46`).
- `runtime_session_execution_anchors.created_by_kind` has no `backfill` default (`0001_init.sql:724`-`735`), which is the better shape for new baseline.

### External references

- None. This research used only repository source, Trellis specs, migration SQL, and task artifacts.

### Related specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/context-compaction-projection.md`
- `.trellis/spec/backend/session/session-lineage-projection.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## Caveats / Not Found

- `task.py current --source` returned no active task, so this research used the explicit task path provided in the user prompt: `.trellis/tasks/06-03-database-semantic-baseline-audit`.
- This slice did not run tests or cargo checks because the task is read-only research and no code/schema changes were made.
- I did not prove `session_events` foreign key presence in the full init output from the targeted FK search; final report should verify all session table FKs with a schema parser or a broader FK extraction.
- Table/field recommendations are semantic and assume the project remains pre-launch with no compatibility migration requirement, matching the task PRD.
