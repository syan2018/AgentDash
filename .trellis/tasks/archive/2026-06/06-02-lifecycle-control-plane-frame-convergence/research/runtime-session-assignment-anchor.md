# Research: RuntimeSession Assignment Anchor

- Query: RuntimeSession -> Frame -> Assignment -> Activity terminal 链路中哪些地方仍依赖启发式反查？直接 anchor 的目标 repository/API/DTO 可以如何设计？
- Scope: internal
- Date: 2026-06-02

## Findings

### Files Found

- `.trellis/workflow.md` — Trellis planning/research workflow，要求 research 输出持久化到任务目录。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/prd.md` — 父任务目标，明确 Session 降级为 turn/runtime supervision，Frame/Assignment 承载执行锚点。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/design.md` — 目标 data flow：RuntimeSession terminal 应通过 runtime session execution anchor 直达 AgentAssignment。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/implement.md` — Phase 1 要设计并实现 runtime session 到 frame / assignment 的直接锚定查询或实体。
- `.trellis/spec/backend/session/execution-context-frames.md` — `ExecutionContext` 是 connector projection，不是 application 事实源。
- `.trellis/spec/backend/session/runtime-execution-state.md` — Session runtime 只回答 turn claim/active/cancel/cleanup 与 terminal effect，不承担 owner/context/VFS/MCP 解析。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` — Activity attempt key 必须包含 `graph_instance_id + activity_key + attempt`，AgentAssignment 是 Agent/Frame 到 attempt 的标准桥。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` — RuntimeSession 是 runtime trace container；trace 反查可以从 RuntimeSession 到 AgentFrame 再到 association。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` — 前端运行观察应通过 LifecycleRun/WorkflowGraphInstance/LifecycleAgent/AgentFrameRuntimeView，不以 session id 作为 lifecycle 主索引。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — 浏览器消费 DTO 应由 `agentdash-contracts` 生成，不在前端长期手写跨层 DTO。
- `crates/agentdash-domain/src/workflow/agent_assignment.rs` — `AgentAssignment` 实体已经保存 `run_id + graph_instance_id + activity_key + attempt + agent_id + frame_id`。
- `crates/agentdash-domain/src/workflow/agent_frame.rs` — `AgentFrame` 用 `runtime_session_refs_json` 保存 delivery/trace refs，并提供 runtime session selection policy。
- `crates/agentdash-domain/src/workflow/repository.rs` — repository trait 目前有 `AgentFrameRepository.find_by_runtime_session`、`AgentAssignmentRepository.find_for_attempt`、`list_by_run`，但没有 runtime session -> assignment 的 direct query。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` — Postgres frame lookup 用 JSON containment 查 `runtime_session_refs_json`，assignment lookup 支持 attempt key 和 run scan。
- `crates/agentdash-application/src/workflow/session_association.rs` — RuntimeSession terminal/advance 的核心 resolver，目前先查 frame，再 list run assignments，再选择。
- `crates/agentdash-application/src/workflow/orchestrator.rs` — `on_session_terminal` 和 `advance_current_activity` 都调用 runtime session association resolver。
- `crates/agentdash-application/src/workflow/projection.rs` — session 与 hook target projection 也复用 `select_assignment_for_runtime_frame`。
- `crates/agentdash-application/src/hooks/workflow_snapshot.rs` — hook control target 从 session 解析 assignment 仍 list run assignments 并 select。
- `crates/agentdash-application/src/workflow/agent_executor.rs` — Agent activity launch 创建 assignment、frame 和 runtime session，但只把 runtime session ref 附到 frame JSON，没有写入 runtime session -> assignment anchor。
- `crates/agentdash-application/src/workflow/execution_log.rs` — output port map 仍从 run 级 inline container `port_outputs` 读取。
- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs` — lifecycle journey 仍按 run/container/path 写读 port outputs。
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs` — lifecycle VFS artifact 路径当前按 run scoped `artifacts/{port_key}` 读写。
- `crates/agentdash-api/src/routes/lifecycle_views.rs` — API 有 `/agent-frames/{id}/runtime` 和 `/sessions/{id}/trace`，session trace 仍先 `find_by_runtime_session`。
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts` — 前端通过遍历本地 frames 和 fallback current frame 猜 session 对应 frame。
- `packages/app-web/src/services/lifecycle.ts` — 前端已有 fetch frame runtime 与 runtime trace service，但缺少 session -> frame runtime 的直接 endpoint/service。
- `crates/agentdash-contracts/src/workflow.rs` — 现有 `RuntimeSessionTraceView` 只有 optional `frame_ref`，`AgentFrameRuntimeView` 没有 assignment/attempt anchor。
- `crates/agentdash-infrastructure/migrations/0073_lifecycle_target_anchors.sql` — 目标 anchor 表已建，但 `agent_assignments` 缺少 unique attempt 约束和 runtime session anchor 表/列。

