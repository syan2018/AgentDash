# Research: WI-08 Fork Lineage Current State

- Query: WI-08 Fork Lineage Baseline 当前代码事实清点
- Scope: mixed, internal code and Trellis task/spec artifacts only
- Date: 2026-07-04

## Title / Scope / Date

Title: WI-08 Fork Lineage Baseline 当前代码事实清点

Scope:
- 清点当前 product fork canonical facts 分别落在哪些文件、表、DTO、service。
- 区分哪些路径把 RuntimeSession lineage 当成 product fork 第一事实，哪些只是 internal trace provenance。
- 清点 duplicate replay 的 child refs/lineage 来源，并给出按 D-013 迁移到 canonical fork record 的最小方向。
- 判断 `agent_run_lineages` 是否应保留为 child table，以及保留时需要删除或降级的列、索引。
- 给出后续 WI-08 最小实现切片、并行冲突建议和预期验证命令。

Date: 2026-07-04

## Findings

### Files found

- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/prd.md`: D-013 目标产品语义，要求 product fork canonical fact 为 AgentRun 事实。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/design.md`: 当前 runtime-first fork 问题与目标 transaction 形态。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md`: D-013 决策正文。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-08-fork-lineage-baseline.md`: WI-08 范围、依赖、验收。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/research/fork-lineage-baseline.md`: 既有 fork baseline 研究。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/research/runtime-session-internal-model.md`: RuntimeSession internal model 研究。
- `.trellis/spec/backend/session/runtime-execution-state.md`: RuntimeSession、execution state、trace 的 spec 语义。
- `.trellis/spec/backend/workflow/architecture.md`: workflow 层边界和 AgentRun 语义。
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs`: 当前跨 AgentRun fork edge domain model。
- `crates/agentdash-domain/src/workflow/repository.rs`: `AgentRunLineageRepository` trait。
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql`: 当前 `agent_run_lineages` schema。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs`: `agent_run_lineages` repository 和 fork materialization transaction。
- `crates/agentdash-application-ports/src/agent_run_fork_materialization.rs`: application port for fork materialization。
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs`: AgentRun fork orchestration、receipt result cache、duplicate replay。
- `crates/agentdash-domain/src/workflow/command_receipt.rs`: AgentRun command receipt model。
- `crates/agentdash-application-agentrun/src/agent_run/command_receipt.rs`: receipt claim/digest helper。
- `crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql`: command receipt base table。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql`: receipt `result_json` addition。
- `crates/agentdash-infrastructure/migrations/0039_agent_run_command_receipt_fork_kinds.sql`: fork command kinds。
- `crates/agentdash-infrastructure/migrations/0042_agent_run_command_receipt_mailbox_move_kind.sql`: current receipt command kind check。
- `crates/agentdash-spi/src/session_persistence.rs`: RuntimeSession lineage SPI model and store trait。
- `crates/agentdash-application-runtime-session/src/session/branching.rs`: RuntimeSession fork implementation。
- `crates/agentdash-infrastructure/migrations/0001_init.sql`: `session_lineage` schema。
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`: RuntimeSession lineage persistence and delete behavior。
- `crates/agentdash-api/src/routes/sessions.rs`: internal session lineage diagnostic API。
- `crates/agentdash-contracts/src/agent/run_mailbox.rs`: AgentRun fork product contracts。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`: AgentRun fork API routes and response mapping。
- `packages/app-web/src/generated/agent-run-mailbox-contracts.ts`: generated product fork DTOs。
- `packages/app-web/src/services/agentRunMailbox.ts`: frontend product fork service calls。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts`: frontend redirect consumption。
- `packages/app-web/src/generated/session-contracts.ts`: generated RuntimeSession lineage DTOs。
- `packages/app-web/src/features/session/ui/SessionLineageView.tsx`: RuntimeSession lineage UI component。
- `crates/agentdash-contracts/src/runtime/workflow.rs`: same-run control tree DTO currently named `AgentRunLineageRef`。

### Related specs and decisions

