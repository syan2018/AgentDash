# Research: P0-02 Activity State Ownership

- Query: P0-02 LifecycleRun.activity_state 与 WorkflowGraphInstance ownership 拆分
- Scope: internal
- Date: 2026-06-01

## Findings

### Files Found

- `.trellis/workflow.md` - Trellis research 输出必须持久化，Phase 1.2 要把研究写入 task `research/`。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/prd.md` - 本任务要求按结构风险分析事实源 ownership，不直接实现。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md` - 目标不变量明确写着 `WorkflowGraphInstance owns Activity execution state`。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md` - Phase 2 gate 要求 `WorkflowGraphInstance` repository 支持读写 activity state，关键查询覆盖 `run_id + graph_instance_id`。
- `.trellis/spec/backend/workflow/architecture.md` - Workflow 模块不变量：同一 `LifecycleRun` 可包含多个 `WorkflowGraphInstance`，Activity key 必须以 `graph_instance_id` namespace。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - 目标 contract：`WorkflowGraph -> WorkflowGraphInstance -> ActivityState / ActivityAttemptState`。
- `.trellis/spec/backend/story-task-runtime.md` - Story/Task runtime truth 不在 Task，本应从 `WorkflowGraphInstance`、Assignment、Attempt 派生。
- `.trellis/spec/backend/repository-pattern.md` - repository port 对应 aggregate 边界，跨聚合一致性要显式 command port / unit of work。
- `.trellis/spec/backend/database-guidelines.md` - schema 事实源在 `crates/agentdash-infrastructure/migrations/`，预研阶段 baseline migration 与 forward migration 要收敛到同一目标 schema。
- `crates/agentdash-domain/src/workflow/entity.rs` - `LifecycleRun` 仍持有 `activity_state`，并通过 `new_activity` / `replace_activity_state` 写状态。
- `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs` - `WorkflowGraphInstance` 已有 `activity_state_json` 字段，但类型是 `serde_json::Value`。
- `crates/agentdash-domain/src/workflow/repository.rs` - 已有 `WorkflowGraphInstanceRepository` port，只有 `create/get/list_by_run/update`。
- `crates/agentdash-domain/src/workflow/value_objects/run_state.rs` - `ActivityLifecycleRunState` 已包含 `graph_instance_id`。
- `crates/agentdash-application/src/workflow/activity_run.rs` - 当前核心 engine/scheduler service 仍以 `run_id` 加 `LifecycleRun.activity_state` 为上下文。
- `crates/agentdash-application/src/workflow/engine.rs` - `LifecycleEngine` 是纯 state machine，已经接收 `graph_instance_id` 初始化 `ActivityLifecycleRunState`。
- `crates/agentdash-application/src/workflow/scheduler.rs` - scheduler 使用 `state.graph_instance_id` 创建 claim，但 state 仍由 caller 从 run 取出。
- `crates/agentdash-application/src/workflow/orchestrator.rs` - terminal / advance 仍调用 `ActivityLifecycleRunService.apply_event(run.id, ...)`。
- `crates/agentdash-application/src/workflow/agent_executor.rs` - Agent executor 已用 claim 的 `graph_instance_id` 创建 frame 与 assignment。
- `crates/agentdash-application/src/workflow/session_association.rs` - P0-01 后的 resolver 已能返回 assignment，天然能拿到 graph instance 证据。
- `crates/agentdash-application/src/workflow/dispatch_service.rs` - 当前 dispatch 已创建/复用 `WorkflowGraphInstance`，并把 `AgentFrame` / entry assignment 锚到 graph instance，但创建 run 时 `activity_state` 仍为 `None`。
- `crates/agentdash-api/src/routes/workflows.rs` - manual lifecycle run 与 human decision route 仍直接使用 `ActivityLifecycleRunService`。
- `crates/agentdash-api/src/routes/lifecycle_views.rs` - 通用 run view 会读取 graph instance `activity_state_json`，但缺失时回退到 run `activity_state`。
- `crates/agentdash-api/src/routes/story_runs.rs` - story-specific view 完全从 run `activity_state` 组装 graph instance view。
- `crates/agentdash-application/src/task/view_projector.rs` - Task boot projection 文档与实现仍声明 `LifecycleRun.activity_state` 是真相源。
- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs` - lifecycle VFS journey helpers 从 run `activity_state` 提供 steps/current step/session id。
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs` - VFS provider 大量调用 journey helpers，间接依赖 run state。
- `crates/agentdash-application/src/hooks/provider.rs` - hook snapshot 的 active workflow meta 从 run `activity_state` 找当前 activity status。
- `crates/agentdash-application/src/workflow/projection.rs` - session -> active workflow projection 校验 run `activity_state.graph_instance_id == assignment.graph_instance_id`，但不加载 `WorkflowGraphInstance`。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` - `LifecycleRunRepository` 仍序列化/反序列化 `lifecycle_runs.activity_state`。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - `PostgresWorkflowGraphInstanceRepository` 已持久化 `activity_state_json`。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - baseline 同时包含 `lifecycle_runs.activity_state` 与 `lifecycle_workflow_instances.activity_state_json`。
- `crates/agentdash-infrastructure/migrations/0049_lifecycle_run_activity_state.sql` - forward migration 给 `lifecycle_runs` 增加 `activity_state`。
- `crates/agentdash-infrastructure/migrations/0073_lifecycle_target_anchors.sql` - forward migration 建立 `lifecycle_workflow_instances.activity_state_json` 和 root 唯一索引。
- `packages/app-web/src/services/workflow.ts` / `packages/app-web/src/types/workflow.ts` - 前端仍能消费 raw `LifecycleRun.activity_state`。
- `packages/app-web/src/stores/lifecycleStore.ts` - 前端 lifecycle store 已有 `WorkflowGraphInstanceView` map，可承接后端 view 的 graph-instance projection。

