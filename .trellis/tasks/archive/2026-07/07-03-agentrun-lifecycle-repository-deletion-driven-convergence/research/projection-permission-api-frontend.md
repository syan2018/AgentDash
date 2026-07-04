# Projection / Permission / API + Frontend Identity 研究结论

研究边界：本结论只从代码、contracts、migration、tests/可执行事实推导；未读取 `.trellis/tasks/` 下既有规划文档或 references。

## 基本真理

1. 产品身份必须是用户能理解并能长期稳定引用的业务对象。AgentRun workspace 的产品身份是 `run_id + agent_id`，project 页面身份是 `project_id`，runtime session 只是某次 delivery/trace 的技术坐标。

   证据：产品路由已经是 `/agent-runs/:runId/:agentId`，前端启动后导航使用 `response.run_ref.run_id` 和 `response.agent_ref.agent_id`，而不是 `runtime_session_id`（`packages/app-web/src/App.tsx:155`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:397-405`）。后端 AgentRun scoped runtime API 也已经通过 `run_id + agent_id` 解析当前 delivery runtime（`crates/agentdash-api/src/routes/lifecycle_agents.rs:1155-1290`）。

2. Projection 是可从事实重放或重新派生的读模型；如果一个字段决定“当前绑定到谁”“谁能控制”“命令是否已经接收”，它就不是 projection，而是 state/binding。

   必须可重建的 projection/read model：

   - workspace snapshot：由 run/agent、current delivery binding、session meta/execution state、frame/VFS、mailbox、settings、lifecycle associations 派生（`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:67-279`）。
   - workspace list：workspace snapshot 的轻量派生，不能成为新的事实源（`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:282-347`）。
   - conversation feed：由 context envelope/session projection 和 runtime events 派生；当前实现明确说 feed 只表达 inherited projection seed，完整 runtime 历史由 AgentRun scoped runtime events/stream 提供（`crates/agentdash-application-agentrun/src/agent_run/conversation_feed.rs:1-6`, `crates/agentdash-application-agentrun/src/agent_run/conversation_feed.rs:116-147`）。
   - runtime trace view：由 `session_events`、`SessionMeta`、anchor 派生（`crates/agentdash-application-lifecycle/src/presentation_read_model.rs:92-122`）。
   - context projection：由 session events、projection head、compaction segments/checkpoints 派生；head/segment 是可重建的加速结构，不是权限或产品身份（`crates/agentdash-application-runtime-session/src/session/context_projector.rs:67-111`, `crates/agentdash-application-runtime-session/src/session/context_projector.rs:164-235`）。
   - lifecycle view：由 lifecycle run/agent、runtime nodes、subject associations、execution anchors、lineage 派生（`crates/agentdash-application-ports/src/lifecycle_read_model.rs:15-104`）。

   实际是 state/binding 的对象：

   - `lifecycle_agents.current_delivery_*`：当前 delivery runtime 绑定，决定 AgentRun scoped API 应解析到哪个 runtime（`crates/agentdash-infrastructure/migrations/0017_lifecycle_agent_current_delivery_binding.sql:1-8`）。
   - `runtime_session_execution_anchors`：runtime session 到 run/frame/agent/orchestration 的执行锚点（`crates/agentdash-infrastructure/migrations/0001_init.sql:533-545`）。
   - mailbox messages/states：命令队列、barrier、drain、pause/resume 状态，是 durable command state（`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:59-85`, `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:214-222`）。
   - command receipts：幂等与命令接收事实（`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:1-21`, `crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:27-32`）。
   - permission grants：授权状态机，不是可随意重建的视图（`crates/agentdash-domain/src/permission/entity.rs:18-50`, `crates/agentdash-domain/src/permission/entity.rs:115-185`）。
   - active grant admission projection：由 grant state 派生出的 admission view，可重建，但 grant 本身是事实（`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:25-57`, `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:272-298`）。
   - fork lineage：父子 run/agent 关系与 fork point，是业务 lineage binding（`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1-18`）。

3. 权限必须分三层：Project access、AgentRun control、Runtime/Tool admission。Project Use 只能说明“这个用户可以进入项目并使用项目能力”，不能自动变成“可以控制别人正在运行的 AgentRun”。

   证据：Project 权限目前只有 `Use / Configure / ManageSharing`，`Use` 对成员开放（`crates/agentdash-domain/src/project/authorization.rs:47-70`）。composer submit 已经隐含承认 owner 边界：非 run creator 提交会自动 fork，而不是写入父 run（`crates/agentdash-api/src/routes/lifecycle_agents.rs:647-769`）。但 cancel、mailbox mutate、tool approval/reject 仍主要以 Project Use + command policy 解析 runtime，需要补上统一的 owner/control grant check（`crates/agentdash-api/src/routes/lifecycle_agents.rs:883-1152`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1155-1268`）。

