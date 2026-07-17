# Type Safety

> 前端类型安全规范。

---

## 核心原则

- **严格模式**：TypeScript strict 已启用，禁止 `any`、类型断言（`as`）、非空断言（`!`）
- **snake_case 直接映射**：前端类型字段名与后端 Rust DTO 直接对齐，不做 camelCase 转换
- **Generated wire 单源**：内部 API 响应通过 `src/generated/*` 的 contract type 消费，service 层信任 generated wire，不做逐字段 identity rebuild

---

## 类型分层

| 位置 | 用途 |
|------|------|
| `types/index.ts` + 拆分文件 | 跨 Feature 共享的领域类型 |
| `features/{name}/model/types.ts` | Feature 内部类型 |
| `generated/backbone-protocol.ts` | 自动生成的协议类型，禁止手动修改 |
| `generated/*-contracts.ts` | Rust contract crate 生成的 HTTP / NDJSON DTO，作为跨层 wire type 来源 |
| `generated/ndjson-stream-validators.ts` | Rust contract crate 生成的 NDJSON runtime validator，作为流 envelope 运行时校验来源 |

---

## Mapper 边界

mapper 只负责：
- UI view model 转换
- 外部/用户输入、第三方 payload、iframe/plugin bridge 等非内部 API 边界的 `unknown → typed object` 转换
- 尚未进入 contract crate 的 route-local 过渡 DTO

mapper 不负责：
- 同时接受 `camelCase` + `snake_case`（出现 `fooBar ?? foo_bar` 时应修后端 DTO）
- 猜测后端字段别名
- 重新声明后端 enum/string union；跨层 DTO 的联合类型来自 `src/generated/*`
- 对 generated DTO 做逐字段 identity rebuild

## Generated Contract Boundary

前端把 `src/generated/*` 当作 wire DTO 事实源。Feature 可以定义 view model，但 view model 必须由 generated DTO 显式转换而来，原因是 UI 形态与 transport 形态有不同变化节奏。

Session timeline 消费 `generated/backbone-protocol.ts` 中的 `UserInputSubmittedNotification.source`
作为 input channel/source provenance。`features/session/model/types.ts` 可以把 generated
`UserInputSource` 转成 UI view model（例如 user / companion / channel presentation），但不能
手写另一套 wire DTO 或从 system event 文本反推来源，原因是模型投递通道与 UI 展示差分需要共享
同一份 Backbone 事实。

Canvas 资产 UI 消费 `generated/canvas-contracts.ts` 中的 `CanvasResponse`、`CanvasScopeDto`、`CanvasAccessDto`、`CanvasListScopeDto`、`PublishCanvasToProjectRequest`、`CopyCanvasToPersonalRequest` 和 `UnpublishCanvasResponse`。`services/canvas.ts` 只封装 endpoint 和 query/body 传递；Mine/Shared 分组、按钮可见性和 editor 只读状态全部读取 `canvas.scope` 与 `canvas.access`。

Canvas access-driven UI contract:

| `CanvasResponse.access` field | Frontend behavior |
| --- | --- |
| `can_edit_source` | 显示源文件/绑定保存入口，允许 `updateCanvas` |
| `can_publish` | 显示“发布到项目共用”；“发布为插件”仍是独立 action |
| `can_copy` | Shared 视图显示“复制为我的 Canvas” |
| `can_manage_shared` | Shared 视图显示取消发布/删除共用源 |
| `runtime_write_allowed=false` | runtime preview 保持可用，source editor 以只读状态展示或禁用 |

Validation:

- `handleBindingsSave`、source file save 等 mutation handler 在调用 `updateCanvas` 前检查 `canvas.access.can_edit_source`，原因是 UI disabled state 不是权限边界。
- 复制 shared Canvas 成功后，UI 刷新列表、切到 Mine 并选中新 personal Canvas；后续编辑只作用于 clone。
- 前端不从 `owner_user_id`、Project role 或当前用户缓存推导编辑权限；这些事实已经由后端合并为 `access`。

Project extension runtime surface 消费 `generated/extension-runtime-contracts.ts`，`services/extensionRuntime.ts` 只保留 endpoint 调用与 webview asset URL 拼装。`features/extension-runtime` 以 Project ID 为 key 缓存 runtime projection，并向 WorkspacePanel 输出 tab descriptor 与 webview bridge；installation 的 `installed_source` 与 `package_artifact` 是显式可空字段，用来区分 Shared Library 安装来源与 packaged artifact 安装来源；前端不从 Shared Library payload 或 Session Context 推断 extension runtime 声明。

