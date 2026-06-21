# Research: frontend-contracts-topology

- Query: 盘查前端 app/features/packages 与 generated contracts 的主链路拓扑与耦合点，产出后续 review 应覆盖的问题清单
- Scope: internal
- Date: 2026-06-21

## Findings

### 相关规范与基线

- `.trellis/spec/frontend/architecture.md`：前端以后端 view/DTO 为业务事实源，Project 是顶层导航和隔离单元，`runtime_surface` 是 Session workspace panel、context overview 和 VFS tab 的唯一 UI 输入。
- `.trellis/spec/frontend/directory-structure.md`：`packages/app-web/src` 按 `api/`、`services/`、`stores/`、`features/<feature>/model|ui`、`pages/`、`types/`、`generated/` 组织。
- `.trellis/spec/frontend/state-management.md`：store 消费 service 层 typed DTO 或 view model；`lifecycleStore` 是 SubjectExecution、runtime artifacts、latest runtime node 与 linked runs 的唯一执行投影缓存；AgentRun 输入区命令来自 `AgentConversationSnapshot.commands`。
- `.trellis/spec/frontend/type-safety.md`：内部 API 响应通过 `src/generated/*` 的 contract type 消费；service 层不应对 generated DTO 做逐字段 identity rebuild；mapper 只用于 view model、第三方/iframe/plugin bridge 或尚未进入 contract crate 的过渡 DTO。
- `.trellis/spec/frontend/hook-guidelines.md`：Session NDJSON envelope 属于 cross-layer contract；hook 只消费解析后的 envelope，并把业务聚合交给 reducer。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：标准链路是 Rust contract type -> serde wire shape -> ts-rs TypeScript generation -> `packages/app-web/src/generated/*` -> frontend service/reducer。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md`：已覆盖 AgentRun workspace command 投影重复、RuntimeSession runtime-control 漂移、Permission/companion grant 双事实源、capability/tool catalog 绕过 generated contract、前端大组件消费过宽 DTO 等问题，本文件只作为基线引用，不重复展开。

### 1. 模块/子模块清单与一句话职责

#### app shell / route

- `packages/app-web/src/App.tsx`：React Router 根与 lazy page composition root；`/agent-runs/:runId/:agentId` 路由进入 AgentRun 工作台（`packages/app-web/src/App.tsx:30`, `packages/app-web/src/App.tsx:347`）。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`：AgentRun 交互工作台页面，装配 workspace projection、Session chat feed、AgentRun command hook 与 WorkspacePanel（`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:2`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:167`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:407`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:762`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:805`）。
- `packages/app-web/src/pages/StoryPage.tsx`：Story 详情页，消费 `storyStore` 与 Story Task projection，不直接拥有执行事实（`packages/app-web/src/pages/StoryPage.tsx:168`, `packages/app-web/src/pages/StoryPage.tsx:170`, `packages/app-web/src/pages/StoryPage.tsx:250`）。
- `packages/app-web/src/pages/LifecyclePages.tsx` / `LifecycleEditorShellPage.tsx`：运行态观察读取 `lifecycleStore`，定义态编辑读取 `workflowStore`（`packages/app-web/src/pages/LifecyclePages.tsx:12`, `packages/app-web/src/pages/LifecycleEditorShellPage.tsx:14`）。

#### API / services / generated contracts