### Code Patterns

`AgentAssignment` 已经是精确 Activity attempt anchor。实体字段包含 run、graph instance、activity、attempt、agent、frame，注释也说明目标 key 必须包含 `graph_instance_id + activity_key + attempt`，用于 scheduler 精确定位 attempt（`crates/agentdash-domain/src/workflow/agent_assignment.rs:5`、`crates/agentdash-domain/src/workflow/agent_assignment.rs:10`、`crates/agentdash-domain/src/workflow/agent_assignment.rs:12`）。

`AgentFrame` 的 runtime session refs 当前是 JSON 数组，不是结构化关联表。注释声明 `runtime_session_refs_json` 是 trace/delivery refs，不是 subject association；`select_runtime_session_id` 只能按 `Specific` / `LaunchPrimary` / `LatestAttached` 从数组取 session id（`crates/agentdash-domain/src/workflow/agent_frame.rs:22`、`crates/agentdash-domain/src/workflow/agent_frame.rs:46`、`crates/agentdash-domain/src/workflow/agent_frame.rs:140`）。

Repository trait 缺少 direct anchor API。`AgentFrameRepository.find_by_runtime_session` 返回 frame；`AgentAssignmentRepository.find_for_attempt` 只按 attempt key 查；`list_by_run` 仍是 run 级扫描入口（`crates/agentdash-domain/src/workflow/repository.rs:121`、`crates/agentdash-domain/src/workflow/repository.rs:133`、`crates/agentdash-domain/src/workflow/repository.rs:145`）。

Postgres `find_by_runtime_session` 对 `runtime_session_refs_json::jsonb` 做 containment 查询，并按 `created_at DESC LIMIT 1` 取最新 frame。这是典型 JSON/ref-list 反查，不能表达 assignment_id、attempt 或 delivery role（`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:509`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:520`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:523`）。

RuntimeSession terminal/advance resolver 目前链路是 `find_by_runtime_session -> agent_repo.get -> assignment_repo.list_by_run -> select_assignment_for_runtime_frame`。这会把 runtime session -> frame 解析、run 内 assignment 扫描和 heuristic selection 串在 terminal 主路径上（`crates/agentdash-application/src/workflow/session_association.rs:121`、`crates/agentdash-application/src/workflow/session_association.rs:127`、`crates/agentdash-application/src/workflow/session_association.rs:151`、`crates/agentdash-application/src/workflow/session_association.rs:159`）。

`select_assignment_for_runtime_frame` 是当前最核心的启发式。它先找 active + same agent，再优先 exact `frame_id`，其次用 current frame 的 `graph_instance_id + activity_key` 匹配，最后在 active_for_agent 只有一条时兜底接受；多条则报 ambiguous（`crates/agentdash-application/src/workflow/session_association.rs:196`、`crates/agentdash-application/src/workflow/session_association.rs:200`、`crates/agentdash-application/src/workflow/session_association.rs:206`、`crates/agentdash-application/src/workflow/session_association.rs:213`、`crates/agentdash-application/src/workflow/session_association.rs:237`）。

`on_session_terminal` 通过 `resolve_activity_session_association` 推进 ActivityEvent；`complete_lifecycle_node` 的 `advance_current_activity` 也同样从 `runtime_session_id` 反查 association。两条 terminal/advance 路径共享同一启发式源头（`crates/agentdash-application/src/workflow/orchestrator.rs:133`、`crates/agentdash-application/src/workflow/orchestrator.rs:138`、`crates/agentdash-application/src/workflow/orchestrator.rs:186`、`crates/agentdash-application/src/workflow/orchestrator.rs:190`）。

