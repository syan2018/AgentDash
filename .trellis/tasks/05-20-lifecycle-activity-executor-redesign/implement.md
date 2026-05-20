# Lifecycle Activity/Executor 重新设计实施拆解

## 执行原则

这次重构不应从 UI 或 Function executor 开始。正确推进顺序是：

1. 先修当前 lifecycle 基础边界。
2. 再落 Activity domain schema。
3. 再落纯状态机 LifecycleEngine。
4. 再落 durable scheduler claim。
5. 最后接 Agent / Human / Function executor 与 UI。

任何阶段如果无法用自动化测试证明关键状态转移，就不进入下一阶段。

## Phase 0：计划冻结与旧路线止损

目标：让团队停止沿旧 `LifecycleNodeType` 扩展方向继续投入。

交付：

- [ ] 确认 `05-20-workflow-generic-function-nodes` 不再按 `function_node + FunctionNodeSpec` 直接实现。
- [ ] 将 Function 能力落点改为 `ActivityExecutorSpec::Function`。
- [ ] 明确 `phase_node` 不再作为长期 node type；其能力迁移到 `AgentExecutor + ContinueRoot` 或专门 runtime transition policy。
- [ ] 确认 Transition condition 首版只做 typed matcher，不做表达式 DSL。

出口标准：

- [ ] 用户确认 Activity/Executor 是后续唯一重构方向。
- [ ] 后续实现任务不再新增 `LifecycleNodeType` 变体。

## Phase 1：当前 Lifecycle 基础修复

目标：先修掉会污染新模型的现有硬问题。

建议拆成独立 PR：`fix(workflow): 收紧生命周期定义作用域`

范围：

- [x] `LifecycleRunService::resolve_lifecycle` 按 key 解析时使用 `get_by_project_and_key(project_id, key)`。
- [x] 按 id 读取 lifecycle definition 后校验 `definition.project_id == cmd.project_id`。
- [x] active workflow projection 解析 workflow 时使用 `get_by_project_and_key(lifecycle.project_id, workflow_key)`。
- [x] ProjectAgent 的 `default_workflow_key` / auto lifecycle 解析使用 project-scoped lookup。
- [x] workflow / lifecycle definition 的 create / update / delete / get / list API 加 project permission gate。
- [x] 修正 `PostgresWorkflowRepository::initialize` 与 `LifecycleRunRepository::create` 的 `record_artifacts` 列不一致问题。

验证：

- [x] 后端单测覆盖同 key 不同 project 不串 definition。
- [ ] API route 单测或集成测试覆盖无权限不能读取 / 修改 lifecycle definition。
- [ ] 空库初始化后可以创建 lifecycle run。
- [ ] `cargo test -p agentdash-application workflow`
- [x] `cargo test -p agentdash-api workflow`
- [x] `cargo check -p agentdash-infrastructure`

出口标准：

- [ ] 现有 lifecycle 仍可启动。
- [ ] Function/Activity 新模型尚未引入。

## Phase 2：Activity Domain Schema

目标：在 domain 层引入新模型，但暂不替换 runtime。

建议拆成独立 PR：`feat(workflow): 引入 Activity 定义模型`

范围：

- [x] 新增 `ActivityDefinition`。
- [x] 新增 `ActivityExecutorSpec::{Agent, Function, Human}`。
- [x] 新增 `AgentSessionPolicy::{SpawnChild, ContinueRoot, AttachExisting}`。
- [x] 新增 `ActivityTransition` 与 typed `TransitionCondition`。
- [x] 新增 `ActivityAttemptState`、`ActivityAttemptStatus`、`ExecutorRunRef`。
- [x] 新增 `ActivityCompletionPolicy`、`ActivityIterationPolicy`、`ArtifactAliasPolicy`。
- [x] 新增 validation：
  - [x] activity key 唯一。
  - [x] entry activity 存在。
  - [x] Agent executor 必须有 workflow key。
  - [x] Function / Human executor 不携带 workflow key。
  - [x] transition from / to 均存在。
  - [x] typed condition 引用的 activity / port 存在。
  - [x] guarded loop 必须有 max attempts 或 max traversals。
  - [x] 无条件自环拒绝。

