# Runtime Gateway

## V1 Target Contract

RuntimeGateway 的目标合同是 actor-neutral canonical Operation discovery、admission 与 dispatch。MCP tool、
ExtensionProtocol method、Runtime Action 和 host operation 都投影为 provider-qualified、versioned
`OperationDescriptor`；direct invoke、OperationScript nested invoke 与 replay-safe effect invoke 共享同一个
`OperationExecutionCore`。

```text
OperationDescriptor {
  operation_ref,
  input_schema,
  output_schema,
  effect_summary,
  required_capabilities,
  actor_visibility,
  execution_policy,
  replay_policy: non_replayable | idempotent | replay_safe,
  readiness,
  provenance,
  dispatch
}

RuntimeInvocationEnvelope {
  operation_ref,
  input,
  principal,
  authorization_scope,
  origin,
  placement,
  trace,
  deadline,
  idempotency_key,
  optional_attachment_ref
}
```

- `OperationRef` 包含 exact provider identity/version，不能按全局短 key 首个命中。
- principal、authorization scope、origin、placement 与 trace 正交；origin 可以是 AgentRun、UserWorkshop、
  Canvas/Interaction、Workflow 或 EffectReplay，但不代替 principal/scope。
- browser/iframe 只提交 OperationRef + input。backend、workspace root、RuntimeSession、capability、principal、
  scope、placement 与 trace 均由宿主 resolver 生成。
- AgentRun adapter 可以从 AgentFrame/current delivery surface 解析 envelope；RuntimeSession 只作为 connector
  delivery/trace evidence，不进入 canonical Operation authority。Canvas、Interaction、Extension panel 与
  Workflow 调用不要求 RuntimeSession。
- `OperationExecutionCore` 固定执行 exact resolution、input schema、capability/actor admission、readiness/
  placement、deadline/cancellation/idempotency、dispatch、output schema/size、trace/audit/result-ref finalize。
  Declaration/discovery 不执行 provider side effect。
- Workspace Module 只组织 descriptor/projection，不拥有第二套 Operation identity、schema、resolver 或 dispatch。

### OperationScript V1

```text
OperationScriptRequest {
  dialect: "rhai_v1",
  host_api_version: 1,
  source,
  input,
  allowed_operations,
  limits
}
```

- Application 定义 async `OperationScriptExecutor` / `OperationScriptEngine` port；Infrastructure 首个 adapter
  复用 bounded `RhaiScriptRuntime`、JSON bridge 与 AST cache。
- Rhai V1 语法同步；execution-scoped `ops.invoke()` 隐式等待 async Operation，`ops.invoke_all()` 提供
  有界 structured concurrency。脚本退出时所有 child invocation 已完成或取消，不允许 detached task。
- evaluator 运行在有界专用 worker pool，通过 request/response bridge 调用 async OperationExecutionCore，
  不阻塞 Tokio core worker。limits 覆盖 concurrent scripts/queue/deadline、operation count/parallelism、
  Rhai steps/call depth/collection size 与 input/output bytes。
- progress/cancellation/deadline 同时中止纯脚本循环与 nested invocation。每个 nested invoke 校验 allowed
  manifest 并重新执行完整 admission；外层 script admission 不是 blanket authorization。
- preflight token 绑定 dialect/host API、source/input digest、exact descriptor/effect manifest、normalized
  limits、principal/scope 与 expiry。Run 任一不匹配即失败；V1 禁止递归 OperationScript。
- 失败返回 bounded diagnostic、trace 与 completed-call evidence。即时 script 不自动 retry/rollback/replay，
  不创建 asset/job/AgentRun，不调用 LLM，不包含 human gate；result ref 继承 caller owner/scope/capability/TTL。
- AgentRun、standalone UserWorkshop、Canvas/Interaction 与 Workflow 复用同一 executor。Canvas/Workflow 可以
  保存 Rhai source，但执行时仍把完整 source 交给服务端。

### Target Validation

- direct 与 nested invoke admission parity、provider-qualified resolution、input/output schema 与 result ownership。
- 客户端 authority injection、revoked capability、readiness/placement change、descriptor digest mismatch。
- Rhai CPU-loop/nested cancellation、worker/queue exhaustion、deadline、parallel limit、recursive call 与 bounded output。
- 非 AgentRun caller 的 invocation path 静态与行为测试均无 RuntimeSession dependency。

