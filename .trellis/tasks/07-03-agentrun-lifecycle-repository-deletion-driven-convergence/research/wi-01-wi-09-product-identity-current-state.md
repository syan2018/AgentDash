# Research: WI-01 / WI-09 product identity current state

- Query: 基于当前代码事实清点 API/contracts/frontend 中 `runtime_session_id` / `sessionId` / `delivery_runtime_ref` / stale guard / tool approval 的 product identity 残留，并拆出可执行小切片。
- Scope: internal
- Date: 2026-07-04

## Findings

### Files found

- `crates/agentdash-api/src/routes/sessions.rs` - raw `/sessions/*` diagnostic/read route，当前只挂 GET。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - AgentRun scoped workspace/runtime/mailbox/tool approval route 和 contract mapper。
- `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs` - AgentRun command response / accepted refs / runtime state DTO mapper。
- `crates/agentdash-api/src/routes/project_agents.rs` - ProjectAgent start response mapping。
- `crates/agentdash-api/src/routes/permission_grants.rs` - permission grant response mapping。
- `crates/agentdash-api/src/routes/workspace_module.rs` - workspace module present request 对 runtime session project 归属的校验。
- `crates/agentdash-contracts/src/runtime/workflow.rs` - workspace/conversation/lifecycle/read-model DTO。
- `crates/agentdash-contracts/src/agent/run_mailbox.rs` - AgentRun command / mailbox / fork / tool approval DTO。
- `crates/agentdash-contracts/src/agent/project_agent.rs` - ProjectAgent start result DTO。
- `crates/agentdash-contracts/src/system/permission.rs` - permission grant DTO。
- `crates/agentdash-contracts/src/surface/workspace_module.rs` - workspace module present DTO。
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs` - command snapshot/stale guard model。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs` - stale guard 校验逻辑。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs` - workspace/list projection 组装 delivery runtime trace meta。
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts` - 前端 AgentRun workspace state 持有 runtime session id。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` - 产品工作台将 runtime session id 写入 workspace runtime data。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts` - AgentRun chat model 仍携带 sessionId。
- `packages/app-web/src/features/session/model/streamTransport.ts` - AgentRun scoped stream transport 仍要求 `sessionId` 参数。
- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx` and `SessionChatViewParts.tsx` - context projection UI 在 AgentRun target 存在时仍先 gate `sessionId`。
- `packages/app-web/src/features/session/model/agentRunConversationFeed.ts` - AgentRun feed projection 合成 Backbone event 时用 runtime id 作为 `threadId/sessionId`。
- `packages/app-web/src/services/agentRunRuntime.ts` and `agentRunMailbox.ts` - 当前产品 runtime/mailbox service 已走 AgentRun scoped URL。
- `packages/app-web/src/services/session.ts` - raw `/sessions` service 注释为 diagnostic/legacy 查询。

### Related specs and task artifacts

- `.trellis/spec/backend/architecture.md` - API 层只做鉴权、DTO 和错误映射，业务进入 application；跨聚合一致性用显式 command port。
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession 是 delivery/trace substrate，AgentRun delivery/control command 使用 AgentRun workspace public identity。
- `.trellis/spec/backend/session/agentrun-mailbox.md` - mailbox command response 是 AgentRun workspace command contract，runtime id 只能作 nullable delivery/runtime trace ref。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - generated contracts 是前端 wire DTO 单源；ProjectAgent start 前端只用 `run_ref/agent_ref` 导航，不能从 runtime id / turn id 推断投递状态。
- `.trellis/spec/frontend/architecture.md` - AgentRun workspace 使用 AgentRun scoped runtime endpoints；RuntimeSession detail 仅是 trace/diagnostic 视角。
- `.trellis/spec/frontend/type-safety.md` - 前端不手写 generated DTO，也不做字段级 identity rebuild。
- `.trellis/spec/frontend/state-management.md` - AgentRun workspace command authority 来自 `ConversationCommandView` / stale guard / mailbox projection；RuntimeSession trace metadata 只进入 trace/feed/debug。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/inventory.md` - inventory 已校正 raw `/sessions/*` 当前是 read/diagnostic trace surface。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-01-runtime-session-product-internalization.md` - WI-01 目标是删除 RuntimeSession 产品写入口和前端 product identity 依赖。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-09-projection-permission-api-frontend.md` - WI-09 目标是 API/frontend product identity、permission 和 projection 收束。