Hook control target 也有同类反查：从 session 找 frame、取 agent、`list_by_run` assignments，再 `select_assignment_for_runtime_frame` 得到 optional assignment id（`crates/agentdash-application/src/hooks/workflow_snapshot.rs:82`、`crates/agentdash-application/src/hooks/workflow_snapshot.rs:86`、`crates/agentdash-application/src/hooks/workflow_snapshot.rs:102`、`crates/agentdash-application/src/hooks/workflow_snapshot.rs:107`）。

Agent activity launch 生成了足够写 direct anchor 的事实：新建 frame 时绑定 graph/activity，新建 assignment 后，再创建 runtime session 并 attach 到 frame refs。但此处只更新 frame JSON，没有持久化 runtime session -> assignment 关系（`crates/agentdash-application/src/workflow/agent_executor.rs:375`、`crates/agentdash-application/src/workflow/agent_executor.rs:393`、`crates/agentdash-application/src/workflow/agent_executor.rs:422`、`crates/agentdash-application/src/workflow/agent_executor.rs:434`）。

Continue/reuse frame 路径也会创建 assignment，但 target 只验证 frame 已绑定 delivery runtime session；没有为该 runtime session 写入 assignment anchor。因此同一个 root session 承接多个 sequential activity 时，terminal/advance 更容易依赖当前 frame activity scope 或 single active assignment 兜底（`crates/agentdash-application/src/workflow/agent_executor.rs:228`、`crates/agentdash-application/src/workflow/agent_executor.rs:241`、`crates/agentdash-application/src/workflow/agent_executor.rs:279`）。

Session launch 自身仍多次用 `find_by_runtime_session` 作为 frame lookup。启动后还会基于 session id 找 current frame 写新 frame revision，且该新 revision 未显式复制 graph/activity/runtime refs 的代码路径可疑；这会增加 current frame 与 launch frame 的漂移风险（`crates/agentdash-application/src/session/launch/orchestrator.rs:287`、`crates/agentdash-application/src/session/launch/orchestrator.rs:290`、`crates/agentdash-application/src/session/launch/orchestrator.rs:301`）。

Output artifact 仍是 run scoped。`load_port_output_map` 从 `InlineFileOwnerKind::LifecycleRun + run_id + "port_outputs"` 加载，port key 直接作为 path；completion gate 用这个 run 级 map 判断 required ports，并构造 ActivityCompleted outputs（`crates/agentdash-application/src/workflow/execution_log.rs:166`、`crates/agentdash-application/src/workflow/execution_log.rs:171`、`crates/agentdash-application/src/workflow/orchestrator.rs:243`、`crates/agentdash-application/src/workflow/orchestrator.rs:250`、`crates/agentdash-application/src/workflow/orchestrator.rs:268`）。

Lifecycle journey/VFS 同样 run scoped 写读 port outputs。`write_port_output(run_id, port_key, content)` 写入 `LifecycleRun / port_outputs / port_key`；VFS `artifacts/{port_key}` 调用该接口（`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:261`、`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:280`、`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:296`、`crates/agentdash-application/src/vfs/provider_lifecycle.rs:418`、`crates/agentdash-application/src/vfs/provider_lifecycle.rs:440`）。

API 层已有 session trace endpoint，但返回的是 trace view，不是 frame runtime view。`/sessions/{id}/trace` 仍通过 JSON frame ref 反查 frame，并只返回 optional `frame_ref`、events、turns（`crates/agentdash-api/src/routes/lifecycle_views.rs:37`、`crates/agentdash-api/src/routes/lifecycle_views.rs:41`、`crates/agentdash-api/src/routes/lifecycle_views.rs:112`、`crates/agentdash-api/src/routes/lifecycle_views.rs:117`、`crates/agentdash-api/src/routes/lifecycle_views.rs:152`）。