## Current Implementation And Migration Source

下述 RuntimeAction/Session contracts 记录现有实现与需保留的具体 provider、schema、transport、error matrix 和
测试要求。WI-01 将这些执行细节迁入上述 Operation contract；凡涉及 Canvas/Extension 必须依赖
`RuntimeContext::Session`、RuntimeSession 注入 authority 或按裸 action key 解析的表述，以 V1 Target
Contract 为最终语义，不能继续成为新调用方合同。

> application 层的统一运行时能力调用入口，承载 Session Runtime Action 与 Setup Action 的调用协议。

---

## 核心抽象

`RuntimeGateway`（`agentdash-application/src/runtime_gateway/gateway.rs`）：

- `register(provider)` — 注册 RuntimeProvider
- `surface_for(context)` — 按 ActionKind 过滤（不做 actor 校验，仅调试用）
- `surface_for_actor(actor, context)` — 完整 actor/context 校验，并合并 static provider 与 dynamic provider 产出的 concrete action descriptor（**消费端必须使用此入口**）
- `invoke(request)` — 执行 action

`RuntimeProvider` trait：`action_key()` + `action_kind()` + `invoke()`

---

## Action 分类

| Kind | Actor 约束 | Context 约束 | 示例 |
|------|-----------|-------------|------|
| Session | `AgentSession` / `UserCanvas` / `WorkflowNode` / `SessionUser` | `RuntimeContext::Session` | `mcp.list_tools`, `mcp.call_tool` |
| Setup | `PlatformUser` / `EnvironmentSetup` | `RuntimeContext::Setup` | `mcp.probe_transport`, `workspace.detect`, `workspace.browse_directory` |

---

## Actor / Context 校验规则

| 条件 | 错误 |
|------|------|
| Session context 的 `session_id` 为空 | `InvalidRequest` |
| Session context 搭配无 session actor | `CapabilityDenied` |
| actor/session context 的 `session_id` 不一致 | `CapabilityDenied` |
| Setup context 搭配 Agent/Canvas/Workflow/SessionUser actor | `CapabilityDenied` |
| Session actor 查询 Setup context | `CapabilityDenied` |
| action key 未注册 | `ProviderUnavailable` |

---

## surface_for vs surface_for_actor

- `surface_for(context)` 只按 `RuntimeActionKind` 过滤 provider，**不做 actor 校验**，仅用于内部调试
- `surface_for_actor(actor, context)` 做完整 actor/context 绑定校验，**消费端必须使用此入口**

消费端必须使用 `surface_for_actor`，`surface_for` 不能作为授权 manifest。

---

## Session MCP Action

Session Action baseline：`mcp.list_tools`、`mcp.call_tool`

关键约束：
- MCP runtime server surface 来源于 AgentRun / Lifecycle current runtime surface query 的闭包结果；该 query 从 `runtime_session_id` 经 `RuntimeSessionExecutionAnchor` 回到 AgentRun runtime address，再读取 current surface revision。
- `RuntimeSessionMcpAccess` 是 RuntimeGateway provider 的 backing port；其实现消费 AgentRun runtime surface query port 与 MCP discovery port，不进入 `SessionHub`，也不直接持有 `AgentFrame`、`AgentFrameSurfaceExt` 或 current frame resolver。
- `CapabilityState` 负责工具能力与 tool policy 裁决；空或未授权的 `CapabilityState` 不暴露任何工具。
- 所有工具暴露都必须经过 `capability_state.is_capability_tool_enabled()`。
- `surface_for_actor(Session)` 只表达 action visibility：actor/context 可以看到 `mcp.list_tools` / `mcp.call_tool` 这类 action；具体 MCP tool surface 只能来自 query-backed `mcp.list_tools` 输出。
- 后端绑定型 MCP discovery / call 必须消费 backend-required current surface，原因是 relay MCP 需要与 VFS、backend anchor、capability state 同源闭包的 call context。

`RuntimeSessionExecutionAnchor` 在这里只服务 trace/runtime session 到 AgentRun control-plane identity
的 backlink。MCP runtime action 仍然是 Session action，原因是调用发生在一个可投递的 delivery
runtime session 上；可执行 MCP server 的事实源回到 AgentRun current runtime surface，而不是
Session live runtime cache。

