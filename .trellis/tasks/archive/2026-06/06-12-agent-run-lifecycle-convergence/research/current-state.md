# 当前状态证据

## 启动与模型配置入口

- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:382` 只检查 `executorConfig.executor` 非空；`provider_id/model_id` 可以为空。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:156` 将 `agentDefaults` 作为初始 source，但只在组件生命周期与 hydration key 中生效。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:232` 构造发送给后端的 `executorConfig`，provider/model 为空时直接省略。
- `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:113` 说明当前优先级是 initialSource > localStorage > 空值；这是前端本地状态，不是后端权威会话事实。
- `packages/app-web/src/features/executor-selector/model/useExecutorDiscoveredOptions.ts:12` 初始 discovered options 包含 `default_model`，但当前没有作为发送前权威 resolved model 写回。
- `packages/app-web/src/features/project/agent-preset-editor/preset-form-fields.tsx:241` ProjectAgent preset 表单提供“不指定模型”选项。
- `packages/app-web/src/features/project/agent-preset-editor/form-state.ts:73` `formToPreset` 只在 provider/model 非空时写入 config。
- `packages/app-web/src/stores/projectStore.ts:386` `createProjectAgentRun` catch 后返回 `null`，页面随后用泛化文案覆盖真实 API 错误。
- `crates/agentdash-api/src/routes/project_agents.rs:262` 将 request executor_config 反序列化后交给 `ProjectAgentRunStartService`。
- `crates/agentdash-application/src/workflow/project_agent_run_start.rs:285` 首轮消息把 command executor_config 传入 `AgentRunMessageService`。
- `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:70` ProjectAgent composer 调用 `merge_user_executor_config(user_config, preset_config)`。
- `crates/agentdash-application/src/workflow/frame_construction/mod.rs:276` 当前 merge 只补 system prompt/mode，不补 preset provider/model；executor-only user config 会覆盖掉 preset model/provider。

## 消息命令入口

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:55` 注册 AgentRun workspace、messages、steering、pending、resume、delete、promote、cancel routes。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1190` `ensure_send_next_allowed` 允许 idle/completed/failed/interrupted 发送下一轮，拒绝 running/cancelling。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1172` `ensure_pending_enqueue_allowed` 只允许 running。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1233` pending enqueue 在 completed/idle 等状态下返回“请直接发送下一轮消息”类 conflict。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:957` `steer_runtime_session` 校验 expected runtime session 与 expected turn；snapshot/前端状态过期时会触发 mismatch。
- `crates/agentdash-application/src/workflow/agent_steering.rs:135` steering service 再次检查 runtime execution state 必须是 running 且有 active turn。
- `crates/agentdash-application/src/session/hub_support.rs:168` runtime turn state 已区分 `Idle`、`Claimed`、`Active`、`Cancelling`；AgentRun projection 当前没有把 `Claimed` 与 `Active(turn_id)` 作为 command state 区分。

## 前端交互入口

- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts:90` 前端从 workspace actions/control_plane 派生 `primaryAction/secondaryAction`。
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts:93` running + enqueue enabled 时把 enqueue 设为 primary，并把 steer 设为 secondary。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:486` 键盘事件本地处理 Enter/Ctrl+Enter。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:499` 当 primary 是 enqueue 且 secondary enabled 时，Ctrl/Cmd+Enter 直接提交 steer。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:462` steer request 携带 `runtimeControl.delivery_trace_meta.last_turn_id` 作为 expected turn。

## Pending queue 入口

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:706` workspace projection 调用 `pending_queue.is_paused(session_id)`。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:843` `pending_queue_state_view` 只要有 pause reason 就设置 paused。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:653` 前端在 `pendingMessages.length > 0 || pendingQueue?.paused` 时展示 pending list。
- `packages/app-web/src/features/session/ui/composer/PendingMessageRow.tsx:31` `messages.length === 0 && !queue?.paused` 才不渲染；因此 paused 且无消息仍会展示“Pending 队列已暂停”。

## Resource surface 与 lifecycle mount 入口

- `crates/agentdash-api/src/session_construction.rs:12` 已有从 runtime session anchor 到 AgentFrame typed VFS 的 resolver。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:540` AgentRun workspace projection 读取 current AgentFrame，并返回 frame runtime view。
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:134` 前端先 fetch AgentRun workspace。
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:136` 前端再用 `delivery_runtime_ref.runtime_session_id` 调 `resolveVfsSurface({ source_type: "session_runtime" })`。
- `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:77` ProjectAgent composer 会解析 active workflow projection；但前端 workspace panel 是否可见 lifecycle mount 取决于 session_runtime surface 解析路径，而不是 AgentRun workspace snapshot 直接声明。
- `crates/agentdash-application/src/session/launch/commit.rs:157` pending frame commit 会用 `accepted_capability_state` 重写 `vfs_surface_json`；如果 accepted capability state 未同步 lifecycle VFS，最终 frame surface 会丢 mount。

## Architecture Assessment

- `RuntimeSession` 仍应是 delivery/trace substrate；AgentRun conversation state 应由 lifecycle run/agent/frame/anchor/runtime/pending/model/resource facts 投影。
- 当前最大问题是同一事实被多处推断：模型默认值、命令可用性、keyboard mapping、pending attention、resource surface 都缺少一个后端权威 snapshot。
- 正确收束点不是单个 route，而是 shared resolver + generated contract + frontend snapshot consumption。
- 会话状态 resolver 需要保留 runtime 内部 `Claimed`/`Active` 差异；resource resolver 需要同时验证 active workflow、persisted frame VFS、resolved VFS 三者一致。

## Misleading Path Inventory

- `crates/agentdash-api/src/routes/project_agents.rs:82` 仍暴露 ProjectAgent `/launch`；`crates/agentdash-contracts/src/project_agent.rs:55` 和 frontend `ProjectAgentLaunchResult` 会让该路径看起来仍是 ProjectAgent 运行入口。
- `crates/agentdash-contracts/src/workflow.rs:1144` / `1153` 仍有 `SessionRuntimeActionSetView` 与 `SessionRuntimeControlView`；它们和 AgentRun workspace actions 并存时会暗示 RuntimeSession 是交互控制面。
- `crates/agentdash-api/src/routes/sessions.rs:88` 暴露 `/sessions/{id}/runtime-control`；它应只保留为 runtime diagnostic 或被 AgentRun snapshot 替代。
- `packages/app-web/src/services/lifecycle.ts:59` 提供 `fetchSessionRuntimeControl`；实现阶段要确认没有交互 UI 继续消费它。
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts:37` 仍输出 `SessionChatControlState` 的 `primaryAction/secondaryAction`，这是前端二次 command mental model。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:499`、`SessionChatViewParts.tsx:321`、`ComposerSendButton.tsx:37` 将 command 语义拆在多个组件里。
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:97` 和 `useAgentRunWorkspaceState.ts:138` 都可从 `session_runtime` 解析 VFS；AgentRun workspace 主路径应转为 snapshot `resource_surface`。
- `packages/app-web/src/stores/projectStore.ts:386` 的 command store 返回 `null` 会吞掉真实 API 错误，是误导性失败路径。
- `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:132` 仍以 running projection 推导 Ctrl/Cmd+Enter steer；实现后应改成 command list 快捷键合同测试。