- `prd.md:112` says fork 是 AgentRun 产品操作，`AgentRunForkRecord` 或等价 record 是唯一 product fork fact，包含 parent AgentRun、fixed turn/message boundary、child AgentRun、child baseline、fork owner；RuntimeSession lineage 只保留 internal trace provenance。
- `design.md:58` describes the current P0: product fork first creates a RuntimeSession fork, then backfills AgentRun; target is `AgentRunForkRecord` as the only product fork fact, with RuntimeSession attached as trace.
- `design.md:175` says fork baseline must have a single source of truth: parent AgentRun, message/turn boundary, child AgentRun, child baseline, fork owner; RuntimeSession lineage may remain internal trace provenance or derived data.
- `design.md:214-215` says product fork currently depends on Session lineage as a parallel fact; target is AgentRun fork transaction plus optional runtime trace provenance, and fork receipt `result_json` must not store child refs/lineage as the fact source.
- `decisions.md:187-196` D-013 says product fork uses `AgentRunForkRecord` as the only fact; fork replay reads fork record, not receipt result cache; `agent_run_lineages` must not force `runtime_session_id` to become product lineage identity.
- `WI-08-fork-lineage-baseline.md:19-24` requires defining canonical `AgentRunForkRecord`, making product fork transaction write AgentRun fork fact first, downgrading RuntimeSession lineage, and keeping receipt `result_json` to idempotent outcome refs only.
- `WI-08-fork-lineage-baseline.md:28-34` excludes WI-03 admission implementation and WI-06 current delivery selection implementation, but depends on their semantics.
- `work-items/README.md:12-18` orders WI-03 before later lifecycle convergence and marks WI-08 as dependent on WI-03/WI-06 semantics.
- `work-items/README.md:90-101` warns not to parallelize shared migrations, generated contracts, public application service constructors, or current delivery semantics changes without coordination.
- `WI-03-agentrun-admission-boundary.md:5-16` defines the desired admission boundary: AgentRun start and fork materialization should be atomic product admission instead of being pieced together across API/services.
- `WI-06-delivery-binding-anchor.md:5-18` defines current delivery as binding/state, not mutable evidence anchor; WI-08 currently reads parent baseline through this area.

## Current Facts With File:Line Evidence

### 1. `AgentRunForkRecord` does not exist in code

`AgentRunForkRecord` is currently a target concept from task/design docs, not an implemented domain type or table.

Evidence:
- `decisions.md:187` names `AgentRunForkRecord` in D-013.
- `design.md:58` and `prd.md:112` define it as desired canonical product fork fact.
- `work-items/WI-08-fork-lineage-baseline.md:5` and `work-items/WI-08-fork-lineage-baseline.md:19` scope WI-08 around introducing it.
- Code search found no definition under `crates/` or `packages/`.

Current equivalent implementation is distributed across:
- `AgentRunLineage` domain model and `agent_run_lineages` table.
- `AgentRunForkService` runtime-first orchestration.
- `CommandReceipt.result_json` cache.
- RuntimeSession `SessionLineageRecord` and projection fork.
- Product contracts and frontend redirect consumption.

### 2. `agent_run_lineages` is the closest product fork child table, but it is runtime-coupled

Domain model:
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:8-12` says same-run `AgentLineage` remains the control tree while this model links a forked child AgentRun back to its parent AgentRun/runtime trace boundary.
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:12` defines `pub struct AgentRunLineage`.
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:13-18` stores `id`, parent run/agent ids, child run/agent ids, and `relation_kind`.
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:19-28` stores `fork_point_event_seq`, `fork_point_ref_json`, `parent_runtime_session_id`, `child_runtime_session_id`, `forked_by_user_id`, `metadata_json`, `created_at`.
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:33-55` creates fork lineage through `new_fork`, hardcoding `relation_kind: "fork"` and copying parent/child runtime session ids.

Schema:
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1-17` creates `agent_run_lineages`.
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:10-11` makes both `parent_runtime_session_id` and `child_runtime_session_id` `text NOT NULL`.
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:15` enforces `CHECK (relation_kind = 'fork')`.
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:16` makes `(child_run_id, child_agent_id)` unique.
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:20-30` adds indexes for parent, child, parent runtime session, and child runtime session.

Repository:
- `crates/agentdash-domain/src/workflow/repository.rs:136-148` exposes `AgentRunLineageRepository` with `create`, `find_parent`, `list_children`, and `list_by_run`.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:39-94` implements the repository using child lookup, parent lookup, and run lookup.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:267-320` inserts all columns including `relation_kind`, fork point fields, parent/child runtime session ids, fork owner, and metadata.

