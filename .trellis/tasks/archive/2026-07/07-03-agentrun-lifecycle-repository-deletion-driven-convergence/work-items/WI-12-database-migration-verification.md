# WI-12 Database Migration Verification

## Objective

统筹本轮破坏式 schema 变更，确保表重命名、字段删除、ownership 调整、FK/cascade、索引和数据迁移与正式决策一致。

## Decisions

D-003, D-005, D-010, D-011, D-013, D-016, D-017, D-019

## Research Inputs

- `research/database-physical-design.md`
- `research/command-mailbox-delivery.md`
- `research/wi-04-command-mailbox-current-state.md`
- `research/runtime-session-internal-model.md`
- `research/agentframe-context-surface.md`
- `research/fork-lineage-baseline.md`

## Scope

- 维护 schema change ledger。
- 为每个工作项记录：新增表、删除表、字段迁移、FK/cascade、唯一约束、索引、backfill。
- 维护 redundant table ledger：每个疑似冗余表必须给出删除、合并、降级或保留结论。
- 验证 mailbox ownership 从 RuntimeSession 转向 AgentRun。
- 验证 runtime session trace 表命名和 FK/cascade。
- 验证 AgentFrame revision append-only 约束。
- 验证 fork lineage baseline 约束。
- 验证 current delivery binding / projection 的恢复和约束。

## Out Of Scope

- 不独立决定领域边界；只执行已被对应工作项和 `decisions.md` 接受的 schema 方案。
- 不保留旧 API/schema 兼容路径。

## Dependencies

依赖 WI-00 inventory。实际 migration 随 WI-02、WI-04、WI-06、WI-07、WI-08、WI-10 分批进入。

## Implementation Notes

- 项目未上线，migration 可以破坏式删除旧字段和旧表。
- 每个 migration 应让 schema 更接近事实所有权，而不是留下长期双写。
- 对 child table 保留的事实，需要在代码入口上隐藏为父聚合能力。

## Acceptance

- 每个 schema change 都能映射到 D-016 / D-017 的分类理由。
- 每个保留物理表都能映射到 D-016 / D-017 / D-019 的正向资格。
- 每个删除或合并的物理表都有 canonical replacement、数据迁移或可重建说明。
- 删除 RuntimeSession 不会 cascade 删除 AgentRun-owned durable facts。
- AgentRun delete 的 cascade 或显式 cleanup 覆盖 mailbox、receipts、frames、anchors、lineage、gates、subjects 等 run-owned rows。
- migration 后 repository tests 和关键用例 tests 通过。

## Validation

- 迁移应用和回放验证。
- FK/cascade 查询审计。
- Postgres repository roundtrip tests。
- AgentRun start、submit、accepted turn、fork、delete 的数据库级集成验证。

## WI-10 Ledger Entry 2026-07-04 / Worker A2

### Schema Changes

| Migration | Change | Decision mapping |
| --- | --- | --- |
| `crates/agentdash-infrastructure/migrations/0041_drop_lifecycle_run_context_view_projection.sql` | Drops `lifecycle_runs.context` and `lifecycle_runs.view_projection` | D-016: neither column is an independent fact source or qualified child table. D-017: neither column has lock, scan, claim, pagination, recovery, or reverse-query requirements. D-019: both are redundant embedded storage surfaces replaced by canonical lifecycle/agent/frame/subject/read-model facts. |

### Redundant Table / Field Ledger

| Candidate | Conclusion | Canonical replacement or qualification |
| --- | --- | --- |
| `lifecycle_runs.context` | Deleted | AgentRun and frame refs come from `lifecycle_agents`, `agent_frames`, `runtime_session_execution_anchors`, `agent_lineages`, `agent_run_lineages`, and read models. Subject context comes from `lifecycle_subject_associations`. |
| `lifecycle_runs.view_projection` | Deleted | Lifecycle views are rebuilt from `LifecycleRun` aggregate state, agents, subject associations, runtime trace refs, and execution log through application read-model builders. |
| `lifecycle_gates` | Retained as Lifecycle-owned child table | Open gate scanning, status transition, correlation resume, workflow human gate, companion gate, wait activity, and workspace waiting projection need indexed rows and local updates. |
| `lifecycle_subject_associations` | Retained as indexed relationship table | Subject reverse lookup and anchor-to-subject context lookup are production query paths. |
| `agent_lineages` | Retained as same-run control-tree child table | Parent/children/run queries support API tree projection, run view filtering, descendant counts, and companion parent routing. |
| `agent_run_lineages` | Deferred to WI-08 | Product fork canonical record work remains outside WI-10; this ledger only records that Lifecycle context/projection deletion no longer requires fork materialization to clone those columns. |

### Migration Risk For Merge

`0041_drop_lifecycle_run_context_view_projection.sql` is already present and no additional WI-10 migration file was added in this worker. Main-session merge should still keep migration ordering stable with other Batch A workers and run `pnpm run migration:guard` after all migration-touching diffs are combined.

