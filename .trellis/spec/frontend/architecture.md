# Frontend Architecture

## Role

前端负责以 Project 为中心组织业务视图，消费后端权威状态与实时事件，提供 Workspace、Story、Task、AgentRun、Workflow、VFS、Assets 等交互界面。前端不创建第二套业务事实源。

## Invariants

- API 字段以后端 `snake_case` 契约为准，前端不引入 camelCase/snake_case 双风格解析。
- API 响应必须经 mapper 从 `unknown` 转换为 typed object。
- Story / Task / AgentRun / Workflow 等业务状态以后端为准，前端不自行推断权威状态。
- Lifecycle 运行态以后端 `LifecycleRunView` / `SubjectExecutionView` / `AgentFrameRuntimeView` / `AgentRunWorkspaceView` 为准；用户可见执行工作台展示 AgentRun Workspace，`RuntimeSession` trace view 只展示 trace，不作为业务执行归属事实源。
- Project 是顶层导航和隔离单元；Workspace、Story、Assets、runtime preview 都按 Project scope 组织。
- AgentRun workspace 的 runtime feed、context overview 和 VFS tab 以 AgentRun scoped runtime endpoints 与 `runtime_surface` 作为 UI 输入；RuntimeSession detail 仅作为内部 trace/diagnostic 视角。
- Feature module 遵循 model / ui 分离，跨 feature 共享能力进入明确的 shared package 或 primitive。
- Workspace tab、runtime data context 和 tab descriptor contract 放在 `features/workspace-runtime`，原因是 extension-runtime、workspace-panel 与 canvas-panel 都需要消费同一 workspace runtime surface，但不应形成 feature 间双向依赖。

## Current Baseline

主要包：

| Package | 当前职责 |
| --- | --- |
| `packages/app-web` | React Web 主应用 |
| `packages/app-tauri` | Tauri 桌面入口 |
| `packages/ui` | 共享 UI primitive 与样式 |
| `packages/core` | 共享核心逻辑与 ports |
| `packages/views` | 可复用 view components |
| `packages/extension` | Extension 一体化作者 API、host SDK、panel bridge、React helper 与 `agentdash-ext` CLI |

主应用组织：`api/`、`services/`、`stores/`、`features/<feature>/model`、`features/<feature>/ui`、`pages/`、`types/`、`generated/`。

## Local Decisions

- 前端类型直接使用 `snake_case`，原因是它让 DTO 契约错误暴露在 mapper / typecheck 边界，而不是被双读字段掩盖。
- 设计系统优先使用 `@agentdash/ui` primitive，原因是重复业务布局会让视觉语言和交互状态持续漂移。
- 长连接统一使用 fetch + ReadableStream 消费 NDJSON，原因是鉴权、resume、HMR cleanup 需要与普通 API 和 stream registry 对齐。
- Extension authoring surface 使用单一 `packages/extension` 工作区包，原因是 App authoring、host SDK、webview bridge 与开发 CLI 必须共享同一套 manifest/projection 生成合同，避免作者在 sdk/ui/dev 三个入口之间拆心智。
- WorkspacePanel 的插件 tab 由 `features/extension-runtime` 消费 Project scoped runtime projection 后注册，原因是插件 catalog 是 Project enabled installation 的全局视图，不应随单个 session 生命周期被创建或销毁。
- Extension webview action target 优先使用 Session runtime surface backend，缺省时使用当前 Project workspace binding，原因是 WorkspacePanel 插件 tab 的生命周期归属 Project，而本机 extension host 的可执行 backend 来自 workspace 授权事实。
- `canvas_panel` 插件 tab 在主前端读取 package artifact 内的 Canvas runtime snapshot 并复用 `CanvasRuntimePreview`，原因是 Canvas-derived extension 需要沿用 Canvas runtime sandbox/asset bridge，同时保持 Project extension installation 作为 WorkspacePanel tab catalog 的事实源。
- `@agentdash/extension/browser` 的 webview bridge 只让 panel 传递 method 与 JSON params；Project、session、backend、consumer extension 和 trace context 由 `ExtensionWebviewPanel` 组装，原因是 panel 运行在 iframe 中，不应成为 Project runtime routing 的事实源。
- Extension panel 的 bridge request surface 包含 `metadata.get_context`、`workspace.open_tab`、`runtime.invoke_action`、`extension.invoke_channel`、`vfs.read` 和 `vfs.write`；`events` 是 panel-local event bus，原因是 workspace-level 或 extension-runtime-level event 需要后端路由和订阅模型，不能混入本地 helper。
- Canvas runtime 如需消费 extension protocol channel，通过父页面注入的 `extensionChannelBridge` 进入同一 Project extension channel invocation service，原因是 Canvas 与 webview panel 都应依赖 Project runtime projection 和 Gateway admission，而不是在 iframe 里硬编码 provider extension key。
- Assets Extension 类目消费 Project extension management API，原因是安装、来源状态、package mode 与卸载/下载动作的事实源是 `ProjectExtensionInstallation`，runtime projection 只服务 WorkspacePanel 与 Gateway admission。
- Marketplace Extension 卡片和详情抽屉使用 `LibraryAssetDto.extension_package_artifact` 判断 packaged template 可安装性，原因是浏览、安装与发布后的 package 可用状态需要共享同一 Shared Library 合同。
- WorkspacePanel 是 extension/canvas tab 的 composition root；extension-runtime 与 canvas-panel 不反向依赖 workspace-panel，原因是插件 tab 注册、Canvas 预览和 workspace runtime context 需要保持单向装配关系。
- WorkspacePanel 打开 Canvas tab 使用 `workspace_module_presented.presentation_uri = canvas://{canvas_mount_id}`，原因是 Canvas 展示身份属于 workspace module UI entry；`{canvas_mount_id}://...` 保留给 Agent/runtime VFS 编辑面。
- Workflow 资产入口是 `WorkflowGraph` 定义态入口；Agent Activity 关联的 `AgentProcedure` contract 可以作为编辑器配套 draft 一起维护。运行态观察进入 `lifecycleStore`，原因是 graph definition 与 lifecycle projection 的变化节奏不同。
- VFS Browser 和 Extension webview 的 runtime VFS 读写入口共享 `vfs-browser-panel-policy.ts` 的 `selectDefaultVfsMount` / `selectVfsBackendTarget` 解析 mount/backend 选择，原因是 `runtime_surface` 是同一份 UI 输入，VFS tab 和插件 iframe 不能各自推断默认 mount 或本机 backend。