4. `runtime_session_id` 可以出现在 diagnostic/runtime trace/audit DTO 里，但不应出现在产品 API 的主 identity、产品列表、composer stale guard、前端 workspace model 的必填字段里。

   证据：contracts 同时存在稳定的 `AgentRunRefDto { run_id, agent_id }` 和技术性的 `RuntimeSessionRefDto { runtime_session_id }`（`crates/agentdash-contracts/src/runtime/workflow.rs:834-853`）。但多个产品 DTO 把 runtime session 暴露到了顶层或 stale guard 中（`crates/agentdash-contracts/src/agent/project_agent.rs:78-96`, `crates/agentdash-contracts/src/runtime/workflow.rs:1069-1081`, `crates/agentdash-contracts/src/runtime/workflow.rs:1373-1409`, `crates/agentdash-contracts/src/runtime/workflow.rs:1701-1729`）。前端 `WorkspaceRuntimeData` 又把同一个 delivery runtime 填成 `sessionId` 和 `runtimeSessionId`（`packages/app-web/src/features/workspace-runtime/model/types.ts:45-69`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:511-525`）。

## 推荐设计

### 1. Projection / state 分层

最小正确分层如下：

```text
Project
  └─ AgentRun + Agent
       ├─ state/binding
       │    ├─ current_delivery_runtime_session_id
       │    ├─ runtime_session_execution_anchor
       │    ├─ mailbox messages/states
       │    ├─ command receipts
       │    ├─ permission grants
       │    └─ fork lineage
       └─ read models / projections
            ├─ workspace snapshot
            ├─ conversation feed seed
            ├─ lifecycle view
            ├─ runtime trace view
            └─ context projection
```

具体判断规则：

- 能从 event、state table、anchor、grant、mailbox、lineage 重新推导的，允许缓存为 projection，但不能成为业务写入的 source of truth。
- 会改变命令路由、当前 delivery、权限生效范围、幂等行为、fork 亲缘关系的，必须作为 state/binding 管理。
- `session_projection_heads` / `session_projection_segments` / `session_compactions` 是 runtime context 的 checkpoint 层。它们优化重建成本，但产品层不能引用它们作为身份或权限依据（`crates/agentdash-infrastructure/migrations/0001_init.sql:547-627`, `crates/agentdash-infrastructure/migrations/0001_init.sql:972-979`）。

### 2. Workspace snapshot、conversation feed、runtime trace、context projection、lifecycle view

workspace snapshot 是产品聚合根视图。它回答“这个 AgentRun workspace 当前能做什么、显示什么、命令是否可用”。它可以包含 shell、conversation command availability、mailbox、resource surface、lineage、必要的 trace meta，但不能要求前端持有 runtime session 才能操作 workspace。现有 `AgentRunWorkspaceView` 方向正确：它以 `run_ref`、`agent_ref`、`project_id` 为顶层（`crates/agentdash-contracts/src/runtime/workflow.rs:1373-1409`）。

conversation feed 是 workspace 的消息视图，不是 runtime trace 的替代品。feed 应只返回投影后的消息、projection head/replay start、必要的 count/version。由于调用路径已经是 AgentRun scoped，response 不需要再把 `runtime_session_ref` 当产品身份回传（`crates/agentdash-contracts/src/runtime/workflow.rs:1271-1297`, `crates/agentdash-contracts/src/runtime/workflow.rs:1350-1368`）。

runtime trace view 是诊断和调试层。它保留 `runtime_session_id`，返回 session events、execution state、stream、tool approval transport 细节。产品 workspace 如果需要 trace，只通过 AgentRun scoped route 让服务端解析当前 delivery runtime；前端不把 session id 作为导航或 command target（`crates/agentdash-api/src/routes/lifecycle_agents.rs:398-430`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1155-1268`）。

