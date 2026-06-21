# Research: RC02 implementation scope

- Query: Runtime Coordinate 收敛任务中，RC02 `DeliveryRuntimeSelectionService` 的实现级范围、数据模型、首批写入面、后续依赖顺序与验证命令。
- Scope: internal
- Date: 2026-06-21

## Findings

### Planning context

- `.trellis/tasks/06-21-runtime-coordinate-convergence/prd.md`: 目标是统一 AgentRun 当前执行链路、RuntimeSessionExecutionAnchor selection、SubjectExecutionView history 与 AgentRun resource surface 坐标；AgentRun 应持有或可唯一解析 current delivery binding；repository raw latest 不表达业务语义。
- `.trellis/tasks/06-21-runtime-coordinate-convergence/design.md`: current delivery binding 决策落在 `LifecycleAgent` 粒度；第一版不新增独立 binding 表；policy surface 包含 `CurrentDelivery`、`RunScopedLatest`、`LaunchPrimary`、`SubjectLatestObserved`。
- `.trellis/tasks/06-21-runtime-coordinate-convergence/implement.md`: Phase 1 要先锁定 binding 字段、selection service 输入输出错误语义、raw latest API 边界；Phase 2 才迁移 workspace/cancel/mailbox；Phase 3 才做 SubjectExecutionView history 与 resource surface coordinate。
- `.trellis/tasks/06-21-runtime-coordinate-convergence/work-items/index.md`: RC02 当前为 `ready`，RC04/RC05/RC06/RC07/RC08 均 `blocked_by_RC02`。
- `.trellis/tasks/06-21-module-topology-coupling-review/design-coupling-tracker.md`: D02/D03 已决定由 `LifecycleAgent` current delivery binding + application-level selection service 统一 owner；D12 要让 SubjectExecution history 成为 latest 的来源；D15 仍 open，resource surface DTO 要表达 surface source coordinate。

### Files found

- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs`: `LifecycleAgent` entity 当前只有 `current_frame_id`，`set_current_frame` 会更新该字段和 `updated_at`。
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs`: `RuntimeSessionExecutionAnchor` 是 runtime session 到 run/agent/launch frame/orchestration node 的 launch evidence；已有旧的 `RuntimeDeliverySelectionPolicy`，但它仍是 anchor-first `Specific | LaunchPrimary | LatestAttached`，不符合新 policy surface。
- `crates/agentdash-domain/src/workflow/repository.rs`: `LifecycleAgentRepository` 只有 create/get/list/update；`RuntimeSessionExecutionAnchorRepository::latest_updated_anchor_for_agent` 注释已限定为 raw `updated_at DESC`。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`: Postgres `LifecycleAgent` row mapping、insert/update/select 都只包含 `current_frame_id`；同文件实现 anchor repository raw ordering 查询。
- `crates/agentdash-infrastructure/migrations/0001_init.sql`: `lifecycle_agents` 表只有 `current_frame_id`；`runtime_session_execution_anchors` 表保存 `runtime_session_id/run_id/launch_frame_id/agent_id/.../updated_at`。
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs`: dispatch/materialization 创建 initial frame、写 `agent.current_frame_id`、再 upsert anchor，是 current delivery binding 的首个写入边界。
- `crates/agentdash-application/src/session/launch/commit.rs`: connector accepted 后才写新的 AgentFrame revision 并推进 `current_frame_id`；connector setup/planner 失败测试已要求不能提前推进 current frame。
- `crates/agentdash-application/src/agent_run/workspace/query.rs`: workspace 当前自行用 run anchors 的 max `updated_at` 选择 delivery runtime，并用 anchor launch frame 投影 lifecycle VFS surface。
- `crates/agentdash-application/src/agent_run/workspace/command_policy.rs`: command policy 也自行用 run anchors 的 max `updated_at` 做 frame fallback，并把 route context 的 runtime id 写入 stale guard。
- `crates/agentdash-application/src/agent_run/mailbox.rs`: mailbox command 在缺 `message_stream` 时直接调用 `latest_updated_anchor_for_agent`；已有 `AgentRunMailboxCommandTarget` 可承载 run/agent/frame + optional message stream。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`: composer/mailbox/cancel route 从 `resolve_agent_run_context` 取得 `delivery_runtime_session_id` 后直接投递或 cancel。
- `crates/agentdash-application/src/lifecycle/run_view_builder.rs`: `SubjectExecutionView` 当前只有 `latest_runtime_node`，latest 是从 associations/runs/current_frame_id/anchors 派生，没有 execution history list。
- `crates/agentdash-contracts/src/runtime/workflow.rs`: browser-facing DTO 已有 `AgentRunWorkspaceControlPlaneStatus`、`AgentRunView.delivery_runtime_ref`、`SubjectExecutionView.latest_runtime_node`，尚无 selection output 或 execution history DTO。
- `crates/agentdash-application/src/test_support/workflow_repositories.rs`: memory `LifecycleAgentRepository` 和 `RuntimeSessionExecutionAnchorRepository` 是 RC02 单元测试可复用的 test double。

### Code patterns

- `LifecycleAgent` 当前字段和 mutator：`current_frame_id` 在 `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:95`，`set_current_frame` 在 `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:131`。
- Anchor entity 当前表达 launch evidence：`runtime_session_id/run_id/launch_frame_id/agent_id/orchestration_id/node_path/node_attempt` 在 `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29`。
- 旧 `RuntimeDeliverySelectionPolicy` 是 anchor-first 且包含 `LatestAttached`，位置在 `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:11`；RC02 应替换或降级该语义，避免继续暴露业务 `latest`。
- `RuntimeSessionExecutionAnchorRepository::latest_updated_anchor_for_agent` 的 raw ordering 注释在 `crates/agentdash-domain/src/workflow/repository.rs:161`，Postgres 实现在 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:930`。
- Postgres `LifecycleAgent` row mapping 当前只 roundtrip `current_frame_id`：row 字段在 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:40`，insert 在 `:80`，select get/list 在 `:102`/`:114`，update 在 `:127`。
- 初始 dispatch 写 frame/current_frame/anchor：plain dispatch 在 `crates/agentdash-application/src/lifecycle/dispatch_service.rs:567`、`:571`、`:592`；graph dispatch 在 `:473`、`:477`、`:497`；workflow AgentCall materialization 在 `:396`、`:405`、`:414`。
- Accepted turn 推进 current frame 的边界在 `crates/agentdash-application/src/session/launch/commit.rs:147`；pending frame path 在 `:160`-`:188`，regular accepted revision path 在 `:200`-`:241`。
- 现有测试已经保护“connector accepted 前失败不能推进 current_frame_id”：`crates/agentdash-application/src/session/hub/tests.rs:3178`、`:3215`、`:3307`。
- Workspace current delivery 选择重复逻辑：`delivery_runtime_session_for_agent_run` 在 `crates/agentdash-application/src/agent_run/workspace/query.rs:314`，直接 `list_by_run` + filter agent + max `updated_at`。
- Workspace resource surface 同时混用 current frame 与 anchor launch frame：`resolve_agent_run_frame_vfs` 在 `crates/agentdash-application/src/agent_run/workspace/query.rs:327`；surface address 使用 `anchor.launch_frame_id` 在 `:355`-`:363`。
- Command policy frame ref fallback 重复 anchor max：`crates/agentdash-application/src/agent_run/workspace/command_policy.rs:188`。
- Mailbox target fallback 用 raw latest：`crates/agentdash-application/src/agent_run/mailbox.rs:1813`；runtime id 路径通过 `find_by_session` 反查控制面在 `:1848`。
- Cancel route 直接从 route context 的 runtime id 调 `session_runtime.cancel`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:701`、`:736`。
- SubjectExecutionView latest 当前从 anchors + current frame 派生：`crates/agentdash-application/src/lifecycle/run_view_builder.rs:332`；DTO 只有 `latest_runtime_node` 在 `crates/agentdash-contracts/src/runtime/workflow.rs:1277`。

### RC02 data model

#### Domain value object

Add a domain value object in `agentdash-domain/src/workflow/lifecycle_agent.rs` or a sibling module re-exported by `workflow/mod.rs`:

```rust
pub struct LifecycleAgentCurrentDeliveryBinding {
    pub runtime_session_id: String,
    pub launch_frame_id: Uuid,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
    pub status: DeliveryBindingStatus,
    pub observed_at: DateTime<Utc>,
}
```