Extension webview bridge 的 `runtime.invoke_action` 与 `extension.invoke_channel` 校验 Project、AgentRun target、backend 与 action/channel key，并把 generated request DTO 交给 AgentRun scoped extension runtime service。后端从 AgentRun current delivery 推导内部 runtime context，原因是具体 action/channel 是否在当前 actor/context 下可执行由 Gateway catalog / invoke 同源裁决，而产品执行身份属于 AgentRun workspace。Project extension runtime projection 的 `runtime_actions` 服务资产展示，不作为前端执行可用性 gate。

新增或修改跨层 DTO 时同步运行：

```powershell
pnpm run contracts:check
```

## NDJSON Stream Validation

NDJSON stream transport consumes generated envelope unions through `generated/ndjson-stream-validators.ts`, then keeps transport classes focused on fetch, reconnect, cursor, lifecycle, and dispatch. The generated validator owns `unknown -> generated envelope branch` checks because streamed data crosses the network boundary one line at a time, and the runtime shape rules must stay with the same Rust contract generator that emits the wire union.

### 1. Scope / Trigger

- Trigger: adding or changing an internal NDJSON stream such as Session stream or Project event stream.
- Scope: generated `*-contracts.ts` envelope type, generated NDJSON validator schema, stream-specific error wrapper, transport connection code, and focused stream parser tests.

### 2. Signatures

```ts
export type GeneratedNdjsonEnvelopeParseResult<TEnvelope extends { type: string }> =
  | {
      [TKind in TEnvelope["type"]]: {
        ok: true;
        kind: TKind;
        envelope: Extract<TEnvelope, { type: TKind }>;
      };
    }[TEnvelope["type"]]
  | { ok: false; failure: GeneratedNdjsonEnvelopeValidationFailure };

export function parseGeneratedSessionNdjsonEnvelope(
  payload: unknown,
): GeneratedNdjsonEnvelopeParseResult<SessionNdjsonEnvelope>;

export function parseGeneratedProjectEventStreamEnvelope(
  payload: unknown,
): GeneratedNdjsonEnvelopeParseResult<ProjectEventStreamEnvelope>;
```

### 3. Contracts

- The generated validator return type is parameterized by the generated envelope union, such as `SessionNdjsonEnvelope` or `ProjectEventStreamEnvelope`.
- `packages/app-web/src/generated/ndjson-stream-validators.ts` is emitted by `agentdash-contracts`; frontend stream-specific validator files may wrap generated failures into local `Error` messages and map accepted branches into local view models, but they must not own branch field shape.
- Generated runtime guards validate object shape, required numeric cursor fields, required identifiers, JSON object payloads, and nested envelope presence before dispatch.
- Runtime guards do not duplicate generated enum value unions as frontend allowlists. When a generated field is a backend-owned string union, the runtime guard accepts the field shape and lets Rust contract generation plus TypeScript compilation own the value set.
- Transport code calls the validator once per parsed NDJSON line, reports validator errors through `onError`, updates cursors from valid envelope branches, and ignores heartbeat branches without storing long-lived stream facts.
- Adding a new internal NDJSON stream with a generated envelope requires adding its validator schema to the contract generator and running `pnpm run contracts:check`; keeping the validator only in app-web leaves the runtime boundary with a second source of truth.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| NDJSON line is not valid JSON | Transport reports parse error and keeps connection lifecycle handling intact |
| Parsed payload is not an object | Validator returns `ok=false` with an object-shape error |
| Session stream receives Project envelope shape | Session validator rejects before cursor/event dispatch |
| Project event stream receives Session envelope shape | Project validator rejects before cursor/event dispatch |
| Known branch misses required cursor or payload fields | Validator returns `ok=false` with branch-specific diagnostic |
| Unknown branch type arrives | Validator returns `ok=false` with stream-specific unknown-type diagnostic |
| Backend adds a new generated enum value inside an existing branch | Shape-valid payload is accepted; generated type drift is caught by contract/typecheck work, not by a frontend runtime allowlist |

### 5. Good/Base/Bad Cases