Materialization:
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:97-103` implements `PostgresAgentRunForkMaterialization`.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:110-168` materializes a child run/agent/frame, creates an anchor from the child runtime session, binds current delivery, and creates `AgentRunLineage::new_fork`.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:128-140` copies parent frame capability/context/VFS/MCP/execution/canvas/workspace fields into the child frame.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:183-211` performs one SQL transaction for lifecycle run, lifecycle agent, agent frame, anchor upsert, and `agent_run_lineages` insert.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:419-436` upserts an anchor with `ON CONFLICT (runtime_session_id) DO UPDATE`, which overlaps WI-06 because anchor evidence is still mutable by runtime session id.

Application port:
- `crates/agentdash-application-ports/src/agent_run_fork_materialization.rs:8-18` defines input from parent run/agent/frame, parent runtime session id, child runtime session id, fork point, fork owner, and metadata.
- `crates/agentdash-application-ports/src/agent_run_fork_materialization.rs:21-25` returns child run, child agent, child frame, and lineage.
- `crates/agentdash-application-ports/src/agent_run_fork_materialization.rs:51-55` defines `materialize_forked_agent_run`.

Interpretation:
- `agent_run_lineages` already has the useful parent-owned child-table shape for product fork queries.
- It is not D-013 compliant because runtime session ids are mandatory columns and indexed as first-class lookup dimensions.
- It also lacks explicit canonical baseline snapshot fields such as parent frame id/revision, parent projection version/head event seq/active compaction id, child baseline snapshot refs, and receipt/client command linkage.

### 3. `fork.rs` currently creates runtime fork first, then product materialization

Dependencies and orchestration:
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:8` imports `SessionBranchingService` and `SessionForkRequest`.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:36-44` `AgentRunForkRepos` includes lifecycle repos, `execution_anchor_repo`, `command_receipt_repo`, `mailbox_repo`, `agent_run_lineage_repo`, and `agent_run_fork_materialization`.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:199-204` resolves the parent run/agent/frame/runtime delivery before forking.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:205-219` builds a request digest from user id, parent run/agent/frame/runtime session id, fork point, metadata/input/executor/backend selection.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:229-239` claims an `agent_run_fork` command receipt scoped by current user, parent run, and parent agent.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:250-251` sends duplicate receipts to `replay_duplicate`.

Runtime-first branch:
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:254-268` calls `session_branching.fork_session` with the parent runtime session id before any canonical AgentRun fork record exists.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:281-287` marks the receipt terminal failed if runtime fork fails.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:291-310` then calls `agent_run_fork_materialization.materialize_forked_agent_run`, using `fork_result.child_session.id` and `fork_result.lineage.fork_point_event_seq`.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:322-329` cleans up the child runtime session and marks receipt terminal failed if product materialization fails.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:568-570` cleanup deletes the child RuntimeSession through `session_core.delete_session`.

Parent baseline source:
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:511-565` resolves the parent through `DeliveryRuntimeSelectionService::select_current_delivery`.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:611-617` stores parent run, agent, frame, runtime session id, and selection in `ResolvedForkParent`.

Mailbox and receipt updates:
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:343-361` accepts a child mailbox message for fork-submit and schedules on submit.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:390-405` attaches mailbox message id to the receipt separately.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:406-419` marks the receipt accepted separately.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:420-430` stores the fork result JSON separately.

Interpretation:
- Product fork currently materializes after RuntimeSession fork and projection work.
- The product transaction is only the later DB materialization for lifecycle rows plus `agent_run_lineages`; it is not the full fork admission transaction required by D-013.

### 4. Receipt `result_json` currently stores fork refs and lineage as replay facts

Result cache construction:
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:714-748` builds `fork_result_json` with outcome, parent refs, child refs, full lineage object, mailbox data, and redirect refs.
- The stored lineage object includes `id`, parent/child run and agent ids, `relation_kind`, `fork_point_event_seq`, `fork_point_ref`, parent/child runtime session ids, fork owner, metadata, and created timestamp.

Result cache readers:
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:763-792` reads accepted refs from `result_json`.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:795-834` reconstructs `AgentRunLineage` from `result_json`, defaulting missing `relation_kind` to `fork`, missing runtime session ids to empty strings, and missing created time to now.

Duplicate replay:
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:457-491` handles duplicate replay by checking receipt status, requiring `record.result_json`, reading parent refs from JSON, child refs from `record.accepted_refs`, and lineage from JSON.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:492-507` loads mailbox message by receipt mailbox id when present and derives mailbox outcome from the message or result JSON.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:1093-1119` tests duplicate explicit fork replay and asserts child refs and lineage id match the first result.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:1123-1151` tests pending duplicate conflict before runtime branch creates children.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:1155-1190` tests terminal failed duplicate replay without creating a new child.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:1194-1220` tests materialization failure cleanup and failed receipt.

Receipt model/schema:
- `crates/agentdash-domain/src/workflow/command_receipt.rs:40-58` defines `AgentRunCommandKind` including `AgentRunFork` and `AgentRunForkSubmit`.
- `crates/agentdash-domain/src/workflow/command_receipt.rs:101-111` defines `AgentRunCommandReceipt` fields including `mailbox_message_id`, `accepted_refs`, and `result_json`.
- `crates/agentdash-domain/src/workflow/command_receipt.rs:129-142` defines created/duplicate claim states.
- `crates/agentdash-domain/src/workflow/command_receipt.rs:147-169` exposes `claim`, `mark_accepted`, `attach_mailbox_message`, and `store_result_json`.
- `crates/agentdash-application-agentrun/src/agent_run/command_receipt.rs:37-63` wraps receipt claim and flags duplicate receipts.
- `crates/agentdash-application-agentrun/src/agent_run/command_receipt.rs:108-120` canonicalizes digest JSON and hashes it.
- `crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:1-31` creates base receipt table and unique idempotency scope.
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:34` adds `result_json`.
- `crates/agentdash-infrastructure/migrations/0039_agent_run_command_receipt_fork_kinds.sql:5-15` adds fork receipt kinds.
- `crates/agentdash-infrastructure/migrations/0042_agent_run_command_receipt_mailbox_move_kind.sql:5-17` keeps fork kinds in the current command kind check.