`run_id`, `agent_id`, and `current_frame_id` should be present in the selection output, but not duplicated inside the persisted binding object unless the implementer chooses a flat entity field layout. They already exist as `LifecycleAgent.run_id`, `LifecycleAgent.id`, and `LifecycleAgent.current_frame_id`; duplicating them in storage would create another consistency surface.

Add this field to `LifecycleAgent`:

```rust
pub current_delivery: Option<LifecycleAgentCurrentDeliveryBinding>
```

If the implementer prefers flat fields for SQL ergonomics, use these entity fields and provide `current_delivery()` / `set_current_delivery(...)` helpers so application code still consumes a single binding:

- `current_delivery_runtime_session_id: Option<String>`
- `current_delivery_launch_frame_id: Option<Uuid>`
- `current_delivery_orchestration_id: Option<Uuid>`
- `current_delivery_node_path: Option<String>`
- `current_delivery_node_attempt: Option<u32>`
- `current_delivery_status: Option<DeliveryBindingStatus>`
- `current_delivery_observed_at: Option<DateTime<Utc>>`

The helper should reject partially populated bindings when reading from persistence. A partial binding is a data error, not an implicit fallback to anchor latest.

#### Status enum

Add `DeliveryBindingStatus` with wire/persistence slugs:

```rust
#[serde(rename_all = "snake_case")]
pub enum DeliveryBindingStatus {
    Ready,
    Running,
    Terminal,
    Lost,
    FrameMissing,
    DeliveryMissing,
}
```

Semantics:

- `ready`: binding exists and current AgentRun can accept mailbox/user commands; no active turn is required.
- `running`: connector accepted/current turn is active or starting for this runtime session.
- `terminal`: bound runtime session reached completed/failed/interrupted/cancelled and remains the last current delivery binding.
- `lost`: delivery/runtime carrying surface is known lost, reserved for D16 projection work; RC02 can define and roundtrip it but should not implement backend disconnect projection.
- `frame_missing`: binding references a current/launch frame that no longer resolves; selection returns this as typed failure.
- `delivery_missing`: no current delivery binding exists for a policy that requires one; selection returns this as typed failure.

Do not model `canceling` here in RC02. Existing workspace/control projections already express `Cancelling` by inspecting `SessionExecutionState`; the binding status remains a delivery binding fact, not live control state.

#### Selection policy enum

Create a new application-level enum, not a repository enum:

```rust
pub enum DeliveryRuntimeSelectionPolicy {
    CurrentDelivery { run_id: Uuid, agent_id: Uuid },
    RunScopedLatest { run_id: Uuid, agent_id: Option<Uuid> },
    LaunchPrimary { run_id: Uuid, agent_id: Uuid },
    SubjectLatestObserved { subject: SubjectRef },
}
```

Ownership:

- Define the policy and service in `agentdash-application/src/agent_run/delivery_runtime_selection.rs` or `agentdash-application/src/lifecycle/delivery_runtime_selection.rs`.
- Keep `RuntimeSessionExecutionAnchorRepository` as raw evidence access only.
- Existing domain `RuntimeDeliverySelectionPolicy::{Specific, LaunchPrimary, LatestAttached}` should be removed if unused, or renamed to an internal raw anchor query helper if still needed by tests. It should not be the public business policy enum.

#### Selection output struct

Minimum service output:

```rust
pub struct DeliveryRuntimeSelection {
    pub policy: DeliveryRuntimeSelectionPolicy,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub current_frame_id: Uuid,
    pub launch_frame_id: Uuid,
    pub runtime_session_id: String,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
    pub status: DeliveryBindingStatus,
    pub observed_at: DateTime<Utc>,
    pub address: AgentRunRuntimeAddress,
    pub message_stream: MessageStreamProjectionRef,
    pub anchor: RuntimeSessionExecutionAnchor,
}
```

Notes:

- `current_frame_id` is the AgentRun control-plane current frame.
- `launch_frame_id` is the anchor/evidence launch frame.
- `address.frame_id` should use `current_frame_id` for AgentRun workspace/control commands.
- `anchor.launch_frame_id` remains available for launch-evidence surface and trace drill-down.
- Do not fetch `SessionMeta` or inspect live runtime state inside RC02 selection. Selection resolves target identity; workspace/cancel/mailbox can continue to inspect runtime execution state after selection.

#### Error semantics