context projection 是模型上下文/audit 层。它回答“模型实际看见了什么上下文”，由 session projection head、compaction、events 重建。它可以被 workspace 展示，但不能反向决定 workspace 命令权限或产品 identity（`crates/agentdash-application-runtime-session/src/session/eventing.rs:424-433`, `crates/agentdash-application-runtime-session/src/session/eventing.rs:501-512`, `crates/agentdash-application-runtime-session/src/session/eventing.rs:700-788`）。

lifecycle view 是运行编排和 lineage 的控制账本视图。它应该显示 run、agent、frame、subject association、runtime node、attempt，但不承载 chat/composer/mailbox 的产品命令状态。`runtime_session_id` 在这里是 trace link，不是 lifecycle entity 的主键（`crates/agentdash-application-ports/src/lifecycle_read_model.rs:84-104`, `crates/agentdash-contracts/src/runtime/workflow.rs:1530-1565`）。

### 3. 最小正确权限模型

权限边界应收敛为三步判定：

1. Project Use：用户能看见项目、进入项目、启动自己的 AgentRun、读取可见 AgentRun 的 workspace/read model、从可见 AgentRun fork 出自己的 child run。
2. AgentRun owner/control grant：用户能修改一个既有 AgentRun 的控制面，包括向父 run 写 composer input、promote/delete/resume mailbox、cancel、approve/reject tool call。owner 是 `run.created_by_user_id` 的隐式控制者；control grant 是显式委派控制权。
3. Runtime/Tool admission：agent 在 runtime 内尝试调用工具或访问资源时，由 PermissionGrant state + active grant projection 决定 admission。`source_runtime_session_id` 是审计来源，不是 grant 生效锚点；生效锚点应是 `effect_frame_id`/scope（`crates/agentdash-domain/src/permission/entity.rs:18-50`, `crates/agentdash-domain/src/permission/value_objects.rs:76-120`, `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:272-298`）。

各动作的最小授权：

- read workspace/feed/context/trace：Project Use；trace 通过 AgentRun route 解析 runtime，不暴露 session route 给产品 UI。
- start run：Project Use，创建者自动成为 owner。
- composer submit：Project Use + owner/control grant 才写入目标 run；非 owner 对可见 run 的 submit 返回 fork outcome，当前实现已经接近这个模型（`crates/agentdash-api/src/routes/lifecycle_agents.rs:710-759`）。
- fork：Project Use 足够，因为它创建当前用户控制的新 child run，不改变父 run。
- cancel：Project Use + owner/control grant。
- mailbox promote/delete/resume：Project Use + owner/control grant。
- tool approve/reject：Project Use + owner/control grant；tool admission 本身仍由 PermissionGrant/active grants 决定。
- project run delete：不属于普通 Project Use。最小正确模型是 owner 删除自己的 run，或 Project Configure/ManageSharing 级别的项目治理删除。当前 delete route 使用 `ProjectPermission::Use`，需要收紧（`crates/agentdash-api/src/routes/lifecycle_agents.rs:276-305`）。

control grant 的最小实现形态：不要扩展 Project Role。Project Role 回答“能不能使用项目”；AgentRun control grant 回答“能不能控制这个 run”。可以建独立的 `agent_run_control_grants(run_id, agent_id?, grantee_user_id, scope, created_by_user_id, created_at, revoked_at)`，也可以复用现有 lifecycle association，但必须让 actor subject 和业务 subject 明确分离，避免把 story/task/routine 的 subject association 误当成人的控制权。

### 4. API 和前端产品 identity

API 产品面统一使用：

```rust
AgentRunRefDto {
    run_id,
    agent_id,
}
```

runtime trace 面单独使用：

```rust
RuntimeTraceRefDto {
    runtime_session_id,
}
```

只有 diagnostic/trace/audit DTO 暴露 `runtime_session_id`。产品 DTO 即使内部需要通过 runtime 解析，也只返回 run/agent identity、status、receipt、message、fork outcome。

不应继续暴露 runtime session 作为产品 identity 的 DTO：