## WI-06 Ledger Entry 2026-07-04 / Worker C1

### Schema Changes

| Migration | Change | Decision mapping |
| --- | --- | --- |
| None in this worker | WI-06 code aligned with the existing 0044 shape: `agent_run_delivery_bindings` is current delivery state and `runtime_session_execution_anchors` remains immutable launch evidence. | D-010: current delivery binding belongs to AgentRun state; anchor is insert-once evidence. |

### Redundant Table / Field Ledger

| Candidate | Conclusion | Canonical replacement or qualification |
| --- | --- | --- |
| `lifecycle_agents.current_delivery_*` | Deletion remains completed by 0044 | `agent_run_delivery_bindings` keyed by `(run_id, agent_id)` owns current delivery state. |
| Anchor coordinate rewrite on runtime session conflict | Removed from fork materialization | `RuntimeSessionExecutionAnchor` create-once semantics plus coordinate conflict detection preserve evidence immutability. |
| Latest-updated anchor selection | Removed from test fake repository surfaces and absent from production repository trait | Delivery selection resolves `AgentRunDeliveryBinding` first, then validates the matching anchor. |

### Migration Risk For Merge

No WI-06 migration was added. If later cleanup drops immutable anchor `updated_at` or renames runtime trace tables, that migration should sequence after mailbox/runtime FK work so delivery binding and anchor FKs keep one canonical runtime trace target.

## WI-04 Ledger Entry 2026-07-04 / Worker B2

### Schema Changes

| Migration | Change | Decision mapping |
| --- | --- | --- |
| `crates/agentdash-infrastructure/migrations/0047_agent_run_mailbox_delivery_runtime_ref.sql` | Renames `agent_run_mailbox_messages.runtime_session_id` and `agent_run_mailbox_states.runtime_session_id` to `delivery_runtime_session_id`; recreates both RuntimeSession FKs as nullable `runtime_sessions(id) ON DELETE SET NULL`; recreates reverse delivery-runtime indexes with delivery naming. | D-005: mailbox owner is AgentRun, not RuntimeSession. D-006: command receipt, queue item, and runtime delivery evidence are separate facts. D-017: mailbox remains a child table because queue claim, ordering, recovery, and scan paths require indexed rows and row locks. |

### Redundant Table / Field Ledger

| Candidate | Conclusion | Canonical replacement or qualification |
| --- | --- | --- |
| `agent_run_mailbox_messages.runtime_session_id` | Renamed and downgraded from owner-shaped field to nullable delivery trace ref | `agent_run_mailbox_messages.delivery_runtime_session_id`; durable owner remains `run_id + agent_id`, and claim/recover/order paths do not filter by runtime session. |
| `agent_run_mailbox_states.runtime_session_id` | Renamed and downgraded from owner-shaped field to nullable delivery trace ref | `agent_run_mailbox_states.delivery_runtime_session_id`; state remains keyed by `(run_id, agent_id)` for pause and backend preference. |
| `agent_run_mailbox_messages` | Retained as AgentRun queue child table | D-017 qualification: priority/order scan, claim lease, recovery, payload cleanup, and queue lifecycle need a physical table. |
| `agent_run_mailbox_states` | Retained as AgentRun queue state child table | D-017 qualification: pause/resume/backend preference are keyed state for an AgentRun agent and are updated independently from message rows. |

### Migration Risk For Merge

`0047_agent_run_mailbox_delivery_runtime_ref.sql` sequences after runtime trace table rename `0045` and fork lineage cleanup `0046`. It intentionally does not create a DeliveryAttempt table; lease/attempt/accepted refs remain embedded in mailbox rows until WI-05 can split delivery attempts without double-writing partial state.

## WI-08 Ledger Entry 2026-07-05 / Worker D2

### Schema Changes

| Migration | Change | Decision mapping |
| --- | --- | --- |
| `crates/agentdash-infrastructure/migrations/0048_agent_run_lineage_baseline_refs.sql` | Adds `parent_frame_id`, `parent_frame_revision`, `child_frame_id`, and `child_frame_revision` to `agent_run_lineages`. | D-013: product fork uses the AgentRun fork record as canonical fact. D-017: frame baseline belongs on the retained child lineage table because duplicate replay, child lookup, parent audit, and fork provenance need a durable product record independent of RuntimeSession trace storage. |

### Redundant Table / Field Ledger

| Candidate | Conclusion | Canonical replacement or qualification |
| --- | --- | --- |
| Receipt `result_json` parent/child/redirect fork refs | Downgraded to idempotent outcome ref storage | Canonical parent/child fork refs come from `agent_run_lineages`; receipt state keeps command outcome and mailbox outcome refs for idempotent replay. |
| `agent_run_lineages.parent_runtime_session_id` / `agent_run_lineages.child_runtime_session_id` | Deletion remains covered by `0046_agent_run_lineage_product_refs.sql` | RuntimeSession lineage is trace provenance. Product fork replay reads `agent_run_lineages` by child run/agent and reconstructs parent/child AgentRun refs from that record. |
| `agent_run_lineages` | Retained as AgentRun child lineage table | The table has a unique child `(run_id, agent_id)` record, parent lookup index, fork owner, fixed fork point, and parent/child frame baseline fields needed for replay and audit. |