Use a typed application error that can later map cleanly into existing `WorkflowApplicationError` / API errors:

```rust
pub enum DeliveryRuntimeSelectionError {
    RunNotFound { run_id: Uuid },
    AgentNotFound { agent_id: Uuid },
    AgentRunMismatch { run_id: Uuid, agent_id: Uuid, actual_run_id: Uuid },
    CurrentDeliveryMissing { run_id: Uuid, agent_id: Uuid },
    BindingIncomplete { run_id: Uuid, agent_id: Uuid, field: &'static str },
    AnchorMissing { runtime_session_id: String },
    AnchorMismatch { runtime_session_id: String, expected_run_id: Uuid, expected_agent_id: Uuid, actual_run_id: Uuid, actual_agent_id: Uuid },
    CurrentFrameMissing { agent_id: Uuid },
    CurrentFrameNotFound { frame_id: Uuid },
    LaunchFrameNotFound { frame_id: Uuid },
    SubjectNotFound { subject: SubjectRef },
    SubjectDeliveryMissing { subject: SubjectRef },
}
```

Mapping recommendation:

- Missing run/agent/subject: `NotFound`.
- Missing current delivery for CurrentDelivery: `Conflict` for command surfaces and nullable/diagnostic projection for read surfaces.
- Incomplete binding or anchor mismatch: `Conflict` with diagnostic detail; this is inconsistent control-plane data, not fallback territory.
- Repository/database errors: preserve as internal/application error.

### Minimum first write set for RC02

RC02 should be implementation-ready but narrow. First batch should not migrate workspace/cancel/mailbox consumers yet; it should make the single service and binding persistent/covered.

1. Domain entity/model:
   - Add `LifecycleAgentCurrentDeliveryBinding` and `DeliveryBindingStatus`.
   - Add `LifecycleAgent.current_delivery` or flat equivalent fields.
   - Add helpers:
     - `bind_current_delivery_from_anchor(&mut self, anchor: &RuntimeSessionExecutionAnchor, status: DeliveryBindingStatus, observed_at: DateTime<Utc>)`
     - `clear_current_delivery(&mut self, status: DeliveryBindingStatus, observed_at: DateTime<Utc>)` only if a terminal/lost clearing path is explicitly implemented; otherwise skip.
     - `current_delivery_binding(&self) -> Result<Option<LifecycleAgentCurrentDeliveryBinding>, DomainError>` if flat fields are used.
2. Migration:
   - Add `crates/agentdash-infrastructure/migrations/0017_lifecycle_agent_current_delivery_binding.sql`.
   - Add nullable columns to `lifecycle_agents`:
     - `current_delivery_runtime_session_id text`
     - `current_delivery_launch_frame_id text`
     - `current_delivery_orchestration_id text`
     - `current_delivery_node_path text`
     - `current_delivery_node_attempt integer`
     - `current_delivery_status text`
     - `current_delivery_observed_at timestamp with time zone`
   - Add a check constraint for status in `ready/running/terminal/lost/frame_missing/delivery_missing`.
   - Add an index on `(run_id, id, current_delivery_runtime_session_id)` or at least `(current_delivery_runtime_session_id)` for anchor cross-check/debug.
   - A one-time deterministic backfill is optional. If done, use raw `updated_at DESC` only inside the migration to seed pre-existing dev rows, and document that runtime code must not use raw latest as fallback.
3. Postgres repository roundtrip:
   - Extend `AgentRow`, `TryFrom<AgentRow>`, insert/select/update queries in `PostgresLifecycleAgentRepository`.
   - Ensure partial SQL rows fail loudly during read conversion rather than silently producing `None`.
4. Test support:
   - Existing `MemoryLifecycleAgentRepository` stores whole entity values, so it will work after struct update; adjust test fixture constructors to include `current_delivery: None`.
