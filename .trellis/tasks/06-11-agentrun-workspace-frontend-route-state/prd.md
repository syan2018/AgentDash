# AgentRun 工作台前端路由与模型状态

Parent: `06-11-session-model-delivery-state-chain`

## Goal

移除 SessionPage 交互入口，建立 AgentRun Workspace 前端路由、页面状态和模型选择水合，让 draft 到正式运行的转场稳定落在 AgentRun/AgentFrame 状态上。

## Dependencies

- 依赖 `06-11-agentrun-workspace-api-contract` 的 generated TypeScript contracts。
- 依赖 `06-11-agentrun-runtime-trace-meta-convergence` 的 shell/meta 边界：工作台标题、侧栏列表和状态不再从 `session_meta` 派生。
- 依赖 `06-11-agentrun-delivery-command-receipts` 的 `client_command_id` 语义。
- 最终手动验证依赖 `06-11-launch-frame-hook-atomicity`，保证后端 frame/hook 状态稳定。

## Requirements

- App route 使用 `/agent-runs/new` 和 `/agent-runs/:runId/:agentId`。
- 删除或重命名 `SessionPage` 交互工作台，新增 `AgentRunWorkspacePage`。
- 所有 active list、Project Agent draft path、Run/Subject 回跳入口改为 AgentRun route。
- 新增 `useAgentRunWorkspaceState`，从 AgentRun Workspace API 获取 run/agent/frame/delivery/actions。
- AgentRun sidebar/list 使用 workspace/list shell，点击进入 `/agent-runs/:runId/:agentId`；RuntimeSession trace 入口显式使用 trace wording。
- 聊天组件使用 AgentRun workspace key 水合 executor config。
- 从 `frame_runtime.execution_profile` 的 snake_case `AgentConfig` 转为 executor selector source。
- localStorage 只保留最近选择，不覆盖 workspace 权威 execution profile。
- `start_draft` 和 `send_next` 生成并复用 `client_command_id`。
- transport failure 后以前一个 command id 恢复或重试，不重复提交新命令。

## Acceptance Criteria

- [ ] `rg -n "/session/new|/session/:sessionId|SessionPage" packages/app-web/src` 不再命中交互工作台。
- [ ] Project Agent draft route 为 `/agent-runs/new?...`。
- [ ] 首条消息 accepted 后导航到 `/agent-runs/{runId}/{agentId}`。
- [ ] 侧栏/快捷列表不以 `runtime_session_id` 和 `SessionMeta.title` 作为交互工作台入口事实源。
- [ ] 正式 workspace 下模型选择器从 frame execution profile 显示 provider/model/thinking。
- [ ] executor 为空时 discovered options 不会表现为不可选死状态；发送前展示明确模型配置错误。
- [ ] transport failure retry 复用原 `client_command_id`。
- [ ] 前端 focused tests 覆盖 draft -> AgentRun workspace executor hydration。