## Scenario: Runtime VFS Panel Policy

### 1. Scope / Trigger

- Trigger: Session VFS tab 与 Extension webview 都需要从同一 `runtime_surface` 发起 VFS read/write；mount id、backend id 和只读状态必须由共享策略解析。

### 2. Signatures

```ts
export function resolveDefaultMountId(
  mounts: Array<{ id: string; provider: string; browsable: boolean }>,
  initialMountId?: string,
  defaultMountId?: string | null,
): string | null

export function selectDefaultVfsMount<T extends VfsMountSelectionPolicy>(
  mounts: T[],
  options?: VfsMountSelectionOptions,
): T | null

export function selectVfsBackendTarget<T extends VfsMountBackendPolicy>(
  mounts: T[],
  options?: VfsMountSelectionOptions,
): VfsBackendTargetSelection | null
```

### 3. Contracts

- default mount 从 `runtimeSurface.vfs.mounts` 选择 requested mount，缺省使用 runtime surface default mount。
- backend target 只从可浏览且携带 `backend_id` 的 mount 中选择；Project workspace binding 作为第二输入源时，先由调用方转换成 mount policy 输入。
- Extension webview bridge 的 `vfs.read` / `vfs.write` 使用同一 selector 生成请求上下文。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| runtime surface 为空 | policy 返回不可读写状态 |
| requested mount 不存在 | policy 返回不可用状态并保留诊断 |
| mount 只读 | write action 不可用 |
| backend 无法解析 | 本机 VFS relay action 不可用 |

### 5. Good/Base/Bad Cases

- Good: VFS tab 和 extension iframe 指向同一个 session default mount 时得到同一个 backend id。
- Base: Project workspace binding 只作为 extension tab 缺少 session backend 时的第二输入源。
- Boundary mismatch: VFS tab 和 webview bridge 各自实现 mount/backend 选择会让同一 runtime surface 产生两个 UI 行为。
- Canonical flow: 两个入口都调用 `selectDefaultVfsMount()` / `selectVfsBackendTarget()`，再按 selector 结果发起 API/bridge request。

### 6. Tests Required

- `vfs-browser-panel.test.ts` 覆盖 default mount、requested mount、readonly 与 backend secondary-source selection。
- `extension-runtime/model/bridge.test.ts` 覆盖 webview bridge 复用 policy 后的 VFS read/write request context。

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```ts
const backendId = projectWorkspaceBinding.backend_id
const mountId = requestedMountId ?? "main"
```

#### Canonical

```ts
const target = selectVfsBackendTarget(runtimeSurface.vfs.mounts, {
  initialMountId: requestedMountId,
  defaultMountId: runtimeSurface.vfs.default_mount_id,
})
```

## Scenario: AgentRun Workspace Runtime Feed And Round Actions

### 1. Scope / Trigger

