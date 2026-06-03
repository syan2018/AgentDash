# Agent 页 Draft 会话启动

## Goal

Agent 页点击 ProjectAgent 后先进入未持久化的 Draft 会话编辑态，只有用户提交首条消息时才创建 `RuntimeSession` 与 graphless lifecycle 控制面实体，并立即投递首条消息。

目标是避免用户只是打开 Agent 会话入口、未真正发送消息时，在 `sessions`、`lifecycle_runs`、`lifecycle_agents`、`agent_frames`、`runtime_session_execution_anchors` 和 `lifecycle_subject_associations` 中留下空执行数据。

## User Value

- Agent 页可以像打开聊天窗口一样快速进入输入态。
- 未发送消息的空窗口不污染活跃会话列表和 lifecycle 控制面。
- 首条消息一旦提交，仍然获得完整 RuntimeSession trace、LifecycleRun、LifecycleAgent、AgentFrame 和 RuntimeSessionExecutionAnchor，后续发送、取消、runtime-control、stream、trace 继续沿用现有主链路。

## Confirmed Facts

- `AgentTabView` 当前点击 Agent 会调用 `launchProjectAgent`，然后用 `delivery_runtime_ref.runtime_session_id` 跳转 `/session/:id`。
- `/projects/{id}/agents/{project_agent_id}/launch` 当前构造 `AgentLaunchIntent`，并固定使用 `RuntimePolicy::CreateRuntimeSession`。
- graphless dispatch 当前会创建 `LifecycleRun`、`LifecycleAgent`、`LifecycleSubjectAssociation`、`RuntimeSession`、`AgentFrame` 和 `RuntimeSessionExecutionAnchor`。
- `SessionChatView` 已支持 `sessionId: null` 和 `customSend` 全接管发送流程，适合承载前端未持久化 Draft。
- `SessionPage` 当前只支持真实 runtime session，发送消息依赖 `/lifecycle-agents/by-runtime-session/{runtime_session_id}/messages`，该接口要求已经存在 `RuntimeSessionExecutionAnchor`。
- `delete_session` 当前只删除 session 侧事件、投影、命令和 `sessions` 行；不会清理 lifecycle 控制面和 runtime session anchor。
- `runtime_session_execution_anchors` 当前没有到 `sessions` 的外键，也没有删除 repository 方法。

## Requirements

- Draft 必须是前端未持久化准备态，不创建 database row、不创建 lifecycle run、不创建 runtime session anchor。
- Agent 页点击“启动 Agent”应进入 Draft 会话界面，而不是立即调用现有 `/launch`。
- Draft 会话界面必须展示所选 ProjectAgent 的基本上下文，并允许用户输入首条消息和调整执行器配置。
- 用户提交首条消息时，后端必须通过一个业务原子入口完成：
  - 校验 project Edit 权限。
  - 校验 ProjectAgent 存在且属于 project。
  - 校验 prompt_blocks 非空且 executor_config 可解析。
  - 创建 graphless lifecycle 控制面和 RuntimeSession。
  - 立即投递首条消息。
  - 返回 runtime session、turn、run、agent、frame refs。
- 首条消息 materialize 成功后，前端必须 replace 跳转到 `/session/{runtime_session_id}`，后续交互继续复用现有真实 session 页面和发送接口。
- Project 会话列表和全局 Session shortcut 不展示未发送 Draft。
- 已存在的真实 session 继续按当前 `/session/:sessionId`、runtime-control、NDJSON stream、cancel、title edit、trace 等路径工作。
- 启动失败边界必须有明确处理：如果首条消息在 connector accepted 前失败且没有 session event 产生，不能留下空 lifecycle 控制面；如果 connector accepted 后失败，则保留真实执行证据。
- 数据库迁移可以直接进入目标结构，不为旧开发库保留兼容分支。

## Acceptance Criteria

- [ ] 点击 Agent 页的启动按钮不会新增 `sessions`、`lifecycle_runs`、`lifecycle_agents`、`agent_frames`、`runtime_session_execution_anchors` 或 `lifecycle_subject_associations` 行。
- [ ] Draft 页面可以在 `sessionId=null` 状态下输入首条消息，并正确展示所选 Agent 的名称和默认执行器信息。
- [ ] 提交首条消息后，后端创建完整 graphless runtime 控制面，返回 `runtime_session_id`，前端 replace 到 `/session/{runtime_session_id}`。
- [ ] 首条消息提交成功后，现有 NDJSON stream、runtime-control、继续发送、取消、标题更新、运行详情入口可用。
- [ ] 未提交消息就离开 Draft 页面不会在活跃会话列表中出现条目。
- [ ] Project 会话列表仍只展示真实 runtime session，且不会靠过滤空 session 掩盖已创建的空 lifecycle 数据。
- [ ] 首条消息 connector accepted 前失败时，系统不会留下 `last_event_seq = 0` 的 orphan runtime/lifecycle 数据。
- [ ] 相关 Rust contracts 生成的 TypeScript 类型与前端 service mapper 保持一致。
- [ ] 后端单元或集成测试覆盖“打开 Draft 不落库”和“首条消息 materialize 并投递”主路径。
- [ ] 前端测试覆盖 Draft 启动按钮不调用旧 `/launch`，以及首条消息成功后跳转真实 session。

## Out Of Scope

- 不重新设计真实 SessionPage 的流式展示、投影、compaction、lineage 或 workspace panel。
- 不把普通 Agent 会话改成显式 WorkflowGraph；graphless 仍是普通 Agent runtime 的控制面拓扑。
- 不为历史开发数据做兼容迁移；只维护当前预研阶段的正确目标模型。
- 不把 `LifecycleRunStatus::Draft` 扩展为 UI Draft 状态；UI Draft 没有控制面实体。

## Open Questions

- 无阻塞开放问题。默认推荐路线是新增“ProjectAgent 首条消息 materialize”接口，并让现有 `/launch` 只保留给明确需要预创建控制面实体的内部或后续业务入口。