兼容策略：

- 项目处于预研期，不做旧 schema 兼容。
- 若实现风险需要降低，可暂时保留持久化列名，但 Rust / API / TS 类型必须使用 Activity 命名。

验证：

- [x] serde roundtrip 覆盖 Agent / Function / Human 三类 executor。
- [x] validation 覆盖 bad cases。
- [x] Plan -> Approval -> Implement definition 能通过校验。
- [x] 无条件自环被拒绝。
- [x] `cargo test -p agentdash-domain workflow`

出口标准：

- [x] 新 domain 类型可独立编译和测试。
- [x] application runtime 尚未切换。

## Phase 3：Persistence / DTO / Catalog 接线

目标：让后端能够保存、读取、验证 Activity lifecycle definition。

建议拆成独立 PR：`feat(workflow): 接入 Activity 生命周期定义`

范围：

- [x] 新增 migration：
  - [x] `lifecycle_definitions.activities`
  - [x] `lifecycle_definitions.transitions`
  - [ ] 必要时新增 `lifecycle_runs.attempts_json`
  - [ ] 新增 `activity_execution_claims` 表。
- [x] Repository 读写 Activity definition。
- [x] API DTO 与 domain schema 对齐。
- [x] `WorkflowCatalogService::validate_lifecycle_definition` 使用 Activity validation。
- [ ] builtin JSON seed 迁移为 Activity graph。
- [x] 前端 TS 类型先补齐，不要求 UI 完整可编辑。

验证：

- [ ] migration 在空库通过。
- [ ] seed bootstrap 通过。
- [x] API create / update / validate 可保存 Activity lifecycle。
- [x] `cargo test -p agentdash-infrastructure workflow`
- [ ] `cargo test -p agentdash-api workflow`
- [x] `pnpm --filter app-web typecheck`

出口标准：

- [ ] 新 definition 数据可 CRUD。
- [ ] 旧 orchestrator 尚未负责执行新模型。

## Phase 4：LifecycleEngine 纯状态机

目标：先构建不做 IO 的核心状态机。

建议拆成独立 PR：`feat(workflow): 增加 Activity 状态机`

范围：

- [x] 新增 `ActivityEvent`。
- [x] 新增 `LifecycleEngine::apply_event(definition, run, event, artifacts)`。
- [ ] 实现 attempt 状态转移：
  - [x] `pending -> ready`
  - [x] `ready -> claiming`
  - [x] `claiming -> running`
  - [x] `running -> completed`
  - [x] `running -> failed`
  - [x] `completed -> new attempt ready`
- [ ] 实现 transition condition：
  - [x] `Always`
  - [x] `HumanDecisionEquals`
  - [x] `ArtifactFieldEquals`
- [x] 实现 All join。
- [x] 实现 latest/history artifact alias 计算，不直接写 inline file。
- [x] 实现 run status 派生。

验证：

- [x] Plan -> Approval rejected -> Plan #2。
- [x] Approval approved -> Implement ready。
- [x] output port 缺失不完成 attempt。
- [x] failed attempt 不隐式激活后继。
- [x] All join 等待全部依赖。
- [x] completed attempt 不会回到 ready。
- [x] `cargo test -p agentdash-application workflow::engine`

出口标准：

- [x] Engine 不依赖 session service / repository / HTTP / local executor。
- [x] Engine 单测覆盖核心 case。

## Phase 5：Durable ExecutorScheduler

目标：解决当前 orchestrator 非原子调度问题。

建议拆成独立 PR：`feat(workflow): 增加 Activity 调度 claim`

范围：