- `packages/app-web/src/api/client.ts`：统一 HTTP client，暴露 generic `get/post/put/patch/delete`，业务类型由调用侧 service 填入（`packages/app-web/src/api/client.ts:69`）。
- `packages/app-web/src/api/ndjsonStream.ts`：通用 fetch + ReadableStream NDJSON transport，支持 `x-stream-since-id`（`packages/app-web/src/api/ndjsonStream.ts:15`, `packages/app-web/src/api/ndjsonStream.ts:59`, `packages/app-web/src/api/ndjsonStream.ts:118`）。
- `packages/app-web/src/generated/*`：Rust contract 生成的 wire DTO；文件头均指向 `cargo run -p agentdash-contracts --bin generate_contracts_ts`，例如 `workflow-contracts.ts`、`session-contracts.ts`、`vfs-contracts.ts`、`extension-runtime-contracts.ts`（`packages/app-web/src/generated/workflow-contracts.ts:1`, `packages/app-web/src/generated/session-contracts.ts:1`, `packages/app-web/src/generated/vfs-contracts.ts:1`）。
- `packages/app-web/src/services/lifecycle.ts`：Lifecycle/AgentRun/RuntimeSession projection 的 endpoint client；返回 `LifecycleRunView`、`SubjectExecutionView`、`AgentRunWorkspaceView` 等 generated DTO（`packages/app-web/src/services/lifecycle.ts:23`, `packages/app-web/src/services/lifecycle.ts:31`, `packages/app-web/src/services/lifecycle.ts:65`, `packages/app-web/src/services/lifecycle.ts:84`）。
- `packages/app-web/src/services/agentRunMailbox.ts`：AgentRun composer、mailbox promote/delete/resume、cancel 命令 endpoint client；返回 generated command DTO（`packages/app-web/src/services/agentRunMailbox.ts:20`, `packages/app-web/src/services/agentRunMailbox.ts:48`, `packages/app-web/src/services/agentRunMailbox.ts:63`, `packages/app-web/src/services/agentRunMailbox.ts:104`）。
- `packages/app-web/src/services/vfs.ts`：VFS surface resolve/read/write 等 generated DTO endpoint client（`packages/app-web/src/services/vfs.ts:79`, `packages/app-web/src/services/vfs.ts:83`, `packages/app-web/src/services/vfs.ts:108`, `packages/app-web/src/services/vfs.ts:144`）。
- `packages/app-web/src/services/extensionRuntime.ts`：Project extension runtime projection、runtime action/channel invocation、uninstall endpoint client（`packages/app-web/src/services/extensionRuntime.ts:12`, `packages/app-web/src/services/extensionRuntime.ts:24`, `packages/app-web/src/services/extensionRuntime.ts:34`, `packages/app-web/src/services/extensionRuntime.ts:40`）。
- `packages/app-web/src/services/workflow.ts`：Workflow/AgentProcedure definition CRUD、validation、capability/tool catalog endpoint client（`packages/app-web/src/services/workflow.ts:40`, `packages/app-web/src/services/workflow.ts:51`, `packages/app-web/src/services/workflow.ts:166`, `packages/app-web/src/services/workflow.ts:170`）。
- `packages/app-web/src/services/taskPlan.ts` / `story.ts`：Run-scoped Task plan 与 Story Task projection endpoint client（`packages/app-web/src/services/taskPlan.ts:37`, `packages/app-web/src/services/taskPlan.ts:47`, `packages/app-web/src/services/story.ts:147`）。

#### stores / feature model

