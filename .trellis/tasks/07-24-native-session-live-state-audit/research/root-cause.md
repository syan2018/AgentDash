# 根因审计

## 结论

问题不是单个按钮失效，而是 Native Agent 命令、usage、Product side effect 与旧审计旁路在几次架构收口后没有一起迁移。

## 1. Native Submit 阻塞到回合终态

- `DashAgentCompleteService::execute` 先把外层 effect 写为 `Accepted`，随后直接 `await DashAgentService::execute`。
- `DashAgentService::execute_submit` 在写入 `InputAccepted + TurnStarted` 后继续运行完整 provider/tool loop，直到 Succeeded/Failed/Interrupted 才返回。
- Product command facade 与 HTTP Composer route 同步等待该调用，所以前端 `handleSubmit` 的 `isSending` 覆盖整个回合。
- `SessionChatView.handleSubmit` 在 `isSending` 为真时直接返回，因此 active turn 期间 Ctrl/Cmd+Enter 无法发出 Steer。
- Complete Agent 接口、Product handoff spec 与 Codex adapter 均允许普通执行返回 `Accepted`；Native 当前把 command admission 和 turn terminal 错绑成一次 HTTP 生命周期。

## 2. stop 状态依赖未被跨层测试保护

- Native history callback 已发布 durable `TurnStarted`，页面也把 `onLiveEvent` 接入 `planAgentRunLiveEvent`。
- planner 在 `turn_started` 时会刷新 Workspace snapshot；cancel/steer 命令则只从该 snapshot 的 active turn 派生。
- 运行视觉状态同时可由 canonical feed 的 `isReceiving` 触发，但 stop 是否可点只看 workspace command snapshot。两条状态若未在真实 Native active turn 上收敛，就会出现“显示运行、stop disabled/点击无动作”。
- 现有前端测试只覆盖手工构造的 command/feed 状态，没有覆盖 Native `Accepted -> TurnStarted -> workspace refresh -> cancel/steer` 生产链。

## 3. Native usage 在 Core 事件边界被丢弃

- `BridgeDashProvider` 从 provider response 取得 `context_input_tokens()` 与 `output`。
- `DashProviderEvent::Completed` 携带 token，但转换后的 `CoreEvent::ProviderRoundCompleted` 与 `DashCoreEvent::ProviderRoundCompleted` 只保留 round/finish reason。
- Native history 没有 usage payload，`canonical_projection` 也没有 `BackboneEvent::TokenUsageUpdated` 分支。
- 前端 reducer 已支持规范 usage 事件；缺失点完全在 Native owner 链。
- 生产模型配置已经提供 `context_window`，但 `bridge_dash_execution_dependencies` 创建 provider 时将它丢弃。

## 4. Task 状态刷新在统一 planner 重构时断链

- main reference 的 `SessionChatView` 在 successful completed `task_write` 后调用 `onTaskPlanChanged`。
- commit `06f5f2e28` 删除了组件内专用 live 扫描，改用 `planAgentRunLiveEvent`。
- 新 planner 只规划 turn/title/workspace presentation，没有规划 Task owner refresh；`SessionStatusBar` 只在 mount/target 变化时拉取。
- 正确修复位置是统一 typed live-event effect planner，而不是恢复组件内第二条扫描链。

## 5. `inspector://session` 是已断开的旧旁路

- 前端 pinned “审计” Tab 每 3 秒调用 `/agent-runs/{run}/agents/{agent}/runtime/context/audit`。
- API route 已在 Host/Product API 收口时删除，因此页面稳定返回 404。
- `useAgentRunWorkspaceState` 每次加载都显式写 `hook_runtime: null`，审计 Tab 的 Hook 卡片没有生产数据源。
- 后端仍创建 `InMemoryContextAuditBus` 并在 ContextFrame 构造阶段写入 Bundle fragment，但已没有 API/其他消费者。
- 当前上下文接纳、历史、live 与前端展示已由 Agent-native `ContextFrame` 统一表达；恢复 process-local polling bus 会重新制造第二权威。

## 参考范围

- Backend: Agent Core/Dash history/Native Complete Agent/Product command facade/API AppState
- Frontend: Managed Runtime feed/Session command state/control-plane planner/Task plan store/workspace tabs
- Git evidence: `06f5f2e28`, `a535ae016`
- main reference: `D:/ABCTools_Dev/AgentDash-main-reference` at `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`