- `ProjectAgentRunStartResult.runtime_session_id` 和顶层 `turn_id`：启动响应应返回 `run_ref`、`agent_ref`、`frame_ref`、`initial_message`、`command_receipt`、`accepted_refs` 的产品结果；runtime trace 需要时放进嵌套 `runtime_trace_meta` 或诊断 endpoint（`crates/agentdash-contracts/src/agent/project_agent.rs:78-96`, `crates/agentdash-api/src/routes/project_agents.rs:263-312`）。
- `AgentRunWorkspaceListEntry.delivery_runtime_ref` / `delivery_trace_meta`：列表应展示 shell/status/last_activity，不应把 trace id 扩散到列表状态（`crates/agentdash-contracts/src/runtime/workflow.rs:1701-1729`）。
- `AgentRunView.delivery_runtime_ref`：lifecycle run view 可有 trace link，但产品 run view 不应把 delivery runtime 作为主字段（`crates/agentdash-contracts/src/runtime/workflow.rs:1476-1494`）。
- `AgentConversationLifecycleContext.delivery_runtime_ref`：conversation lifecycle context 应表达 run/agent/frame/subject，runtime trace 放到 trace meta（`crates/agentdash-contracts/src/runtime/workflow.rs:1205-1241`）。
- `ConversationCommandStaleGuardView.runtime_session_id`：stale guard 应使用 `snapshot_id`、`run_id`、`agent_id`、`frame_id`、`active_turn_id`、`workspace_revision` 或 `delivery_generation`，而不是 runtime session id（`crates/agentdash-contracts/src/runtime/workflow.rs:1069-1081`）。
- `AgentConversationFeedSnapshot.runtime_session_ref`：feed route 本身已经由 AgentRun target 定位；runtime ref 仅可作为 debug meta（`crates/agentdash-contracts/src/runtime/workflow.rs:1350-1368`）。
- `AgentRunMessageAcceptedRefs.runtime_session_ref` / `AgentRunAcceptedRefs.runtime_session_ref`：产品响应应以 receipt/outcome/run/agent/message 为主；runtime ref 可移到 debug trace nested field（`crates/agentdash-contracts/src/agent/run_mailbox.rs:91-106`, `crates/agentdash-contracts/src/agent/run_mailbox.rs:196-208`）。
- `/sessions/{id}/tool-approvals/...` 的 session 响应可以保留 `session_id`，但产品 workspace 应使用 AgentRun scoped approval response，不把 session id 回传为产品状态（`crates/agentdash-contracts/src/runtime/session.rs:136-147`, `packages/app-web/src/services/agentRunRuntime.ts:65-83`）。

前端目标形态：

- `AgentRunRuntimeTarget { runId, agentId }` 是 workspace/chat/tool approval/stream/context projection 的默认 identity（`packages/app-web/src/services/agentRunRuntime.ts:11-18`）。
- `SessionChatModel` 和 `WorkspaceRuntimeData` 不再要求 `sessionId`；`sessionId` 只存在于 session diagnostics 页面或 trace tab。
- `SessionStreamTransportOptions` 应允许 AgentRun branch 完全不传 `sessionId`。当前实现虽然有 AgentRun scoped stream，但 options 仍要求 `sessionId`，这是产品 identity 泄漏（`packages/app-web/src/features/session/model/streamTransport.ts:16-44`）。
- `SessionProjectionView` 在有 `agentRunTarget` 时不应因缺少 `sessionId` 提前退出；当前 `refresh` 先检查 `!sessionId`，导致 AgentRun target 不是完整身份（`packages/app-web/src/features/session/ui/SessionProjectionView.tsx:338-365`）。
- `ToolCallCardShell` 的产品路径只使用 `agentRunTarget`；session fallback 只用于 session diagnostic UI（`packages/app-web/src/features/session/ui/ToolCallCardShell.tsx:95-121`）。

### 5. 最小 contracts / service / frontend target

contracts 最小目标：

```rust
pub struct AgentRunRefDto {
    pub run_id: Uuid,
    pub agent_id: Uuid,
}

pub struct AgentRunWorkspaceView {
    pub run_ref: AgentRunRefDto,
    pub project_id: Uuid,
    pub shell: AgentRunWorkspaceShell,
    pub control_plane: AgentConversationSnapshot,
    pub conversation: AgentConversationFeedSnapshot,
    pub frame_runtime: Option<AgentFrameRuntimeView>,
    pub resource_surface: Option<AgentResourceSurfaceView>,
    pub lineage: Option<AgentRunLineageView>,
}

pub struct RuntimeTraceMeta {
    pub runtime_trace_ref: RuntimeTraceRefDto,
    pub delivery_status: Option<...>,
    pub last_event_seq: Option<u64>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub struct AgentRunCommandStaleGuard {
    pub snapshot_id: Uuid,
    pub run_ref: AgentRunRefDto,
    pub frame_ref: Option<AgentFrameRefDto>,
    pub active_turn_id: Option<Uuid>,
    pub workspace_revision: Option<u64>,
}

pub struct AgentRunToolApprovalResponse {
    pub run_ref: AgentRunRefDto,
    pub tool_call_id: String,
    pub receipt: Option<AgentRunCommandReceipt>,
}
```