5. Application service:
   - Add `DeliveryRuntimeSelectionService` using `LifecycleRunRepository`, `LifecycleAgentRepository`, `AgentFrameRepository`, `RuntimeSessionExecutionAnchorRepository`, and `LifecycleSubjectAssociationRepository` if `SubjectLatestObserved` is implemented in RC02.
   - Implement policies:
     - `CurrentDelivery`: read run, agent, agent.current_delivery, anchor by bound runtime id, current frame, launch frame; validate all ids.
     - `RunScopedLatest`: read raw anchors by run (and optional agent), order as repository returns or max by `updated_at`; return selection with explicit policy. This remains diagnostics/transition only, not current command target.
     - `LaunchPrimary`: read raw anchors by agent/run, min `created_at` or stable launch ordering; return selection.
     - `SubjectLatestObserved`: if fully implemented, reuse current SubjectExecutionView runtime projection logic and return latest observed selection. If not, define the enum but return `Unsupported/SubjectDeliveryMissing` and leave RC07 to implement history.
6. Binding write points:
   - In `LifecycleDispatchService` dispatch/materialization paths, after anchor upsert succeeds, set `agent.current_delivery` from that anchor with `DeliveryBindingStatus::Ready` and update agent. Existing code currently updates `current_frame_id` before anchor upsert; RC02 should either reorder to update agent once after anchor success or make sure failed anchor upsert does not leave a current delivery binding without anchor evidence.
   - In `TurnCommitter::commit_accepted_agent_frame`, after accepted frame and `current_frame_id` update succeeds, re-read the bound anchor by `session_id` and update `current_delivery` with `DeliveryBindingStatus::Running`, preserving `launch_frame_id` from anchor and `current_frame_id` from agent.
   - Do not update binding on connector setup/planner failure. Existing tests at `session/hub/tests.rs:3178` and `:3307` should remain green and should gain binding assertions.

### Tests for RC02

Minimum tests:

- Domain unit:
  - `LifecycleAgent::bind_current_delivery_from_anchor` copies runtime id, launch frame id, orchestration node coordinate, status and observed_at.
  - Binding helper rejects partial flat field rows if flat fields are used.
  - `DeliveryBindingStatus` slug roundtrips.
- Infrastructure repository:
  - `PostgresLifecycleAgentRepository` create/get/update/list roundtrip includes current delivery binding.
  - Partial row failure test if existing infra test harness supports direct SQL insert.
  - Migration check verifies added columns and status constraint.
- Application selection service:
  - `CurrentDelivery` returns run/agent/current_frame/launch_frame/runtime_session/message_stream/address and validates anchor matches.
  - `CurrentDelivery` returns `CurrentDeliveryMissing` when agent has no binding.
  - `CurrentDelivery` returns `AnchorMissing` when bound runtime id has no anchor.
  - `CurrentDelivery` returns `AnchorMismatch` when anchor points to different run/agent.
  - `CurrentDelivery` returns `CurrentFrameMissing` or `CurrentFrameNotFound` for missing current frame.
  - `RunScopedLatest` is explicit policy only and can select the newest raw anchor without changing agent binding.
  - `LaunchPrimary` selects earliest launch evidence for the agent/run.
- Launch/dispatch integration-style application tests:
  - Plain dispatch writes `current_frame_id` and current delivery binding from the same created anchor.
  - Graph dispatch / workflow AgentCall materialization writes orchestration coordinate into current delivery binding.
  - Accepted turn commits a new `current_frame_id` and updates binding status to `running` without replacing `launch_frame_id`.
  - Connector setup failure and planner invalid config leave current delivery binding unchanged, mirroring existing current_frame tests.

### Focused validation commands

Use focused commands first:

```powershell
cargo test -p agentdash-domain lifecycle_agent_current_delivery
cargo test -p agentdash-application delivery_runtime_selection
cargo test -p agentdash-application accepted_turn_commits_agent_frame_revision_and_current_frame
cargo test -p agentdash-application connector_setup_failure_leaves_current_frame_unchanged
cargo test -p agentdash-application planner_invalid_config_leaves_current_frame_unchanged
cargo test -p agentdash-infrastructure lifecycle_agent_current_delivery
cargo check -p agentdash-application
cargo check -p agentdash-infrastructure
```

Run broader checks before handing to RC04+:

```powershell
cargo test -p agentdash-application agent_run
cargo test -p agentdash-application lifecycle
cargo test -p agentdash-domain workflow
cargo check -p agentdash-api
```

`pnpm run contracts:check` is not required for RC02 if no browser-facing DTO changes are made. It becomes required in RC07/RC08 when `SubjectExecutionView` history or resource surface coordinate DTOs are added.

### RC04-RC08 dependency and recovery execution order

After RC02 lands and tests pass, resume in this order:

1. RC04 Workspace query migration:
   - Replace `AgentRunWorkspaceQueryService::delivery_runtime_session_for_agent_run` and `resolve_agent_run_frame_vfs` raw anchor selection with `DeliveryRuntimeSelectionService::select(CurrentDelivery)`.
   - Replace command policy frame/runtime guard source with the same selection.
   - Keep `runtime_refs_for_agent` as history/list raw refs; it is not current selection.
   - This should be first because it is the user-visible read model that supplies stale guards for later commands.
2. RC05 Cancel / subject control migration:
   - Change cancel route/context resolution to select `CurrentDelivery` at command time.
   - Use selection output for request digest, policy context, accepted refs, and `session_runtime.cancel`.
   - Subject execution control should consume `SubjectLatestObserved` only after RC07 if it needs history; otherwise use `CurrentDelivery` for AgentRun commands.
3. RC06 Mailbox delivery target migration:
   - Replace mailbox API command construction with `AgentRunMailboxCommandTarget` from `CurrentDelivery`.
   - Replace mailbox service `None => latest_updated_anchor_for_agent` fallback with selection service; keep runtime-session-adapter path for anchored delegate callbacks.
   - Preserve hook delegate behavior where unbound traces can route direct fallback; anchored traces with missing mailbox target should stay diagnostic/error.
4. RC07 SubjectExecutionView execution history:
   - Add an internal `SubjectExecutionAttemptView`/history read model derived from associations + runs + anchors + runtime node evidence.
   - Derive `latest_runtime_node` from the same history list, not from a parallel latest helper.
   - Add contract DTO + generated TS + `pnpm run contracts:check`.
5. RC08 AgentRun resource surface coordinate contract:
   - Add DTO fields that distinguish current frame VFS source from anchor launch-frame/source coordinate.
   - Reuse RC02 selection output so resource surface does not independently select anchors.
   - This should follow RC04 and RC07 because workspace current surface and subject history semantics determine which coordinates are exposed.

### Related specs

- `.trellis/spec/backend/architecture.md`: application owns use-case orchestration; domain owns entities/repository traits; infrastructure implements persistence.
- `.trellis/spec/backend/directory-structure.md`: new domain entity/repository changes belong in `agentdash-domain`, persistence in `agentdash-infrastructure`, use-case/service in `agentdash-application`.
- `.trellis/spec/backend/session/architecture.md`: RuntimeSession is delivery/trace substrate; AgentRun control commands use AgentRun workspace public identity; runtime trace callbacks use `RuntimeSessionExecutionAnchor` as evidence.
- `.trellis/spec/backend/session/runtime-execution-state.md`: `SessionRuntimeRegistry` hook runtime is a delivery binding cache; business owner remains `HookControlTarget { run_id, agent_id, frame_id }`; AgentRun workspace is built from AgentRun runtime address.
- `.trellis/spec/backend/session/agentrun-mailbox.md`: mailbox is the durable AgentRun message intake and scheduler; route-local launch/queue/steer must not be authority.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: browser DTO changes must live in `agentdash-contracts` and generated TS; RC02 can avoid contract changes, RC07/RC08 cannot.

### External references

- None. This is an internal architecture/data-flow research task. No third-party API or version lookup was needed.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task. The user explicitly provided the target task directory and output path, so this research used `.trellis/tasks/06-21-runtime-coordinate-convergence/` as the write boundary.
- No existing `DeliveryRuntimeSelectionService` implementation was found. Current behavior is spread across workspace query, command policy, mailbox service, route context resolution, and SubjectExecutionView projector.
- No existing persisted current delivery binding fields were found on `LifecycleAgent`; only `current_frame_id` exists.
- The existing `RuntimeDeliverySelectionPolicy` in domain is not the RC02 policy surface. It is anchor-first and includes `LatestAttached`; keeping it as-is would confuse the owner boundary.
- RC02 should not fully implement RC04/RC05/RC06 consumer migration. It can add the service and tests, then later work items replace each consumer.
- RC02 should not implement SubjectExecutionView history DTO or resource surface coordinate DTO. Those require browser contract changes and are explicitly RC07/RC08.
- RC02 can define `lost` as a binding status, but backend disconnect lost projection belongs to the runtime failure/placement D16 task and should not be inferred here.