Interpretation:
- `result_json` is currently a product fact cache for duplicate replay.
- D-013 requires replay to read canonical fork record; receipt result JSON should only carry an idempotent outcome pointer/ref, not reconstruct product lineage.

### 5. RuntimeSession lineage is implemented as a runtime trace tree

SPI model:
- `crates/agentdash-spi/src/session_persistence.rs:693-708` defines `SessionLineageRelationKind` with `Fork`, `Companion`, `SpawnedAgent`, and `RollbackBranch`.
- `crates/agentdash-spi/src/session_persistence.rs:758-772` defines `SessionLineageRecord` with child/parent session ids, relation kind, fork point event/ref, compaction id, status, timestamps, and metadata. It has no product run id, agent id, fork owner, receipt id, or child AgentRun baseline.
- `crates/agentdash-spi/src/session_persistence.rs:919-947` defines `SessionLineageStore` operations for upsert, get, list children, list ancestors/descendants, and set status.
- `crates/agentdash-spi/src/session_persistence.rs:953-960` includes lineage store inside the broad `SessionPersistence` trait.

Runtime fork service:
- `crates/agentdash-application-runtime-session/src/session/branching.rs:27-31` returns child session, lineage record, and projection commit from `SessionForkResult`.
- `crates/agentdash-application-runtime-session/src/session/branching.rs:65-162` resolves fork point, builds parent model context, creates child session metadata, creates `SessionLineageRecord`, creates child session, upserts session lineage, commits initial fork projection, and returns result.
- `crates/agentdash-application-runtime-session/src/session/branching.rs:124-132` creates the child session then upserts session lineage, deleting the child session if lineage write fails.
- `crates/agentdash-application-runtime-session/src/session/branching.rs:135-147` commits the child fork projection and deletes the child session if projection write fails.
- `crates/agentdash-application-runtime-session/src/session/branching.rs:257-280` exposes `lineage_view` and `lineage_parent` over the runtime lineage store.

Postgres persistence:
- `crates/agentdash-infrastructure/migrations/0001_init.sql:587-598` creates `session_lineage` with child/parent session ids, relation kind, fork point fields, compaction id, status, metadata, and self-check.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1162-1164` indexes fork point and parent/status/kind.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1251-1258` adds FKs for child/parent sessions and compaction.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:1143-1217` upserts session lineage with cycle checks and `ON CONFLICT(child_session_id) DO UPDATE`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:1240-1265` lists session children.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:1270-1299` lists session ancestors.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:1301-1341` lists session descendants.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:1343-1368` updates lineage status.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:264-312` deletes session lineage rows by child or parent runtime session during `delete_session`.

Diagnostic API and UI:
- `crates/agentdash-api/src/routes/sessions.rs:104-105` registers `/sessions/{id}/lineage`.
- `crates/agentdash-api/src/routes/sessions.rs:921-947` labels session lineage route as internal diagnostics, checks session permission, calls `session_branching.lineage_view`, and returns `SessionLineageViewResponse`.
- `packages/app-web/src/generated/session-contracts.ts:21-27` exposes generated session lineage DTOs and relation kinds.
- `packages/app-web/src/features/session/ui/SessionLineageView.tsx:1-7` imports session lineage types.
- `packages/app-web/src/features/session/ui/SessionLineageView.tsx:13-22` labels relation kinds.
- `packages/app-web/src/features/session/ui/SessionLineageView.tsx:97-168` renders the lineage panel.
- `packages/app-web/src/features/session/ui/index.ts:46-48` exports the lineage UI.

Interpretation:
- RuntimeSession lineage is well-shaped as internal trace/provenance.
- It becomes a D-013 problem only where AgentRun fork orchestration treats `fork_session` as the first materialized product fork step.

### 6. Product fork contracts and frontend consume AgentRun fork routes, not raw session lineage

Contracts:
- `crates/agentdash-contracts/src/agent/run_mailbox.rs:276-290` defines `AgentRunMessageCommandResponse` with optional `fork: Option<AgentRunForkOutcomeView>`.
- `crates/agentdash-contracts/src/agent/run_mailbox.rs:295-305` defines `AgentRunForkRequest`.
- `crates/agentdash-contracts/src/agent/run_mailbox.rs:310-327` defines `AgentRunForkSubmitRequest`.
- `crates/agentdash-contracts/src/agent/run_mailbox.rs:332-345` defines `AgentRunForkLineageView` with id, parent, child, relation kind, fork point, fork owner, and created time.
- `crates/agentdash-contracts/src/agent/run_mailbox.rs:349-355` defines `AgentRunForkOutcomeView`.
- `crates/agentdash-contracts/src/agent/run_mailbox.rs:359-365` defines `AgentRunForkResponse`.