RuntimeGateway provider 边界保持 action input/output 与 actor/context admission：provider 不解析
current `AgentFrame`，不读取 session hub idle state，也不自行拼 VFS/backend/MCP surface。active
turn connector tool refresh 是 session live runtime coordination，继续消费已闭合的
`ExecutionContext`；它不是 `mcp.list_tools` / `mcp.call_tool` 的 current surface query 路径。

## AgentRun Runtime Surface Query Boundary

AgentRun runtime surface query 是 application 层 current surface 读取入口。它的公开 contract 以
`runtime_session_id`、AgentRun runtime address、surface revision、VFS、MCP servers、
`CapabilityState`、`RuntimeBackendAnchor` 和 provenance 为中心，不把 domain `AgentFrame` entity
或 `FrameLaunchSurface` 暴露给 RuntimeGateway / API current-surface consumer。

允许直接持有 `AgentFrame` 的区域限于 frame construction、launch closure、surface query 内部实现、
surface update use case、repository adapter、以及明确的 presentation read-model。RuntimeGateway
和 API current-surface consumer 使用 query DTO，原因是它们需要的是可执行 runtime surface，而不是
frame storage entity。

---

## Session Turn Control

Session 输入控制面按 Codex app-server protocol 建模：

```text
browser generated DTO Vec<UserInput>
  -> lifecycle steering service resolves active turn
  -> SessionTurnSteerCommand { session_id, expected_turn_id, input }
  -> relay/local connector payload
  -> Codex turn/steer 或 native connector 内部映射
```

`expected_turn_id` 是控制命令的一部分，原因是运行中 steer 必须绑定到浏览器看到的 active turn；relay、本机 handler 和 connector 只能透传或校验该前置条件，不能在下游重新猜测。Steer 成功投递后，应用层写入 `UserInputSubmitted(submission_kind = steer)`，让时间线、projection 和 lifecycle recall 共享同一个事实。

---

## Setup Action

Setup Action baseline：`mcp.probe_transport`、`workspace.detect`、`workspace.detect_git`、`workspace.browse_directory`

关键约束：
- API route 只做鉴权、请求解析、组装 `RuntimeInvocationRequest`，业务必须进入 provider
- Setup Action 不进入 Session Runtime Surface
- HTTP route 保持原响应契约，不让前端看到 Gateway 内部 envelope

API route 不直接调用底层业务函数，必须通过 Gateway invoke。

---

## RuntimeActionToolAdapter

Adapter 是 AgentTool → Gateway 的桥接基础件，不是产品注入策略：

- Adapter 不自行做 capability 裁决；由 Gateway/provider 负责
- Adapter 不直接调用底层 provider，必须通过 `gateway.invoke()`
- 不应把裸 Runtime Action 直接作为 Agent 工具面默认注入
- 平台自定义工具应显式定义工具名/参数/权限，内部选择性复用 Gateway invocation

## Runtime Tool Declaration Boundary

Provider-visible tool declaration 只构造工具名、描述、JSON schema、capability gate 和执行 adapter。`RuntimeToolProvider::build_tools` 与 session tool assembly 不得调用 `RuntimeGateway::invoke()`、session launch/control-plane mutation 或 extension action execution，原因是 declaration 会在 prompt preparation、capability refresh 和 schema/token estimation 路径重复运行；任何可执行副作用都必须留在 `AgentTool::execute`、API invoke route 或 runtime gateway provider invocation 阶段。

Session launch preparation 与 hub runtime refresh 共享 `session::tool_assembly::assemble_tools_for_execution_context`，原因是 runtime tools、direct MCP tools 与 relay MCP tools 的声明面必须由同一份 `ExecutionContext` 装配，避免 launch 前与运行中 refresh 看到不同的 capability/VFS/MCP surface。

---

## Canvas Runtime Bridge

Canvas iframe 通过 Gateway 调用 Session Action 的约束：

