# Lifecycle Dispatch Service

## 目标

建立统一的 `ExecutionIntent -> ExecutionDispatchResult` 入口，替代 Project / Story / Task / Routine 各自拼 Session owner / binding / launch plan 的分散路径。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-session-lifecycle-spec-convergence`
- 依赖：`06-01-session-lifecycle-target-anchors-schema`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B2 Lifecycle Dispatch。
- 退出贡献：业务执行通过 `ExecutionIntent` 选择 same-run `WorkflowGraphInstance` 或 linked `LifecycleRun`，并返回稳定 run/graph/agent/frame/runtime/gate refs。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 即使 ProjectAgent、Story、Task、Companion、Routine caller 暂时不完整，也优先把入口替换为 `ExecutionIntent`。
- 不保留 session-owner launch 作为 fallback path。

## 需求

- 定义 `ExecutionIntent`，输入稳定 `SubjectRef`、procedure preference、dispatch policy、project/workspace context。
- `ExecutionIntent` 能表达：复用现有 `LifecycleRun` 并追加 `WorkflowGraphInstance`，或创建新的独立 `LifecycleRun`。
- 定义 `ExecutionDispatchResult`，输出 run、agent、frame、runtime session、gate、subject execution view refs。
- ProjectAgent open 作为首个迁移入口，后续 Story / Task / Routine 接同一 service。
- Dispatch service 负责创建或复用 LifecycleRun / WorkflowGraphInstance / LifecycleAgent / AgentFrame，不让业务模块直接组装 RuntimeSession owner。

## 交付物

- `ExecutionIntent` / `ExecutionDispatchResult` contract。
- same-run vs linked-run 判定规则。
- ProjectAgent open 首个接入。
- dispatch service 与目标 repositories 的编排边界。
- `design.md` 与 `implement.md` 中声明的后续接入断点。

## 不承担

- 不拥有 AgentFrame 内部构造。
- 不直接暴露 connector `ExecutionContext`。
- 不完成 Task / Companion / Routine 的业务迁移。

## 验收标准

- [ ] 至少一个业务入口不再直接构造 session binding / owner，而是通过 dispatch service。
- [ ] same-run dispatch 可以在既有 LifecycleRun 下追加 WorkflowGraphInstance，而不是创建 child run。
- [ ] 返回 contract 不包含 `binding_id` / `owner_type` / `owner_id` 作为控制面主字段。
- [ ] dispatch 结果足够前端进入 subject view、agent view 或 runtime trace view。
- [ ] 权限与 project scope 从 Subject / Association / Frame 推导，不从 RuntimeSession owner 推导。