API:
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:118-123` registers `/agent-runs/{run_id}/agents/{agent_id}/fork` and `/fork-submit`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:781-786` maps explicit fork to `Json<AgentRunForkResponse>`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:865-890` maps fork-submit request into `AgentRunForkSubmitCommand`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:2395-2404` constructs the fork service with `session_branching`, `session_core`, and mailbox service dependencies.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:2407-2422` maps fork-submit to `AgentRunMessageCommandResponse` with nested `fork`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:2425-2434` maps explicit fork response.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:2437-2459` builds `AgentRunForkOutcomeView` and sets redirect to child refs.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:2631-2686` tests that fork response preserves redirect and lineage.

Frontend product route consumption:
- `packages/app-web/src/generated/agent-run-mailbox-contracts.ts:25-31` defines generated product fork DTOs.
- `packages/app-web/src/generated/agent-run-mailbox-contracts.ts:45` defines `AgentRunMessageCommandResponse.fork`.
- `packages/app-web/src/services/agentRunMailbox.ts:26-34` posts explicit fork requests to `/fork`.
- `packages/app-web/src/services/agentRunMailbox.ts:37-45` posts fork-submit requests to `/fork-submit`.
- `packages/app-web/src/services/agentRunMailbox.test.ts:82-94` tests explicit fork API call shape using a stable runtime message ref.
- `packages/app-web/src/services/agentRunMailbox.test.ts:97-110` tests fork-submit API call shape.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:112-120` resolves redirect from `response.fork?.redirect`.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:137-154` calls fork service, fetches the child lifecycle run, and redirects to child run/agent.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:309-324` handles composer fork-submit redirect.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:530-542` wires explicit fork from message ref.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.test.ts:70-74` tests composer fork redirect.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.test.ts:77-102` tests explicit fork from stable `MessageRef` redirects to child.

Interpretation:
- Frontend product flow already consumes AgentRun fork contracts and child redirect, not `/sessions/{id}/lineage`.
- The backend response is still backed by the current runtime-first/materialization/result-cache facts, not by a canonical fork record.
- `AgentRunForkLineageView.relation_kind` is redundant for a fork-specific DTO.

### 7. Same-run control tree has a confusing `AgentRunLineageRef` name

Evidence:
- `crates/agentdash-contracts/src/runtime/workflow.rs:1374-1409` defines `AgentRunWorkspaceView` with `parent: Option<AgentRunLineageRef>` and `children: Vec<AgentRunLineageRef>`.
- `crates/agentdash-contracts/src/runtime/workflow.rs:1653-1668` comments say this is a lineage control tree hop whose `relation_kind` comes from `AgentLineage`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:401-404` populates workspace parent/children through `resolve_agent_run_lineage`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:565` defines `resolve_agent_run_lineage`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:569` reads `agent_lineage_repo.list_by_run(run.id)`, not `AgentRunLineageRepository`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:645` builds `AgentRunLineageRef`.
- `packages/app-web/src/generated/workflow-contracts.ts:155-163` mirrors the workspace parent/children DTO.

Interpretation:
- `AgentRunLineageRef` in workflow contracts is same-run `AgentLineage` control-tree data, not cross-run product fork lineage.
- WI-08 can note this as DTO naming debt. Renaming may conflict with frontend/generated contract work, so it should be scoped carefully.

### 8. Current delivery and admission are direct WI-08 dependencies

Current delivery:
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:127-137` defines `LifecycleAgentCurrentDeliveryBinding`.
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:171-187` stores `current_delivery` on `LifecycleAgent`.
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:235-247` binds current delivery from an anchor and updates the agent.
- `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:144-163` reads `agent.current_delivery`, loads anchor by runtime session id, and validates anchor coordinates.
- `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:164-188` reads the current frame and returns runtime selection.
- `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:439-465` tests current delivery selection from binding.
- `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:468-489` tests missing binding errors.
- `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:493-521` tests anchor mismatch rejection.
- `crates/agentdash-infrastructure/migrations/0017_lifecycle_agent_current_delivery_binding.sql:1-8` adds current delivery columns to `lifecycle_agents`.
- `crates/agentdash-infrastructure/migrations/0017_lifecycle_agent_current_delivery_binding.sql:27-31` indexes current delivery runtime session.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:50-56` includes current delivery columns in `AgentRow`.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:61-97` parses current delivery row fields and rejects partial bindings.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:167-174` and `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:184-224` persist current delivery fields.
- `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs:289-365` binds current delivery after accepted launch commit.

Admission:
- `WI-03-agentrun-admission-boundary.md:5-16` requires an atomic AgentRun admission boundary for run/agent/frame/anchor/mailbox/receipt creation.
- `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:46-55` defines `ProjectAgentRunStartCommand`.
- `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:58-84` maps initial mailbox command with `schedule_on_submit: false`.
- `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:88-99` returns runtime session id, turn id, run id, agent id, frame id, receipt, and initial message.
- `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:102-118` shows current start service still depends on many repos and runtime/session/materialization collaborators.
- `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:2639-2664` tests durable initial mailbox envelope without synchronous scheduling.