- Good: `parseGeneratedSessionNdjsonEnvelope` accepts `connected`, `event`, `ephemeral_event`, and `heartbeat`; `parseSessionNdjsonEnvelope` only maps generated failures to stream-local errors, and `streamTransport.ts` only advances cursor, updates ephemeral epoch, dispatches events, or ignores heartbeat.
- Good: `parseGeneratedProjectEventStreamEnvelope` accepts `Connected`, `StateChanged`, `BackendRuntimeChanged`, and `Heartbeat`; `parseProjectEventStreamEnvelopeResult` only maps generated failures to stream-local errors, while `eventStream.ts` owns only URL setup and cursor reading.
- Base: Stream-specific UI adapters can convert a generated event branch into a local view model after the validator has accepted the envelope.
- Bad: A transport class owns both fetch/reconnect and a full set of branch-specific field guards, because parser drift then hides inside connection code and cannot be tested as a contract seam.

### 6. Tests Required

- Session stream tests cover valid `connected`, valid `event`, valid `heartbeat`, invalid event shape, Project-shape rejection, and unknown type rejection.
- Project event stream tests cover valid `Connected`, valid `StateChanged`, valid `Heartbeat`, invalid `StateChanged`, Session-shape rejection, unknown type rejection, and enum-value forward extension.
- `cargo test -p agentdash-contracts` covers generated validator rendering and no enum allowlist duplication.
- Typecheck must pass so validator branch narrowing remains tied to generated envelope unions.

### 7. Non-canonical / Canonical

#### Non-canonical

```ts
const PROJECT_STATE_CHANGE_KINDS = new Set(["story_created", "story_updated"]);

function parsePayload(payload: unknown): ProjectEventStreamEnvelope | null {
  // fetch/reconnect code plus branch validation in one transport file
}
```

#### Canonical

```ts
const result = parseProjectEventStreamEnvelopeResult(payload);
if (!result.ok) {
  options.onError(result.error);
  return null;
}
return result.envelope;
```

---

## CapabilityDirective 契约

`CapabilityDirective` 使用 qualified path 字符串（`{ add: string } | { remove: string }`），支持能力级、工具级、MCP 能力。`CapabilityKey` 仅用于前端内置能力选项的 UI 展示，不要用它收窄 API 配置中的 `capability_directives`。

## Session Runtime Projection DTO

AgentRun workspace panel、context overview与VFS tab以`resource_surface: ResolvedVfsSurface`和AgentRun-scoped Runtime endpoints为输入。界面只读取final AgentFrame/Business Surface与canonical Runtime context projection。

Project/Story/Task/Agent knowledge预览使用`ResolvedVfsSurfaceSource`；AgentRun入口消费current resource surface。两类入口共享browser组件但source显式分型。

AgentRun 右侧 WorkspacePanel 消费 current workspace projection state。该 state 以 `run_id + agent_id + frame/runtime projection key` 为边界，携带 loading / ready / refreshing / error 状态；key 不匹配时不暴露上一份 runtime surface、capabilities、context snapshot 或 `workspace_modules`。`capability_state_changed`、`context_frame_changed` 等真实 surface 变化触发当前 state 的 invalidate/refetch，界面不创建新的长期快照事实源。`AgentRunWorkspaceView.workspace_modules` 是菜单、presentation validation与持久化tab恢复校验的generated wire事实源：Canvas打开事件先读取generated payload的`module_id`、`view_key`、`renderer_kind`与`presentation_uri`，再等待Workspace refetch并与当前ready descriptor精确匹配；布局恢复在该projection ready后清理不存在的Canvas URI，且更早发起的异步恢复不能覆盖currentness校验。`presentation_uri=canvas://{canvas_mount_id}` 是 tab identity；`view_key`、`module_id` 与 `{canvas_mount_id}://...` 分别保留 UI entry selection、module ref 与 VFS authoring URI 语义。

## Scenario: AgentRun Product Projection 与 Runtime Command 分权

### 1. Scope / Trigger

- 修改 AgentRun 详情加载、模型选择、composer/cancel/compact command 或 Runtime inspect 时适用。
- 该分权防止退役的 workspace conversation DTO 再次同时充当产品、模型和执行状态权威。

### 2. Signatures

```text
GET  /agent-runs/{run_id}/agents/{agent_id}/workspace -> AgentRunProductView
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime   -> Managed Runtime inspect
POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit
     { input, client_command_id, delivery_intent?: "steer" }
POST .../cancel | .../runtime/context/compact
     { client_command_id }
POST .../runtime/interactions/{interaction_id}/respond
     InteractionResponse
```

### 3. Contracts