### 1. Raw `/sessions` route current state

当前 raw `/sessions/*` route 没有产品写入口。router 只挂 `GET /sessions/{id}`、`GET /runtime-control`、`GET /meta`、`GET /state`、`GET /events`、`GET /context/projection`、`GET /lineage`、`GET /context/audit`、`GET /stream/ndjson`，见 `crates/agentdash-api/src/routes/sessions.rs:83-114`。`rg "axum::routing::(post|delete|patch|put)" crates/agentdash-api/src/routes/sessions.rs` 未找到写 route。

raw trace route 的权限检查通过 execution anchor 回到 LifecycleRun/Project：`ensure_session_permission` 注释写明 "Session trace 权限检查通过 RuntimeSessionExecutionAnchor 进入 LifecycleRun project"，实现先查 session meta，再 `execution_anchor_repo.find_by_session(session_id)`，最后 `load_project_with_permission`，见 `crates/agentdash-api/src/routes/sessions.rs:50-77`。这类 `runtime_session_id -> anchor -> project permission` 是可接受 diagnostic access，不是产品控制面。

AgentRun scoped runtime read endpoints 仍在服务端解析 current delivery runtime 后复用 session read helpers：`get_agent_run_runtime_control` 从 `context.delivery_runtime_session_id` 读取并调用 `sessions::load_session_runtime_control_view`，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1210-1223`；events/context/stream 同样先 `resolve_agent_run_delivery_runtime` 再调用 session read helper，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1232-1236`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:1298-1303`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:1327-1330`。这是内部 trace selection，后续实现不应围绕已不存在的 raw fork/rollback/delete/title patch/tool approval 写入口派发。

### 2. Contracts current product-path exposure

`ProjectAgentRunStartResult` 顶层已经不再暴露 `runtime_session_id` 或 `turn_id`。当前字段是 `command_receipt`、`accepted_refs`、`initial_message`、`effective_executor_config`、`agent`、`run_ref`、`agent_ref`、`frame_ref`、`subject_ref`，见 `crates/agentdash-contracts/src/agent/project_agent.rs:78-92`。但 API mapper 仍把 `dispatch.runtime_session_id` 和 `turn_id` 放进 `accepted_refs.runtime_session_ref` / `accepted_refs.turn_id`，见 `crates/agentdash-api/src/routes/project_agents.rs:246-269`；`initial_message` 继续复用 `AgentRunMessageCommandResponse`，见 `crates/agentdash-api/src/routes/project_agents.rs:270`。

`RuntimeSessionRefDto` 和 `RuntimeSessionTraceMeta` 本身是可接受 trace DTO：`RuntimeSessionRefDto { runtime_session_id }` 定义在 `crates/agentdash-contracts/src/runtime/workflow.rs:849-853`，`RuntimeSessionTraceMeta` 嵌套 `runtime_session_ref`、event seq、trace title 等 trace 字段，见 `crates/agentdash-contracts/src/runtime/workflow.rs:891-908`。问题不在 DTO 存在，而在 product DTO 顶层或 command precondition 消费它。

stale guard 仍是最明确的 product identity 污染。`ConversationCommandStaleGuardView` 暴露 `runtime_session_id`，见 `crates/agentdash-contracts/src/runtime/workflow.rs:1068-1082`；application model 同样持有 `runtime_session_id`，见 `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:141-149`；API contract mapper 来回映射该字段，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:2199-2209` 和 `crates/agentdash-api/src/routes/lifecycle_agents.rs:2217-2225`。后端 policy 还把 `command.stale_guard.runtime_session_id` 与当前 `availability.runtime_session_id` 比较，失败时返回 `runtime_session_mismatch`，见 `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:376-428`。同时 `conversation_snapshot_id` 字符串也包含 `runtime:{runtime}`，见 `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:858-872`。

AgentRun workspace / list DTO 仍把 delivery runtime ref 放在产品 read model 中：`AgentRunWorkspaceView.delivery_runtime_ref` / `delivery_trace_meta` 见 `crates/agentdash-contracts/src/runtime/workflow.rs:1374-1384`；`AgentRunView.delivery_runtime_ref` 见 `crates/agentdash-contracts/src/runtime/workflow.rs:1477-1489`；`AgentRunListChild.delivery_runtime_ref` 与 `AgentRunWorkspaceListEntry.delivery_runtime_ref` / `delivery_trace_meta` 见 `crates/agentdash-contracts/src/runtime/workflow.rs:1676-1727`。API mapper 直接把 application projection 的 `delivery_runtime_session_id` 映射成 `RuntimeSessionRefDto`，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1728-1738`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:1741-1767`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:1806-1816`。application list/query 由 current delivery selection 读 session meta 和 execution state，再返回 `delivery_runtime_session_id` / `delivery_trace_meta`，见 `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:79-88` 和 `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:286-344`。

