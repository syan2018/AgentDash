# 实施计划

## Phase 0: Evidence Lock

- [ ] 用户审阅 `prd.md`、`design.md`、`implement.md`。
- [ ] 将本任务从 planning 转入 implementation 前，确认 `AgentRun` 是否继续作为 URL/product identity。
- [ ] 为后续 sub-agent 固定 research manifest：启动/模型、消息命令、前端交互、resource surface 四个切片各自独立可验证。

## Phase 1: Conversation Snapshot Skeleton

- [ ] 在 backend contracts 中扩展或引入 `AgentConversationSnapshot` 相关 DTO。
- [ ] 在 application 层新增 `AgentConversationSnapshotResolver`，集中读取 run / agent / current frame / execution anchor / runtime execution state / pending queue / ProjectAgent preset。
- [ ] 保持现有 AgentRun routes，但所有 workspace projection 先经 resolver。
- [ ] 增加 focused tests：
  - no delivery runtime -> delivery missing
  - no current frame -> frame missing
  - running with active turn -> running command set
  - completed/idle -> ready send_next command set
  - terminal agent -> readonly terminal snapshot

## Phase 2: Model Config Resolver

- [ ] 新增 `ConversationModelConfigResolver` 或等价模块，定义 resolved/model_required 状态。
- [ ] ProjectAgent summary / draft AgentRun / AgentFrame runtime view 暴露同形 `effective_executor_config`，包含 source 与 validity。
- [ ] ProjectAgent 创建/编辑保存路径对可运行 Agent 执行模型解析：动态 Pi Agent 必须有 provider/model，或明确进入未配置状态。
- [ ] 修改 ProjectAgent executor config 合并：用户 override 按字段级合并 preset/frame defaults。
- [ ] 明确 executor-only override 保留合法 preset provider/model。
- [ ] 后端 workspace snapshot 返回当前权威 model config 和 selector default source。
- [ ] 前端 model selector 只显示并编辑 snapshot model config；localStorage 仅作为最近选择提示，不作为 ProjectAgent 默认来源。
- [ ] `createProjectAgentRun` 这类命令 store 改为传递真实错误，不再用 `null` 泛化 command failure。
- [ ] Tests:
  - ProjectAgent preset provider/model 在用户只传 executor 时保留。
  - 用户显式 provider/model override 覆盖 preset。
  - dynamic/default model 缺失时 snapshot 为 `model_required`。
  - `start_draft` 在 `model_required` 下不可发送且不进入 ProjectAgent materialization。
  - 后端“缺少模型选择”错误能被前端原样展示。

## Phase 3: Command Intent Resolver

- [ ] 新增 `ConversationCommandIntent` / `ConversationCommandSetView`，把 allowed command、keyboard mapping、precondition token 合并成一份 contract。
- [ ] 将 `/messages`、`/steering`、`/pending-messages`、`promote`、`resume`、`cancel` 入口统一经过 command precondition checker。
- [ ] 将 delivery state 显式拆成 `Ready.NoTurn`、`Starting.Claimed`、`Running.Active(turn_id)`、`Cancelling`、`Terminal.*`。
- [ ] `steer` 和 `promote_pending_to_steer` 只允许 `Running.Active(turn_id)` 的 command token；idle/ready/completed/claimed 状态没有 steer intent。
- [ ] `enqueue` 只允许 `Running.Active(turn_id)`，或产品明确允许 `Starting.Claimed` 队列时单独建语义；completed/idle snapshot 的 primary intent 是 `send_next`。
- [ ] pending auto-drain 和 hook auto-resume 都表现为系统 source 的 `Terminal.Completed -> Starting.Claimed`，避免前端同时看到 completed 可 send_next 与 pending 正在自动发送。
- [ ] Tests:
  - idle Ctrl/Cmd+Enter maps to send_next, not steer。
  - completed last turn maps to send_next, not enqueue。
  - `Starting.Claimed` 不暴露 steer/promote。
  - running with steer support maps Ctrl/Cmd+Enter to steer and validates active turn token。
  - running without steer support maps Ctrl/Cmd+Enter to enqueue or no secondary intent。
  - stale expected_turn_id returns structured stale snapshot conflict, not generic mismatch-only UX。

## Phase 4: Pending Queue Projection