`AgentFrameRuntimeView` 目前包含 frame ref、procedure、graph/activity、capability/context/VFS/MCP/runtime refs，但不包含 assignment ref、attempt ref 或 anchor source，前端无法直接知道 session 对应哪个 assignment/attempt（`crates/agentdash-contracts/src/workflow.rs:742`、`crates/agentdash-contracts/src/workflow.rs:751`、`crates/agentdash-contracts/src/workflow.rs:764`）。

`AgentAssignmentRefDto` 目前只含 assignment/run/agent/frame，缺少 graph_instance_id、activity_key、attempt。`ActivityAttemptView` 反过来含 graph/activity/attempt 和 optional assignment_ref，但 assignment ref 本身不能独立表达完整 attempt key（`crates/agentdash-contracts/src/workflow.rs:616`、`crates/agentdash-contracts/src/workflow.rs:618`、`crates/agentdash-contracts/src/workflow.rs:667`、`crates/agentdash-contracts/src/workflow.rs:676`）。

前端 `useSessionRuntimeState` 明确通过遍历 `lifecycleStore.frames` 的 `runtime_session_refs` 查 frame，如果没有命中还 fallback 到任意 agent `current_frame_id`。这是前端本地 cache 推断 session -> frame 的直接证据（`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:69`、`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:78`、`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:83`、`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:99`）。

Migration 0073 已给 `agent_assignments(graph_instance_id, activity_key, attempt)` 建索引，但不是 unique，也没有 `run_id`/`runtime_session_id` direct anchor。`agent_frames` 只保存 `runtime_session_refs_json` 文本列，没有 runtime session ref 表（`crates/agentdash-infrastructure/migrations/0073_lifecycle_target_anchors.sql:54`、`crates/agentdash-infrastructure/migrations/0073_lifecycle_target_anchors.sql:65`、`crates/agentdash-infrastructure/migrations/0073_lifecycle_target_anchors.sql:81`、`crates/agentdash-infrastructure/migrations/0073_lifecycle_target_anchors.sql:97`）。

### Remaining Heuristic Reverse Lookups

1. **RuntimeSession -> AgentFrame**: `AgentFrameRepository.find_by_runtime_session` scans JSON refs and chooses latest created frame. This is acceptable only as a trace adapter, not as an execution authority.
2. **AgentFrame -> AgentAssignment**: `select_assignment_for_runtime_frame` guesses by exact frame id, then graph/activity scope, then single active assignment. The last two branches are heuristic, especially when frame revisions change or one agent handles multiple attempts.
3. **HookControlTarget assignment_id**: hook runtime target resolution still repeats the same run scan + selection path and can return target without assignment_id.
4. **Frontend session -> frame**: `useSessionRuntimeState` derives frame from locally cached frame refs, then falls back to any agent current frame.
5. **Activity output lookup**: completion gate/artifact binding still reads run-level `port_outputs/{port_key}`, so graph instance/activity/attempt are inferred from the currently resolved association rather than encoded in artifact identity.
6. **Runtime trace DTO**: `/sessions/{id}/trace` returns optional frame_ref but not the authoritative assignment/attempt anchor, preserving the need for callers to do additional inference.

### Target Repository Design

Preferred shape is a first-class runtime delivery/assignment anchor rather than more JSON scanning:

```rust
pub struct RuntimeSessionExecutionAnchor {
    pub runtime_session_id: String,
    pub frame_id: Uuid,
    pub agent_id: Uuid,
    pub assignment_id: Option<Uuid>,
    pub run_id: Uuid,
    pub graph_instance_id: Option<Uuid>,
    pub activity_key: Option<String>,
    pub attempt: Option<i32>,
    pub delivery_role: RuntimeSessionDeliveryRole,
    pub attached_at: DateTime<Utc>,
}

pub enum RuntimeSessionDeliveryRole {
    ActivityAttempt,
    AgentSurface,
    ContinueRoot,
    TraceOnly,
}

#[async_trait]
pub trait RuntimeSessionExecutionAnchorRepository: Send + Sync {
    async fn attach_frame_runtime(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError>;
    async fn find_by_runtime_session(&self, runtime_session_id: &str) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError>;
    async fn find_activity_assignment_by_runtime_session(&self, runtime_session_id: &str) -> Result<Option<AgentAssignment>, DomainError>;
    async fn list_by_frame(&self, frame_id: Uuid) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError>;
    async fn list_by_assignment(&self, assignment_id: Uuid) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError>;
}
```

