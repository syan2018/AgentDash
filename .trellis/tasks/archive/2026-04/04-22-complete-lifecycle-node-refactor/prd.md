# CompleteLifecycleNodeTool 收束重构

## Goal

将 `CompleteLifecycleNodeTool` 从“集工具适配、workflow 状态推进、artifact gate、phase 热更新、session runtime 协调、后继编排于一体”的超级入口，重构为一个薄工具壳；把真正的业务语义收束回现有 `LifecycleOrchestrator` / `LifecycleRunService` / `step_activation` 三个宿主，使 lifecycle node 完成/失败的推进链路只有一个清晰的 workflow 入口，而不是继续新增平行 service。

## What I already know

* 当前实现位于 `crates/agentdash-application/src/workflow/tools/advance_node.rs`
* 该工具同时负责：
  * 参数解析与 active workflow 定位
  * `LifecycleRun` 完成/失败状态变更
  * output port gate 校验与碰撞计数
  * step summary 物化
  * hook snapshot refresh
  * 调用 `LifecycleOrchestrator` 激活后继
  * PhaseNode capability/MCP 热更新
  * steering notification 与 capability changed hook 发射
* 项目中已经存在 `workflow/step_activation.rs`，其注释明确声明要把 step 激活计算统一为“纯计算 + applier”
* 归档任务 `04-21-workflow-step-activation-unify` 已经把 `advance_node.rs` 点名为待迁移路径之一

## Assumptions (temporary)

* 这次重构优先目标是“收束入口与职责边界”，不要求同时解决所有 workflow 历史模型债
* 行为语义要保持兼容：成功/失败推进、artifact gate、PhaseNode 热更新顺序、用户可见提示不应发生功能性回归
* 允许分多 PR 渐进迁移，中间态可以短暂共存，但必须保证唯一 authority 逐步明确
* 不允许为了“保险”保留回退实现、影子实现、双入口或兼容壳；每一轮迁移完成后，旧实现必须立刻删除

## Open Questions

* `failed` 分支的领域原语应直接下沉到 `LifecycleRun` 还是仅在 `LifecycleRunService` 提供命令式入口
* `bind_step_session + activate_step` 最终是保留两个原语，还是收敛成一个组合命令

## Requirements (evolving)

* `CompleteLifecycleNodeTool` 只保留 tool adapter 职责：解析参数、定位当前 node、调用应用层入口、格式化结果
* workflow node 完成/失败语义必须收束到现有 `LifecycleOrchestrator` 的统一入口，而不是在 tool 中直接改 run
* artifact gate 检查与 gate collision 计数必须从 tool 中抽离
* PhaseNode 热更新必须从 tool 中抽离到 `step_activation` 中的可复用 runtime applier
* AgentNode 激活、PhaseNode 热更新、bootstrap step 激活三条路径应尽量共享同一套 `StepActivation` 计算
* `LifecycleRunService` / `LifecycleRun` 必须补齐当前缺失的低层原语，避免 orchestrator/tool 继续手改 `step_states`
* orchestration 只负责推进当前 node 后的工作流编排，不再在 tool 中内联 capability resolver / MCP 替换细节
* `WorkflowSnapshotBuilder` 仅作为 hooks façade 复用同一套低层推进原语，不升级为新的总入口
* 不允许保留旧实现作为 fallback：迁移到新宿主后，原位置的旧逻辑必须在同一 PR 内删除

## Acceptance Criteria (evolving)

* [ ] `CompleteLifecycleNodeTool::execute()` 不再直接修改 `LifecycleRun` 结构字段
* [ ] `advance_node.rs` 中不再直接调用 `CapabilityResolver::resolve()`
* [ ] `advance_node.rs` 中不再直接调用 `replace_runtime_mcp_servers()` / `push_session_notification()` / `emit_capability_changed_hook()`
* [ ] 成功/失败推进都通过 `LifecycleOrchestrator` 的统一 workflow 入口完成
* [ ] `LifecycleOrchestrator` 不再直接手改 `LifecycleRun.step_states[*].session_id` 或同类字段
* [ ] `LifecycleRunService` / `LifecycleRun` 提供 `fail_step`、gate collision、step session 绑定等缺失原语
* [ ] PhaseNode 热更新通过 `step_activation` 的显式 applier 执行，时序有测试锁定
* [ ] `step_activation` 成为唯一 capability/MCP/kickoff prompt 计算入口
* [ ] hooks 路径与 tool 路径在低层 run 推进原语上复用，而不是各自手写一套
* [ ] 不存在旧入口 fallback、影子实现、兼容壳或“双写后择一”逻辑
* [ ] `cargo test` 中覆盖 complete/failed/gate reject/phase activation 四类关键行为

## Definition of Done

* Tests added/updated (unit/integration where appropriate)
* Lint / typecheck / CI green
* Docs/notes updated if behavior changes
* Rollout/rollback considered if risky

## Out of Scope (explicit)

* 修改 workflow DAG 语义或 lifecycle edge 模型
* 修改 VFS `lifecycle://` mount 协议
* 改写用户提示文案风格（除非为迁移必须）
* 一次性清空所有 `SessionWorkflowContext` 历史债；本任务只聚焦 `complete_lifecycle_node` 收束
* 为了降低迁移风险而保留旧实现备份、compat 分支、隐藏开关或临时 fallback 路径

## Technical Approach

### 宿主分工

* `workflow/tools/advance_node.rs`
  * 只做 tool adapter：参数解析、从 hook snapshot 定位 `(run_id, step_key)`、调用 orchestrator、格式化结果