- [ ] 将 pending queue view 升级为 queue mechanics + visible work + user attention。
- [ ] 后端只在有可见 pending work 或可恢复动作需要用户处理时设置 `user_attention`。
- [ ] 前端 `PendingMessageList` 只根据 snapshot pending display contract 渲染。
- [ ] Tests:
  - paused + no visible messages + terminal/ready cleanup -> no banner。
  - paused + visible messages -> banner with resume/delete controls。
  - interrupted/failed turn with queued messages -> user_attention true。
  - resume/delete 后 snapshot 不再保留误导性 paused banner。

## Phase 5: Resource Surface Resolver

- [ ] 在 AgentRun workspace snapshot 中加入 `resource_surface`，直接来自 current AgentFrame typed VFS surface。
- [ ] 合并 active workflow/lifecycle mount projection 到 resource surface。
- [ ] 增加资源一致性校验：active workflow projection、最终 persisted `AgentFrame.vfs_surface_json`、`resolveVfsSurface(session_runtime)` 必须同时包含相同 lifecycle mount。
- [ ] 修正 `session_runtime` VFS resolver 的 frame 选择策略，使 delivery runtime session 绑定到 delivery/accepted frame surface。
- [ ] 前端 `useAgentRunWorkspaceState` 停止把 `session_runtime` surface 作为 workspace panel 的事实源，改为消费 snapshot `resource_surface`。
- [ ] 保留 `session_runtime` resolver 作为 API 支撑能力或诊断路径，而非前端主路径。
- [ ] Tests:
  - ProjectAgent graphless run shows owner surface mounts。
  - ProjectAgent explicit lifecycle run shows owner surface plus lifecycle mount。
  - Workflow AgentCall shows node-scoped lifecycle mount。
  - workspace panel/resource browser and connector launch use matching mount ids。

## Phase 6: Frontend Integration

- [ ] `AgentRunWorkspacePage` 只把 snapshot 传给 chat/view/panel，不再组装业务状态。
- [ ] `SessionChatView` 接收 command model 和 keyboard mapping；移除本地 `primaryAction.kind === "enqueue"` 的 steer 推断。
- [ ] Model selector 展示 snapshot resolved/default/model_required 状态；localStorage 只作为用户最近选择提示，不作为权威默认。
- [ ] Pending UI 根据 `pending.user_attention` 和 `pending.visible_messages` 渲染。
- [ ] Workspace panel 直接消费 snapshot resource surface。
- [ ] Frontend focused tests:
  - draft model_required disables submit。
  - draft resolved model starts run with full executor/provider/model。
  - ready Ctrl/Cmd+Enter sends `send_next`。
  - running Ctrl/Cmd+Enter sends `steer` only when snapshot says so。
  - terminal with stale running bits remains readonly。
  - no pending visible work means no pending paused banner。
  - lifecycle mount appears in VFS tab for explicit lifecycle AgentRun。

## Phase 7: Misleading Path Eradication

- [ ] Build a code inventory for misleading paths before deleting: `SessionRuntimeControlView`, `SessionRuntimeActionSetView`, `ProjectAgentLaunchResult`, `launchProjectAgent`, ProjectAgent `/launch`, `primaryAction/secondaryAction`, `resolveVfsSurface({ source_type: "session_runtime" })` in AgentRun workspace, command store methods returning `null`, stale AgentRun action tests.
- [ ] Delete or internalize ProjectAgent `/launch` if it is no longer a product path; if a materialization helper remains, name it as internal control-plane materialization, not launch.
- [ ] Remove `SessionRuntimeControlView` from interactive frontend paths; keep Session APIs only for trace/events/context audit/tool approvals/terminal inspection.
- [ ] Replace `SessionChatControlState.primaryAction/secondaryAction` with command-list view models whose command semantics originate from snapshot commands.
- [ ] Remove frontend keyboard/button logic that branches on `primaryAction.kind`, `isRunning`, or `isEnqueueMode` for business decisions.
- [ ] Remove AgentRun workspace resource loading through `session_runtime` once snapshot `resource_surface` is available.
- [ ] Replace command store `Promise<T | null>` behavior with thrown/typed errors and update all callers.
- [ ] Delete tests that encode old ambiguity; replace them with snapshot command/resource/model tests.
- [ ] Run grep gates:
  - `rg -n "primaryAction|secondaryAction|SessionRuntimeControlView|SessionRuntimeActionSetView" packages/app-web/src crates/agentdash-contracts/src`
  - `rg -n "launchProjectAgent|ProjectAgentLaunchResult|/launch" packages/app-web/src crates/agentdash-api/src crates/agentdash-contracts/src`
  - `rg -n "resolveVfsSurface\\(\\{ source_type: \"session_runtime\"" packages/app-web/src/features/workspace-panel packages/app-web/src/pages`
  - `rg -n "return null" packages/app-web/src/stores`