- [x] 新增 `ActivityExecutionClaimRepository`。
- [x] 同一 `run_id + activity_key + attempt` 只允许一个 active claim。
- [x] Scheduler 扫描 ready attempts，先写 claim。
- [ ] Scheduler claim 后启动 executor。
- [ ] claim 状态：
  - [x] `claiming`
  - [x] `running`
  - [x] `succeeded`
  - [x] `failed`
  - [x] `abandoned`
- [x] 实现 idempotency key。
- [x] 实现 claiming 超时恢复策略。
- [ ] Scheduler 启动成功后提交 `ExecutorStarted` event。
- [ ] Scheduler 启动失败时按可重试 / 不可重试更新 attempt。

验证：

- [ ] 并发 schedule 同一 ready attempt 只产生一个 executor start。
- [ ] prompt 未 accepted 不会留下 running attempt。
- [ ] claiming 超时后可恢复。
- [x] `cargo test -p agentdash-application workflow::scheduler`
- [x] `cargo test -p agentdash-infrastructure workflow_claim`

出口标准：

- [ ] Scheduler claim 能替代 orchestrator 里“先创建 session 再补状态”的路径。

## Phase 6：Agent Executor 迁移

目标：用新 Scheduler 承载现有 AgentNode / PhaseNode 行为。

建议拆成独立 PR：`feat(workflow): 迁移 Agent Activity 执行器`

范围：

- [ ] `Agent + SpawnChild` 映射现有 AgentNode：
  - [ ] 创建 child session。
  - [ ] 创建 lifecycle activity binding。
  - [ ] 继承 executor config。
  - [ ] launch prompt accepted 后提交 `ExecutorStarted`。
  - [ ] session terminal / `complete_lifecycle_node` 转为 ActivityEvent。
- [ ] `Agent + ContinueRoot` 映射现有 PhaseNode：
  - [ ] 限制同一 root session 仅一个 running ContinueRoot attempt。
  - [ ] capability / MCP / VFS transition 进入 runtime command 或 pending transition。
  - [ ] 完成事件仍走 ActivityEvent。
- [ ] 旧 `complete_lifecycle_node` 工具改为提交 ActivityEvent，不直接调用 `complete_step`。
- [ ] 保留现有 hook gate 能力，但结果转换为 completion policy / ActivityEvent。

验证：

- [ ] 单 step SpawnChild lifecycle 能启动并完成。
- [ ] 多 step SpawnChild lifecycle 能激活后继。
- [ ] ContinueRoot capability transition 生效。
- [ ] 并行 ContinueRoot 被拒绝。
- [ ] session terminal failed 转为 attempt failed。
- [ ] `cargo test -p agentdash-application workflow::agent_executor`

出口标准：

- [ ] 旧 AgentNode / PhaseNode 行为由 Activity executor 承载。
- [ ] 旧 runtime 用户路径未断。

## Phase 7：Human Approval MVP

目标：用 Human executor 验证退回重规划闭环。

建议拆成独立 PR：`feat(workflow): 支持 Human Approval Activity`

范围：

- [ ] Human Approval executor 创建 pending decision。
- [ ] 决策提交 API：
  - [ ] `approved`
  - [ ] `rejected`
  - [ ] `comment / feedback`
- [ ] decision 写入 lifecycle artifact。
- [ ] decision 转为 `HumanDecisionSubmitted` event。
- [ ] `HumanDecisionEquals` transition 激活 approved / rejected 分支。
- [ ] rejected 分支创建 plan 下一 attempt。

验证：

- [ ] rejected 后生成 plan attempt #2。
- [ ] feedback artifact 被 plan #2 消费。
- [ ] approved 后 implement ready。
- [ ] decision history 可查询。
- [ ] `cargo test -p agentdash-application workflow::human_executor`
- [ ] `cargo test -p agentdash-api workflow_approval`

出口标准：