- Canvas iframe 代码不可信，不能直接拿 relay/MCP/http secret
- iframe SDK 只发送 `action_key` + `input`，actor/context 由父页面/API route 组装
- Canvas 专用 `/runtime-invoke` 不接受 iframe 传入的 actor/context/trace
- API route 必须通过 AgentRun current runtime surface 校验 Session 与 Canvas Project 绑定关系，再进入 Gateway/provider invocation，原因是 Canvas Project 和 runtime session current surface 必须共享同一份 AgentRun control-plane fact。

---

## Extension Runtime Action

Project extension runtime action 通过动态 provider 接入 `RuntimeGateway`。Provider 在 invocation 阶段读取 Project enabled extension installations，按 `project_id + action_key` 解析 extension identity、action schema、权限声明和 packaged artifact 引用。

调用约束：

- extension action 使用 `RuntimeContext::Session`，并要求 context 携带 `project_id`。
- `surface_for_actor` 是 Project extension concrete runtime action catalog 的入口：dynamic provider 从 Project enabled installations 产出 action descriptor，并与 invocation 使用同一套 `project_id + action_key` resolver。
- `extension.runtime_action` 只作为 provider 内部 identity，不作为 actor-visible action descriptor 暴露，原因是浏览器、Canvas 和 Agent 需要看到真实可执行 action key、schema 与权限声明。
- API / panel invocation 必须先通过 AgentRun current runtime surface 校验 path Project 与 runtime session Project 一致，再查询 installation 或进入 Gateway/provider invocation，原因是 extension 执行权限来自 Project 安装事实，而可执行 runtime surface 来自当前 AgentRun。
- backend placement 使用 `RuntimeTarget::Backend`，原因是 action 需要路由到实际承载本机 TS Extension Host 的 local backend。
- API / panel 调用面只提交 `action_key + input`，actor、context、target 和 trace 由宿主侧组装。
- Relay payload 使用 `command.extension_action_invoke` / `response.extension_action_invoke`，携带 extension key/id、action key、project/session、trace、invocation 与 package artifact。
- Gateway output metadata 合并 extension key/id、action key、backend id、trace id 与 invocation id，原因是 extension action 的执行结果需要可审计到具体安装与本机后端。
- packaged artifact 调用由 local backend 在收到 relay payload 后完成 archive 下载、cache 命中与 TS Extension Host activation，原因是云端只持有 Project 安装事实和 artifact 存储，本机 runtime 才持有可执行 host 与 workspace roots。
- runtime action invocation 要求对应 installation 拥有 `package_artifact`，原因是 action 执行必须绑定到可校验 archive、manifest digest 与本机 Host activation。
- local backend 下载 archive 使用 `/api/local-runtime/projects/{project_id}/extension-artifacts/{artifact_id}/archive`，通过 backend relay bearer token 鉴权，并复用 Project backend access 校验，原因是插件包执行权限必须绑定到已授权的 Project/backend 关系。

## Scenario: Extension Runtime Gateway Admission

### 1. Scope / Trigger

- Trigger: Extension runtime action 与 protocol 是跨 Gateway、relay、本机 Extension Host 和 SDK 的可执行契约；Gateway 必须在进入本机 transport 前完成 Project、schema 与运行态权限入站校验。
- Scope: `ExtensionRuntimeActionProvider`、`ExtensionRuntimeProtocolInvoker`、extension manifest action/protocol schema、relay action/protocol payload 与 local host output schema validation。

### 2. Signatures

```rust
pub struct RuntimeInvocationRequest {
    pub action_key: String,
    pub input: serde_json::Value,
    pub actor: RuntimeActor,
    pub context: RuntimeContext,
    pub target: Option<RuntimeTarget>,
    pub trace: RuntimeTrace,
}

pub struct ExtensionActionInvokeRequest {
    pub extension_key: String,
    pub extension_id: String,
    pub action_key: String,
    pub project_id: Uuid,
    pub session_id: String,
    pub input: serde_json::Value,
    pub package_artifact: Option<ExtensionRuntimePackageArtifact>,
}

pub struct ExtensionProtocolInvokeRequest {
    pub provider_extension_key: String,
    pub provider_extension_id: String,
    pub protocol_key: String,
    pub method: String,
    pub input: serde_json::Value,
}
```

### 3. Contracts