service 最小目标：

- `AgentRunWorkspaceQueryService`：只负责 workspace/read model 派生。当前 `workspace/query.rs` 已经接近该形态，应继续保持“读时组装”，不要把 snapshot 持久化为事实（`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:67-279`）。
- `AgentRunRuntimeReadService`：输入 `AgentRunRefDto`，服务端解析 current delivery runtime，然后读取 session events/context/stream。前端不参与 runtime session 选择。
- `AgentRunCommandPolicyService`：同一套 resolver 同时产出 UI command availability 和服务端提交校验，避免 UI 可用但后端拒绝或反之。当前 command policy 已经重用 snapshot resolver，但还需要纳入 owner/control grant（`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:40-154`, `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:156-210`）。
- `AgentRunControlAuthorizationService`：封装 Project Use + owner/control grant。所有 parent-run mutation route 调它。
- `AgentRunPermissionAdmissionService`：由 `PermissionGrant` active projection 处理 runtime 内工具/resource admission，不和 Project Role 混在一起（`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:240-252`）。

frontend 最小目标：

- 页面路由：`/projects/:projectId`、`/agent-runs/:runId/:agentId`。
- workspace store：主 key 是 `AgentRunRuntimeTarget`；`runtimeTraceMeta` 是可选诊断信息。
- chat/model/stream/tool approval/context projection：以 `AgentRunRuntimeTarget` 调用 `services/agentRunRuntime.ts` 和 `services/agentRunMailbox.ts`（`packages/app-web/src/services/agentRunRuntime.ts:20-63`, `packages/app-web/src/services/agentRunMailbox.ts:15-125`）。
- session diagnostic 页面：继续使用 `/sessions/{id}`，但它是调试入口，不是产品 workspace 的依赖。

## 删除清单

下面的“删除”是产品面收敛，不是删除底层 runtime trace 能力。

1. 删除 `ProjectAgentRunStartResult` 的顶层 `runtime_session_id` 和顶层 `turn_id`，或移动到 diagnostic nested meta。启动完成后的产品导航只使用 `run_ref + agent_ref`（`crates/agentdash-contracts/src/agent/project_agent.rs:78-96`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:397-405`）。

2. 从产品 `AgentRunWorkspaceListEntry` 删除 `delivery_runtime_ref` / `delivery_trace_meta`。列表需要的是 shell status、last activity、agent title、project/run identity；trace 详情在进入 workspace 或打开 debug trace 后获取（`crates/agentdash-contracts/src/runtime/workflow.rs:1701-1729`）。

3. 从产品 `AgentRunView` / `AgentConversationLifecycleContext` 删除 `delivery_runtime_ref`。runtime ref 只保留在 `RuntimeSessionTraceMeta` 或 lifecycle/debug panels（`crates/agentdash-contracts/src/runtime/workflow.rs:1205-1241`, `crates/agentdash-contracts/src/runtime/workflow.rs:1476-1494`）。

4. 删除 `ConversationCommandStaleGuardView.runtime_session_id`。以 `snapshot_id + run_ref + frame_ref + active_turn_id/workspace_revision` 判断 stale；runtime session id 变化是 delivery 实现细节，不应成为用户命令 precondition（`crates/agentdash-contracts/src/runtime/workflow.rs:1069-1081`）。

5. 从产品 response 中移除 `AgentRunMessageAcceptedRefs.runtime_session_ref` / `AgentRunAcceptedRefs.runtime_session_ref`。命令接收结果应该是 receipt/outcome/message/fork；trace ref 只做 debug meta（`crates/agentdash-contracts/src/agent/run_mailbox.rs:91-106`, `crates/agentdash-contracts/src/agent/run_mailbox.rs:196-208`）。

6. 前端删除 workspace 对 `agentRunWorkspaceState.runtime_session_id` 的产品依赖。`WorkspaceRuntimeData.sessionId`、`WorkspaceRuntimeData.runtimeSessionId` 不应成为 chat/stream/tool approval 的必填输入；改为 `agentRunRuntimeTarget` + optional `runtimeTraceMeta`（`packages/app-web/src/features/workspace-runtime/model/types.ts:45-69`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:511-525`）。