Interpretation:
- WI-08 should not stabilize fork baseline against the current LifecycleAgent-owned delivery binding if WI-06 is still moving that semantic boundary.
- WI-08 also should not invent a one-off fork admission transaction that conflicts with WI-03's shared admission boundary.

## RuntimeSession Lineage: Product First Fact vs Internal Trace

### Paths that currently treat RuntimeSession lineage/projection as product fork first fact

- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:254-268`: `AgentRunForkService::execute` calls `SessionBranchingService::fork_session` before writing any canonical product fork record.
- `crates/agentdash-application-runtime-session/src/session/branching.rs:65-162`: `fork_session` resolves fork point, builds context/projection baseline, writes child RuntimeSession, writes `session_lineage`, commits fork projection, then returns child session and lineage.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:291-310`: product materialization consumes `fork_result.child_session.id` and `fork_result.lineage.fork_point_event_seq` from the runtime fork result.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:110-168`: product materialization adopts the already-created child RuntimeSession into child AgentRun, child frame, anchor, and `AgentRunLineage`.
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:10-11`: `agent_run_lineages` requires parent and child runtime session ids.
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:26-30`: runtime session ids are indexed directly on the product fork table.
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:457-491`: duplicate replay reconstructs product child refs/lineage from receipt JSON generated after runtime-first fork, not from the product child table.

### Paths that are internal trace provenance

