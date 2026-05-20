# Lifecycle Activity/Executor 重新设计实施拆解

## 执行原则

这次重构不应从 UI 或 Function executor 开始。正确推进顺序是：

1. 先修当前 lifecycle 基础边界。
2. 再落 Activity domain schema。
3. 再落纯状态机 LifecycleEngine。
4. 再落 durable scheduler claim。
5. 再收敛普通 session 到 freeform LifecycleRun。
6. 最后接 Agent / Human / Function executor 与 UI。

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
- [x] Scheduler claim 后启动 executor。
- [ ] claim 状态：
  - [x] `claiming`
  - [x] `running`
  - [x] `succeeded`
  - [x] `failed`
  - [x] `abandoned`
- [x] 实现 idempotency key。
- [x] 实现 claiming 超时恢复策略。
- [x] Scheduler 启动成功后提交 `ExecutorStarted` event。
- [x] Scheduler 启动失败时按可重试 / 不可重试更新 attempt。
- [x] 新增 `ActivityLifecycleRunService`，将 engine / scheduler 结果写回 `LifecycleRun.activity_state`。

验证：

- [x] 并发 schedule 同一 ready attempt 只产生一个 executor start。
- [x] prompt 未 accepted 不会留下 running attempt。
- [x] claiming 超时后可恢复。
- [x] `cargo test -p agentdash-application workflow::scheduler`
- [x] `cargo test -p agentdash-application workflow::activity_run`
- [x] `cargo test -p agentdash-infrastructure workflow_claim`

出口标准：

- [x] Scheduler claim 能替代 orchestrator 里“先创建 session 再补状态”的路径。

## Phase 5.5：Freeform LifecycleRun 归属收敛

目标：消除裸业务 session 心智，让普通自由会话也依附于无外围约束的 LifecycleRun。

建议拆成独立 PR：`feat(workflow): 为普通会话创建 freeform lifecycle`

范围：

- [x] 新增内置 `builtin.freeform_session` Activity lifecycle definition。
- [x] 新增或复用 `builtin.freeform_agent` workflow contract，表达普通会话默认能力与上下文，不施加固定产物约束。
- [x] 新增 open-ended / manual completion policy，表达普通会话不随每轮 prompt terminal 自动完成。
- [x] 普通 session 创建入口在未指定 lifecycle 时自动创建 freeform LifecycleRun。
- [x] `LifecycleRun.session_id` 指向普通会话自身，作为 root session / host session。
- [x] freeform run 初始化为 `main_conversation#1`，executor 为 `Agent + ContinueRoot`。
- [x] freeform lifecycle 使用 open-ended / manual completion policy。
- [x] session 查询 / session binding / run association 能反查 freeform LifecycleRun。
- [x] 启动对账或 migration 为既有裸业务 session 补齐 freeform LifecycleRun。
- [ ] 内部健康检查、系统探测类非业务 session 明确不进入用户过程视图。

验证：

- [x] 创建普通 session 时自动生成 freeform LifecycleRun。
- [ ] 指定显式 lifecycle 时不创建额外 freeform run。
- [ ] session 历史能展示其归属 LifecycleRun。
- [ ] 既有裸业务 session 对账后拥有 freeform LifecycleRun。
- [x] freeform 普通 prompt terminal 不会自动完成 `main_conversation` attempt。
- [x] `cargo test -p agentdash-application workflow::freeform`
- [ ] `cargo test -p agentdash-api sessions`

出口标准：

- [ ] 面向用户的业务 session 均能归属某个 LifecycleRun。
- [ ] 后续 Agent / Human / Function executor 不再需要处理裸 session 分支。

## Phase 6：Agent Executor 迁移

目标：用新 Scheduler 承载现有 AgentNode / PhaseNode 行为。

建议拆成独立 PR：`feat(workflow): 迁移 Agent Activity 执行器`

范围：

- [ ] `Agent + SpawnChild` 映射现有 AgentNode：
  - [x] 创建 child session。
  - [x] 创建 lifecycle activity binding。
  - [x] 继承 executor config。
  - [x] launch prompt accepted 后提交 `ExecutorStarted`。
  - [x] session terminal 转为 ActivityEvent。
  - [x] Activity 子 session 内 `complete_lifecycle_node` 转为 ActivityEvent。
- [x] `Agent + ContinueRoot` 映射现有 PhaseNode：
  - [x] 限制同一 root session 仅一个 running ContinueRoot attempt。
  - [x] capability / MCP / VFS transition 进入 runtime command 或 pending transition。
  - [x] 完成事件仍走 ActivityEvent。