- [ ] Plan -> Approval -> Replan -> Approval -> Implement 后端链路可跑通。

## Phase 8：Frontend 最小可用体验

目标：让用户能编辑和观察 Activity lifecycle。

建议拆成独立 PR：`feat(workflow): 更新 Activity 生命周期编辑器`

范围：

- [ ] TS 类型改为 Activity / Executor / Attempt / Transition。
- [ ] DAG canvas 节点展示 Activity。
- [ ] Inspector 一级选择 executor kind。
- [ ] Agent panel 支持 workflow_key + session_policy。
- [ ] Human Approval panel 支持 form / decision schema。
- [ ] Transition panel 支持 Always / HumanDecisionEquals。
- [ ] Run view 展示 activity attempts timeline。
- [ ] Artifact panel 支持 latest 与 history。

验证：

- [ ] 前端单测覆盖 store draft rename / add / remove。
- [ ] UI 能创建 Plan -> Approval -> Implement。
- [ ] rejected -> replan -> approved 的 run view 可读。
- [ ] `pnpm --filter app-web test workflow`
- [ ] `pnpm --filter app-web typecheck`
- [ ] 浏览器手动验证主要流程。

出口标准：

- [ ] MVP case 可由 UI 创建、运行、观察。

## Phase 9：Function Executor

目标：把原 Function Node 能力作为 Function executor 落地。

建议拆成独立 PR：`feat(workflow): 支持 Function Activity 执行器`

范围：

- [ ] `ActivityExecutorSpec::Function::ApiRequest`。
- [ ] `ActivityExecutorSpec::Function::BashExec`。
- [ ] Function executor 不直接访问 API 层 provider 细节。
- [ ] Bash 通过本机 runtime / relay port 执行，云端不直接访问本地文件系统。
- [ ] Function result 写 output artifact，再提交 `FunctionCompleted`。
- [ ] Function failed 提交 `FunctionFailed`。

验证：

- [ ] API request success / failure。
- [ ] Bash exec routing 到本机。
- [ ] Function -> Agent artifact transition。
- [ ] 连续 Function activities 不依赖 session terminal。
- [ ] `cargo test -p agentdash-application workflow::function_executor`

出口标准：

- [ ] 原通用功能节点目标在新模型中落地。

## Phase 10：旧模型清理

目标：删除旧概念，避免双模型长期共存。

范围：

- [ ] 删除 `LifecycleNodeType` 或只保留迁移内部类型。
- [ ] 删除 `LifecycleStepDefinition.workflow_key` 顶层字段。
- [ ] 删除旧 `complete_step` / `fail_step` 直推路径。
- [ ] 删除旧 Orchestrator 对 `node_type` 的 match 调度。
- [ ] 文档更新 `.trellis/spec/backend/workflow/lifecycle-edge.md` 或新建 Activity lifecycle spec。
- [ ] builtin / seed / tests 全部改为 Activity。

验证：

- [ ] `rg "LifecycleNodeType|node_type|complete_step\\(|fail_step\\(" crates packages` 确认只剩迁移或测试允许位置。
- [ ] 全量后端测试。
- [ ] 前端 workflow 测试。
- [ ] 手动跑 MVP case。

出口标准：

- [ ] 系统只剩 Activity / Executor / Attempt / Transition 心智。

## 推荐首批实际任务

建议从以下三个任务开始，不要直接进入完整重构：

1. `fix(workflow): 收紧 lifecycle project scope 与权限`
2. `feat(workflow): 引入 Activity domain schema 与校验`
3. `feat(workflow): 增加 LifecycleEngine 纯状态机`

这三个完成后，再决定 Agent executor 与 Human Approval 的实现顺序。

## 不启动实现

本 task 当前处于 planning。完成这些文档后，应先交给用户评审。只有用户确认进入实现后，才运行：

```bash
python ./.trellis/scripts/task.py start 05-20-lifecycle-activity-executor-redesign
```