- Runtime action input 必须满足 manifest `runtime_actions[].input_schema`；protocol method input 必须满足 `protocols[].methods[].input_schema`。
- Gateway 支持的 JSON Schema 子集是 `true/false` schema、`type`、`required`、`properties`、`additionalProperties: false`、`items`、`enum` 和 `const`。
- 运行态 process 权限键只使用 `process.exec`、`process.shell`、`process.env.set` 与 `process.env.set:{KEY}`；manifest 顶层 process capability 仍作为安装摘要和审计信息。
- Gateway 校验 action/method 自身 `permissions` 声明；local host 在执行 Host API 时再次用当前 action 或 provider protocol method 的声明裁决。
- 本机 Extension Host 返回结果必须满足 action/protocol `output_schema`，原因是插件代码运行在本机进程，输出仍需回到 Gateway 可审计协议面。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `RuntimeContext::Session` 缺少 `project_id` | `InvalidRequest` |
| action key 在 Project enabled installations 中不存在 | `ProviderUnavailable` |
| action installation 缺少 `package_artifact` | `ProviderUnavailable` |
| action/protocol input 不满足 schema | `InvalidRequest`，不发送 relay transport |
| schema 超出支持子集或格式非法 | `InvalidRequest`，错误信息包含 schema 字段 |
| runtime permission key 未知 | `CapabilityDenied` |
| action/method 未声明请求的 Host API 权限 | `CapabilityDenied` 或 local host permission error |
| local host output 不满足 output schema | 本机 host 调用错误，不写成成功 response |

### 5. Good/Base/Bad Cases

- Good: action schema 要求 `{ "username": "string" }`，Gateway 在 relay 前拒绝 `username: 42`。
- Good: protocol method 声明 `process.env.set:DEMO_TOKEN`，local host 只允许该 action/method 设置对应 env key。
- Base: `{}` input schema 表达允许任意 JSON object，`true` schema 表达允许任意 JSON value。
- Boundary mismatch: extension webview 或 SDK 传入 project/backend/session 事实会绕过宿主路由事实。
- Canonical flow: API/panel 只提交 `action_key + input` 或 `channel + method + input`；宿主组装 actor/context/target/trace，Gateway admission 后再进入 relay/local host。

### 6. Tests Required

- `runtime_gateway::extension_actions` 测试 action input schema 与 channel input schema 在 transport 前失败。
- `runtime_gateway::extension_actions` 测试未知 runtime permission key 返回 capability denied。
- `agentdash-local extensions::host` 测试 Host API 权限按 action/protocol method 声明裁决，并验证 action/protocol output schema。
- `@agentdash/extension` manifest 测试要求 action/protocol schema 必填且拒绝未知 runtime permission key。
- `@agentdash/extension` typecheck 覆盖 runtime permission key union，并验证 `defineApp()` 生成的 operation catalog 不会自动把 runtime action 暴露给 Agent。

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```json
{
  "action_key": "local-hello.profile",
  "input": { "username": 42 },
  "project_id": "from-webview",
  "backend_id": "from-webview"
}
```

#### Canonical

```json
{
  "action_key": "local-hello.profile",
  "input": { "username": "syan" }
}
```

宿主侧负责把 Project、session、backend placement 与 trace 写入 `RuntimeInvocationRequest`，Gateway 负责入站校验和 relay payload 生成。

## Scenario: Extension backendService Bridge Invocation

### 1. Scope / Trigger

- Trigger: Extension `backend_services[]` 把 packaged Web App 的本机 Node service 暴露给 panel fetch route 和 Workspace Module Agent operation，属于跨 Gateway、relay、本机 runtime、contracts 与 frontend bridge 的可执行合同。
- Scope: `ExtensionRuntimeBackendServiceInvoker`、`ExtensionBackendServiceTransport`、AgentRun scoped backendService invoke API、relay `command.extension_backend_service_invoke`、local backend service manager、Workspace Module backendService dispatch。

### 2. Signatures

```rust
pub struct ExtensionBackendServiceInvokeRequest {
    pub extension_key: String,
    pub extension_id: String,
    pub service_key: String,
    pub route: String,
    pub project_id: String,
    pub session_id: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub package_artifact: ExtensionPackageArtifactPayload,
    pub workspace: Option<ExtensionInvocationWorkspacePayload>,
    pub trace_id: String,
    pub invocation_id: String,
}

pub struct ExtensionBackendServiceInvokeResponse {
    pub metadata: ExtensionBackendServiceInvokeMetadataPayload,
    pub response: Option<ExtensionBackendServiceHttpResponsePayload>,
    pub diagnostic: Option<ExtensionBackendServiceInvokeDiagnosticPayload>,
}
```