### Code Patterns

#### 1. Current activity_state write paths

- `LifecycleRun` 领域实体仍直接拥有状态字段：`activity_state: Option<ActivityLifecycleRunState>` 在 `crates/agentdash-domain/src/workflow/entity.rs:197`，`new_activity` 在 `entity.rs:210` 创建 run 时写入，`replace_activity_state` 在 `entity.rs:235` 更新状态、run status、active_node_keys、timestamps。
- `ActivityLifecycleRunService::start_run` 在 `crates/agentdash-application/src/workflow/activity_run.rs:45` 启动 run，`LifecycleEngine::initialize` 在 `activity_run.rs:52` 生成 state，然后 `LifecycleRun::new_activity` 写入 run；该路径没有创建或更新 `WorkflowGraphInstance.activity_state_json`。
- `ActivityLifecycleRunService::apply_event` 在 `activity_run.rs:60` 以 `run_id` 为命令目标，`load_context` 在 `activity_run.rs:91` 从 `run.activity_state` 取 state，`LifecycleEngine::apply_event` 在 `activity_run.rs:66` 修改内存 state，`run.replace_activity_state` 在 `activity_run.rs:68` 再写回 run。
- `ActivityLifecycleRunService::launch_ready_attempts` 在 `activity_run.rs:73` 同样从 run state 取上下文，scheduler 修改 state 后在 `activity_run.rs:86` 写回 run。
- `LifecycleOrchestrator` terminal callback 在 `crates/agentdash-application/src/workflow/orchestrator.rs:170` 调用 `apply_event(association.run.id, event)`；agent 主动 `complete_lifecycle_node` 在 `orchestrator.rs:264` 也按 run id 推进；后继调度在 `orchestrator.rs:321` 调用 `launch_ready_attempts(run.id, ...)`。
- Manual lifecycle API 在 `crates/agentdash-api/src/routes/workflows.rs:313` 创建 `ActivityLifecycleRunService`，`workflows.rs:320` 调用 `start_run`，`workflows.rs:344` 调用 `launch_ready_attempts`。Human decision route 在 `workflows.rs:387` 创建 service，`workflows.rs:394` 调用 `apply_event`，`workflows.rs:423` 再调度 ready attempts。
- `LifecycleRunRepository` 在 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:533` create 时序列化 `run.activity_state`，在 `workflow_repository.rs:614` update 时写 `lifecycle_runs.activity_state=$5`，在 `workflow_repository.rs:766` 反序列化 row 的 `activity_state`。
- `DispatchService` 已创建 run ledger 时保持 `activity_state: None`，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:521` 到 `dispatch_service.rs:530`。这说明统一 dispatch 入口已向 run 不 owning state 方向靠拢，但 engine/scheduler 主路径还没切过去。

#### 2. Current activity_state read paths