conversation DTO 里也有 runtime ref 扩散：`ConversationExecutionView.runtime_session_ref` 见 `crates/agentdash-contracts/src/runtime/workflow.rs:1135-1144`；`AgentConversationLifecycleContext.delivery_runtime_ref` 见 `crates/agentdash-contracts/src/runtime/workflow.rs:1214-1221`；`AgentConversationFeedSnapshot.runtime_session_ref` 见 `crates/agentdash-contracts/src/runtime/workflow.rs:1351-1356`。

AgentRun command response 继续暴露 runtime trace refs / runtime state：`AgentRunMessageAcceptedRefs.runtime_session_ref` 见 `crates/agentdash-contracts/src/agent/run_mailbox.rs:91-104`；`AgentRunAcceptedRefs.runtime_session_ref` 和 `turn_id` 见 `crates/agentdash-contracts/src/agent/run_mailbox.rs:216-227`；`RuntimeSessionCommandStateDto` 挂在 `AgentRunMessageCommandResponse.runtime_state`，见 `crates/agentdash-contracts/src/agent/run_mailbox.rs:249-259` 和 `crates/agentdash-contracts/src/agent/run_mailbox.rs:274-291`。API mapper 将 result 的 `accepted_refs` 和 `runtime_state` 原样放进产品响应，见 `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:15-24`；accepted refs mapper 从 domain refs 映射 `runtime_session_id`，见 `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:52-75`；mailbox message view 也从 message runtime id 构造 accepted refs，见 `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:334-357`。

tool approval 当前已经收束到 AgentRun scoped 产品路径，且 product response 不暴露 runtime session id。API router 只挂 `/agent-runs/{run_id}/agents/{agent_id}/runtime/tool-approvals/{tool_call_id}/approve|reject`，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:184-190`；handler 内部用 `delivery_runtime_session_from_agent_run_context` 得到 session id 后调用 `session_control.approve_tool_call/reject_tool_call`，但 response 只返回 `run_ref`、`agent_ref`、`tool_call_id`，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1338-1415`。contract 也只定义 `approved/rejected + run_ref + agent_ref + tool_call_id`，见 `crates/agentdash-contracts/src/agent/run_mailbox.rs:108-124`。

permission grants 仍在通用产品 DTO 中返回 `source_runtime_session_id`：contract 定义见 `crates/agentdash-contracts/src/system/permission.rs:100-108`，API mapper 见 `crates/agentdash-api/src/routes/permission_grants.rs:137-142`。当前 `PermissionGrantCard` 只展示 status/scope/expiry/reason/paths/escalation/actions，未展示 source runtime id，见 `packages/app-web/src/features/permission/PermissionGrantCard.tsx:55-81`。因此它更像 audit meta 暴露在通用 DTO，而不是 UI 已依赖的 product identity。

`WorkspaceModulePresentRequest` 仍允许产品/模块展示请求携带 `runtime_session_id`：contract 注释称这是可选展示上下文，见 `crates/agentdash-contracts/src/surface/workspace_module.rs:198-208`；API 在 Project scoped present route 中用该 runtime id 反查 anchor 并校验 run.project_id，见 `crates/agentdash-api/src/routes/workspace_module.rs:128-135` 和 `crates/agentdash-api/src/routes/workspace_module.rs:169-194`。这不是 raw Session 写入口，但属于 product route 参数仍接受 runtime trace 坐标。

### 3. Frontend current product-path usage

AgentRun workspace state 仍把 delivery runtime ref 复制成 workspace state key。`AgentRunWorkspaceProjectionState` 有 `runtime_session_id`，见 `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:10-20`；load 时从 `workspace.delivery_runtime_ref?.runtime_session_id` 读取并写回 state，见 `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:138-157`。