* `workflow/orchestrator.rs`
  * 成为“当前 node 推进 + 后继激活”的统一 workflow 入口
  * 负责串联：run 推进、summary 物化、hook snapshot refresh、successor activation、phase runtime apply
* `workflow/run.rs` + `domain::workflow::LifecycleRun`
  * 补齐低层领域原语：`fail_step`、gate collision、自增失败阈值、step session 绑定
  * orchestrator / tool / hooks 都只能调原语，不得手改字段
* `workflow/step_activation.rs`
  * 保持唯一 step 激活计算入口
  * 新增 `apply_to_running_session(...)`，统一 PhaseNode capability/MCP/notification/hook 应用
* `hooks/workflow_snapshot.rs`
  * 保持 hook façade 身份
  * 只复用 `LifecycleRunService` 的低层推进原语，不承接 tool 路径的 orchestration 逻辑

### 关键设计决定

* 不新增平行的 `LifecycleNodeAdvanceService`
* “手动推进当前 node”并回 `LifecycleOrchestrator`
* “如何改 run”收口到 `LifecycleRunService` / `LifecycleRun`
* “如何把激活结果应用到运行中 session”收口到 `step_activation`
* `WorkflowSnapshotBuilder` 只做 hooks façade，不升级为新的 workflow 总入口

## Implementation Plan

### PR1：锁当前行为

* 为 `complete_lifecycle_node` 补 characterization tests
* 锁定以下行为：
  * `outcome=failed` 不触发后继编排
  * gate reject 返回错误并自增 collision
  * 第 3 次 collision 自动转 failed
  * 完成后 successor activation 行为
  * PhaseNode runtime hot update 的调用顺序

### PR2：补齐底层原语

* 在 `LifecycleRun` / `LifecycleRunService` 中补齐：
  * `fail_step`
  * `increment_gate_collision` 或等价命令
  * `bind_step_session`
  * 如有必要补 `bind_session_and_activate_step`
* 同步把 `orchestrator.create_agent_node_session()` 中直接手改 `step_states` 的逻辑下沉，并删除原地手改实现

### PR3：扩展 `LifecycleOrchestrator`

* 新增 `advance_current_node(...)` 作为统一 workflow 入口
* 内部串联：
  * 当前 node 推进
  * summary 物化
  * hook snapshot refresh
  * `after_node_advanced(...)`
  * 对 activated phase nodes 执行 runtime apply
* `CompleteLifecycleNodeTool` 改为只调用该入口，并删除旧的内联推进逻辑

### PR4：补全 `step_activation` runtime applier

* 在 `step_activation.rs` 实现 `apply_to_running_session(...)`
* 统一承接：
  * `hook_session.update_capabilities(...)`
  * `replace_runtime_mcp_servers(...)`
  * `push_session_notification(...)`
  * `emit_capability_changed_hook(...)`
* 删除 `advance_node.rs` 中对应内联逻辑，不保留 fallback 分支

### PR5：hooks 路径对齐与收尾

* 让 `WorkflowSnapshotBuilder.advance_workflow_step(...)` 复用新的低层 run 推进原语
* 确认 tool path / hook path 只在 orchestration 层分叉，不在 run mutation 层分叉
* 更新相关 spec / 任务文档

## Decision (ADR-lite)

**Context**：`CompleteLifecycleNodeTool` 当前不是简单的 adapter，而是把 workflow 推进、artifact gate、phase 热更新和后继编排都塞在一起；与此同时，现有 `LifecycleOrchestrator`、`LifecycleRunService`、`step_activation` 已经各自承担了相邻职责，但边界还未收口。

**Decision**：

* 不新增新的平行 workflow service
* 把“当前 node 推进”并回 `LifecycleOrchestrator`
* 把 run mutation 原语补回 `LifecycleRunService` / `LifecycleRun`
* 把 PhaseNode runtime apply 收口到 `step_activation`
* 不保留旧实现备份或回退路径，迁移完成即删旧逻辑

**Consequences**：

* + 避免再长一层新的 workflow façade
* + 当前 node 推进与后继激活回到同一个宿主，语义更连贯
* + hooks/tool/orchestrator 可以共享同一套低层 run 原语
* + 后续 AI / 人工协作不会被隐藏旧路径误导
* - 需要先补 domain/application 原语，不能只在 orchestration 层平移逻辑
* - `LifecycleOrchestrator` 会从“只看后继”扩展为“当前 node 推进 + 后继激活”的更完整入口，需要小心控制不把 capability 细节重新内联进去

## Technical Notes

* 关键文件：
  * `crates/agentdash-application/src/workflow/tools/advance_node.rs`
  * `crates/agentdash-application/src/workflow/step_activation.rs`
  * `crates/agentdash-application/src/workflow/orchestrator.rs`
  * `crates/agentdash-application/src/workflow/run.rs`
  * `crates/agentdash-application/src/session/assembler.rs`
  * `crates/agentdash-application/src/session/hook_runtime.rs`
  * `crates/agentdash-application/src/session/hub.rs`
* 相关规范：
  * `.trellis/spec/backend/hooks/execution-hook-runtime.md`
  * `.trellis/spec/backend/capability/tool-capability-pipeline.md`
  * `.trellis/spec/backend/workflow/lifecycle-edge.md`
* 相关归档任务：
  * `.trellis/tasks/archive/2026-04/04-21-workflow-step-activation-unify/prd.md`