- Scheduler/engine 纯逻辑已经围绕 `ActivityLifecycleRunState` 工作：`ActivityLifecycleRunState` 在 `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:67` 包含 `graph_instance_id`；scheduler claim 创建时用 `state.graph_instance_id`，见 `crates/agentdash-application/src/workflow/scheduler.rs:119`。问题是 caller 仍从 run state 提供这个 state。
- `ActiveWorkflowProjection` 解析 session 时，`crates/agentdash-application/src/workflow/projection.rs:89` 读取 `run.activity_state`，再在 `projection.rs:92` 比对 `activity_state.graph_instance_id != assignment.graph_instance_id`。它只用 graph id 做校验，没有加载 graph instance 的 state owner。
- Hook metadata 在 `crates/agentdash-application/src/hooks/provider.rs:160` 从 `workflow.run.activity_state` 查 activity status。
- Task boot projection 在 `crates/agentdash-application/src/task/view_projector.rs:7` 明确写着 `LifecycleRun.activity_state` 是真相源，`view_projector.rs:78` 取 attempts，`view_projector.rs:238` 到 `view_projector.rs:239` 返回 `run.activity_state.attempts`。
- Lifecycle journey helper 在 `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:452` 的 overview、`journey/mod.rs:504` 的 steps、`journey/mod.rs:511` 的 find step、`journey/mod.rs:543` 的 current step session id 都从 run state 派生。`provider_lifecycle.rs:211`、`provider_lifecycle.rs:214`、`provider_lifecycle.rs:217` 等 VFS paths 间接依赖这些 helper。
- 通用 API read model 在 `crates/agentdash-api/src/routes/lifecycle_views.rs:256` 读取 graph instances，`lifecycle_views.rs:470` 先尝试 `instance.activity_state_json`，但 `lifecycle_views.rs:475` 明确 fallback 到 `run.activity_state`。同文件 `lifecycle_views.rs:491` 到 `lifecycle_views.rs:501` 还会在没有 graph instance rows 时从 run state 构造 synthetic root graph view。
- Story-specific read model 没有读取 `WorkflowGraphInstanceRepository`：`crates/agentdash-api/src/routes/story_runs.rs:262` 直接从 `run.activity_state` 开始，`story_runs.rs:283` 构造 synthetic root `WorkflowGraphInstanceView`。
- SubjectExecution artifacts / latest attempt 仍由 run state 派生：通用 route 在 `lifecycle_views.rs:196` 取 outputs，在 `lifecycle_views.rs:568` 取 latest attempt；story route 在 `story_runs.rs:137` 和 `story_runs.rs:334` 做同样事情。
- Frontend raw workflow service 仍映射 `LifecycleRun.activity_state`，见 `packages/app-web/src/services/workflow.ts:517`；workspace context overview 仍直接读 raw run attempts，见 `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:84` 和 `ContextOverviewTab.tsx:224`。

#### 3. WorkflowGraphInstance current repository/state/migration status

- Domain entity 已存在：`crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:10` 定义 `WorkflowGraphInstance`，`workflow_graph_instance.rs:17` 有 `activity_state_json: Option<serde_json::Value>`。现状是 untyped JSON，不是 `ActivityLifecycleRunState`。
- Repository port 已存在：`crates/agentdash-domain/src/workflow/repository.rs:100` 定义 `WorkflowGraphInstanceRepository`，包含 `create/get/list_by_run/update`。缺口是没有 `get_by_run_and_graph_instance` 这种组合查询；`get(id)` 可以覆盖 graph_instance_id 精确加载，但 service 当前主命令仍没有以 graph_instance_id 为 primary target。
- PostgreSQL implementation 已存在：`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:30` 定义 `PostgresWorkflowGraphInstanceRepository`，`lifecycle_anchor_repository.rs:47` row 有 `activity_state_json`，`lifecycle_anchor_repository.rs:82` insert、`lifecycle_anchor_repository.rs:102` get、`lifecycle_anchor_repository.rs:114` list_by_run、`lifecycle_anchor_repository.rs:133` update 都覆盖该列。
- Baseline migration 已包含 graph instance 表和状态列：`crates/agentdash-infrastructure/migrations/0001_init.sql:272` 创建 `lifecycle_workflow_instances`，`0001_init.sql:278` 有 `activity_state_json TEXT`，`0001_init.sql:283` 有 run_id 索引，`0001_init.sql:286` 有 `(run_id, role)` root 唯一索引。
- Forward migration 已包含同样目标表：`crates/agentdash-infrastructure/migrations/0073_lifecycle_target_anchors.sql:9` 创建 `lifecycle_workflow_instances`，`0073_lifecycle_target_anchors.sql:15` 有 `activity_state_json TEXT`，`0073_lifecycle_target_anchors.sql:20` 有 run_id index，`0073_lifecycle_target_anchors.sql:23` 有 root unique index。
- 旧 run state schema 仍存在：`crates/agentdash-infrastructure/migrations/0001_init.sql:266` 有 `lifecycle_runs.activity_state`，`crates/agentdash-infrastructure/migrations/0049_lifecycle_run_activity_state.sql:1` 到 `0049_lifecycle_run_activity_state.sql:2` 也在 forward migration 中添加该列。

