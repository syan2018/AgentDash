# LifecycleRun 活跃 Activity 投影结构化

## Goal

将 `LifecycleRun.active_node_keys` 这类字符串拼接的 run-level 活跃节点投影收敛为结构化 `ActiveActivityRef`，并确保 runtime 业务路径以 `WorkflowGraphInstance.activity_state` 为事实源。

本任务同时承担公开类型暴露收敛目标：Agent / Lifecycle 是运行态和业务态的主入口，Session 只表达 runtime trace、turn supervision、transport delivery 这些运行边界事实。凡是需要展示或推进 Agent / Lifecycle runtime state 的入口，应返回 Agent / Lifecycle 锚定的 read model，而不是让调用方继续以 session resource 为主索引再跨层反查。

## User Value

- run-level active projection 可读、可校验，不再隐藏 `graph_instance_id:activity_key` 字符串协议。
- 前后端 read model 能明确展示 graph instance、activity、attempt。
- 公开 DTO / service / route 的入口语义更稳定：业务 UI 从 Agent / Lifecycle read model 进入，Session 只作为 trace adapter 连接到这些 read model。
- 后续多 graph instance、fork/join、routine phase 等场景不会被旧单当前节点语义限制。

## Confirmed Facts

- `LifecycleRun.active_node_keys` 当前由 `sync_graph_instance_activity_projections` 拼接 `graph_instance_id:activity_key`。
- `LifecycleRun.current_activity_key()` 返回第一个 active key。
- `LifecycleRunView` 仍暴露 `lifecycle_id`，而 root graph instance 已成为目标语义。
- `WorkflowGraphInstance.activity_state` 已经持有 graph-scoped attempts。
- 前端业务 UI 当前没有直接消费 `active_node_keys`；后端 runtime/control path 仍有 `select_active_run`、`advance_node` 等位置使用 run-level active projection。
- 父任务已确认所有运行态入口正在收束到 Agent / Lifecycle；Session 仍作为 runtime trace container 存在，但不应继续扩张为业务 read model 的主索引。
- 前端 session runtime 查询任务已选择 `GET /sessions/{runtime_session_id}/frame-runtime` 这类 adapter 形状；它的返回体仍应锚定到 `AgentFrameRuntimeView` / assignment / attempt，而不是新增一套 session-first runtime view。

## Requirements

- 定义结构化 `ActiveActivityRef`，至少包含 `graph_instance_id`、`activity_key`、可选 `attempt`、`status`。
- runtime 业务逻辑不应依赖 run-level active projection 推进 Activity。
- `LifecycleRunView` 可暴露 structured active refs 作为 read model。
- `lifecycle_id` 的目标命名应与 root graph / workflow graph instance 语义对齐。
- 字符串 active key 若存在，只能作为调试 display，不作为事实源。
- 新增或调整公开类型时，应优先把业务运行态字段放入 Agent / Lifecycle / WorkflowGraphInstance / ActivityAttempt 相关 generated contracts。
- Session-indexed endpoint 只能作为 adapter：用 `runtime_session_id` 定位 Agent / Lifecycle 锚点，并返回锚点后的 read model。
- 前端 service / store 不应长期保留绕过 Agent / Lifecycle 锚点的 session runtime 类型；session 类型只承载 trace、turn、hook runtime metadata 和 transport delivery 所需字段。

## Acceptance Criteria

- [ ] `active_node_keys` 被替换为或降级为结构化 read projection。
- [ ] `current_activity_key()` 不再作为业务路径事实源。
- [ ] `LifecycleRunView` 暴露 structured active refs。
- [ ] 前端使用 structured fields 展示 active Activity。
- [ ] 后端 `select_active_run`、`advance_node` 等入口不再依赖字符串 active key。
- [ ] 公开 DTO / generated TS 中的 active runtime exposure 锚定在 `LifecycleRunView`、`WorkflowGraphInstanceView`、`ActivityAttemptView`、`AgentFrameRuntimeView` 或显式 attempt ref 上。
- [ ] Session-indexed 查询返回 Agent / Lifecycle anchored runtime view；调用方不需要再访问 session resource 后自行推导 Agent / Frame / Activity。
- [ ] 测试覆盖多 graph instance 同名 activity key。

## Out Of Scope

- 不重构 WorkflowGraph topology。
- 不实现 fork/join 新功能，仅保证当前投影支持目标语义。

## Dependency Notes

- 可在 runtime anchor 和 scoped artifact 后实施，减少同时修改 Activity identity 的风险。