- `crates/agentdash-spi/src/session_persistence.rs:758-772`: `SessionLineageRecord` contains only runtime session lineage metadata, fork point, compaction, status, and metadata.
- `crates/agentdash-api/src/routes/sessions.rs:921-947`: `/sessions/{id}/lineage` is documented as an internal diagnostic route.
- `packages/app-web/src/features/session/ui/SessionLineageView.tsx:97-168`: session lineage UI displays runtime trace relations.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:264-312`: runtime session deletion removes related `session_lineage` rows with session lifecycle, consistent with trace data.

## Fork Duplicate Replay: Current Source and D-013 Target

Current source:
- Receipt duplicate handling starts at `crates/agentdash-application-agentrun/src/agent_run/fork.rs:457-491`.
- Parent refs are read from `record.result_json`.
- Child refs are read from `record.accepted_refs`.
- Lineage is reconstructed by `lineage_from_result_json` at `crates/agentdash-application-agentrun/src/agent_run/fork.rs:795-834`.
- Mailbox outcome is reconstructed from receipt mailbox id or `result_json` at `crates/agentdash-application-agentrun/src/agent_run/fork.rs:492-507`.

D-013 target:
- Receipt duplicate handling should use receipt status and a stable pointer/ref, such as `fork_record_id` or equivalent, then load canonical `AgentRunForkRecord`.
- `result_json` may cache idempotent outcome refs, but must not be the source for child AgentRun refs, lineage id, runtime lineage fields, or baseline facts.
- Replay response mapping should be built from canonical record plus mailbox message state when needed.

Minimal migration path:
1. Add canonical fork record lookup by receipt id or fork record id.
2. Store only `fork_record_id` and optional mailbox outcome summary in receipt `result_json`.
3. Change `replay_duplicate` to load record from repository and map to `AgentRunForkOutcome`.
4. Remove `lineage_from_result_json` once all replay paths use the record.
5. Keep `accepted_refs` only as receipt outcome summary if needed by generic receipt semantics; do not treat it as fork lineage source.

## Mismatches vs D-013

- Missing canonical type/table: `AgentRunForkRecord` does not exist in code.
- Runtime-first ordering: `fork.rs` creates RuntimeSession fork, `session_lineage`, and projection before product fork fact.
- Split transaction boundary: RuntimeSession creation, product materialization, mailbox append, receipt acceptance, and result JSON update happen as separate operations.
- Receipt cache as fact: duplicate replay reconstructs child refs/lineage from receipt `result_json`.
- Runtime ids as product identity: `agent_run_lineages.parent_runtime_session_id` and `child_runtime_session_id` are `NOT NULL` and indexed.
- Redundant relation kind: `agent_run_lineages.relation_kind` is constrained to only `fork`, and `AgentRunForkLineageView` also carries `relation_kind`.
- Materialization lacks baseline snapshot completeness: current child frame copy and runtime projection commit are split; `agent_run_lineages` does not contain an explicit canonical baseline snapshot sufficient for replay/audit.
- Current delivery dependency is unsettled: parent runtime/frame baseline is resolved through `LifecycleAgent.current_delivery`, while WI-06 is supposed to move current delivery into a dedicated binding/state shape.
- Naming debt: workflow `AgentRunLineageRef` names same-run `AgentLineage` control tree data, which can be confused with cross-run fork lineage.

## Table / Repository / DTO Classification

| Item | Current role | D-013 classification |
| --- | --- | --- |
| `AgentRunForkRecord` | Not implemented. Exists only in PRD/design/decision/work item text. | Target canonical product fork fact. |
| `AgentRunLineage` domain model | Current cross-run fork edge with parent/child AgentRun ids plus runtime session ids. | Either rename/reshape into `AgentRunForkRecord`, or keep as implementation backing table with D-013-compliant fields. |
| `agent_run_lineages` table | Current product child edge table, but runtime-coupled by required runtime ids and indexes. | Should be retained only if it becomes the canonical child table for product fork record; runtime ids downgraded to trace refs. |
| `AgentRunLineageRepository` | Public create/read repo for cross-run fork lineage. | Read methods can remain; create should be hidden behind fork admission/transaction port. |
| `PostgresAgentRunForkMaterialization` | Current transaction for child run/agent/frame/anchor/lineage after runtime fork. | Should move behind canonical fork transaction/admission; must not rely on runtime fork as first fact. |
| `SessionLineageRecord` / `session_lineage` | RuntimeSession trace/provenance relation and diagnostic lineage. | Internal trace only; not product fork identity. |
| `SessionBranchingService::fork_session` | Runtime branch implementation and currently first product fork step. | Runtime helper to attach trace/projection after canonical AgentRun fork fact exists. |
| `AgentRunCommandReceipt.result_json` | Stores full fork outcome, refs, and lineage for duplicate replay. | Idempotency result pointer/cache only, not canonical lineage source. |
| `AgentRunForkLineageView` | Product fork response lineage DTO. | Keep as product DTO backed by canonical fork record; remove/reduce redundant `relation_kind` if contract cleanup is in scope. |
| `AgentRunForkResponse` / `AgentRunForkOutcomeView` | Product fork response and redirect contract. | Correct product-facing shape, but must be backed by canonical fork record. |
| Frontend `forkAgentRun` and workspace command redirect | Product flow calls AgentRun fork routes and redirects to child AgentRun. | Mostly correct consumer; update generated DTOs if backend contract changes. |
| Frontend `SessionLineageView` | Runtime lineage diagnostic UI component. | Internal trace UI only; should not be product fork source. |
| `AgentRunLineageRef` in workflow workspace DTO | Same-run control tree reference backed by `AgentLineage`. | Naming debt; should be renamed separately or within contract cleanup with coordination. |

## Should `agent_run_lineages` remain as a child table?

Recommendation: keep the table concept as the product fork child table, but reshape it into a D-013 canonical fork record. The current table already supports important product queries: find parent by child, list children by parent, and list all edges for a run. Those are product-level AgentRun workspace queries and should not move to RuntimeSession lineage.

Required removals or downgrades if retained:
- Remove `relation_kind`, or make it a generated/read-only constant outside persisted schema. The table is fork-only today via `CHECK (relation_kind = 'fork')`.
- Make `parent_runtime_session_id` and `child_runtime_session_id` nullable trace refs, or move them into trace metadata/snapshot fields. They must not be product identity.
- Drop or downgrade `idx_agent_run_lineages_parent_runtime` and `idx_agent_run_lineages_child_runtime` unless a diagnostic/maintenance query is explicitly retained. Runtime lineage queries should use `session_lineage` or a trace-specific projection.
- Add canonical baseline fields: parent frame id/revision or delivery snapshot id, fork point event/ref, parent projection version/head event seq/active compaction id where required, child baseline refs, fork owner, and receipt/client command linkage.
- Restrict writes through the fork transaction/admission boundary. General `AgentRunLineageRepository::create` should not remain a public write path.
- Keep read APIs for product navigation: `find_parent(child)`, `list_children(parent)`, and `list_by_run(run)`.

## Minimal Implementation Slices

### Slice 1: canonical record shape and migration

- Introduce `AgentRunForkRecord` domain shape, or rename/reshape `AgentRunLineage` if minimizing churn.
- Evolve `agent_run_lineages` toward the canonical record table:
  - Add explicit baseline snapshot columns needed by fork replay/audit.
  - Add receipt/client command linkage if replay will look up by receipt.
  - Remove or deprecate `relation_kind`.
  - Downgrade runtime session id columns from mandatory identity fields to trace refs.
  - Drop or downgrade runtime session indexes.
- Coordinate migration numbering and generated contracts with WI-12 expectations.

Dependency:
- Do not choose a final parent baseline field shape until WI-06 current delivery semantics are stable.

### Slice 2: repository/port boundary

- Add `AgentRunForkRecordRepository` or a clearly named fork record read repository.
- Keep read methods for parent/child/run queries.
- Move record creation behind a fork transaction/admission port instead of public `AgentRunLineageRepository::create`.
- Ensure duplicate replay can load canonical fork record by receipt id or stored fork record id.

Dependency:
- Align with WI-03 admission so fork materialization does not invent a separate long-lived transaction boundary.

### Slice 3: duplicate replay reads canonical record

- Change `fork.rs` duplicate replay so receipt `result_json` points to the fork record instead of storing full child refs/lineage.
- Build replay response from canonical fork record plus mailbox state.
- Remove `lineage_from_result_json` once replay no longer needs it.
- Keep receipt status/idempotency checks intact.

Dependency:
- Requires Slice 1/2 record lookup.

### Slice 4: fork transaction order

- Change product fork flow so canonical AgentRun fork fact is written before RuntimeSession trace/projection is attached.
- RuntimeSession `fork_session` should become trace/projection helper called from the product fork transaction boundary or a clearly sequenced post-record attach step.
- If runtime trace attach fails, failure handling should update product fork state deliberately instead of erasing the only product fact.

Dependency:
- Strongly depends on WI-03 admission boundary and WI-06 delivery binding semantics.

### Slice 5: DTO and frontend cleanup

- Keep product fork response/redirect behavior stable for frontend.
- Back `AgentRunForkLineageView` from canonical record.
- Remove/reduce redundant `relation_kind` if contract cleanup is accepted.
- Decide whether confusing same-run `AgentRunLineageRef` should be renamed in this work item or deferred to a separate contract cleanup.
- Regenerate TypeScript contracts and update frontend tests when DTOs change.

Dependency:
- Contract/generated file edits should not run in parallel with other workers changing API DTOs.

## Parallelization and Conflict Advice

Do not edit these files in parallel with other workers unless ownership is explicitly coordinated:
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs`: WI-08 core orchestration; overlaps WI-03 admission and WI-04 mailbox/receipt work.
- `crates/agentdash-application-ports/src/agent_run_fork_materialization.rs`: fork materialization port; likely overlaps admission port work.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs`: product fork materialization and `agent_run_lineages` SQL.
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs`: current cross-run fork model.
- `crates/agentdash-domain/src/workflow/repository.rs`: repository trait surface.
- `crates/agentdash-domain/src/workflow/command_receipt.rs`: receipt model shared with mailbox/admission work.
- `crates/agentdash-contracts/src/agent/run_mailbox.rs`: generated product fork DTO source.
- `crates/agentdash-contracts/src/runtime/workflow.rs`: same-run `AgentRunLineageRef` contract source.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`: fork route wiring and response mapping.
- `packages/app-web/src/generated/*`: generated contract outputs.
- `packages/app-web/src/services/agentRunMailbox.ts`: frontend fork API calls.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts`: fork redirect consumption.
- `crates/agentdash-application-runtime-session/src/session/branching.rs`: RuntimeSession branch/trace implementation.
- `crates/agentdash-spi/src/session_persistence.rs`: RuntimeSession lineage SPI.
- Migrations touching `agent_run_lineages`, command receipts, lifecycle current delivery, or `session_lineage`.

Parallel-safe areas:
- Additional research docs under the same task's `research/` directory.
- Test-only work that does not change shared generated contracts, migrations, or fork/admission/current-delivery service constructors.
- UI-only diagnostic work around `SessionLineageView` if it does not alter contracts or product fork flow.

WI-08 sequencing guidance:
- WI-08 should not make final baseline-field choices before WI-06 defines current delivery binding/state semantics.
- WI-08 should not introduce a separate fork admission abstraction that conflicts with WI-03.
- WI-08 can work after WI-04后半段 only if command receipt/mailbox surfaces are stable and no shared migration/contract files are edited concurrently.

## Validation Commands Expected for WI-08

Expected backend validation:

```powershell
cargo test -p agentdash-application-agentrun agent_run::fork
cargo test -p agentdash-application-runtime-session session::branching
cargo test -p agentdash-infrastructure agent_run_lineage_repository
cargo test -p agentdash-infrastructure agent_run_command_receipt_repository
cargo check -p agentdash-api
```

Expected API/contract validation:

```powershell
cargo test -p agentdash-api lifecycle_agents
cargo test -p agentdash-contracts
```

Expected frontend validation when contracts or redirect behavior change:

```powershell
pnpm --filter app-web test -- agentRunMailbox
pnpm --filter app-web test -- useAgentRunWorkspaceCommands
```

Expected migration/static checks when `agent_run_lineages` or receipt schema changes:

```powershell
rg "lineage_from_result_json|fork_result_json|result_json.*lineage" crates/agentdash-application-agentrun/src/agent_run/fork.rs
rg "parent_runtime_session_id|child_runtime_session_id|idx_agent_run_lineages_parent_runtime|idx_agent_run_lineages_child_runtime" crates/agentdash-infrastructure/migrations crates/agentdash-infrastructure/src/persistence
rg "AgentRunForkRecord|AgentRunLineage|agent_run_lineages" crates packages
```

Use the repository's migration guard/generation command if WI-08 adds migrations or changes contracts; this research did not verify an exact command name.

## External References

- None. This was an internal code/spec/task artifact audit only.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task. This research used the task path and output file explicitly provided by the user.
- No code definition of `AgentRunForkRecord` was found under `crates/` or `packages/`.
- No direct frontend service call to `/sessions/{id}/lineage` was found in the product fork path; session lineage UI/contracts exist as diagnostic/runtime lineage surfaces.
- Line evidence is from current workspace files at research time; no tests or validation commands were run because the request was research-only and forbids code edits.