### Migration Risk For Merge

`0048_agent_run_lineage_baseline_refs.sql` sequences after runtime provenance columns were dropped in `0046` and after mailbox runtime ref renaming in `0047`. Main-session merge should keep migration numbering serialized with any concurrent WI-07 AgentFrame surface migration.

## R3a Ledger Entry 2026-07-05 / Database Physical Design Final Demolition

### Schema Changes

| Migration | Change | Decision mapping |
| --- | --- | --- |
| `crates/agentdash-infrastructure/migrations/0049_agent_frame_surface_document.sql` | Adds `agent_frames.surface`, backfills it from existing AgentFrame split surface columns, and drops redundant indexes `idx_agent_frames_agent_id`, `idx_agent_frame_transitions_run_phase`, and `idx_agent_run_mailbox_states_delivery_runtime_ref`. | D-011: AgentFrame revision owns capability/cognition surface. D-016/D-017: split columns do not qualify as independent facts and are downgraded to projections of one canonical surface document; indexes without live query, lock, claim, scan, or reverse-lookup qualification are deleted. |

### Redundant Table / Field / Index Ledger

| Candidate | Conclusion | Canonical replacement or qualification |
| --- | --- | --- |
| `agent_frames.surface` | Merged canonical document | Canonical AgentFrame revision surface now lives in one JSON document. Repository writes derive split physical columns from this document, and reads project the document back to old domain fields while application/API consumers are still being narrowed. |
| `agent_frames.effective_capability_json`, `context_slice_json`, `vfs_surface_json`, `mcp_surface_json`, `execution_profile_json`, `visible_canvas_mount_ids_json`, `visible_workspace_module_refs_json` | Downgraded to projection columns | These columns are no longer repository-level write sources. They are kept only because application/API consumers outside R3a still read field-shaped projections; future removal source is `agent_frames.surface`. |
| `agent_frame_transitions` | Kept for now | Session delivery still writes and reads it as part of `RuntimeCommandRecord`: `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:689` accepts the transition record, `:729` inserts `agent_frame_transitions`, `:777` inserts `runtime_session_delivery_commands.frame_transition_id`, `:807`/`:861` select command rows, and `:819`/`:873` join transitions by command FK. Reconstruction in `crates/agentdash-infrastructure/src/persistence/session_core.rs:148` reads both rows, validates delivery/transition identity at `:163`, and rebuilds the transition at `:193`. Final replacement path: move `target_frame_id`, `capability_keys_json`, `transition_json`, and turn linkage into the accepted input / ContextDeliveryRecord fact consumed by runtime delivery, then let `runtime_session_delivery_commands` reference that accepted fact directly before dropping this table. |
| `runtime_session_delivery_commands` | Kept | Runtime command status, retry/error timestamps, and delivery payload are updated independently from AgentFrame revisions and still need queryable pending/status rows. |
| `idx_agent_frames_agent_id` | Deleted | The unique `(agent_id, revision)` constraint already supports agent-scoped frame lookup and revision ordering; no independent query path requires a second `agent_id` index. |
| `idx_agent_frame_transitions_run_phase` | Deleted | No live repository query filters transitions by `(run_id, phase_node)`; delivery command reads join by `runtime_session_delivery_commands.frame_transition_id` to transition primary key. |
| `idx_agent_run_mailbox_states_delivery_runtime_ref` | Deleted | Mailbox state is keyed and updated by `(run_id, agent_id)`; nullable delivery runtime ref is trace evidence, not a reverse lookup path. |
| `idx_agent_frame_transitions_target_frame` | Kept | The FK from transition to target frame still uses this index for target-frame cleanup/audit while `agent_frame_transitions` remains a physical delivery command child fact. |
| `idx_agent_run_mailbox_messages_delivery_runtime_status` | Kept | Mailbox messages remain the recoverable AgentRun queue child table; delivery runtime/status scan is the retained diagnostic/recovery surface for message rows, unlike mailbox state runtime refs. |
| `idx_agent_run_lineages_parent_runtime` / `idx_agent_run_lineages_child_runtime` | Deletion remains covered by `0046_agent_run_lineage_product_refs.sql` | RuntimeSession lineage refs were trace provenance and are already removed from product fork storage. |

### Migration Risk For Merge

`0049_agent_frame_surface_document.sql` intentionally keeps split AgentFrame columns as projections because public/application readers outside this worker still consume those field names. The next schema demolition slice can delete the split columns after the application boundary reads `AgentFrameSurfaceDocument` directly and all direct `agent_frames` inserts bind `surface`.