- `packages/app-web/src/stores/lifecycleStore.ts`：Lifecycle run、SubjectExecution、AgentFrame runtime 等运行态 projection cache（`packages/app-web/src/stores/lifecycleStore.ts:33`, `packages/app-web/src/stores/lifecycleStore.ts:38`, `packages/app-web/src/stores/lifecycleStore.ts:182`, `packages/app-web/src/stores/lifecycleStore.ts:208`）。
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts`：AgentRun workspace projection hook，调用 `fetchAgentRunWorkspace`，并从 `conversation.resource_surface ?? workspace.resource_surface` 解析 WorkspacePanel 输入（`packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:7`, `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:62`, `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:139`）。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts`：AgentRun command hook，把 generated command id、kind、stale guard 回传给 command endpoints（`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:98`, `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:100`, `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:221`, `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:306`）。
- `packages/app-web/src/features/session/model/useSessionStream.ts` / `streamTransport.ts` / `sessionStreamReducer.ts` / `useSessionFeed.ts`：Session NDJSON -> generated Backbone event -> display entry -> feed aggregation 链路（`packages/app-web/src/features/session/model/useSessionStream.ts:215`, `packages/app-web/src/features/session/model/streamTransport.ts:255`, `packages/app-web/src/features/session/model/sessionStreamReducer.ts:77`, `packages/app-web/src/features/session/model/useSessionFeed.ts:302`）。
- `packages/app-web/src/stores/storyStore.ts`：Story CRUD 与 Story Task projection cache（`packages/app-web/src/stores/storyStore.ts:18`, `packages/app-web/src/stores/storyStore.ts:265`）。
- `packages/app-web/src/stores/taskPlanStore.ts`：Run/AgentRun scoped Task plan cache 与 write-through update（`packages/app-web/src/stores/taskPlanStore.ts:14`, `packages/app-web/src/stores/taskPlanStore.ts:48`, `packages/app-web/src/stores/taskPlanStore.ts:63`, `packages/app-web/src/stores/taskPlanStore.ts:91`）。
- `packages/app-web/src/stores/workflowStore.ts`：WorkflowGraph definition draft、AgentProcedure draft、save bundle 与 validation orchestration（`packages/app-web/src/stores/workflowStore.ts:187`, `packages/app-web/src/stores/workflowStore.ts:397`, `packages/app-web/src/stores/workflowStore.ts:1033`, `packages/app-web/src/stores/workflowStore.ts:1061`）。
- `packages/app-web/src/features/extension-runtime/model/extensionRuntimeStore.ts`：Project extension runtime projection cache；HTTP-only projection 通过 `fetchProject(projectId)` 刷新（`packages/app-web/src/features/extension-runtime/model/extensionRuntimeStore.ts:31`, `packages/app-web/src/features/extension-runtime/model/extensionRuntimeStore.ts:34`, `packages/app-web/src/features/extension-runtime/model/extensionRuntimeStore.ts:50`）。
- `packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts`：VFS tab 与 Extension webview 共享 mount/backend selection policy（`packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:92`, `packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:109`）。
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts`：Extension iframe bridge adapter；补齐 Project/session/backend/runtime surface context 后调用 runtime action/channel 与 VFS read/write（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:86`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:123`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:142`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:211`）。

#### UI primitives / shared packages

- `packages/ui/src/index.ts`：导出 Button、Card、Notice、StatusScreen 等共享 UI primitive（`packages/ui/src/index.ts:5`, `packages/ui/src/index.ts:11`, `packages/ui/src/index.ts:37`, `packages/ui/src/index.ts:49`）。
- `packages/views/src/index.ts`：导出 directory-browser、local-runtime、mcp-shared 复用 view components（`packages/views/src/index.ts:1`, `packages/views/src/index.ts:2`, `packages/views/src/index.ts:3`）。
- `packages/views/src/local-runtime/LocalRuntimeView.tsx`：桌面/本机 runtime 管理视图，依赖 `@agentdash/ui` primitive 和 `LocalRuntimeClient` port（`packages/views/src/local-runtime/LocalRuntimeView.tsx:22`, `packages/views/src/local-runtime/LocalRuntimeView.tsx:39`, `packages/views/src/local-runtime/LocalRuntimeView.tsx:303`）。
- `packages/core/src/local-runtime/index.ts`：local runtime port/types 与默认本机 runtime 配置（`packages/core/src/local-runtime/index.ts:1`, `packages/core/src/local-runtime/index.ts:71`, `packages/core/src/local-runtime/index.ts:79`）。

#### desktop / extension packages

- `packages/app-tauri/src/App.tsx`：Tauri shell 注入 `__AGENTDASH_DESKTOP_LOCAL_RUNTIME__`、目录浏览和 external open port，不参与业务 generated DTO 事实源（`packages/app-tauri/src/App.tsx:32`, `packages/app-tauri/src/App.tsx:33`, `packages/app-tauri/src/App.tsx:34`）。
- `packages/app-tauri/src/runtimeApi.ts`：Tauri `invoke()` 到 profile/runtime/log/MCP/browse commands 的 local runtime client adapter（`packages/app-tauri/src/runtimeApi.ts:38`, `packages/app-tauri/src/runtimeApi.ts:53`, `packages/app-tauri/src/runtimeApi.ts:58`, `packages/app-tauri/src/runtimeApi.ts:96`）。
- `packages/extension-sdk/src/index.ts`：插件作者 API 与 contribution collector，定义 runtime actions、workspace panels、protocol channels、input/output schema（`packages/extension-sdk/src/index.ts:52`, `packages/extension-sdk/src/index.ts:58`, `packages/extension-sdk/src/index.ts:178`, `packages/extension-sdk/src/index.ts:297`, `packages/extension-sdk/src/index.ts:307`）。
- `packages/extension-ui/src/index.ts`：iframe panel bridge API；通过 `postMessage` 发起 `metadata.get_context`、`runtime.invoke_action`、`extension.invoke_channel`、`workspace.open_tab`、`vfs.read/write`（`packages/extension-ui/src/index.ts:115`, `packages/extension-ui/src/index.ts:129`, `packages/extension-ui/src/index.ts:132`, `packages/extension-ui/src/index.ts:141`, `packages/extension-ui/src/index.ts:145`）。
- `packages/extension-dev/src/*`：插件 init/validate/pack/install/dev preview CLI；校验 manifest/runtime surface parity，并提供 dev preview bridge stub（`packages/extension-dev/src/cli.js:7`, `packages/extension-dev/src/manifest.js:99`, `packages/extension-dev/src/pack.js:39`, `packages/extension-dev/src/dev-runtime.js:95`, `packages/extension-dev/src/dev-runtime.js:113`）。

### 2. 主链路拓扑

#### AgentRun workspace / chat / command 主链路

```text
Route /agent-runs/:runId/:agentId
  -> AgentRunWorkspacePage
  -> useAgentRunWorkspaceState
  -> services/lifecycle.fetchAgentRunWorkspace()
  -> generated/workflow-contracts.AgentRunWorkspaceView
  -> SessionChatView + WorkspacePanel + SessionStatusBar/Mailbox UI
```

- route 入口：`packages/app-web/src/App.tsx:347`。
- page composition：`AgentRunWorkspacePage` 同时装配 projection hook、command hook、SessionChatView 与 WorkspacePanel（`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:167`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:407`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:762`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:805`）。
- projection service：`fetchAgentRunWorkspace()` 返回 generated `AgentRunWorkspaceView`（`packages/app-web/src/services/lifecycle.ts:80`, `packages/app-web/src/services/lifecycle.ts:84`）。
- DTO source：`AgentRunWorkspaceView` 包含 shell、delivery trace meta、control plane、frame runtime、subject associations、resource surface、conversation snapshot（`packages/app-web/src/generated/workflow-contracts.ts:133`）。
- command source：`AgentConversationSnapshot.commands` / keyboard / stale guard 进入 page helper 与 command hook（`packages/app-web/src/generated/workflow-contracts.ts:32`, `packages/app-web/src/pages/AgentRunWorkspacePage.conversationCommandState.ts:176`, `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:98`）。
- command transport：composer submit、mailbox promote/delete/resume、cancel 通过 `agentRunCommandPath` 下的 command endpoints（`packages/app-web/src/services/agentRunMailbox.ts:20`, `packages/app-web/src/services/agentRunMailbox.ts:48`, `packages/app-web/src/services/agentRunMailbox.ts:63`, `packages/app-web/src/services/agentRunMailbox.ts:104`）。
- UI primitive：SessionChatView 与 workspace-specific UI 消费 generated command view，而视觉基础件来自 `@agentdash/ui` 和 feature local components。

#### Session NDJSON / feed 主链路

```text
Session stream endpoint
  -> streamTransport(fetch NDJSON + since id)
  -> generated/session-contracts.SessionNdjsonEnvelope
  -> generated/backbone-protocol.BackboneEnvelope/BackboneEvent
  -> sessionStreamReducer
  -> useSessionFeed aggregation
  -> SessionChatView event cards / feed UI
```

- generated session envelope：`SessionNdjsonEnvelope` 是 connected/event/heartbeat discriminated union（`packages/app-web/src/generated/session-contracts.ts:47`）。
- transport parse：`streamTransport` 校验 `type=event` 后调用 `parseSessionEventEnvelopePayload`，用 `event_seq` 更新 since id（`packages/app-web/src/features/session/model/streamTransport.ts:255`, `packages/app-web/src/features/session/model/streamTransport.ts:261`）。
- hook connection：`useSessionStream` 创建 transport 并批量 flush 到 reducer（`packages/app-web/src/features/session/model/useSessionStream.ts:215`, `packages/app-web/src/features/session/model/useSessionStream.ts:221`）。
- reducer：`sessionStreamReducer` 从 `SessionEventEnvelope.notification.event` 生成 display entry（`packages/app-web/src/features/session/model/sessionStreamReducer.ts:77`, `packages/app-web/src/features/session/model/sessionStreamReducer.ts:103`）。
- feed：`useSessionFeed` 基于 BackboneEvent 类型做 tool burst、context frame 与 platform event 聚合（`packages/app-web/src/features/session/model/useSessionFeed.ts:51`, `packages/app-web/src/features/session/model/useSessionFeed.ts:129`, `packages/app-web/src/features/session/model/useSessionFeed.ts:288`）。

#### Story / Task / execution 主链路

```text
Story route/page
  -> storyStore.fetchStoryTaskProjection()
  -> services/story.fetchStoryTaskProjection()
  -> generated/story-contracts.StoryTaskProjectionResponse
  -> Story/Task UI list

Task execution panel
  -> lifecycleStore.fetchSubjectExecution()
  -> services/lifecycle.fetchSubjectExecution()
  -> generated/workflow-contracts.SubjectExecutionView
  -> execution summary UI
```

- Story page 读取 `storyTaskProjectionByStoryId` 并按缺失触发 fetch（`packages/app-web/src/pages/StoryPage.tsx:168`, `packages/app-web/src/pages/StoryPage.tsx:250`）。
- Story service 返回 generated `StoryTaskProjectionResponse`（`packages/app-web/src/services/story.ts:147`）。
- Task plan store 独立管理 Run/AgentRun scoped plan facts（`packages/app-web/src/stores/taskPlanStore.ts:14`, `packages/app-web/src/services/taskPlan.ts:37`, `packages/app-web/src/services/taskPlan.ts:47`）。
- Task drawer 明确提示执行事实来自 SubjectExecutionView（`packages/app-web/src/features/task/task-drawer.tsx:281`）。
- Subject execution DTO source：`SubjectExecutionView` 包含 associations、runs、current_agent、latest_runtime_node、artifacts（`packages/app-web/src/generated/workflow-contracts.ts:253`）。

#### Workflow definition / lifecycle runtime 主链路

```text
Lifecycle editor route
  -> workflowStore definition/draft
  -> services/workflow WorkflowGraph + AgentProcedure APIs
  -> generated/workflow-contracts
  -> editor panels / DAG canvas / capability panels

Lifecycle runtime pages
  -> lifecycleStore
  -> services/lifecycle LifecycleRunView / SubjectExecutionView / AgentFrameRuntimeView
  -> generated/workflow-contracts
  -> runtime observation UI
```

- `workflowStore` 将 WorkflowGraph definition 与 AgentProcedure drafts 组合保存；single save 先 upsert activity procedures，再 upsert lifecycle（`packages/app-web/src/stores/workflowStore.ts:187`, `packages/app-web/src/stores/workflowStore.ts:1061`）。
- `services/workflow.ts` 直接返回 generated Workflow/AgentProcedure DTO（`packages/app-web/src/services/workflow.ts:40`, `packages/app-web/src/services/workflow.ts:51`, `packages/app-web/src/services/workflow.ts:125`, `packages/app-web/src/services/workflow.ts:140`）。
- Capability/tool catalog 在 workflow service 与 CapabilityPanel 中消费（`packages/app-web/src/services/workflow.ts:158`, `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx:276`, `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx:415`）。
- runtime observation 走 `lifecycleStore` 而不是 `workflowStore`（`packages/app-web/src/stores/lifecycleStore.ts:33`, `packages/app-web/src/stores/lifecycleStore.ts:38`）。

#### VFS / WorkspacePanel / Extension webview 主链路

```text
AgentRunWorkspaceView.conversation.resource_surface
  -> useAgentRunWorkspaceState.runtime_surface
  -> WorkspacePanel workspace data
  -> VfsBrowserPanel / ExtensionWebviewPanel
  -> shared vfs-browser-panel-policy
  -> services/vfs surface read/write
  -> generated/vfs-contracts.ResolvedVfsSurface / Surface* DTO
```

- AgentRun workspace state 优先使用 `conversation.resource_surface`，再用 workspace top-level `resource_surface`（`packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:62`）。
- VFS policy 提供默认 mount 与 backend target selection（`packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:92`, `packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:109`）。
- VFS browser 读写 surface file（`packages/app-web/src/features/vfs/vfs-browser-panel.tsx:155`, `packages/app-web/src/features/vfs/vfs-browser-panel.tsx:222`, `packages/app-web/src/features/vfs/vfs-browser-panel.tsx:240`）。
- Extension webview bridge 复用同一 policy，从 runtime surface 解析 backend/mount；没有 runtime backend 时使用 Workspace binding backend（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:211`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:216`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:234`）。
- generated VFS surface source 包括 project/story/task preview、session runtime、agent run、project skill assets、project VFS mount、project agent knowledge（`packages/app-web/src/generated/vfs-contracts.ts:29`, `packages/app-web/src/generated/vfs-contracts.ts:31`）。

#### Extension runtime / SDK / dev tooling 主链路

```text
Project extension management / installation
  -> services/extensionRuntime.fetchProjectExtensionRuntime()
  -> generated/extension-runtime-contracts.ExtensionRuntimeProjectionResponse
  -> extensionRuntimeStore/useProjectExtensionRuntime
  -> extensionTabDescriptors
  -> WorkspacePanel tab / ExtensionWebviewPanel
  -> extension-ui bridge
  -> RuntimeGateway invocation / VFS surface operations
```

- Project projection DTO 包含 installations、runtime_actions、protocol_channels、workspace_tabs、permissions、bundles 等（`packages/app-web/src/generated/extension-runtime-contracts.ts:58`）。
- `extensionRuntimeStore.fetchProject()` 是 projection cache 刷新入口（`packages/app-web/src/features/extension-runtime/model/extensionRuntimeStore.ts:34`, `packages/app-web/src/features/extension-runtime/model/extensionRuntimeStore.ts:50`）。
- tab descriptors 从 projection.workspace_tabs 生成（`packages/app-web/src/features/extension-runtime/model/extensionTabDescriptors.tsx:17`）。
- Extension iframe bridge 将 panel-local requests 转成 runtime action/channel/VFS/workspace tab/open context（`packages/extension-ui/src/index.ts:129`, `packages/extension-ui/src/index.ts:132`, `packages/extension-ui/src/index.ts:141`, `packages/extension-ui/src/index.ts:145`）。
- SDK/dev tooling 是插件作者侧事实声明与验证边界；`extension-dev` 校验 manifest 与 TS 注册 surface parity（`packages/extension-sdk/src/index.ts:297`, `packages/extension-sdk/src/index.ts:307`, `packages/extension-dev/src/manifest.js:99`）。

#### Desktop / local runtime 主链路

```text
Tauri shell
  -> app-tauri runtimeApi invoke()
  -> window injected LocalRuntimeClient / browseDirectory
  -> @agentdash/views LocalRuntimeView
  -> @agentdash/core local-runtime port
  -> local Rust commands / desktop backend
```

- app-tauri 注入 local runtime/browse/open external 三个 browser globals（`packages/app-tauri/src/App.tsx:32`, `packages/app-tauri/src/App.tsx:33`, `packages/app-tauri/src/App.tsx:34`）。
- runtimeApi 通过 Tauri invoke 连接 profile、runtime、logs、MCP、directory browse（`packages/app-tauri/src/runtimeApi.ts:38`, `packages/app-tauri/src/runtimeApi.ts:58`, `packages/app-tauri/src/runtimeApi.ts:83`, `packages/app-tauri/src/runtimeApi.ts:96`）。
- core 定义 `LocalRuntimeClient` port 与默认本机 runtime 配置（`packages/core/src/local-runtime/index.ts:71`, `packages/core/src/local-runtime/index.ts:79`）。
- views 的 `LocalRuntimeView` 是共享可视化，不直接依赖 app-web generated business DTO（`packages/views/src/local-runtime/LocalRuntimeView.tsx:39`）。

### 3. 与其它模块的耦合点

#### Backend contracts

- `crates/agentdash-contracts/src/generate_ts.rs` 是 generated TS 输出入口，显式导出 BackboneEnvelope、SessionNdjsonEnvelope、PermissionGrantResponse、ResolvedVfsSurface、AgentConversationSnapshot、AgentRunWorkspaceView、SubjectExecutionView、ExtensionRuntimeProjectionResponse、WorkspaceModulePresentation 等（`crates/agentdash-contracts/src/generate_ts.rs:207`, `crates/agentdash-contracts/src/generate_ts.rs:452`, `crates/agentdash-contracts/src/generate_ts.rs:554`, `crates/agentdash-contracts/src/generate_ts.rs:635`, `crates/agentdash-contracts/src/generate_ts.rs:754`）。
- API route 层以 contract DTO 输出前端消费形态：AgentRun workspace route（`crates/agentdash-api/src/routes/lifecycle_agents.rs:268`）、SubjectExecution route（`crates/agentdash-api/src/routes/lifecycle_views.rs:84`）、Session NDJSON route（`crates/agentdash-api/src/routes/sessions.rs:774`）、VFS surface route（`crates/agentdash-api/src/routes/vfs_surfaces.rs:92`）、Extension runtime route（`crates/agentdash-api/src/routes/extension_runtime.rs:92`）。

#### Session / AgentRun

- AgentRun workspace 是用户工作台 shell 与 command/control surface；frontend route/page 通过 `runId + agentId` 定位，RuntimeSession 只作为 delivery trace/meta/ref 进入 workspace view（`packages/app-web/src/generated/workflow-contracts.ts:133`）。
- Session stream 是 feed/event 事实流；命令可执行性不从 Session stream 推导，而从 `AgentConversationSnapshot.commands` 与 stale guard 来（`packages/app-web/src/generated/workflow-contracts.ts:32`, `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:100`）。

#### Workflow / Lifecycle / Task

- Workflow definition 事实在 `workflowStore` + `WorkflowGraph` / `AgentProcedure` generated DTO；runtime 事实在 `lifecycleStore` + `LifecycleRunView` / `SubjectExecutionView` / `AgentFrameRuntimeView`。
- Story projection、Task plan、SubjectExecution 是三条不同事实链：Story 页面读 projection，AgentRun workspace/Task plan store 写 Run-scoped plan facts，执行状态读 SubjectExecution（`packages/app-web/src/stores/storyStore.ts:18`, `packages/app-web/src/stores/taskPlanStore.ts:14`, `packages/app-web/src/generated/workflow-contracts.ts:253`）。

#### VFS

- `ResolvedVfsSurface` 是 VFS Browser、WorkspacePanel 和 Extension webview 的共同 UI 输入；mount/backend selection 已收敛到 `vfs-browser-panel-policy.ts`，边界是 generated `vfs-contracts.ts`（`packages/app-web/src/generated/vfs-contracts.ts:29`, `packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:109`）。
- VFS surface source 覆盖 Project/Story/Task preview、Session runtime、AgentRun、Project skill assets、Project VFS mount、ProjectAgent knowledge，后续 review 应只看 source 归属和调用入口，不把 mount builder 内部细节并入前端问题。

#### Extension

- Project extension runtime projection 是 WorkspacePanel 插件 tab catalog 和 RuntimeGateway admission 的前端事实源；Extension webview iframe 只发 method + JSON params，Project/session/backend/context 由父页面 bridge 补齐（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:59`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:86`, `packages/extension-ui/src/index.ts:129`）。
- Extension SDK/dev 包是插件作者侧声明/校验边界，不直接消费 app-web generated contracts；它与后端 contract 的耦合点在 manifest/runtime_actions/workspace_panels/schema 字段形态。

#### Desktop

- Tauri shell 只提供 local runtime port、目录浏览和 external open 这类宿主能力；业务 route/DTO 仍在 app-web 内部。耦合点是 window globals 与 `@agentdash/core` `LocalRuntimeClient` port（`packages/app-tauri/src/App.tsx:32`, `packages/core/src/local-runtime/index.ts:71`）。

### 4. 值得下一轮深挖的 review 问题

#### P0

1. **Project-level NDJSON 是否仍是手写 envelope，违反“NDJSON envelope 进入 contract”规则。**  
   Session stream 已使用 generated `SessionNdjsonEnvelope`（`packages/app-web/src/generated/session-contracts.ts:47`），但 Project event stream 仍在前端 `api/eventStream.ts` 手写 `Connected/Event/Heartbeat` 解析，并消费 `types/acp.ts` 的 `StreamEvent`（`packages/app-web/src/api/eventStream.ts:35`, `packages/app-web/src/types/acp.ts:123`, `packages/app-web/src/stores/eventStore.ts:69`）。下一轮应确认 `/events/stream/ndjson` 是否已有 Rust contract；若没有，这是跨层 stream contract 漂移风险。

2. **`types/index.ts` 是否已经从“类型入口”漂移成跨域 DTO facade 与手写 DTO 混合事实源。**  
   该文件同时 re-export generated Story/Task/Project/Workspace/VFS/ProjectAgent/Routine 类型，又保留 `ProjectBackendAccessStatus`、`BackendWorkspaceInventoryStatus`、`CapabilityKey` 等手写 union/常量（`packages/app-web/src/types/index.ts:39`, `packages/app-web/src/types/index.ts:93`, `packages/app-web/src/types/index.ts:128`, `packages/app-web/src/types/index.ts:275`）。下一轮应逐项分类：哪些是 UI view model，哪些应进入 generated contract，哪些只是 feature-local 类型。

#### P1

1. **AgentRunWorkspacePage 是否承担过多跨域事实合并。**  
   页面同时处理 workspace projection refresh、session stream event refresh、workspace module presentation、command hook、WorkspacePanel toggle/open（`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:167`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:407`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:474`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:805`）。06-14 已覆盖“大组件消费过宽 DTO”，下一轮不要重复评 UI 大小，而应检查这些 refresh/open 动作的事实源是否都回到 generated event/projection。

2. **`workspace_module_presented` 的前端入口是否完全 contract 化。**  
   HTTP present 返回 generated `WorkspaceModulePresentation`（`packages/app-web/src/services/workspaceModule.ts:26`, `packages/app-web/src/generated/workspace-module-contracts.ts:66`），但 session platform event 进入 page 时仍从 raw platform data 经 `workspaceModulePresentedTabTarget(data)` 解析（`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:474`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:479`, `packages/app-web/src/features/workspace-module/model/presentation.ts:49`）。下一轮应确认 Backbone platform event payload 是否也有同源 generated DTO，避免 HTTP 与 stream 两条 presentation 形态漂移。

3. **route-local/legacy mapper 的 contract 覆盖缺口需要分级。**  
   大多数 service 已直接返回 generated DTO，但仍有 Canvas、SkillAsset、ExtensionManagement、CurrentUser、SessionExecutionState、Story payload guard 等通过 `Record<string, unknown>` mapper 进入（`packages/app-web/src/services/canvas.ts:83`, `packages/app-web/src/services/skillAsset.ts:518`, `packages/app-web/src/services/extensionManagement.ts:149`, `packages/app-web/src/services/currentUser.ts:38`, `packages/app-web/src/services/session.ts:128`, `packages/app-web/src/services/story.ts:48`）。下一轮应按“跨 feature 复用 / 前端消费 / 流式传输”判断哪些必须进入 `agentdash-contracts`。

4. **Workflow definition store 的 draft bundling 是否有明确 owner 边界。**  
   `workflowStore` 同时维护 LifecycleEditorDraft、AgentProcedureDraft、hook presets、validation、save bundle 与 activity procedure draft sync（`packages/app-web/src/stores/workflowStore.ts:187`, `packages/app-web/src/stores/workflowStore.ts:397`, `packages/app-web/src/stores/workflowStore.ts:572`, `packages/app-web/src/stores/workflowStore.ts:1033`, `packages/app-web/src/stores/workflowStore.ts:1061`）。下一轮应检查它是否只负责 definition draft，还是混入 runtime/capability catalog 的事实缓存。

5. **Extension runtime projection 写后刷新是否在所有写入口集中执行。**  
   spec 要求 HTTP-only projection 写后显式 `fetchProject(projectId)`。已看到 ExtensionCategoryPanel uninstall 成功后 `await refresh(currentProjectId)`（`packages/app-web/src/features/assets-panel/categories/ExtensionCategoryPanel.tsx:155`, `packages/app-web/src/features/assets-panel/categories/ExtensionCategoryPanel.tsx:159`），但还应覆盖 install、publish Canvas as extension、package upload/install、uninstall 等所有入口，确认都刷新同一个 `extensionRuntimeStore` projection。

6. **AgentRun resource surface 与 Session runtime surface 的 UI 输入是否仍可能并存冲突。**  
   `useAgentRunWorkspaceState` 优先 `conversation.resource_surface`，fallback 到 top-level `workspace.resource_surface`（`packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:62`），Extension bridge 使用 runtime surface 解析 backend/mount，再 fallback workspace backend（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:211`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:216`）。下一轮应验证 AgentRun workspace 与 RuntimeSession detail 两条入口的 surface source 不会让同一个 WorkspacePanel tab 指向不同 mount/backend。

#### P2

1. **Capability/tool catalog 的前端 cache 与 MCP probe 映射是否仍在 UI panel 内。**  
   `CapabilityPanel` 持有 `toolCatalogCache`，从 capability catalog 和 MCP probe 映射工具列表（`packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx:265`, `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx:357`, `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx:398`, `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx:415`）。06-14 已覆盖 catalog contract 化方向；下一轮只需确认前端拓扑是否已经有可替换的 service/model 边界。

2. **Extension SDK/dev manifest schema 与 runtime projection contract 是否有同源校验。**  
   SDK 要求 `input_schema`/`output_schema`，dev tooling 校验 manifest/runtime surface parity（`packages/extension-sdk/src/index.ts:52`, `packages/extension-sdk/src/index.ts:80`, `packages/extension-dev/src/manifest.js:99`, `packages/extension-dev/src/manifest.js:281`）。下一轮可只查 schema 在后端 RuntimeGateway/local runner 是否同源执行；这属于 Extension contract 边界，不是 app-web UI 问题。

3. **UI primitive 消费边界是否稳定。**  
   `@agentdash/ui` 作为 primitive 入口已经被 app-web/views 广泛消费（`packages/ui/src/index.ts:5`, `packages/views/src/local-runtime/LocalRuntimeView.tsx:22`, `packages/app-web/src/pages/StoryPage.tsx:18`）。下一轮若做 UI review，应只关注重复业务布局是否应沉淀为 view/component，不应与 generated DTO 拓扑混在一起。

### 5. 不应重复 review 的内容

以下内容已由 `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` 覆盖，本轮后续 review 只引用结论，不重复展开问题细节：

- **AgentRun workspace command/action/mailbox 投影重复**：06-14 已建议以 `AgentConversationSnapshot` 作为 chat command/mailbox surface；本轮只检查前端是否消费该 snapshot，不再重新评估后端 projection 设计。
- **RuntimeSession runtime-control 漂移成第二个 AgentRun control 入口**：06-14 已覆盖；本轮只把 `fetchSessionRuntimeControl()` 作为边界引用，不展开后端修法。
- **PermissionGrant 与 companion capability grant 双事实源**：06-14 已覆盖；本轮只记录 `services/permission.ts` 和 `PermissionGrantCard` 属于正式 grant UI，不再深挖 companion broker。
- **Capability catalog / tool catalog contract 化**：06-14 已覆盖后端 SPI 与前端手写 catalog 的事实分裂；本轮只把 `CapabilityPanel` 和 `services/workflow.fetchCapabilityCatalog()` 作为下一轮前端拓扑验证点。
- **SessionChatView / AgentRunWorkspacePage 大组件消费过宽 DTO**：06-14 已指出大组件风险；本轮不做组件拆分建议，只检查这些组件连接了哪些事实源。
- **VFS/local/relay/extension 后端装配层过厚**：06-14 已覆盖 Rust side provider/router/mount/Tauri main 的过厚问题；本轮只看前端 surface、bridge 和 desktop shell 的边界。

## External References

- 未使用外部资料。本次为内部代码与 Trellis 规范静态盘查。

## Related Specs

- `.trellis/spec/frontend/index.md`
- `.trellis/spec/frontend/architecture.md`
- `.trellis/spec/frontend/directory-structure.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/hook-guidelines.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## Caveats / Not Found

- 未运行 `pnpm dev`、`frontend:check`、`contracts:check` 或浏览器验证；本产物是静态拓扑 research，不证明运行时行为正确。
- `task.py current --source` 返回 no active task；本文件按用户提供的 explicit task path 写入，未依赖 session active-task pointer。
- 未全面读取每个 feature UI 文件；抽样重点是 route/page、feature model/hooks、services、stores、generated DTO、Extension bridge、Desktop shell 与 shared packages。
- 对后端只引用 contract/DTO/stream/API route 边界，没有审查 application/domain 内部实现。