- `AgentRunProductView` 只包含 Lifecycle identity/shell、current AgentFrame、`model_config`、subject associations 与 `resource_surface`；不嵌入 Runtime snapshot、mailbox command policy或旧 RuntimeSession source anchor。
- Runtime command enabled 状态只读取 `RuntimeSnapshot.command_availability`。前端 action ID 可以投影 Runtime command kind，但不携带自造 stale guard。
- Draft create可以携带model/runtime/backend selection；既有Run composer禁止executor/backend override。active turn时command projection只发送generated `delivery_intent="steer"`，idle时省略该字段并进入durable mailbox/TurnStart。
- Runtime event只提供interaction identity与展示内容；response按钮读取刷新后的`interaction_respond` availability。context popup直接消费`RuntimeContextView`并用target generation丢弃迟到响应。
- 服务端在 mutating command 前 inspect 当前 Runtime snapshot并生成 `AgentRunCommandGuard`，因此请求只携带幂等 `client_command_id` 与命令 payload。
- workspace product 与 runtime inspect 独立加载、独立记录错误；refresh 单路失败时保留该 owner 上一份成功事实。
- ProjectAgent draft start 继续使用 generated `CreateProjectAgentRunRequest` / `ProjectAgentRunStartResult`；HTTP success不等于 turn terminal。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| workspace失败、runtime inspect成功 | 保留Runtime snapshot并记录`workspace_error` |
| runtime inspect失败、workspace成功 | 保留产品/Frame/model/surface并记录`runtime_inspect_error` |
| refresh单路失败 | 保留该owner上一份成功事实，不清空另一owner的新结果 |
| command availability缺失或unavailable | UI禁用；API在副作用前按当前snapshot拒绝 |
| current AgentFrame缺少cloud-native provider/model | `model_config.status=model_required`并列出missing fields |
| 请求携带旧workspace stale guard | generated request不包含该字段；TypeScript/serde拒绝契约漂移 |
| 既有Run请求携带executor/backend override | TypeScript/serde拒绝；运行配置只来自current AgentFrame |
| interaction event存在但availability未刷新 | 控件disabled并触发Runtime inspect refresh |
| context响应target key不匹配 | 不提交到当前popup state |

### 5. Good / Base / Bad Cases

- Good：详情页从 product route得到current Frame/model/surface，从runtime route得到active snapshot；一侧刷新失败不会抹掉另一侧。
- Base：Runtime尚未创建时product projection仍可展示Lifecycle/Frame，命令保持不可用。
- Bad：使用`Promise.all`统一catch两路请求，或从Lifecycle status/conversation DTO制造Runtime command authority。

### 6. Tests Required

- state model测试首次单路失败与refresh单路失败保留语义。
- command-state测试submit/steer/interrupt/compact只由`command_availability`决定。
- service测试URL encoding与request不再发送dead command precondition。
- service/feed/context测试覆盖generic interaction route、四类Runtime lifecycle invalidation与target-key迟到响应隔离。
- generated contract check、frontend typecheck及真实Draft create-run验证model/surface/runtime三条事实一致。

### 7. Wrong vs Correct

```ts
// Wrong：一个旧聚合DTO和一次统一catch覆盖两个owner。
const [workspace, runtime] = await Promise.all([loadWorkspace(), inspectRuntime()]);
const canCancel = workspace.control_plane.status === "running";

// Correct：两路事实独立settle，命令只消费canonical Runtime availability。
const [productResult, runtimeResult] = await Promise.allSettled([
  loadAgentRunProduct(),
  inspectRuntime(),
]);
const canCancel = runtimeSnapshot?.command_availability.turn_interrupt?.status === "available";
```

Round action只暴露已有canonical command的动作。新增fork前必须先在`AgentRunRuntime` facade实现typed ThreadFork、availability、operation receipt与产品child binding，再生成前端合同。

---

## Task Plan And Story Projection DTO

Task plan DTO、Story Task projection DTO 与 Task status enum 都来自 Rust contract 生成文件。前端只消费 generated plan status union；execution status、artifacts 和 launch hint 字段由各自的 generated DTO 表达。

AgentRun workspace 消费 Run-scoped Task plan DTO 来创建、推进、归档和 assignment。Story 页面消费 Story Task projection DTO，只展示来源关系；runtime artifacts、latest runtime node 和 linked runs 只从 `SubjectExecutionView` / lifecycle generated DTO 读取。

新增或修改 Task plan / projection contract 后必须运行：

```powershell
pnpm run contracts:check
pnpm run frontend:check
```

---

## 禁止模式

- `any` 类型
- `as SomeType` 类型断言（除非编译器无法推断的极少数场景）
- `value!` 非空断言
- 为 generated DTO 编写逐字段 identity mapper