AgentRun workspace page 使用该 state 作为产品工作台 runtime identity：页面读取 `deliveryRuntimeSessionId = agentRunWorkspaceState.runtime_session_id`，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:157-158`；`activeHookRuntime` 用 runtime session id 匹配 hook runtime，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:190-192`；`WorkspaceRuntimeData` 同时填 `sessionId` 和 `runtimeSessionId`，并把 `delivery_trace_meta.runtime_session_ref.runtime_session_id` 映射进 `sessionMeta.id`，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:511-525`。同页还把 `runtime_session_id` 写入 `/run/...` navigation state，但目标页当前只读取 `run_id/frame_id`，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:489-503` 和 `packages/app-web/src/pages/LifecyclePages.tsx:330-347`。

`WorkspaceRuntimeData` 类型仍把 `sessionId` / `runtimeSessionId` 作为一等字段，与 `agentRunRuntimeTarget` 并列，见 `packages/app-web/src/features/workspace-runtime/model/types.ts:45-69`。`WorkspacePanel` 又从 `runtimeData.runtimeSessionId ?? runtimeData.sessionId` 生成 `traceSessionId` 并传给 tab renderer，见 `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:37-45` 和 `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:233-240`。trace/terminal/context diagnostic tab 可以保留该 prop，但它不应是 WorkspacePanel product identity。

AgentRun chat model 继续携带 session id。`SessionChatModel` 类型要求 `sessionId: string | null`，见 `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts:70-75`；AgentRun control plane 将 `deliveryRuntimeSessionId` 传给 chat model，见 `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:283-307`。`SessionChatView` 用 `streamSessionId = sessionId ?? "__placeholder__"`，并把 `enabled` 设为 `sessionId !== null`，见 `packages/app-web/src/features/session/ui/SessionChatView.tsx:271-291`。这使 AgentRun scoped stream/feed 仍被 runtime trace id 是否存在 gate，即使 lower-level service 已支持 AgentRun target。

stream transport/service 已有 AgentRun scoped URL，但 option shape 仍要求 session id。`SessionStreamTransportOptions.sessionId` 是必填，见 `packages/app-web/src/features/session/model/streamTransport.ts:16-23`；`buildStreamEndpoint` 在 `agentRunTarget` 存在时会使用 `/api/agent-runs/.../runtime/stream/ndjson`，否则才用 `/api/sessions/{sessionId}/stream/ndjson`，见 `packages/app-web/src/features/session/model/streamTransport.ts:32-44`。`UseSessionStreamOptions.sessionId` 同样必填，见 `packages/app-web/src/features/session/model/useSessionStream.ts:38-47`；history page fetch 在 AgentRun target 存在时用 `fetchAgentRunRuntimeEvents`，否则用 `fetchSessionEvents(sessionId)`，见 `packages/app-web/src/features/session/model/useSessionStream.ts:240-264`。

context projection UI 在有 AgentRun target 时仍先 gate `sessionId`。`SessionProjectionView.refresh` 先 `if (!sessionId) return`，然后才在 `agentRunTarget` 存在时调用 `fetchAgentRunRuntimeContextProjection`，见 `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:337-365`。`ContextUsageRing` 也在 `if (!sessionId) return null` 后才渲染 `SessionProjectionView`，见 `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:90-128` 和 `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:200-208`。

AgentRun conversation feed adapter 将 `AgentConversationFeedSnapshot.runtime_session_ref` 当成合成 Backbone `threadId/sessionId`。`agentRunConversationFeedEvents` 从 `feed.runtime_session_ref?.runtime_session_id ?? ""` 生成 `threadId`，见 `packages/app-web/src/features/session/model/agentRunConversationFeed.ts:307-315`；合成 envelope 的 `notification.sessionId` 也写该 thread id，见 `packages/app-web/src/features/session/model/agentRunConversationFeed.ts:282-303`。下游 `SessionDisplayEntry` 仍要求 `sessionId`，见 `packages/app-web/src/features/session/model/types.ts:320-323`。这让产品 feed projection 为了复用 session reducer，把 runtime trace id 伪装成 thread identity。

产品 mutation service 已基本正确走 AgentRun scoped route。`agentRunRuntime.ts` 用 `AgentRunRuntimeTarget { runId, agentId }` 构造 runtime events/context/feed/tool approval URL，见 `packages/app-web/src/services/agentRunRuntime.ts:15-87`；`agentRunMailbox.ts` 的 submit/fork/mailbox/cancel 全部用 `agentRunScopedPath({ runId, agentId }, ...)`，见 `packages/app-web/src/services/agentRunMailbox.ts:15-24`、`packages/app-web/src/services/agentRunMailbox.ts:48-85`、`packages/app-web/src/services/agentRunMailbox.ts:117-124`。`ToolCallCardShell` 当前只在 `agentRunTarget != null` 时允许 approve/reject，没有 raw session fallback，见 `packages/app-web/src/features/session/ui/ToolCallCardShell.tsx:88-110`。