7. 删除 AgentRun 产品路径对 `/sessions/{id}` 的依赖。`/sessions/{id}` route 保留为 diagnostics；workspace 使用 `/agent-runs/{run_id}/agents/{agent_id}/runtime/...`（`crates/agentdash-api/src/routes/sessions.rs:110-139`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:90-176`）。

8. 收紧 Project Use 过宽的 mutation route：delete run、cancel、mailbox mutate、tool approve/reject 不应只靠 Project Use。它们必须通过 owner/control grant（`crates/agentdash-api/src/routes/lifecycle_agents.rs:276-305`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:883-1152`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1155-1268`）。

9. 产品 permission UI 删除 `source_runtime_session_id` 展示。PermissionGrant 的 source runtime 是 audit trail；用户需要看到的是 grant scope、effect frame、requested access、status、expiry、approved_by（`crates/agentdash-domain/src/permission/entity.rs:18-50`）。

## 迁移/实施顺序

1. 先改 contracts，把产品 DTO 的 identity 收敛到 `AgentRunRefDto`。保留 runtime trace DTO，但只从 diagnostic/trace field 暴露。此步不需要 DB migration。

2. 增加统一的 `AgentRunControlAuthorizationService`。输入 project_id、run_id、agent_id、viewer_user_id、required_action；内部先要求 Project Use，再判断 implicit owner 或 explicit control grant。若选择独立 control grant 表，这一步先加 migration；若复用 lifecycle association，必须先明确 actor subject 模型。

3. 后端 route 硬化：composer submit 保持非 owner 自动 fork；cancel、mailbox delete/promote/resume、tool approve/reject、delete run 改为 owner/control grant。fork 和 read 继续 Project Use。

4. 重生成/更新 generated contracts。删除前端产品类型中必填 `runtime_session_id`，新增 optional `runtimeTraceMeta` 或 diagnostic-only ref。

5. 前端切 identity：`WorkspaceRuntimeData`、`SessionChatModel`、stream transport、tool approval、context projection 全部以 `AgentRunRuntimeTarget` 为产品 target。`sessionId` 只留 session diagnostics 页面。

6. 修 `SessionProjectionView`：有 `agentRunTarget` 时允许刷新 AgentRun scoped context projection；没有 `agentRunTarget` 才要求 session id。

7. 清理 workspace list/read model：列表只传 shell/status/last activity，不传 delivery runtime trace；workspace detail 如果需要 debug meta，只放在嵌套 trace 字段。

8. 补测试：合同快照/typecheck、API route authorization tests、composer non-owner fork test、cancel/tool approval 非 owner 拒绝或 fork/无效测试、frontend typecheck。

9. 最后再看 DB 清理。不要删除 anchors、mailbox、receipts、permission_grants、lineages、session_events、projection heads/segments；这些不是 runtime id 泄漏，而是必要 state/binding 或 runtime projection 基础。只有确认代码不再使用旧列/旧 DTO 后，才做 migrate 删除真正 dead 的字段。

## 需要验证的代码事实

以下是实施前还需要用 `rg`/tests 精确确认的事实点：

1. `AgentRunWorkspaceCommandPolicyService` 是否已经在所有 mutation 路径校验 owner/control。当前 evidence 显示它主要校验 command availability/stale/context，ownership 输入在部分路径可能为空，不能当作控制授权已完成（`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:40-154`, `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:156-210`）。

2. `AgentRunWorkspaceListEntry.delivery_runtime_ref` 是否只被 trace badge 使用。若有产品逻辑依赖它，迁移时应先改为 status/revision 或 workspace detail fetch。

3. workspace terminal / canvas / extension bridge 是否仍要求 `session_id`。已看到前端 workspace runtime model 和 session stream transport 有 session id 依赖，桥接层需要逐项替换为 AgentRun target（`packages/app-web/src/features/workspace-runtime/model/types.ts:45-69`, `packages/app-web/src/features/session/model/streamTransport.ts:16-44`）。

4. `workspace-module` API 是否仍接受 `runtime_session_id` 作为产品请求字段。若存在，应改为 AgentRun target 或由服务端从 workspace context 解析。

5. PermissionGrant 相关 generated DTO 是否把 `source_runtime_session_id` 直接给产品 permission card 使用。若只是 audit endpoint，可以保留；若在产品 approval card 展示，应移动到 audit/details。

6. session diagnostic fallback 是否仍被 workspace import。目标不是删除 `/sessions/{id}` 能力，而是确保 AgentRun workspace 不需要 session id 才能工作。

7. delete run 的产品语义需要最后确认：如果删除仅表示“删除我自己的 run”，owner 即可；如果删除项目内任意 run，则需要 Project Configure/owner-level project governance。

## 关键 file:line 证据索引

- AgentRun 稳定引用：`crates/agentdash-contracts/src/runtime/workflow.rs:834-837`
- RuntimeSession 技术引用：`crates/agentdash-contracts/src/runtime/workflow.rs:851-853`
- Execution anchor DTO：`crates/agentdash-contracts/src/runtime/workflow.rs:872-888`
- Runtime trace meta：`crates/agentdash-contracts/src/runtime/workflow.rs:893-908`
- Workspace shell：`crates/agentdash-contracts/src/runtime/workflow.rs:913-922`
- Conversation identity/lifecycle/snapshot：`crates/agentdash-contracts/src/runtime/workflow.rs:1205-1241`
- Conversation feed message/snapshot：`crates/agentdash-contracts/src/runtime/workflow.rs:1271-1368`
- Workspace view：`crates/agentdash-contracts/src/runtime/workflow.rs:1373-1409`
- AgentRun view/list DTO runtime exposure：`crates/agentdash-contracts/src/runtime/workflow.rs:1476-1494`, `crates/agentdash-contracts/src/runtime/workflow.rs:1701-1729`
- Command stale guard runtime exposure：`crates/agentdash-contracts/src/runtime/workflow.rs:1069-1081`
- ProjectAgent start runtime exposure：`crates/agentdash-contracts/src/agent/project_agent.rs:78-96`
- Mailbox accepted refs runtime exposure：`crates/agentdash-contracts/src/agent/run_mailbox.rs:91-106`, `crates/agentdash-contracts/src/agent/run_mailbox.rs:196-208`
- AgentRun routes：`crates/agentdash-api/src/routes/lifecycle_agents.rs:90-176`
- Composer submit owner/fork behavior：`crates/agentdash-api/src/routes/lifecycle_agents.rs:647-769`
- Runtime routes resolve delivery internally：`crates/agentdash-api/src/routes/lifecycle_agents.rs:1155-1290`
- Project permission model：`crates/agentdash-domain/src/project/authorization.rs:47-70`
- PermissionGrant state machine：`crates/agentdash-domain/src/permission/entity.rs:18-50`, `crates/agentdash-domain/src/permission/entity.rs:115-185`
- Active grant projection/admission：`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:25-57`, `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:240-298`
- Workspace query assembly：`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:67-279`
- Workspace list projection：`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:282-347`
- Workspace projection derivation：`crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:7-43`
- Conversation feed design note：`crates/agentdash-application-agentrun/src/agent_run/conversation_feed.rs:1-6`
- Context projector：`crates/agentdash-application-runtime-session/src/session/context_projector.rs:67-111`, `crates/agentdash-application-runtime-session/src/session/context_projector.rs:164-235`
- Runtime trace read model：`crates/agentdash-application-lifecycle/src/presentation_read_model.rs:92-240`
- Current delivery binding migration：`crates/agentdash-infrastructure/migrations/0017_lifecycle_agent_current_delivery_binding.sql:1-8`
- Runtime execution anchors migration：`crates/agentdash-infrastructure/migrations/0001_init.sql:533-545`
- Session projection storage migration：`crates/agentdash-infrastructure/migrations/0001_init.sql:547-627`
- Mailbox storage migration：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:59-85`, `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:214-222`
- Command receipt migration：`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:1-32`
- Fork lineage migration：`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1-18`
- Frontend AgentRun runtime service：`packages/app-web/src/services/agentRunRuntime.ts:11-83`
- Frontend mailbox service：`packages/app-web/src/services/agentRunMailbox.ts:15-125`
- Frontend workspace runtime data：`packages/app-web/src/features/workspace-runtime/model/types.ts:45-69`
- Frontend stream transport session dependency：`packages/app-web/src/features/session/model/streamTransport.ts:16-44`
- Frontend projection view session dependency：`packages/app-web/src/features/session/ui/SessionProjectionView.tsx:338-365`
- Frontend workspace fills session id from delivery runtime：`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:511-525`