## Phase 8: Specs And Final Gate

- [ ] Remove duplicated action derivation helpers after frontend consumes command model.
- [ ] Remove executor config merge behavior that replaces preset defaults wholesale.
- [ ] Update `.trellis/spec/backend/workflow/architecture.md` with conversation snapshot and command intent ownership.
- [ ] Update `.trellis/spec/backend/session/runtime-execution-state.md` with ready/running/pending/cancelling command semantics.
- [ ] Update `.trellis/spec/backend/vfs/architecture.md` with AgentRun resource surface projection.
- [ ] Update frontend spec with snapshot-driven UI and model selector ownership.
- [ ] Record the allowed remaining Session routes as trace/diagnostic surfaces so future work does not turn them back into command owners.

## Risky Files

- `crates/agentdash-contracts/src/workflow.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `crates/agentdash-api/src/routes/project_agents.rs`
- `crates/agentdash-api/src/session_construction.rs`
- `crates/agentdash-application/src/workflow/project_agent_run_start.rs`
- `crates/agentdash-application/src/workflow/agent_message.rs`
- `crates/agentdash-application/src/workflow/agent_steering.rs`
- `crates/agentdash-application/src/workflow/frame_construction/mod.rs`
- `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs`
- `crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs`
- `crates/agentdash-application/src/session/pending_queue.rs`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx`
- `packages/app-web/src/features/session/ui/composer/PendingMessageRow.tsx`
- `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts`
- `packages/app-web/src/features/executor-selector/ui/InlineModelSelector.tsx`
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts`
- `packages/app-web/src/stores/projectStore.ts`
- `packages/app-web/src/features/project/agent-preset-editor/form-state.ts`
- `packages/app-web/src/features/project/agent-preset-editor/preset-form-fields.tsx`
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts`
- `packages/app-web/src/services/lifecycle.ts`
- `packages/app-web/src/services/project.ts`
- `crates/agentdash-api/src/routes/sessions.rs`

## Validation Commands

```powershell
cargo test -p agentdash-application project_agent_run_start
cargo test -p agentdash-application workflow::agent_message
cargo test -p agentdash-application workflow::agent_steering
cargo test -p agentdash-api lifecycle_agents
cargo check -p agentdash-contracts -p agentdash-application -p agentdash-api
pnpm run contracts:check
pnpm --filter app-web run typecheck
pnpm --filter app-web run test -- AgentRunWorkspacePage
pnpm --filter app-web run test -- SessionChatView
pnpm --filter app-web run test -- PendingMessageRow
pnpm --filter app-web run test -- useAgentRunWorkspaceState
rg -n "primaryAction|secondaryAction|SessionRuntimeControlView|SessionRuntimeActionSetView" packages/app-web/src crates/agentdash-contracts/src
rg -n "launchProjectAgent|ProjectAgentLaunchResult|/launch" packages/app-web/src crates/agentdash-api/src crates/agentdash-contracts/src
rg -n "resolveVfsSurface\\(\\{ source_type: \"session_runtime\"" packages/app-web/src/features/workspace-panel packages/app-web/src/pages
rg -n "return null" packages/app-web/src/stores
git diff --check
```

## Implementation Notes For Sub-agents

- 启动/模型切片负责 ProjectAgent start、executor merge、model_required contract。
- 消息命令切片负责 send_next/enqueue/steer/promote/resume/cancel 的 shared precondition。
- 前端交互切片负责 composer keyboard/model selector/pending UI。
- resource surface 切片负责 AgentFrame VFS/lifecycle mount/workspace panel。
- 各切片必须写明依赖的 generated contract 字段，不能靠父任务隐含上下文。