- Trigger: AgentRun workspace needs runtime feed rendering, copy-last-reply actions, fork-from-boundary actions, and fork redirect handling without exposing Session as a product operation.
- Scope: `AgentRunWorkspacePage`, `SessionChatView` as reusable runtime-feed component, AgentRun scoped services, generated workflow/session contracts, clipboard helper, navigation, and tests.

### 2. Signatures

Frontend service surface:

```ts
fetchAgentRunRuntimeEvents(runId, agentId, params)
agentRunRuntimeStreamPath(runId, agentId, params)
fetchAgentRunContextProjection(runId, agentId)
fetchAgentRunContextAudit(runId, agentId)
approveAgentRunToolCall(runId, agentId, toolCallId, request)
rejectAgentRunToolCall(runId, agentId, toolCallId, request)
forkAgentRun(runId, agentId, request)
forkSubmitAgentRun(runId, agentId, request)
submitAgentRunComposer(runId, agentId, request)
```

Round action model:

```ts
type RoundActionModel = {
  copyLastAgentReply: {
    enabled: boolean
    text: string
  }
  forkFromHere: {
    enabled: boolean
    forkPointRef?: SessionMessageRefDto
    disabledReason?: string
  }
}
```

### 3. Contracts

- `AgentRunWorkspacePage` owns product identity: route params, workspace snapshot, command submit, fork redirect navigation, and user-visible action state.
- `SessionChatView` may render runtime feed entries but executes only passed intents. It does not decide whether a submit mutates parent AgentRun or creates a fork.
- Runtime stream and projection calls from product workspace use AgentRun refs. A runtime session id can appear as a trace ref inside generated DTOs, but browser code does not compose product URLs from it.
- AgentRun workspace model names product command/control state as AgentRun conversation/workspace state. When a runtime trace id is needed for stream diagnostics or terminal connector lookup, frontend state names it `delivery_trace_session_id` or `traceSessionId` inside runtime/diagnostic data, not `sessionId` as a product identity.
- Composer submit handles `AgentRunMessageCommandResponse.fork` or equivalent fork outcome by navigating to `redirect.run_id + redirect.agent_id` and refreshing that workspace.
- Copy action writes only the current conversation round's last readable agent reply. Tool results, user text, earlier assistant chunks, and reasoning-only entries are excluded from that clipboard payload.
- Fork action sends a backend-provided stable `SessionMessageRefDto` / turn boundary. Frontend disabled state is UX guidance; backend remains the authority for boundary validity.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Runtime round has no readable final agent reply | copy action disabled with tooltip reason |
| Runtime round is streaming or has incomplete tool-call boundary | fork action disabled with backend/user-facing reason |
| Fork response has redirect refs | navigate to child AgentRun route and refresh workspace |
| Composer submit returns fork outcome for non-owner | parent workspace is not appended optimistically; navigation follows child refs |
| AgentRun scoped runtime request fails permission | show workspace command/feed error; do not fall back to raw Session product API |
| Clipboard write fails | keep UI in current workspace and surface bounded failure state |

### 5. Good/Base/Bad Cases

- Good: user copies a round whose last assistant message is a short answer; clipboard contains exactly that answer text.
- Good: user forks a stable completed round and lands in a child AgentRun workspace.
- Base: owner submits in their own AgentRun and receives ordinary mailbox outcome without redirect.
- Boundary mismatch: product component navigates by runtime trace identity after a fork, leaving no AgentRun ownership or mailbox projection.
- Canonical flow: product component calls `forkAgentRun` and navigates by child `run_id + agent_id`.

### 6. Tests Required

- Frontend model tests cover round grouping, last-agent-reply extraction, no reply disabled state, and unstable boundary disabled state.
- Service tests cover AgentRun scoped URLs for runtime events, stream, projection, tool approvals, fork, fork-submit, and composer submit.
- Workspace tests cover fork redirect navigation, non-owner composer fork outcome, self-owned explicit fork action, and absence of product imports for raw Session fork / lineage / rollback helpers.
- Typecheck must consume generated contracts directly for fork outcome and `SessionMessageRefDto`.

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```ts
const result = await runtimeTraceForkDiagnostic(runtimeTraceId, { fork_point_ref })
navigate(runtimeTraceDetailRoute(result.child_trace_id))
```

#### Canonical

```ts
const result = await forkAgentRun(runId, agentId, { fork_point_ref })
navigate(agentRunRoute(result.redirect.run_id, result.redirect.agent_id))
```

## Contract Appendices

- [Directory Structure](./directory-structure.md)
- [Type Safety](./type-safety.md)
- [State Management](./state-management.md)
- [Hook Guidelines](./hook-guidelines.md)
- [Component Guidelines](./component-guidelines.md)
- [Design Language](./design-language.md)
- [Quality Guidelines](./quality-guidelines.md)
- [Activity Lifecycle Frontend Contract](./workflow-activity-lifecycle.md)