raw session frontend service 明确标成 diagnostic/legacy：`/sessions/{id}/state` 注释说明是 RuntimeSession trace 的诊断/legacy 查询入口，AgentRun/workspace 控制 UI 不从这里派生命令事实，见 `packages/app-web/src/services/session.ts:22-24`。Lifecycle/Task/Story 面板把 runtime ids 标为 trace 展示，见 `packages/app-web/src/pages/LifecyclePages.tsx:154-168`、`packages/app-web/src/features/task/task-subject-execution-panel.tsx:92-102`、`packages/app-web/src/features/story/story-subject-execution-panel.tsx:110-117`；这些属于可接受 diagnostic trace meta。

### 4. Acceptable diagnostic trace meta vs product identity pollution

可接受 diagnostic trace meta:

- raw `/sessions/*` GET read surface 和 stream，因 route 只读且权限从 anchor 派生，见 `crates/agentdash-api/src/routes/sessions.rs:83-114` 和 `crates/agentdash-api/src/routes/sessions.rs:50-77`。
- `RuntimeSessionRefDto`、`RuntimeSessionTraceMeta`、`RuntimeSessionTraceView`、`SessionRuntimeControlView` 这类名字明确的 trace/control DTO，见 `crates/agentdash-contracts/src/runtime/workflow.rs:849-908`、`crates/agentdash-contracts/src/runtime/workflow.rs:1591-1641`。
- AgentRun scoped runtime read route 内部解析 delivery runtime 后读取 session events/context/stream，前端请求参数仍是 `runId + agentId`，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1206-1238` 和 `packages/app-web/src/services/agentRunRuntime.ts:20-87`。
- Lifecycle/Task/Story execution panels 显式标注 "RuntimeSession trace"，只作诊断展示，见 `packages/app-web/src/pages/LifecyclePages.tsx:154-168`、`packages/app-web/src/features/task/task-subject-execution-panel.tsx:92-102`、`packages/app-web/src/features/story/story-subject-execution-panel.tsx:110-117`。
- terminal/event replay/context audit 等 tab content 需要 trace session id 读取 diagnostic data；但该 id 应作为 tab content prop，不作为 WorkspacePanel key 或 product command target。

产品 identity 污染:

- `ConversationCommandStaleGuardView.runtime_session_id` 和 command policy 的 `runtime_session_mismatch` 校验，见 `crates/agentdash-contracts/src/runtime/workflow.rs:1070-1082` 和 `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:423-428`。
- `conversation_snapshot_id` 包含 `runtime:{runtime}`，见 `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:858-872`。
- `AgentRunWorkspaceView` / `AgentRunWorkspaceListEntry` / `AgentRunView` 等产品 DTO 顶层的 `delivery_runtime_ref`，见 `crates/agentdash-contracts/src/runtime/workflow.rs:1374-1384`、`crates/agentdash-contracts/src/runtime/workflow.rs:1477-1489`、`crates/agentdash-contracts/src/runtime/workflow.rs:1701-1727`。
- `AgentRunMessageAcceptedRefs` / `AgentRunAcceptedRefs` 中的 `runtime_session_ref` 和 command response 中的 `runtime_state`，见 `crates/agentdash-contracts/src/agent/run_mailbox.rs:91-104`、`crates/agentdash-contracts/src/agent/run_mailbox.rs:216-227`、`crates/agentdash-contracts/src/agent/run_mailbox.rs:274-291`。
- 前端 `AgentRunWorkspaceProjectionState.runtime_session_id`、`WorkspaceRuntimeData.sessionId/runtimeSessionId`、`SessionChatModel.sessionId` 在 AgentRun product page 中作为 state/stream gate，见 `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:10-20`、`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:511-525`、`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:283-307`。
- `SessionProjectionView` 和 `ContextUsageRing` 在 AgentRun target 存在时仍要求 `sessionId`，见 `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:348-358` 和 `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:124-128`。
- `AgentConversationFeedSnapshot.runtime_session_ref` 被前端 adapter 合成 session/thread identity，见 `packages/app-web/src/features/session/model/agentRunConversationFeed.ts:307-315`。
- `WorkspaceModulePresentRequest.runtime_session_id` 作为 Project scoped present route 参数，见 `crates/agentdash-contracts/src/surface/workspace_module.rs:198-208` 和 `crates/agentdash-api/src/routes/workspace_module.rs:128-135`。
- `PermissionGrantResponse.source_runtime_session_id` 暴露在通用 permission DTO 中，见 `crates/agentdash-contracts/src/system/permission.rs:100-108`；当前 UI 没显示，适合降级为 audit/detail。

## Executable slices

### Slice 1 - Remove runtime session from AgentRun command stale guard

Priority: P0 / first.

Deletion target: 删除 product command precondition 中的 `runtime_session_id` 组合，以及 `snapshot_id` 字符串里的 runtime component。保留 `snapshot_id + run_id + agent_id + frame_id + active_turn_id` 的现有 guard 语义；不引入新的 revision 概念，把 current delivery generation 留给 WI-06。

Allowed write range:

- `crates/agentdash-contracts/src/runtime/workflow.rs`
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs`
- `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- generated TS contracts under `packages/app-web/src/generated/`
- frontend stale guard tests under `packages/app-web/src/features/agent-run-workspace/**` and `packages/app-web/src/services/agentRunMailbox.test.ts`

Contract regeneration: required.

Validation commands:

- `pnpm run contracts:check`
- `pnpm run frontend:check`
- `cargo test -p agentdash-application-agentrun`
- `cargo test -p agentdash-api`
- `rg "runtime_session_id" crates/agentdash-contracts/src/runtime/workflow.rs crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs packages/app-web/src/features/agent-run-workspace packages/app-web/src/generated/agent-run-mailbox-contracts.ts`

Parallelism:

- Can run in parallel with WI-10 research/implementation if WI-10 does not touch shared generated contract outputs.
- Can run in parallel with WI-12 ledger/research because no migration is expected.
- Should not run in parallel with another WI-01/WI-09 contracts worker because generated contracts and command DTOs overlap.

### Slice 2 - Contain command response runtime refs as diagnostic trace evidence

Priority: P1 / after Slice 1.

Deletion target: Remove `runtime_session_ref` from product `AgentRunMessageAcceptedRefs` / `AgentRunAcceptedRefs`, and stop placing raw runtime refs in `ProjectAgentRunStartResult.accepted_refs`. Keep run/agent/frame/AgentRun turn refs as product facts. If runtime evidence is still needed for diagnostics, put it behind an explicitly named trace/debug field or rely on workspace/detail diagnostic endpoints; do not keep it in accepted refs.

Allowed write range:

- `crates/agentdash-contracts/src/agent/run_mailbox.rs`
- `crates/agentdash-contracts/src/agent/project_agent.rs` if response shape needs a named diagnostic field
- `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs`
- `crates/agentdash-api/src/routes/project_agents.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` fork/submit mappers that reuse accepted refs
- generated TS contracts and frontend tests consuming accepted refs/fork outcome

Contract regeneration: required.

Validation commands:

- `pnpm run contracts:check`
- `pnpm run frontend:check`
- `cargo test -p agentdash-contracts`
- `cargo test -p agentdash-api`
- `rg "runtime_session_ref" crates/agentdash-contracts/src/agent/run_mailbox.rs crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs crates/agentdash-api/src/routes/project_agents.rs packages/app-web/src/generated/agent-run-mailbox-contracts.ts packages/app-web/src/generated/project-agent-contracts.ts`

Parallelism:

- Can run with WI-10 if generated workflow contracts are not touched there.
- Can run with WI-12 only if WI-12 is not regenerating contracts or editing the same API mapper.
- Should be serial after Slice 1 because both rewrite command contract shape.

### Slice 3 - Make AgentRun frontend stream/context/feed independent of `sessionId`

Priority: P2 / after stale guard shape is stable; can start after Slice 1 even before Slice 2 if generated fields still exist.

Deletion target: Stop filling `WorkspaceRuntimeData.sessionId/runtimeSessionId` and `SessionChatModel.sessionId` from AgentRun delivery runtime for product path. For AgentRun pages, shared session components should run from `agentRunRuntimeTarget`; trace session id remains optional diagnostic prop only for tabs that explicitly read raw trace data.

Allowed write range:

- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts`
- `packages/app-web/src/features/workspace-runtime/model/types.ts`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts`
- `packages/app-web/src/features/session/model/streamTransport.ts`
- `packages/app-web/src/features/session/model/useSessionStream.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.ts`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx`
- `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx`
- `packages/app-web/src/features/session/model/agentRunConversationFeed.ts`
- targeted frontend tests under the same features

Contract regeneration: not required unless paired with Slice 2.

Validation commands:

- `pnpm run frontend:check`
- `pnpm --filter app-web test -- --run SessionChatView SessionProjectionView`
- `pnpm --filter app-web test -- --run useAgentRunWorkspaceState`
- `rg "sessionId: deliveryRuntimeSessionId|runtimeSessionId: deliveryRuntimeSessionId|enabled: hasRuntimeTraceSession|if \\(!sessionId\\)" packages/app-web/src/pages packages/app-web/src/features/session packages/app-web/src/features/agent-run-workspace packages/app-web/src/features/workspace-panel`

Parallelism:

- Can run in parallel with WI-10 and WI-12 if they do not touch app-web.
- Should not run in parallel with another frontend contract/generated consumer slice.
- Can be checked independently from backend if contracts are unchanged.

### Slice 4 - Clean product read-model/list route params and permission audit leakage

Priority: P3 / after command and main workspace path are clean.

Deletion target: Remove `delivery_runtime_ref` from product workspace list entries and child list rows; keep `delivery_trace_meta` only where explicitly diagnostic and not required for list identity. Remove or narrow `WorkspaceModulePresentRequest.runtime_session_id` from user-open product path, replacing it with existing Project/AgentRun context or leaving it only for agent-tool/diagnostic flow. Move `PermissionGrantResponse.source_runtime_session_id` out of general product card DTO into audit/detail if still needed.

Allowed write range:

- `crates/agentdash-contracts/src/runtime/workflow.rs`
- `crates/agentdash-contracts/src/surface/workspace_module.rs`
- `crates/agentdash-contracts/src/system/permission.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `crates/agentdash-api/src/routes/workspace_module.rs`
- `crates/agentdash-api/src/routes/permission_grants.rs`
- generated TS contracts
- `packages/app-web/src/services/workspaceModule.ts`
- list/detail UI consumers under `packages/app-web/src/pages`, `packages/app-web/src/features/workspace-panel`, `packages/app-web/src/features/permission`

Contract regeneration: required.

Validation commands:

- `pnpm run contracts:check`
- `pnpm run frontend:check`
- `cargo test -p agentdash-api`
- `rg "delivery_runtime_ref|source_runtime_session_id|runtime_session_id" crates/agentdash-contracts/src/runtime/workflow.rs crates/agentdash-contracts/src/surface/workspace_module.rs crates/agentdash-contracts/src/system/permission.rs packages/app-web/src/pages packages/app-web/src/features/permission packages/app-web/src/services/workspaceModule.ts`

Parallelism:

- Can run with WI-10 only if WI-10 is not editing lifecycle/workflow DTOs or generated workflow contracts.
- Can run with WI-12 if no migration is introduced. If permission audit split requires schema, coordinate with WI-12 and serialize migration.
- Should be after Slice 2/3 so list/detail cleanup does not fight main workspace runtime-data changes.

## Recommended next implementation slice

Start with Slice 1: remove `runtime_session_id` from AgentRun command stale guard and `conversation_snapshot_id`. It deletes a concrete old product identity combination across contracts, API mapper, application policy, generated TS, and frontend tests without touching database migration or WI-06 current delivery storage. It also reduces downstream conflict risk because later frontend and command response cleanup can stop preserving runtime ids for stale-precondition compatibility.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task; this research used the explicit task path supplied in the worker prompt.
- Raw `/sessions/*` product write route was not found in current `sessions.rs`; do not dispatch implementation around raw Session fork/rollback/delete/title patch/tool approval write handlers unless new code reintroduces them.
- Tool approval raw-session frontend fallback was not found. Current product UI requires `agentRunTarget` for approve/reject and calls AgentRun scoped service.
- `ProjectAgentRunStartResult` no longer has top-level `runtime_session_id` / `turn_id`; remaining exposure is via nested `accepted_refs` and `initial_message`.
- I did not run `git diff --check` or `git status` because this research worker role forbids git operations. The only intended write is this research file.