HTTP panel bridge route:

```text
POST /api/agent-runs/{run_id}/agents/{agent_id}/extension-runtime/invoke-backend-service
```

Relay messages:

```text
command.extension_backend_service_invoke
response.extension_backend_service_invoke
```

### 3. Contracts

- API/panel submits only `extension_key + service_key + route + method + headers + body`; Project, AgentRun delivery runtime, backend target, workspace root, trace and artifact are resolved by host/API.
- `ExtensionRuntimeBackendServiceInvoker` resolves Project enabled extension installation, requires `package_artifact`, checks `backend_services[].service_key`, and checks route against declared `backend_services[].routes` before relay transport.
- Declared backendService routes may be relative paths or explicit HTTP(S) loopback URLs; Gateway/API/Workspace Module validation strips query from the requested route and compares absolute declarations by their URL path component, while the relay invoke payload keeps the original `pathname + search` for HTTP forwarding.
- Relay command payload wraps identity in `metadata { project_id, backend_id, extension_key, extension_id, service_key, route, trace_id, invocation_id }`; local runtime echoes the same metadata in response.
- Cloud/API sends invoke intent only. Local runtime downloads/cache-validates package artifact, materializes backend service, starts/reuses the Node process, runs health/readiness, and proxies HTTP bytes to the service endpoint.
- Service state failures return payload `diagnostic { readiness, code, message, retryable, details }`; relay top-level `error` is reserved for transport/protocol failures.
- HTTP `204 | 205 | 304` responses carry `body = None` through relay, API and browser bridge.
- Workspace Module Agent surface only invokes operations from `operation_catalog`; `visibility = panel_only` remains filtered from describe and invoke.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| installation missing or disabled | Gateway/provider unavailable before relay |
| installation lacks package artifact | `missing_artifact` diagnostic |
| `service_key` not in `backend_services[]` | invalid request / provider unavailable before relay |
| route not declared in `backend_services[].routes` | invalid request before relay or local diagnostic |
| backend target unavailable | Workspace Module readiness unavailable; panel/API returns transport error |
| local service starting/health failed/process exited | relay payload diagnostic with retryable set from readiness |
| local service returns 204/205/304 | response body omitted |

### 5. Good/Base/Bad Cases

- Good: Panel calls `fetch("/api/search", { method: "POST", body })`; parent bridge matches explicit backendService fetch route and sends bytes to the AgentRun scoped backendService API.
- Good: Agent invokes an `operation_catalog` backendService operation; Workspace Module serializes input as JSON bytes, sets `POST`, and routes through the same backendService invoker.
- Base: Service is declared but still starting; caller receives structured readiness diagnostic and can present a stable unavailable state.
- Bad: Treating `runtime_actions` or arbitrary localhost routes as Agent operations would bypass author intent and Project/backend routing facts.

### 6. Tests Required

- `agentdash-application-runtime-gateway` tests assert declared route dispatch and route mismatch rejection.
- `agentdash-api` tests assert API response mapping, relay metadata, unavailable diagnostic and pending response routing.
- `agentdash-relay` protocol tests assert command/response roundtrip and no-body response shape.
- `agentdash-local` tests assert materialize, Node start/health/stop, process exit logs and unsupported runtime diagnostics.
- `agentdash-workspace-module` tests assert backendService readiness, bridge dispatch and `panel_only` filtering.
- Frontend bridge tests assert POST headers/body, route mismatch and 204/205/304 no-body behavior.

### 7. Wrong vs Correct

#### Wrong

```json
{
  "route": "http://127.0.0.1:4317/api/search",
  "backend_id": "from-iframe"
}
```

#### Correct

```json
{
  "extension_key": "local-webapp",
  "service_key": "local-webapp.api",
  "route": "/api/search",
  "method": "POST",
  "headers": { "content-type": "application/json" },
  "body": [123, 34, 113, 34, 58, 34, 100, 101, 109, 111, 34, 125]
}
```

Host/API resolves Project, backend placement, package artifact and trace before relay; local runtime performs private service access.