### Ownership Analysis

当前状态不是没有 graph instance schema，而是有两套 ownership surface：

- 写路径 truth 仍是 `LifecycleRun.activity_state`。
- `WorkflowGraphInstance.activity_state_json` 目前主要是 read-model 可读字段和 dispatch anchor，engine/scheduler/orchestrator 没有把它当主状态。
- `ActivityLifecycleRunState.graph_instance_id` 与 `AgentAssignment.graph_instance_id` 已经存在，但它们被塞在 run-owned state 里，无法阻止同一 run 下多个 graph instance 的同名 activity key 被同一个 run state 覆盖。
- Read model 已开始暴露 `WorkflowGraphInstanceView`，但通用 route 会 fallback 到 run state，story route 甚至只 synthetic root view。这样前端看到的是 graph instance 形状，不等于后端事实源已经迁移。

### Minimal High-Cohesion Encapsulation

推荐落点：

- Domain 层：`crates/agentdash-domain/src/workflow/workflow_graph_instance.rs`
  - 将 `activity_state_json` 收束为 typed `activity_state: Option<ActivityLifecycleRunState>`，或至少新增 typed getter/setter 并让 untyped JSON 只存在于 infrastructure mapping 内。
  - 在 `WorkflowGraphInstance` 上提供 `replace_activity_state(state)`，同步 `status`、`updated_at`。状态字符串可以先映射自 `ActivityRunStatus`，但 API 不应让调用方手写 JSON。

- Application 层：新增 `crates/agentdash-application/src/workflow/graph_instance_runtime.rs`
  - `GraphInstanceExecutionContext`：
    - `run: LifecycleRun`
    - `graph_instance: WorkflowGraphInstance`
    - `definition: WorkflowGraph`
    - `state: ActivityLifecycleRunState`
  - `GraphInstanceActivityStateStore` 或 `WorkflowGraphInstanceRuntime`：
    - owns loading `WorkflowGraphInstance + WorkflowGraph + LifecycleRun`
    - owns initialize/apply event/launch ready attempts persistence
    - takes `graph_instance_id` as primary command target
    - updates `WorkflowGraphInstance.activity_state`
    - updates only run-level ledger fields that are legitimate aggregations: `status`, timestamps, execution_log; not `LifecycleRun.activity_state`

建议不要把这个封装命名为裸 `ExecutionContext`，因为 `agentdash-spi::ExecutionContext` 已经是 connector projection。使用 `GraphInstanceExecutionContext` 能避免 connector context 与 workflow execution state 混名。

接口形状建议：

```rust
pub struct GraphInstanceExecutionContext {
    pub run: LifecycleRun,
    pub graph_instance: WorkflowGraphInstance,
    pub definition: WorkflowGraph,
    pub state: ActivityLifecycleRunState,
}

pub struct WorkflowGraphInstanceRuntime<'a> {
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
    graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
    run_repo: &'a dyn LifecycleRunRepository,
    claim_repo: &'a dyn ActivityExecutionClaimRepository,
}

impl<'a> WorkflowGraphInstanceRuntime<'a> {
    pub async fn initialize_root_run(project_id, graph_ref) -> Result<GraphInstanceExecutionContext, _>;
    pub async fn load(graph_instance_id: Uuid) -> Result<GraphInstanceExecutionContext, _>;
    pub async fn apply_event(graph_instance_id: Uuid, event: ActivityEvent) -> Result<GraphInstanceExecutionContext, _>;
    pub async fn launch_ready_attempts(graph_instance_id: Uuid, launcher: &impl ActivityExecutorLauncher) -> Result<(_, Vec<ActivityExecutorLaunchOutcome>), _>;
}
```

Migration note:

- `ActivityLifecycleRunService` can become a facade over `WorkflowGraphInstanceRuntime`, but its public command target should change from `run_id` to `graph_instance_id` or accept a typed context that has `graph_instance_id`.
- Terminal/advance code already has assignment evidence after P0-01, so orchestrator can call `apply_event(association.assignment.graph_instance_id, event)` instead of `apply_event(association.run.id, event)`.
- API paths that only have `{run_id}/{activity_key}/{attempt}` need either a graph instance id in the contract or an explicit root graph instance resolver. For no-compat target shape, prefer generated DTO/route accepting `graph_instance_id` for activity attempts.

