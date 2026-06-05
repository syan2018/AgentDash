# Dynamic Workflow / Lifecycle 实施计划草案

本文是 planning 阶段的实施拆解。执行前需要用户 review，并通过 Trellis task start 流程进入实现。

## 推荐拆分

这是多阶段架构迁移。推荐拆成可独立验证的子任务：

1. `agent-run-api-naming`
   - 目标：把外露 `/lifecycle-agents/by-runtime-session/...` 收敛为 `/sessions/{runtime_session_id}/...`，并把前后端 command service 命名迁向 AgentRun 语义。
   - 依赖：当前 `RuntimeSessionExecutionAnchor` 反查链路。
   - 验收：前端发送 prompt、steer、pending queue 全部走目标 endpoint，路由表只暴露 session-scoped command API。

2. `orchestration-domain-contract`
   - 目标：新增最小 `LifecycleContext`、`OrchestrationInstance`、`OrchestrationPlanSnapshot`、`RuntimeNodeState`、`StateExchangeSnapshot` domain contract 与持久化字段。
   - 依赖：本任务 `design.md` 的核心合同。
   - 验收：domain unit tests 覆盖 plan/node/status 序列化、Lifecycle aggregate 保存/读取、0..N orchestration 实例。

3. `workflow-graph-compiler`
   - 目标：实现 `WorkflowGraph -> OrchestrationPlanSnapshot`，覆盖现有 Activity graph 语义闭包。
   - 依赖：`orchestration-domain-contract`。
   - 验收：现有 graph fixtures 编译为 plan；agent/function/human executor、transition condition、artifact binding、join/iteration policy 可验证。

4. `common-orchestration-runtime-static-graph`
   - 目标：用 common runtime snapshot/journal 执行静态 graph，生成 LifecycleRunView projection。
   - 依赖：`workflow-graph-compiler`。
   - 验收：静态 graph run 端到端通过新 runtime；旧 `WorkflowGraphInstance.activity_state` 不再作为事实源。

5. `dynamic-script-artifact-compiler`
   - 目标：新增 run script artifact / reusable script definition 与最小 script compiler。
   - 依赖：common orchestration runtime 已承载静态 graph。
   - 验收：脚本原语编译到同一 `OrchestrationPlanSnapshot`，不引入平行 scheduler。

## 第一批实现范围建议

第一批建议只做 1 和 2 的可运行闭包：

- API 命名收敛到 session-scoped command route。
- 领域层落最小 Orchestration contract，静态 graph runtime 在 compiler/runtime 阶段切换。
- migration 添加目标字段/表；旧运行态表作为迁移来源留到 compiler/runtime 可验证后统一收敛。

这样能把概念地基立起来，同时控制 blast radius。

## 任务 1：AgentRun API 命名

### 变更文件

- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `packages/app-web/src/services/lifecycle.ts`
- `packages/app-web/src/services/lifecycle.test.ts`
- generated workflow contracts 相关源文件，视当前生成流程决定是否改 DTO 名。
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/architecture.md`

### 实施步骤

1. 将后端 routes 改为：

   ```text
   POST   /sessions/{runtime_session_id}/messages
   POST   /sessions/{runtime_session_id}/steering
   GET    /sessions/{runtime_session_id}/pending-messages
   POST   /sessions/{runtime_session_id}/pending-messages
   DELETE /sessions/{runtime_session_id}/pending-messages/{message_id}
   POST   /sessions/{runtime_session_id}/pending-messages/{message_id}/promote
   ```

2. 重命名 handler / request service 的外露概念，优先从 API/service 层开始：
   - `send_lifecycle_agent_message` -> `send_agent_run_message` 或 `send_session_message`
   - `steer_lifecycle_agent_message` -> `steer_session`
   - `LifecycleAgentMessage*` DTO -> `AgentRunMessage*` 或 `SessionAgentMessage*`

3. 更新前端 service endpoint。
4. 更新 frontend tests 中的 URL 断言。
5. 路由表只注册目标 session-scoped route。
6. 更新 session runtime spec。

### 验证

- `pnpm --filter app-web test -- lifecycle`
- Rust API route 相关测试，若当前没有专门测试，至少跑涉及 API crate 的 targeted test。
- `pnpm run migration:guard`
- `git diff --check`

## 任务 2：Orchestration domain contract

### 变更文件候选

- `crates/agentdash-domain/src/workflow/value_objects/`
- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-domain/src/workflow/mod.rs`
- `crates/agentdash-infrastructure/migrations/`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
- `crates/agentdash-contracts/src/workflow/` 或当前 generated DTO 源头

### 最小类型

建议新增或等价表达：

```text
LifecycleContext
AgentRunRef / AgentFrameRef
OrchestrationInstance
OrchestrationSourceRef
OrchestrationStatus
OrchestrationPlanSnapshot
PlanNode
PlanNodeKind
RuntimeNodeState
RuntimeNodeStatus
DispatchState
StateExchangeSnapshot
OrchestrationJournalFact
ExecutorRunRef extension for effect refs if needed
```

### 持久化策略

1. 在 `lifecycle_runs` 增加目标 aggregate 字段：
   - `context_json`
   - `orchestrations_json`
   - `view_projection_json`
   - 视需要增加 `version` / `seq`

2. 这些表在第一批作为迁移来源保留：
   - `lifecycle_workflow_instances`
   - `activity_execution_claims`
   - `agent_assignments`

3. 新字段先作为目标 contract 的保存/读取能力，等 graph compiler/runtime 接入后再迁移事实源。