If the implementation should avoid adding another repository, the minimum viable alternative is to extend existing repositories with direct methods:

```rust
#[async_trait]
pub trait AgentFrameRepository {
    async fn find_runtime_anchor(&self, runtime_session_id: &str) -> Result<Option<RuntimeSessionFrameAnchor>, DomainError>;
}

#[async_trait]
pub trait AgentAssignmentRepository {
    async fn find_by_id(&self, assignment_id: Uuid) -> Result<Option<AgentAssignment>, DomainError>;
    async fn find_active_for_frame(&self, frame_id: Uuid) -> Result<Option<AgentAssignment>, DomainError>;
    async fn find_for_activity_attempt(
        &self,
        run_id: Uuid,
        graph_instance_id: Uuid,
        activity_key: &str,
        attempt: i32,
    ) -> Result<Option<AgentAssignment>, DomainError>;
    async fn find_for_runtime_session(&self, runtime_session_id: &str) -> Result<Option<AgentAssignment>, DomainError>;
}
```

Recommended database shape:

```sql
CREATE TABLE runtime_session_execution_anchors (
    runtime_session_id TEXT PRIMARY KEY,
    frame_id TEXT NOT NULL REFERENCES agent_frames(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    assignment_id TEXT REFERENCES agent_assignments(id) ON DELETE SET NULL,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    graph_instance_id TEXT,
    activity_key TEXT,
    attempt INTEGER,
    delivery_role TEXT NOT NULL,
    attached_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_rsea_frame ON runtime_session_execution_anchors(frame_id);
CREATE INDEX idx_rsea_assignment ON runtime_session_execution_anchors(assignment_id);
CREATE INDEX idx_rsea_attempt ON runtime_session_execution_anchors(run_id, graph_instance_id, activity_key, attempt);

CREATE UNIQUE INDEX idx_agent_assignments_attempt_unique
    ON agent_assignments(run_id, graph_instance_id, activity_key, attempt);
```

`runtime_session_id` as primary key is acceptable if one runtime session can have only one authoritative delivery anchor at a time. If a reused root runtime session may execute multiple sequential assignments, use `(runtime_session_id, assignment_id)` plus an `active`/`terminal_at` field, and require terminal/advance to resolve by current turn or hook control target. The current code does not expose a per-turn assignment id, so the cleaner target is to attach assignment id into the hook/runtime launch envelope for each turn.

### Target Application API

Replace `ActivityRuntimeAssociationResolver.resolve_by_runtime_session` internals with:

```text
runtime_session_execution_anchor_repo.find_by_runtime_session(session_id)
  -> if delivery_role != ActivityAttempt: return None
  -> assignment_repo.find_by_id(anchor.assignment_id)
  -> run_repo.get_by_id(anchor.run_id)
  -> graph_instance_repo.get_by_run_and_id(anchor.run_id, anchor.graph_instance_id)
  -> ActivityAttemptState(anchor.activity_key, anchor.attempt)
```

Terminal callback and `complete_lifecycle_node` should fail closed when an activity runtime session lacks `assignment_id`; they should not fall back to run scan. Trace-only sessions can return `None` through an explicit `delivery_role`.

`AgentActivitySessionPort.create_runtime_session_for_agent_activity` is the best write boundary for new activity runtime sessions because it has definition/project, claim, assignment, frame and created runtime session id in one scope. After `create_runtime_session`, it should persist the execution anchor in the same logical operation that attaches the frame runtime ref.

ContinueRoot/reused-session launches need a per-turn anchor, not just a frame-level ref. The assignment is created in `create_agent_activity_assignment_for_existing_frame`; the target runtime session is already known through `AgentFrameRuntimeTarget.delivery_runtime_session_id`. That path should write a new active assignment anchor for the reused session/turn, otherwise terminal/advance cannot be precise when one root session carries multiple activity attempts.

Hook runtime snapshot/control target should carry direct assignment/attempt fields:

```rust
pub struct HookControlTarget {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub assignment_id: Option<Uuid>,
    pub graph_instance_id: Option<Uuid>,
    pub activity_key: Option<String>,
    pub attempt: Option<u32>,
}
```

Then `WorkflowSnapshotBuilder.resolve_hook_control_target_for_runtime_session` can call the anchor repository once and avoid `list_by_run + select_assignment_for_runtime_frame`.

### Target HTTP/DTO Design

Add generated contract DTOs that expose the authoritative anchor:

```rust
pub struct ActivityAttemptRefDto {
    pub run_id: String,
    pub graph_instance_id: String,
    pub activity_key: String,
    pub attempt: u32,
}

pub struct RuntimeSessionExecutionAnchorDto {
    pub runtime_session_ref: RuntimeSessionRefDto,
    pub frame_ref: AgentFrameRefDto,
    pub agent_ref: LifecycleAgentRefDto,
    pub assignment_ref: Option<AgentAssignmentRefDto>,
    pub attempt_ref: Option<ActivityAttemptRefDto>,
    pub delivery_role: String,
}
```

Extend `AgentAssignmentRefDto` so it is self-sufficient:

```rust
pub struct AgentAssignmentRefDto {
    pub assignment_id: String,
    pub run_id: String,
    pub graph_instance_id: String,
    pub activity_key: String,
    pub attempt: u32,
    pub agent_id: String,
    pub frame_id: String,
}
```

Add one backend endpoint for frontend direct lookup:

```text
GET /sessions/{runtime_session_id}/runtime-anchor -> RuntimeSessionExecutionAnchorDto
GET /sessions/{runtime_session_id}/frame-runtime -> AgentFrameRuntimeView
```

`/sessions/{id}/frame-runtime` can return `AgentFrameRuntimeView` extended with `runtime_anchor`:

```rust
pub struct AgentFrameRuntimeView {
    pub frame_ref: AgentFrameRefDto,
    pub runtime_anchor: Option<RuntimeSessionExecutionAnchorDto>,
    ...
}
```

Then `useSessionRuntimeState` should call `fetchSessionFrameRuntime(sessionId)` directly and delete `findFrameIdForSession`. Frontend store can still cache by frame id after the authoritative response arrives.

`RuntimeSessionTraceView` should also include `runtime_anchor` instead of only optional `frame_ref`, so trace page and runtime state page consume the same anchor contract.

### Scoped Artifact Anchor

Output artifact identity should align with Activity attempt identity:

```rust
pub struct ActivityOutputArtifactRef {
    pub run_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
    pub port_key: String,
}

pub trait ActivityOutputArtifactRepository {
    async fn upsert_port_output(&self, artifact: ActivityOutputArtifactRecord) -> Result<(), DomainError>;
    async fn list_for_attempt(&self, run_id: Uuid, graph_instance_id: Uuid, activity_key: &str, attempt: u32) -> Result<Vec<ActivityOutputArtifactRecord>, DomainError>;
    async fn get_port_output(&self, ref_: ActivityOutputArtifactRef) -> Result<Option<ActivityOutputArtifactRecord>, DomainError>;
}
```

If reusing inline file storage during convergence, encode path under the existing owner as:

```text
container = "activity_outputs"
path = "{graph_instance_id}/{activity_key}/{attempt}/{port_key}"
```

and make lifecycle VFS derive graph/activity/attempt from the active runtime anchor. Completion gate should call `list_for_attempt`, not `load_port_output_map(run_id)`.

### External References

No external references were needed. This was an internal architecture/code audit.

### Related Specs

- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## Caveats / Not Found

- `task.py current --source` returned no active task, so the research used the explicit task path from the user prompt: `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence`.
- I did not inspect every non-terminal caller of `find_by_runtime_session`; companion/canvas/permission/session construction also use frame trace lookup, but this research focused on RuntimeSession -> Frame -> Assignment -> Activity terminal as requested.
- I did not run tests or compile; this is planning-phase read-only research.
- The ContinueRoot/reused-runtime shape needs one product/architecture decision before implementation: whether one reused runtime session may own multiple sequential assignment anchors, and whether turn id should be part of the execution anchor key.