- [x] `complete_lifecycle_node` 工具在 Activity 子 session 内提交 ActivityEvent；旧 step session 路径收敛到旧模型清理阶段。
- [x] 保留现有 hook gate 能力，并让 Activity 子 session 的完成结果进入 completion policy / ActivityEvent。

验证：

- [ ] 单 step SpawnChild lifecycle 能启动并完成。
- [ ] 多 step SpawnChild lifecycle 能激活后继。
- [x] ContinueRoot capability transition 生效。
- [x] 并行 ContinueRoot 被拒绝。
- [x] session terminal failed 转为 attempt failed。
- [x] activity 子 session label 可稳定反查 run / activity / attempt。
- [x] Activity 子 session 调用 `complete_lifecycle_node` 会提交 ActivityEvent 并调度后继 ready attempt。
- [x] Activity 子 session 能解析 active workflow projection，保留 workflow 注入与 before_stop hook gate。
- [x] `cargo test -p agentdash-application workflow::agent_executor`
- [x] `cargo test -p agentdash-application workflow::session_association`
- [x] `cargo test -p agentdash-application workflow`
- [x] `cargo check -p agentdash-api`

出口标准：

- [ ] 旧 AgentNode / PhaseNode 行为由 Activity executor 承载。
- [ ] 旧 runtime 用户路径未断。

## Phase 7：Human Approval MVP

目标：用 Human executor 验证退回重规划闭环。

建议拆成独立 PR：`feat(workflow): 支持 Human Approval Activity`

范围：

- [x] Human Approval executor 创建 pending decision。
- [ ] 决策提交 API：
  - [x] `approved`
  - [x] `rejected`
  - [x] `comment / feedback`
- [x] decision 写入 lifecycle artifact。
- [x] decision 转为 `HumanDecisionSubmitted` event。
- [x] `HumanDecisionEquals` transition 激活 approved / rejected 分支。
- [x] rejected 分支创建 plan 下一 attempt。

验证：

- [x] rejected 后生成 plan attempt #2。
- [x] feedback artifact 被 plan #2 消费。
- [x] approved 后 implement ready。
- [ ] decision history 可查询。
- [x] `cargo test -p agentdash-application workflow::engine`
- [ ] `cargo test -p agentdash-api workflow_approval`

出口标准：

- [ ] Plan -> Approval -> Replan -> Approval -> Implement 后端链路可跑通。

## Phase 8：Frontend 最小可用体验

目标：让用户能编辑和观察 Activity lifecycle。

建议拆成独立 PR：`feat(workflow): 更新 Activity 生命周期编辑器`

范围：

- [x] TS 类型改为 Activity / Executor / Attempt / Transition。
- [x] DAG canvas 节点展示 Activity。
- [x] Inspector 一级选择 executor kind。
- [x] Agent panel 支持 workflow_key + session_policy。
- [x] Human Approval panel 支持 form / decision schema。
- [x] Transition panel 支持 Always / HumanDecisionEquals。
- [x] Run view 展示 activity attempts timeline。
- [x] Artifact panel 支持 latest 与 history。

验证：

- [x] 前端单测覆盖 store draft rename / add / remove。
- [x] UI 能创建 Plan -> Approval -> Implement。
- [x] rejected -> replan -> approved 的 run view 可读。
- [x] `pnpm --filter app-web test workflow`
- [x] `pnpm --filter app-web typecheck`
- [ ] 浏览器手动验证主要流程。

出口标准：

- [ ] MVP case 可由 UI 创建、运行、观察。

## Phase 9：Function Executor

目标：把原 Function Node 能力作为 Function executor 落地。

建议拆成独立 PR：`feat(workflow): 支持 Function Activity 执行器`

范围：

- [x] `ActivityExecutorSpec::Function::ApiRequest`。
- [x] `ActivityExecutorSpec::Function::BashExec`。
- [x] Function executor 不直接访问 API 层 provider 细节。
- [x] Bash 通过本机 runtime / relay port 执行，云端不直接访问本地文件系统。
- [x] Function result 写 output artifact，再提交 `ActivityCompleted`。
- [x] Function failed 提交 `ActivityFailed`。

验证：

- [x] API request success / failure。
- [x] Bash exec routing 到本机。
- [x] Function -> Agent artifact transition。
- [x] 连续 Function activities 不依赖 session terminal。
- [x] `cargo test -p agentdash-application workflow::agent_executor`
- [x] `cargo test -p agentdash-application workflow::scheduler`
- [x] `cargo test -p agentdash-application workflow`

出口标准：

- [x] 原通用功能节点目标在新模型中落地。

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