### 测试

- domain serialization roundtrip。
- `LifecycleRun` 能保存 0..N `OrchestrationInstance`。
- `OrchestrationInstance` 中 agent node、function node、human node 的最小 state 能表达。
- repository 保存/读取 target aggregate 字段。
- migration guard。

## 任务 3：WorkflowGraph compiler

### 变更文件候选

- `crates/agentdash-application/src/workflow/`
- `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs`
- 新增 `workflow_graph_compiler.rs` 或 `orchestration/compiler.rs`

### 编译映射

| 当前 WorkflowGraph | OrchestrationPlanSnapshot |
| --- | --- |
| `entry_activity_key` | entry activation rule |
| `ActivityDefinition` | `PlanNode(kind=activity)` |
| `ActivityExecutorSpec::Agent` | `PlanNode.executor=agent_call` |
| `ActivityExecutorSpec::Function(ApiRequest/BashExec)` | `PlanNode.executor=function/local_effect` |
| `ActivityExecutorSpec::Human` | `PlanNode.executor=human_gate` |
| `ActivityTransition` | activation rule / dependency edge |
| `TransitionCondition` | condition expression |
| `ArtifactBinding` | artifact exchange rule |
| `max_traversals` / attempt policies | limit / retry / iteration policy |

### 测试

- entry activity 编译。
- agent/function/human executor 编译。
- condition transition 编译。
- artifact binding 编译。
- bounded loop / max traversal 编译。

## 任务 4：Common orchestration runtime

### 变更文件候选

- `crates/agentdash-application/src/workflow/engine.rs`
- `crates/agentdash-application/src/workflow/activity_run.rs`
- `crates/agentdash-application/src/workflow/scheduler.rs`
- `crates/agentdash-application/src/workflow/agent_executor.rs`
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`

### 实施顺序

1. `PlanActivation` 初始化 entry ready nodes。
2. node claim / dispatch state 内聚到 orchestration snapshot。
3. agent executor 通过 AgentInvocation 创建 AgentRun / AgentFrame / RuntimeTraceAnchor。
4. function executor 写 FunctionRun / EffectTraceRef。
5. terminal event 应用到 `RuntimeNodeState`。
6. transition/materialization 更新 snapshot。
7. `LifecycleRunView` 从 orchestration snapshot 投影。

### 验证

- 静态 graph run e2e。
- function activity e2e。
- human decision e2e。
- cancel/pause/resume 最小控制面，若本阶段范围包含。
- LifecycleRunView 显示保持现有可观察能力。

## 任务 5：Dynamic script artifact / compiler

### 最小范围

- `RunScriptArtifact`：本次运行生成、审批、revision、args、limits、source。
- `WorkflowScriptDefinition`：可复用项目资产。
- `ScriptCompiler`：只负责编译平台原语到 `OrchestrationPlanSnapshot`。

### 首批原语

```text
phase(title)
log(message)
agent(prompt, opts)
parallel(tasks)
pipeline(items, stages)
function(spec)
local_effect(spec)
workflow(name, args)
```

### 验证

- generated script approval draft 保持为草稿资产，正式 Lifecycle 历史从 approve + start 写入。
- approve 后创建 `OrchestrationInstance(role=dynamic_script)`。
- agent/function/local effect 编译到同一 plan/runtime，共享 scheduler。
- journal/cache/resume 最小闭包通过。

## 全局验证命令

按实际 touched package 缩小范围，但跨层阶段至少保留：

```powershell
pnpm run migration:guard
git diff --check
```

涉及 Rust domain/application/infrastructure：

```powershell
cargo test -p agentdash-domain
cargo test -p agentdash-application
cargo test -p agentdash-infrastructure
```

涉及前端 API/service：

```powershell
pnpm --filter app-web test
```

涉及真实运行链路：

```powershell
pnpm dev
```

`pnpm dev` 会拉起 Rust/backend/frontend，Rust 后端无法热重载；更新 Rust 后需要杀先前进程再重新调试。

## 风险文件

- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-domain/src/workflow/value_objects/run_state.rs`
- `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs`
- `crates/agentdash-application/src/workflow/dispatch_service.rs`
- `crates/agentdash-application/src/workflow/activity_run.rs`
- `crates/agentdash-application/src/workflow/engine.rs`
- `crates/agentdash-application/src/workflow/scheduler.rs`
- `crates/agentdash-application/src/workflow/agent_executor.rs`
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `packages/app-web/src/services/lifecycle.ts`
- `crates/agentdash-infrastructure/migrations/0001_init.sql`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`

## 启动前检查

- 用户确认第一批实现范围。
- `design.md` 与 `implement.md` 无未解决概念冲突。
- `.trellis/spec/backend/workflow/*` 与 `.trellis/spec/backend/session/*` 的更新范围明确。
- 明确是否拆子任务；若拆，依赖关系写入子任务文档。
- 执行 `python .trellis/scripts/task.py start .trellis/tasks/06-06-dynamic-workflow-lifecycle-research` 或按项目实际 Trellis 命令启动。

## 推荐第一问

是否将第一批实现范围限定为：

1. session-scoped API 命名迁移；
2. AgentRun 外露语义收敛；
3. 最小 Orchestration domain contract 与 migration；

并把 WorkflowGraph compiler / common runtime 列为下一批？

推荐答案：是。这样第一批能快速完成外露命名清理和领域地基落点，风险可控；如果把 compiler/runtime 一起塞进第一批，容易同时触发 domain、application、infra、API、frontend、UI 多层重写。