### First Implementable Batches

1. Core state-owner cutover
   - Type `WorkflowGraphInstance` activity state in domain.
   - Update `PostgresWorkflowGraphInstanceRepository` serialization/deserialization while keeping schema migration decisions explicit.
   - Introduce `WorkflowGraphInstanceRuntime` / `GraphInstanceExecutionContext`.
   - Change `start_run` equivalent to create `LifecycleRun(activity_state=None)` plus root `WorkflowGraphInstance(activity_state=LifecycleEngine::initialize(...))`.
   - Change `apply_event` and `launch_ready_attempts` to target graph instance id and persist graph instance state.
   - Add application tests proving two graph instances under the same run can both use `activity_key="plan"` without attempts/claims/assignments colliding.

2. Runtime ingress and terminal cutover
   - Orchestrator terminal and `complete_lifecycle_node` use `assignment.graph_instance_id`.
   - `ActiveWorkflowProjection` carries `GraphInstanceExecutionContext` or at least `graph_instance + state`; remove `run.activity_state` lookup.
   - Agent executor launcher trait can stay mostly intact because claim already contains graph_instance_id, but the spec target shape should eventually pass graph instance context to launcher.

3. Read model cutover
   - `lifecycle_views.rs` builds every `WorkflowGraphInstanceView` from graph instance state only; remove run-state fallback and synthetic root fallback.
   - `story_runs.rs` stops duplicating run view assembly and uses the common builder after graph instance state is authoritative.
   - `SubjectExecutionView.latest_attempt` and `artifacts` derive from graph instance states filtered through subject association / assignments.
   - Task boot projector reads `WorkflowGraphInstance` states by run and assignment/subject association, not `LifecycleRun.activity_state`.
   - VFS lifecycle journey helpers accept graph instance context or graph instance id; `run_overview` can remain run ledger only.

4. Schema cleanup
   - Update baseline `0001_init.sql` to remove `lifecycle_runs.activity_state` from target schema.
   - Add forward migration dropping `lifecycle_runs.activity_state`.
   - Decide whether to rename `lifecycle_workflow_instances.activity_state_json` to `activity_state`. If renamed, update both baseline and forward migration, plus repository SQL.
   - Add readiness assertions for required graph instance state column and forbidden run state column.

### Validation Commands

Targeted during first batch:

```bash
cargo test -p agentdash-domain workflow_graph_instance
cargo test -p agentdash-application workflow::engine
cargo test -p agentdash-application workflow::scheduler
cargo test -p agentdash-application workflow::activity_run
cargo test -p agentdash-application workflow::session_association
cargo test -p agentdash-application task::view_projector
cargo test -p agentdash-infrastructure workflow_graph_instance
```

After API/read-model/schema cutover:

```bash
pnpm run contracts:check
pnpm run backend:check
pnpm run backend:clippy
pnpm run backend:test
pnpm run frontend:check
pnpm run e2e:test:critical
```

Final gate for P0-02:

```bash
pnpm run check
```

## External References

- No external references used. This research is internal code/spec inspection only.
- Local toolchain/script references from `package.json`: `pnpm@10.33.3`, `pnpm run backend:check`, `pnpm run backend:test`, `pnpm run contracts:check`, `pnpm run check`.

## Related Specs

- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/domain-payload-typing.md`
- `.trellis/spec/backend/session/execution-context-frames.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task. The output path was still explicit in the user request, so this file was written to the requested task directory rather than guessed.
- I did not run validation commands; this was a read-only research pass.
- Current code already appears to include P0-01 and adjacent P0-03/P0-04 changes in `dispatch_service.rs`: graph key resolution, graph-scoped frame creation, and entry assignment creation are present. This research treats the current workspace contents as source of truth.
- I did not find a production path where `LifecycleEngine` or `ActivityExecutorScheduler` persists state into `WorkflowGraphInstance.activity_state_json`; all durable activity state writes still go through `LifecycleRunRepository`.
- I did not find a typed `WorkflowGraphInstance.activity_state: ActivityLifecycleRunState` field. The existing state field is untyped JSON in domain and SQL.
- I did not find schema cleanup that removes `lifecycle_runs.activity_state`; both baseline and forward migrations still keep it.
