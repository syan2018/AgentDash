# Frontend / Backend Contracts

## Role

前后端契约层定义浏览器、云端 API、本机 runtime 和桌面壳共同消费的 wire DTO、事件 envelope 与生成产物。它的目标是让 JSON/NDJSON 形态由 Rust contract 明确表达，并由生成文件进入前端，而不是让前端长期手写后端 DTO。

## Architecture

标准链路：

```text
Rust contract type
  -> serde wire shape
  -> ts-rs TypeScript generation
  -> packages/app-web/src/generated/*
  -> frontend service / reducer
```

`agentdash-contracts` 是业务 DTO 的归属 crate。它承载 HTTP request/response DTO、NDJSON envelope、跨端共享 enum 和少量 wire value object。`agentdash-api` 使用 contract crate 作为 route 输入输出类型；前端只从 generated 文件消费这些类型。

当前 `agentdash-agent-protocol` 承载 Backbone Protocol 这类 runtime event fact；业务 HTTP DTO 归属 `agentdash-contracts`。

## Invariants

- 业务 HTTP JSON 默认使用 `snake_case`，生成类型保持 Rust serde 字段名。
- Generated TypeScript 只落在 `packages/app-web/src/generated/`，文件头必须注明生成命令。
- 每个生成入口必须有 check mode；CI 或 `pnpm run contracts:check` 用 check mode 发现 drift。
- Frontend service 对内部 API response 信任 generated wire type；字段名、enum 值和 union 形态不在前端重新定义。Mapper 只用于 UI view model 转换、外部/用户输入、第三方 payload，或尚未进入 contract crate 的 route-local 过渡 DTO。
- Route-local DTO 只用于极小的 transport wrapper；跨 feature 复用、前端消费或流式传输的 DTO 必须进入 contract crate。
- NDJSON stream 的 `connected` / `event` / `heartbeat` envelope 也属于 contract，原因是续传游标、事件事实和 reducer 输入需要跨后端与前端共同演进。
- NDJSON stream 的运行时 envelope validator 由 `agentdash-contracts` 生成到 `packages/app-web/src/generated/ndjson-stream-validators.ts`，原因是流式网络边界需要运行时 shape 校验，而校验字段、tag 分支和生成 TypeScript union 必须共享同一个 contract generator 事实源。
- Session turn 控制面复用 Codex app-server protocol 的 input 形态，且 message（新轮）与 steer（运行中注入）入参**同形** `Vec<UserInput>`（canonical，后端封名 `UserInputBlock`），原因是 Codex `turn/start` 与 `turn/steer` 本就共用同一 `Vec<UserInput>`，分裂成两套输入表示是历史负债。浏览器发起运行中 steer 时后端服务继续携带 `expected_turn_id` 进入 session control / relay / connector，因为 Codex `turn/steer` 的幂等前置条件必须由前端可见的实际运行状态一路传递到执行器。
- 用户输入的多模态形态结构化直达：前端把图片以 data URL 放进 `UserInput::Image`，经唯一映射成为 `ContentPart::Image{mime_type,data}` 投递给模型，不再拍平成占位文本。ACP `ContentBlock` 仅存在于 relay 远程边界（单处双向转换），不进入业务 HTTP 入参，也不在内部投递链路透传。

## Contract Crate Shape

当前结构按业务域拆分：

```text
crates/agentdash-contracts/
  src/
    lib.rs
    generate_ts.rs
    mcp_preset.rs        # MCP preset CRUD/probe DTO
    session.rs           # Session event page DTO / NDJSON envelope / runtime projection
    extension_runtime.rs # Project extension runtime surface DTO
    extension_package.rs # Packaged extension artifact upload/install/download DTO
    external_marketplace.rs # 外部 Marketplace 来源浏览、导入和刷新 DTO
    workflow.rs          # AgentProcedureContract / lifecycle / activity DTO
    vfs.rs               # ResolvedVfsSurface / mount / edit capability DTO
    shared_library.rs    # Library asset install/publish DTO
    project_agent.rs     # ProjectAgent config/session summary DTO
```

生成输出按领域拆文件：

```text
packages/app-web/src/generated/
  backbone-protocol.ts
  agent-run-mailbox-contracts.ts
  session-contracts.ts
  extension-runtime-contracts.ts
  extension-package-contracts.ts
  external-marketplace-contracts.ts
  workflow-contracts.ts
  vfs-contracts.ts
  shared-library-contracts.ts
  mcp-preset-contracts.ts
  project-agent-contracts.ts
  ndjson-stream-validators.ts
```

## Current Baseline

`agentdash-contracts` 现在覆盖这些前端消费的业务 DTO：

| Domain | Generated File | Contract Source |
| --- | --- | --- |
| MCP Preset | `mcp-preset-contracts.ts` | `agentdash-contracts::mcp_preset` |
| Session event stream / projection view | `session-contracts.ts` | `agentdash-contracts::session` |
| AgentRun mailbox command / scheduler projection | `agent-run-mailbox-contracts.ts` | `agentdash-contracts::workflow` mailbox DTO |
| Extension Runtime | `extension-runtime-contracts.ts` | `agentdash-contracts::extension_runtime` |
| Extension Package Artifact | `extension-package-contracts.ts` | `agentdash-contracts::extension_package` |
| External Marketplace | `external-marketplace-contracts.ts` | `agentdash-contracts::external_marketplace` |
| Workflow / lifecycle / activity | `workflow-contracts.ts` | `agentdash-contracts::workflow` wire DTO |
| VFS surface / mount / Project VFS mount | `vfs-contracts.ts` | `agentdash-contracts::vfs` |
| Shared Library | `shared-library-contracts.ts` | `agentdash-contracts::shared_library` |
| Project Agent | `project-agent-contracts.ts` | `agentdash-contracts::project_agent` |
| LLM Provider | `llm-provider-contracts.ts` | `agentdash-contracts::llm_provider` |
| Backend Runtime Summary | `backend-contracts.ts` | `agentdash-contracts::backend` |

API routes use contract DTOs for cross-feature HTTP input/output. When a route still needs an application/domain model internally, the API layer owns the mapping into contract DTOs.

Frontend type entrypoints re-export generated contracts directly when the wire shape is ergonomic for UI code. A feature may keep a small UI wrapper around generated contracts when the UI needs a narrower semantic type, such as `AgentPresetConfig` over a JSON blob or nullable view state over omitted wire fields. Service 层不为 generated DTO 做逐字段 identity rebuild，原因是 drift detection 已由 contract check、Rust 编译和 TypeScript 编译负责。

Session projection view DTOs expose `AgentContextEnvelope` provenance to the browser: segment origin, synthetic marker, source range, projection segment id and compaction metadata remain generated contract fields. Frontend service code consumes the generated projection response directly and must not redefine this projection shape outside generated session contracts.

Session diagnostic branch DTOs live in `agentdash-contracts::session`: fork request/response, lineage record/view and projection rollback response. They describe RuntimeSession trace provenance for internal detail/debug surfaces. Product fork flows use AgentRun scoped workflow DTOs so frontend navigation, command receipts, mailbox outcome and ownership remain keyed by `run_id + agent_id`.

LLM Provider DTOs live in `agentdash-contracts::llm_provider`，原因是管理员 Provider Catalog、用户 BYOK effective list、credential mode、probe 请求、用户凭据验证状态和 Codex OAuth 登录状态都由前端设置页与执行器 discovery 共同消费。前端 API 层消费 `generated/llm-provider-contracts.ts`，只保留 service 调用和轻量 view model 状态；`credential_mode`、`effective_api_key_source`、`global_api_key_configured`、`user_api_key_configured`、`user_credential_verification_status`、`CodexOAuthStatusResponse.status` 等字段不在前端手写重声明。

## Local Decisions

- Workflow wire DTOs live in `agentdash-contracts::workflow` because browser-facing TS generation is a protocol concern. `agentdash-domain::workflow` owns persisted/domain value objects and keeps serialization derives needed by persistence, but does not depend on `ts-rs` or `schemars`.
- AgentRun composer submit request（`AgentRunComposerSubmitRequest.input`）使用 `Vec<codex::UserInput>`，并返回 `AgentRunMessageCommandResponse`。浏览器输入区使用该 DTO 提交 Enter/Ctrl-Enter 产生的用户输入，后端先 claim command receipt，再创建 mailbox envelope 或 AgentRun fork outcome，并由 scheduler 返回 `launched | queued | steered | blocked | failed` 等 delivery outcome；非 owner 继续输入时 response 携带 `fork`/redirect child refs。原因是键盘事件可能来自滞后的 snapshot，而用户输入本身应由后端 durable mailbox、AgentRun ownership 与当前 AgentRunTurn 事实决定执行语义。
- ProjectAgent 启动 request（`CreateProjectAgentRunRequest.input`）同样使用 `Vec<codex::UserInput>`，原因是 draft 首轮输入和 AgentRun composer follow-up 在投递链路上共享同一个 canonical 用户输入形态。
- ProjectAgent 启动 response（`ProjectAgentRunStartResult`）携带外层 AgentRun start refs 与 `initial_message: AgentRunMessageCommandResponse`。前端使用外层 refs 进入 AgentRun workspace，并使用 workspace projection / mailbox projection 观察首轮投递结果；不从 route success、`runtime_session_id` 或可选 `turn_id` 推断 connector 已 accepted，原因是 draft start 的 durable workspace materialization 与首条 mailbox delivery 是两个独立恢复边界。
- `CreateProjectAgentRunRequest.subject_ref` 是 ProjectAgent AgentRun 的 subject profile 选择入口。省略时使用 Project context；Story 快速创建 AgentRun 传入 `subject_ref=story` 后仍复用同一 ProjectAgent run contract，原因是 Story/Task 是动态上下文画像而不是独立 Agent owner。
- Task plan request / response DTOs live in `agentdash-contracts` and are scoped by LifecycleRun / AgentRun workspace. Story 页面消费 Story Task projection DTO；Task runtime artifacts、latest runtime node、current agent 和 linked runs 继续消费 `SubjectExecutionView`。Generated files expose only plan status fields for Task plan facts, while execution projection fields stay on `SubjectExecutionView`，原因是 Task plan facts 与 execution projection 是不同事实源。
- `AgentRunComposerSubmit*` 是 AgentRun 输入区 command DTO；`AgentRunView` / `AgentRunRefDto` 表示 Lifecycle control-plane 的 agent run view/ref。
- `SessionMeta` 是 RuntimeSession repository 内部 trace-head projection；浏览器合同使用 `RuntimeSessionTraceMeta` 表达 runtime session ref、event seq、executor session id、trace title provenance、terminal summary 与 trace 更新时间。`AgentRunWorkspaceShell` 表达 AgentRun 工作台 shell：display title、title source、delivery/workspace status、last turn ref、last activity 和 action projection。存在 delivery RuntimeSession meta 时，API 组装层把 `SessionMeta.title` / `title_source` 投影为 workspace shell 的 display title/source，原因是 sidebar/list/header 必须从 AgentRun-facing projection 读取同一用户可见标题，同时 trace/feed/debug、repository rehydrate 与 connector follow-up 继续消费 RuntimeSession trace facts。
- 用户 delivery command receipt 是 AgentRun command contract。`client_command_id`、request digest、duplicate/conflict state、mailbox message ref、accepted refs 与 command-scoped result 随命令 request/response/read model 演进，并与 `RuntimeSessionTraceMeta` / `AgentRunWorkspaceShell` 保持分层，原因是幂等和重放语义属于单次用户命令，不属于 runtime trace head。
- AgentRun mailbox DTOs live in `agentdash-contracts::workflow` and generate into `workflow-contracts.ts` because workspace composer、mailbox row、promote/delete/resume 和 scheduler outcome 是同一组 browser-facing command contract。前端 service 消费 `MailboxMessageView`、`MailboxMessageStatus`、`MailboxDelivery`、`ConsumptionBarrier`、`MailboxDrainMode` 和 `AgentRunMessageCommandResponse`，不手写 pending/message 状态 union。
- AgentRun workspace / conversation snapshot 的内部事实归 application read model，`agentdash-api` lifecycle adapter 负责把 workspace shell、conversation execution、command set、mailbox state、resource surface 与 `resource_surface_coordinate` 映射为 `agentdash-contracts::workflow` wire DTO。这样 application command policy 可以消费稳定的业务事实，browser contract 只表达传输形态与前端消费字段。
- Companion interaction request payload 的业务正文使用 `payload.message`，原因是 human、parent、sub
  和 notification 都是在跨主体传递消息；`capability_grant_request` 保持 `requested_paths`、`reason`、
  `scope` 等结构化授权字段。Agent-facing tool schema、companion-system skill 文档和后端 payload
  registry 必须同步描述这组字段。
- Orchestration HumanGate decision 使用 `SubmitOrchestrationHumanDecisionRequest/Response` 生成到 `workflow-contracts.ts`，原因是浏览器提交的是 `orchestration_id + node_path + attempt + decision` 这个 runtime node command，不是 graph node 或 gate-local ad hoc payload。
- VFS, Shared Library and Project Agent use narrow DTOs in `agentdash-contracts` because their API responses intentionally map application/domain internals into stable browser-facing shapes.
- Backend runtime summary DTOs live in `agentdash-contracts::backend` because `/backends/runtime-summary` is a browser-facing HTTP projection that combines backend config, runtime health, executor availability, and active execution leases. `agentdash-api` keeps the application read model -> wire DTO mapper, while frontend code consumes generated `BackendRuntimeSummaryResponse`, `BackendRuntimeExecutorResponse`, `BackendActiveSessionResponse`, `BackendExecutionSelectionMode`, and `BackendExecutionLeaseState` through generated aliases.
- Generated request/response DTOs model serde wire fields. UI-level convenience such as nullable fields, normalized config objects or derived aliases belongs in frontend type entrypoints rather than in the generated file.
- Project extension runtime surface 使用独立 `agentdash-contracts::extension_runtime` 与 `extension-runtime-contracts.ts`，原因是它是 Project enabled extension installations 派生出的全局 runtime surface，不属于 Shared Library marketplace/source-status，也不是 Session Context 私有字段。
- Extension package artifact 使用独立 `agentdash-contracts::extension_package` 与 `extension-package-contracts.ts`，原因是 packaged archive 的上传、安装引用和下载元数据是平台 artifact 契约，不属于 runtime projection 列表，也不属于 Shared Library payload。
- External Marketplace 使用独立 `agentdash-contracts::external_marketplace` 与 `external-marketplace-contracts.ts`，原因是外部来源浏览、详情、导入和显式刷新是 Marketplace 发现入口的 wire contract，而 Shared Library DTO 只表达导入后的平台资产。
- Workspace webview panel / Canvas extension channel 通过 AgentRun scoped extension runtime invoke routes 进入 RuntimeGateway。父页面 bridge 只补齐 AgentRun target、backend 与 Project context，后端再从 AgentRun current delivery 解析内部 RuntimeSession，原因是 iframe 内插件 UI 只能发送 action/channel key 与 input，产品执行身份必须与 AgentRun workspace 保持一致。
- Packaged panel UI 通过 `GET /api/projects/{project_id}/extension-runtime/webviews/{extension_key}/{*asset_path}` 读取 artifact 内文件，服务端只允许读取已声明 workspace tab renderer entry 所在目录，原因是插件 UI 资源属于安装后的 Project artifact，而不是 Shared Library source payload。
- `canvas_panel` workspace tab renderer 复用 packaged panel asset 读取 contract，entry 指向包内 Canvas runtime snapshot，原因是 Canvas-derived extension 应与其它 packaged extension 共享 artifact/source-status/install 语义，同时复用现有 Canvas runtime preview。

## Scenario: AgentRun Scoped Extension Runtime Invocation

### 1. Scope / Trigger

- Trigger: extension runtime action/channel invocation is a browser-visible execution command and must use AgentRun product identity while RuntimeSession remains internal delivery trace.
- Scope: `agentdash-contracts::extension_runtime`, `extension-runtime-contracts.ts`, AgentRun scoped extension runtime routes, frontend extension runtime services, webview bridge, Canvas extension channel bridge, and RuntimeGateway invocation context.

### 2. Signatures

HTTP surface:

```text
GET  /api/projects/{project_id}/extension-runtime
GET  /api/projects/{project_id}/extension-runtime/webviews/{extension_key}/{*asset_path}
POST /api/agent-runs/{run_id}/agents/{agent_id}/extension-runtime/invoke-action
POST /api/agent-runs/{run_id}/agents/{agent_id}/extension-runtime/invoke-channel
```

Wire DTO shape:

```rust
pub struct ExtensionRuntimeInvokeActionRequest {
    pub action_key: String,
    pub input: serde_json::Value,
}

pub struct ExtensionRuntimeInvokeChannelRequest {
    pub channel_key: String,
    pub method: String,
    pub input: serde_json::Value,
    pub consumer_extension_key: Option<String>,
    pub dependency_alias: Option<String>,
}
```

Frontend service shape:

```ts
invokeAgentRunExtensionRuntimeAction(
  target: AgentRunRuntimeTarget,
  request: ExtensionRuntimeInvokeActionRequest,
): Promise<ExtensionRuntimeInvokeActionResponse>

invokeAgentRunExtensionRuntimeChannel(
  target: AgentRunRuntimeTarget,
  request: ExtensionRuntimeInvokeChannelRequest,
): Promise<ExtensionRuntimeInvokeChannelResponse>
```

### 3. Contracts

- Project extension runtime projection remains Project scoped because installed extension declarations are Project facts.
- Extension runtime invocation is AgentRun scoped because runtime action/channel execution happens against the current AgentRun workspace delivery.
- Request DTOs carry extension action/channel intent only: `action_key`, `channel_key`, `method`, `input`, consumer extension key and dependency alias.
- The API route resolves `run_id + agent_id` through AgentRun/Lifecycle control-plane permission, then resolves the current delivery RuntimeSession and runtime surface server-side.
- RuntimeGateway still receives `RuntimeActor::SessionUser`, `RuntimeContext::Session` and channel invocation `session_id` internally after the route has derived that RuntimeSession from AgentRun current delivery.
- Webview and Canvas bridge availability requires `agentRunRuntimeTarget` and an online backend. Project ID remains required for projection/webview asset URLs.
- Generated TypeScript contracts are consumed directly by frontend services; bridge code does not add hidden transport identity fields to request bodies.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Missing AgentRun target in WorkspacePanel | Extension bridge reports missing AgentRun context before invoking |
| User lacks Project `Use` for the AgentRun | AgentRun scoped route rejects before runtime surface resolution |
| AgentRun has no current delivery runtime | AgentRun scoped route returns unavailable/not-found style API error |
| Current delivery runtime surface belongs to another Project | Route returns conflict before RuntimeGateway invocation |
| Backend for resolved runtime surface is offline | Route returns backend unavailable/conflict before invocation |
| Empty `action_key`, `channel_key` or `method` | Route returns bad request; no RuntimeGateway invocation |
| Contract drift reintroduces a transport identity field | `pnpm run contracts:check` or frontend typecheck fails |

### 5. Reference Cases

- Webview action flow: iframe posts `runtime.invoke_action` with action key and input; parent bridge calls `invokeAgentRunExtensionRuntimeAction(agentRunRuntimeTarget, request)`; backend derives internal RuntimeSession and invokes RuntimeGateway.
- Webview channel flow: iframe posts `extension.invoke_channel`; parent bridge adds consumer extension key and dependency alias; backend derives current delivery and invokes the extension runtime channel.
- Canvas channel flow: Canvas panel invokes an extension channel through the same AgentRun scoped channel service and receives only the invocation output.
- Projection flow: runtime projection and webview asset routes remain Project scoped because they describe installed assets, not current execution identity.

### 6. Tests Required

- Contract check asserts `ExtensionRuntimeInvokeActionRequest` and `ExtensionRuntimeInvokeChannelRequest` generate without runtime trace identity fields.
- API compile/check asserts AgentRun scoped extension runtime routes resolve current delivery and invoke RuntimeGateway with derived runtime context.
- Frontend bridge tests assert action/channel services receive `AgentRunRuntimeTarget` and generated request DTOs without transport identity fields.
- Frontend availability tests assert missing AgentRun target disables action/channel bridge while Project-scoped webview asset URLs still resolve from Project context.

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```ts
await invokeProjectExtensionRuntimeAction(projectId, {
  action_key: "demo.action",
  input,
});
```

#### Canonical

```ts
await invokeAgentRunExtensionRuntimeAction(agentRunRuntimeTarget, {
  action_key: "demo.action",
  input,
});
```

## Scenario: AgentRun Fork Outcome And Runtime Surface Contract

### 1. Scope / Trigger

- Trigger: browser-visible continue/fork/copy flows need AgentRun product identity while backend runtime projection remains RuntimeSession-based.
- Scope: `agentdash-contracts::workflow`, generated `workflow-contracts.ts`, AgentRun fork/composer/runtime routes, `agentdash-contracts::session` message refs, frontend services, workspace reducer/navigation, and Project membership authorization.

### 2. Signatures

HTTP surface:

```text
POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit
POST /agent-runs/{run_id}/agents/{agent_id}/fork
POST /agent-runs/{run_id}/agents/{agent_id}/fork-submit
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/events
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/stream/ndjson
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/context/projection
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/context/audit
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/tool-approvals/{tool_call_id}/approve
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/tool-approvals/{tool_call_id}/reject
```

Wire DTO shape:

```rust
pub struct AgentRunForkOutcomeView {
    pub outcome: AgentRunForkOutcomeKind, // forked
    pub parent: AgentRunForkParentRef,
    pub child: AgentRunForkChildRef,
    pub lineage: AgentRunForkLineageRef,
    pub fork_point: Option<SessionMessageRefDto>,
    pub mailbox_message: Option<MailboxMessageView>,
    pub redirect: AgentRunRedirectRef,
}

pub struct AgentRunMessageCommandResponse {
    pub command_receipt: AgentRunCommandReceiptView,
    pub outcome: AgentRunMessageOutcome,
    pub mailbox_message: Option<MailboxMessageView>,
    pub accepted_refs: Option<AgentRunAcceptedRefs>,
    pub runtime_state: Option<AgentRunRuntimeStateView>,
    pub fork: Option<AgentRunForkOutcomeView>,
}
```

Generated TypeScript is consumed directly from `packages/app-web/src/generated/workflow-contracts.ts`; fork point refs come from generated session contracts only as runtime boundary coordinates.

### 3. Contracts

- Product calls use AgentRun refs. A Session id in a DTO is a trace ref, not a route key for product commands.
- Project `Use` authorizes AgentRun workspace read, runtime trace read, start, continue own run, explicit fork, and fork-submit. Project `Configure` authorizes Project / ProjectAgent / VFS / backend access / workflow / MCP preset / skill asset mutation. Project `ManageSharing` authorizes membership changes.
- AgentRun owner is persisted on LifecycleRun / LifecycleAgent and controls whether composer submit writes parent mailbox or returns a fork outcome.
- `AgentRunMessageCommandResponse.fork` is present when the effective write target is a newly created child AgentRun. Frontend navigation uses `fork.redirect`.
- AgentRun runtime endpoints resolve the current delivery RuntimeSession server-side, then return existing generated session/runtime event or projection DTOs.
- Retained Session diagnostic endpoints can reuse session contracts but must not appear in product service imports for fork / lineage / rollback.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Member without Project `Configure` submits in own AgentRun | accepted through mailbox |
| Member submits in another user's visible AgentRun | response includes `fork` and child redirect |
| User lacks Project `Use` | workspace/runtime/fork routes reject before resolving trace data |
| Fork point ref omitted on explicit fork | backend forks at current model-visible head when valid |
| Fork point ref invalid or unstable | route returns structured invalid/precondition error; no child refs |
| Duplicate accepted fork command | response replays same `AgentRunForkOutcomeView` |
| Frontend sees `fork.redirect` | route changes to child workspace before refreshing conversation |
| Frontend sees no `fork` | existing mailbox outcome refresh path applies |

### 5. Good/Base/Bad Cases

- Good: generated response contains `fork.redirect`, frontend navigates by child `run_id + agent_id`, and sidebar/workspace read model later shows parent/child lineage.
- Good: AgentRun runtime projection service requests `/agent-runs/.../runtime/context/projection` and receives generated session projection DTO after server-side anchor resolution.
- Base: owner submit returns ordinary mailbox delivery outcome without fork field.
- Boundary mismatch: frontend stores a child `session_id` as the navigation target for a product fork.
- Canonical flow: frontend stores child AgentRun refs and treats RuntimeSession id as trace metadata.

### 6. Tests Required

- Contract check asserts `AgentRunForkOutcomeView`, fork command request/response DTOs, `AgentRunMessageCommandResponse.fork`, and runtime endpoint response DTOs generate to TypeScript.
- API tests cover member `Use`, editor/owner no silent parent takeover, fork redirect shape, duplicate replay, and retained Session diagnostic permission.
- Frontend service tests assert AgentRun scoped URLs and generated DTO consumption.
- Workspace tests assert composer fork redirect, explicit fork from round action, copy-last-agent-reply payload, and no product imports for raw Session fork / lineage / rollback services.

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```ts
const response = await runtimeTraceForkDiagnostic(runtimeTraceId, request)
navigate(runtimeTraceDetailRoute(response.child_trace_id))
```

#### Canonical

```ts
const response = await forkAgentRun(runId, agentId, request)
navigate(agentRunRoute(response.redirect.run_id, response.redirect.agent_id))
```

## Validation

```powershell
pnpm run contracts:check
cargo check -p agentdash-agent-protocol
pnpm run frontend:check
```

当 `agentdash-contracts` 引入后，`contracts:check` 同时运行所有 contract 生成器。

## Scenario: MCP Preset Runtime Binding And Probe Contract

### 1. Scope / Trigger

- Trigger: MCP Preset wire contract carries runtime binding declarations, stdio cwd, route policy, and probe target intent; ordinary preset probe accepts the edited transport plus optional binding declaration and resolves relay execution placement server-side.
- Scope: `agentdash-contracts::mcp_preset`, API routes under `/api/projects/{project_id}/mcp-presets`, Runtime Gateway setup action `mcp.probe_transport`, application backend probe target resolver, generated `packages/app-web/src/generated/mcp-preset-contracts.ts`, frontend MCP preset editor helpers, probe cache keys, and Project Agent MCP picker display.

### 2. Signatures

```rust
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfigDto {
    Http { url: String, headers: Vec<McpHttpHeader> },
    Sse { url: String, headers: Vec<McpHttpHeader> },
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<McpEnvVar>,
        cwd: Option<String>,
    },
}

pub struct McpRuntimeBindingConfigDto {
    pub mount_id: Option<String>,
    pub bindings: Vec<McpRuntimeBindingRuleDto>,
}

pub struct McpRuntimeBindingRuleDto {
    pub source: McpRuntimeBindingSourceDto,
    pub target: McpRuntimeBindingTargetDto,
    pub required: bool,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpRuntimeBindingSourceDto {
    VfsRootRef,
    VfsBackendId,
    WorkspaceId,
    WorkspaceBindingId,
    WorkspaceIdentity { path: Vec<String> },
    WorkspaceDetectedFact { path: Vec<String> },
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpRuntimeBindingTargetDto {
    HttpQuery { name: String },
    HttpHeader { name: String },
    StdioEnv { name: String },
    StdioCwd,
}

pub struct CreateMcpPresetRequest {
    pub transport: McpTransportConfigDto,
    pub route_policy: McpRoutePolicy,
    pub runtime_binding: Option<McpRuntimeBindingConfigDto>,
    // identity/display fields omitted
}

pub struct UpdateMcpPresetRequest {
    pub transport: Option<McpTransportConfigDto>,
    pub route_policy: Option<McpRoutePolicy>,
    pub runtime_binding: Option<Option<McpRuntimeBindingConfigDto>>,
    // other patch fields omitted
}

pub struct McpPresetResponse {
    pub transport: McpTransportConfigDto,
    pub route_policy: McpRoutePolicy,
    pub runtime_binding: Option<McpRuntimeBindingConfigDto>,
    // identity/source fields omitted
}

pub struct ProbeMcpPresetRequest {
    pub transport: McpTransportConfigDto,
    pub route_policy: McpRoutePolicy,
    pub probe_target: Option<McpProbeTargetDto>,
    pub runtime_binding: Option<McpRuntimeBindingConfigDto>,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpProbeTargetDto {
    DefaultUserLocal,
    Backend { backend_id: String },
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpProbeBackendTarget {
    DefaultUserLocal,
    Backend { backend_id: String },
}

pub async fn resolve_mcp_probe_backend_target(
    backend_repo: &dyn BackendRepository,
    project_repo: &dyn ProjectRepository,
    project_backend_access_repo: &dyn ProjectBackendAccessRepository,
    identity: &AuthIdentity,
    target: &McpProbeBackendTarget,
    online_backend_ids: &[String],
) -> Result<ResolvedMcpProbeBackendTarget, McpProbeBackendTargetResolutionError>;

#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProbeMcpPresetResponse {
    Ok { latency_ms: u32, tools: Vec<ProbeMcpToolInfo> },
    Error { error: String },
    Unsupported { reason: String },
}
```

Generated TypeScript shape:

```ts
export type ProbeMcpPresetRequest = {
  transport: McpTransportConfigDto;
  route_policy: McpRoutePolicy;
  probe_target?: McpProbeTargetDto;
  runtime_binding?: McpRuntimeBindingConfigDto;
};

export type ProbeMcpPresetResponse =
  | { status: "ok"; latency_ms: number; tools: Array<ProbeMcpToolInfo> }
  | { status: "error"; error: string }
  | { status: "unsupported"; reason: string };
```

### 3. Contracts

- Rust contract types in `agentdash-contracts::mcp_preset` are the wire source; frontend code consumes `generated/mcp-preset-contracts.ts` rather than re-declaring these unions.
- `McpTransportConfigDto::Http` and `Sse` carry `headers`; `Stdio` carries `command`, `args`, `env`, and optional `cwd`.
- `ProbeMcpPresetRequest.route_policy` is part of the probe semantics and cache key. `route_policy=relay` must execute through a resolved relay backend; `direct` and `auto + http/sse` keep direct probe behavior.
- `ProbeMcpPresetRequest.probe_target` expresses placement intent only. `default_user_local` means the backend resolves the current user's Desktop local runtime; `backend` means the backend validates the selected backend id before relay execution.
- Probe target resolution belongs to `agentdash-application::backend::resolve_mcp_probe_backend_target`. API route/bootstrap code may inject `AuthIdentity`, read online backend ids, and adapt DTOs, but owner/scope/registration-source selection and backend authorization are application-layer rules.
- `default_user_local` considers enabled online Desktop enrollment backends only: `backend_type=Local`, `owner_user_id=current_user`, `share_scope=User(current_user)`, and `device.registration_source="desktop_access_token"`. If multiple candidates exist, choose the latest `last_claimed_at` and use stable backend id ordering as the final tie-breaker.
- Explicit `backend` target reuses `BackendAuthorizationService::require_backend(identity, backend_id, BackendPermission::View)` and then requires `enabled=true` and online relay state.
- `CreateMcpPresetRequest.runtime_binding` creates a binding declaration; omission means static preset.
- `UpdateMcpPresetRequest.runtime_binding` is tri-state: missing means unchanged, `null` clears the declaration, and an object replaces the declaration.
- `McpPresetResponse.runtime_binding` mirrors the persisted declaration. The response does not include resolved runtime values because those belong to launch-time `RuntimeMcpServer`.
- `ProbeMcpPresetRequest` always sends the edited `transport` and includes optional `runtime_binding` when the edited form or saved preset has one, allowing the probe cache key to fingerprint both values.
- For HTTP/SSE probes, `ProbeMcpPresetRequest.transport.headers` are part of the connection parameters; backend probe code must pass them into the MCP HTTP client the same way real preset connections do.
- Ordinary preset probe has no AgentRun runtime context. If any binding rule is `required=true`, the response is `Unsupported { reason }` and should be displayed as a diagnostic state, not as a successful connectivity result.
- If all runtime binding rules are optional, ordinary probe continues with the static transport because no runtime fact is required to establish a static connection.
- Project Agent MCP picker preserves the response `runtime_binding` and may show a binding status badge; quick-create or selection flows must not rebuild presets field-by-field in a way that drops the binding declaration.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Generated TS differs from Rust contract | `pnpm run contracts:check` fails |
| Frontend create form has no binding rules | Omit `runtime_binding`; persisted preset remains static |
| Frontend update leaves binding untouched | Omit `runtime_binding` from patch |
| Frontend update clears binding | Send `"runtime_binding": null` |
| Frontend update changes binding | Send full `McpRuntimeBindingConfigDto` object |
| Probe request includes a required runtime binding | Return `status="unsupported"` with source-path diagnostic |
| Probe request includes only optional runtime bindings | Execute static transport probe and return `ok` or `error` |
| `route_policy=relay` and no default user local backend is online | Return `status="unsupported"` with a current-user local runtime diagnostic |
| `route_policy=relay` and multiple default user local backends are online | Probe through the latest claimed candidate without adding UI selection |
| Explicit backend target is missing, unauthorized, disabled, or offline | Return stable unavailable/unsupported diagnostic; do not route to another backend |
| Relay probe target is unresolved | Do not send `CommandMcpProbeTransport` |
| HTTP/SSE header or stdio cwd fields are missing from generated TS after Rust change | Contract drift or TypeScript compile failure |

### 5. Good/Base/Bad Cases

- Good: Creating a P4-aware HTTP preset sends `runtime_binding.bindings[0].source.kind="workspace_detected_fact"` and `target.kind="http_query"`.
- Good: Editing a stdio preset sends `transport.type="stdio"` with optional `cwd` and may bind `workspace.detected_facts.p4.workspace_root` to `stdio_cwd`.
- Good: A relay HTTP preset sends `probe_target.kind="default_user_local"`; the application resolver picks the current user's latest claimed online Desktop backend before the relay command is sent.
- Good: A future explicit runner probe sends `probe_target.kind="backend"`; the application resolver checks backend View permission and online state before relay execution.
- Base: A static HTTP preset has no `runtime_binding` and probes using only `transport`.
- Base: A card-level probe for a required runtime-bound preset returns `status="unsupported"` with a reason mentioning the required source path.
- Boundary mismatch: Frontend code treats probe unsupported as a normal error string and hides the binding diagnostic.
- Boundary mismatch: API bootstrap performs owner/scope/default-backend selection itself instead of delegating to the application backend resolver.
- Canonical flow: Frontend sends generated `ProbeMcpPresetRequest`; backend returns generated `ProbeMcpPresetResponse::Unsupported`; UI renders that status as a runtime-context-required diagnostic.
- Canonical flow: API maps wire target and current `AuthIdentity` into Runtime Gateway setup input; application resolves a concrete backend target; relay provider receives only an explicit backend id.

### 6. Tests Required

- Contract generation check asserts `McpRuntimeBindingConfigDto`, source/target DTO unions, `runtime_binding` request/response fields, `McpProbeTargetDto`, `ProbeMcpPresetRequest.route_policy`, `ProbeMcpPresetRequest.probe_target`, and `McpTransportConfigDto::Stdio.cwd` are present in `mcp-preset-contracts.ts`.
- Rust DTO conversion tests assert domain runtime binding and stdio cwd roundtrip through contract DTOs.
- API route test asserts create/read/update preserve `runtime_binding`, including update unchanged/clear/replace semantics.
- Application backend resolver tests assert default target filters Desktop personal local backends, chooses latest claimed online candidate, returns unavailable when none are online, and explicit backend target uses backend authorization plus online state.
- Runtime Gateway setup tests assert relay probe receives a resolved target before calling relay provider, and target-unavailable cases return `Unsupported` rather than transport failure.
- Probe service tests assert required runtime binding returns `status="unsupported"` and optional runtime binding continues static probe.
- Probe HTTP/SSE tests assert `transport.headers` are forwarded into the MCP HTTP client for ordinary preset probes, including static headers and optional runtime-binding headers that remain on the static transport.
- Frontend helper tests assert form state, create payload, update patch, validation, and probe cache key include `runtime_binding`, `route_policy`, and `probe_target`.
- Frontend picker/component test asserts bound presets are preserved and surfaced as a binding status.

### 7. Non-canonical / Canonical

#### Non-canonical

```ts
type LocalMcpPreset = {
  transport: HandWrittenTransport;
  runtimeBinding?: unknown;
};
```

#### Canonical

```ts
import type {
  McpRuntimeBindingConfigDto,
  ProbeMcpPresetRequest,
  ProbeMcpPresetResponse,
} from "@/generated/mcp-preset-contracts";
```

## Scenario: Workspace Module Presentation Contract

### 1. Scope / Trigger

- Trigger: Canvas Agent-facing create、bind、present 收束到 `workspace_module`，前端 WorkspacePanel 必须从同一事件契约打开 Canvas tab。
- Scope: Rust workspace module contract、generated TypeScript、session event reducer、WorkspacePanel tab opening。

### 2. Signatures

- `workspace_module_operate(operation="canvas.create" | "canvas.attach" | "canvas.copy", input={...}) -> WorkspaceModuleDescriptor`
- `workspace_module_describe(module_id: string) -> WorkspaceModuleDescriptor`
- `workspace_module_invoke(module_id: string, operation_key: string, input: unknown) -> operation result`
- `workspace_module_present(module_id: string, view_key: string) -> workspace_module_presented event`

### 3. Contracts

- Canvas module id is `canvas:{canvas_mount_id}`.
- Canvas bind operation key is `canvas.bind_data` and is discoverable through describe.
- Canvas render diagnostic operation key is `canvas.inspect` and returns `{ observation }`, where `observation` is the latest AgentRun-scoped Canvas runtime observation or `null`.
- Canvas interaction diagnostic operation key is `canvas.get_interaction_state` and returns `{ snapshot }`, where `snapshot` is the latest Canvas interaction snapshot or `null`.
- Canvas UI entry exposes `view_key="preview"` and `presentation_uri="canvas://{canvas_mount_id}"`.
- Canvas VFS edit URI is `{canvas_mount_id}://...` and may appear in tool results or diagnostics as `vfs_mount_uri`.
- `workspace_module_presented` payload includes `module_id`, `view_key`, `renderer_kind`, `presentation_uri`, `title`, and optional Canvas diagnostics such as `vfs_mount_uri`.
- Frontend opens Canvas tabs from `presentation_uri` only. `view_key` selects a module UI entry; it is not a Canvas id.

### 4. Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| Backend emits Canvas presentation without `presentation_uri` | contract/test failure |
| Canvas `presentation_uri` is not `canvas://{canvas_mount_id}` | contract/test failure |
| Frontend receives unsupported `renderer_kind` | ignore or show non-blocking unsupported renderer state |
| Frontend receives Canvas event with malformed `presentation_uri` | keep tab state unchanged; surface compact error/log |
| Generated TS drift after Rust DTO change | `pnpm run contracts:check` failure |

### 5. Reference Cases

- Canvas presentation flow: `workspace_module_present(canvas:{canvas_mount_id}, preview)` refreshes runtime surface, emits `workspace_module_presented.presentation_uri=canvas://{canvas_mount_id}`, and WorkspacePanel opens that URI.
- Canvas diagnostic flow: Canvas preview posts runtime observation and explicit interaction snapshots through AgentRun-scoped routes; Agent reads them through `workspace_module_invoke(canvas.inspect)` and `workspace_module_invoke(canvas.get_interaction_state)`.
- Extension presentation flow: Extension UI entries continue using their own renderer URI fields.
- URI responsibility: `view_key` selects the module UI entry, `canvas://{canvas_mount_id}` identifies the Canvas tab, and `{canvas_mount_id}://...` identifies the VFS authoring mount.

### 6. Tests Required

- Contract generation check for workspace module DTO/event payload.
- Backend test asserts Canvas present refreshes VFS/capability state before emitting `workspace_module_presented`.
- Backend test asserts Canvas descriptor exposes render and interaction diagnostic operations, and invoke returns latest facts without creating mailbox input.
- Frontend focused test asserts Canvas `workspace_module_presented.presentation_uri` opens the tab.
- Frontend typecheck asserts event handling consumes generated DTO fields, not hand-written aliases.

### 7. Wrong vs Correct

#### Wrong

```ts
openWorkspaceTab(`canvas://${event.view_key}`);
```

#### Correct

```ts
openWorkspaceTab(event.presentation_uri);
```

## Scenario: Canvas Runtime Observation, Interaction, And Agent Submit Contract

### 1. Scope / Trigger

- Trigger: Canvas runtime needs to expose user-visible render state, explicit UI interaction state, and Canvas-origin user actions to the current AgentRun.
- Scope: `agentdash-contracts::canvas`, AgentRun-scoped Canvas routes, generated `canvas-contracts.ts`, `CanvasRuntimePreview`, Canvas iframe SDK, AgentRun mailbox command response, and WorkspaceModule Canvas operation contracts.

### 2. Signatures

Browser SDK:

```ts
window.agentdash.interaction.setState(key, value)
window.agentdash.interaction.clearState(key)
window.agentdash.interaction.emit(event)
window.agentdash.interaction.getState()
window.agentdash.agent.submit({
  text?,
  input?,
  include_interaction_state?,
  include_render_observation?,
  delivery_intent?,
  client_command_id?,
})
```

AgentRun-scoped HTTP routes:

```text
GET  /api/agent-runs/{run_id}/agents/{agent_id}/canvases/{canvas_mount_id}/runtime-observation
POST /api/agent-runs/{run_id}/agents/{agent_id}/canvases/{canvas_mount_id}/runtime-observation
GET  /api/agent-runs/{run_id}/agents/{agent_id}/canvases/{canvas_mount_id}/interaction-snapshot
POST /api/agent-runs/{run_id}/agents/{agent_id}/canvases/{canvas_mount_id}/interaction-snapshot
POST /api/agent-runs/{run_id}/agents/{agent_id}/canvases/{canvas_mount_id}/agent-input-submit
```

Workspace module operations:

```text
workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.inspect", input={})
workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.get_interaction_state", input={})
```

### 3. Contracts

- Runtime observation is keyed by AgentRun, Agent, Canvas mount, and frame generation. It records latest ready/error/building status, viewport, DOM summary, diagnostics, and optional screenshot reference.
- Interaction snapshot is keyed by AgentRun, Agent, Canvas mount, and frame generation. It records explicit Canvas source state and recent user events.
- Observation and interaction snapshot uploads are diagnostic facts. They do not create mailbox input and do not automatically enter model-visible history.
- `window.agentdash.agent.submit(...)` is the Canvas-origin user input channel. The backend converts the request to canonical `UserInput` and calls AgentRun mailbox with `MailboxSourceIdentity { namespace: "core", kind: "canvas_action", actor: "user", ... }`.
- Submit response uses the existing `AgentRunMessageCommandResponse` so Canvas UI, workspace composer, scheduler outcome, and command receipt semantics stay aligned.
- `window.agentdash.invoke(...)` remains RuntimeGateway action invocation. It must not be used to submit user input to the Agent.
- The Canvas iframe never sends `sessionId`; parent page and backend resolve AgentRun, Agent, Canvas reference, current delivery runtime, and trace coordinates.
- If no live AgentRun bridge exists, Canvas preview may render but submit-to-Agent and diagnostic upload are unavailable with a clear UI/runtime error.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `frame_id` / `generation` does not match active iframe | Parent page ignores diagnostic upload or returns stale-generation submit error |
| Canvas preview lacks live AgentRun bridge | Runtime action submit returns bridge-unavailable diagnostic |
| Observation has not been uploaded | `canvas.inspect` returns `observation=null` |
| Interaction state has not been uploaded | `canvas.get_interaction_state` returns `snapshot=null` |
| Submit request has no `text` or `input` | Frontend rejects before POST or API returns bad request |
| Submit references current interaction/render ids | Backend accepts the mailbox command using canonical `UserInput` |
| Runtime action uses `agentdash.invoke` | Route goes through RuntimeGateway and does not create mailbox input |

### 5. Reference Cases

- Canvas ready flow: preview posts ready observation, backend stores latest runtime observation, Agent calls `canvas.inspect` and receives DOM/diagnostic summary.
- Canvas selection flow: source calls `interaction.setState("selection", ...)`, Agent calls `canvas.get_interaction_state` and sees the current selection without mailbox side effects.
- Canvas action flow: user clicks a Canvas button that calls `agent.submit({ text, include_interaction_state: true })`; backend creates a `core/canvas_action` mailbox message and scheduler returns the standard command response.

### 6. Tests Required

- Frontend runtime preview test asserts observation, interaction snapshot, and submit envelopes route through AgentRun Canvas bridge and report bridge-unavailable errors when missing.
- API test asserts AgentRun Canvas submit uses `MailboxSourceIdentity { namespace: "core", kind: "canvas_action" }` and returns `AgentRunMessageCommandResponse`.
- WorkspaceModule test asserts diagnostic operations are discoverable from describe and invoke reads latest facts only.
- Contract check asserts canvas observation, interaction snapshot, submit DTO, workspace module operation dispatch, and `MailboxSourceIdentity` stay generated in TypeScript.

## Scenario: Canvas Personal And Project Shared Distribution Contract

### 1. Scope / Trigger

- Trigger: Canvas wire contract now carries ownership, scope, lineage and effective access, and the Project asset UI consumes publish/copy/unpublish commands.
- Scope: `agentdash-contracts::canvas`, `/api/projects/{project_id}/canvases`, `/api/canvases/{id}`, generated `packages/app-web/src/generated/canvas-contracts.ts`, frontend Canvas service/types/UI, VFS runtime mount access and WorkspaceModule descriptor access.

### 2. Signatures

Backend command/API signatures:

```text
GET  /api/projects/{project_id}/canvases?scope=all|mine|shared
POST /api/projects/{project_id}/canvases
GET  /api/projects/{project_id}/canvases/by-mount/{canvas_mount_id}
GET  /api/canvases/{id}
PUT  /api/canvases/{id}
DELETE /api/canvases/{id}
POST /api/canvases/{id}/publish-to-project
POST /api/canvases/{id}/copy-to-personal
POST /api/canvases/{id}/unpublish
POST /api/canvases/{id}/promote-extension
```

Generated DTOs:

```rust
#[serde(rename_all = "snake_case")]
pub enum CanvasScopeDto { Personal, Project }

#[serde(rename_all = "snake_case")]
pub enum CanvasListScopeDto { All, Mine, Shared }

pub struct CanvasAccessDto {
    pub can_view: bool,
    pub can_edit_source: bool,
    pub can_publish: bool,
    pub can_manage_shared: bool,
    pub can_copy: bool,
    pub runtime_write_allowed: bool,
}

pub struct CanvasResponse {
    pub canvas_id: String,
    pub project_id: String,
    pub owner_user_id: Option<String>,
    pub scope: CanvasScopeDto,
    pub access: CanvasAccessDto,
    pub canvas_mount_id: String,
    pub vfs_mount_id: String,
    pub title: String,
    pub description: String,
    pub entry_file: String,
    pub sandbox_config: CanvasSandboxConfigDto,
    pub files: Vec<CanvasFileDto>,
    pub published_from_canvas_id: Option<String>,
    pub shared_canvas_id: Option<String>,
    pub cloned_from_canvas_id: Option<String>,
    pub published_at: Option<String>,
    pub published_by_user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

### 3. Contracts

- New Canvas creation through the Project Canvas API creates a `scope="personal"` Canvas owned by `AuthIdentity.user_id`.
- `scope=mine` returns current user's personal Canvas; `scope=shared` returns project shared Canvas; `scope=all` returns both after effective access filtering.
- A project shared Canvas is an independent deep-copy source record with `published_from_canvas_id`, `published_at` and `published_by_user_id`.
- A copied Canvas is a new personal Canvas with its own `canvas_id`, own `canvas_mount_id`, copied authoring payload and `cloned_from_canvas_id`.
- `PUT /api/canvases/{id}` requires `access.can_edit_source`; project shared source is not edited through the ordinary update route.
- `DELETE /api/canvases/{id}` deletes personal Canvas only for editable owner access; project shared deletion uses management/unpublish semantics and clears the personal source `shared_canvas_id` when applicable.
- `promote-extension` remains packaged extension publication. It is separate from `publish-to-project`, which creates or updates a project shared Canvas source.
- Frontend service and UI consume generated DTOs directly. Canvas UI action availability is driven by `CanvasResponse.access`, not by frontend user-id inference.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Missing/invalid `scope` query | Default to `all` when missing; invalid value returns bad request |
| Other user's personal Canvas requested by list/get/by-mount | Deny or omit through Canvas effective access |
| Personal owner updates source | Allowed when `can_edit_source=true` |
| Project shared member updates source | Forbidden because `can_edit_source=false` |
| Project shared member copies source | Allowed when `can_copy=true`; result is personal editable clone |
| Publisher/project manager unpublishes shared Canvas | Delete shared record and clear source `shared_canvas_id` |
| Shared Canvas enters runtime VFS | Mount has read/list/search and no write capability |
| Shared Canvas WorkspaceModule descriptor | Preview UI entry remains; `canvas.bind_data` is omitted |
| Rust DTO or generated TS drift | `pnpm run contracts:check` fails |

### 5. Good/Base/Bad Cases

- Good: A user creates a personal Canvas, edits files, publishes it to project shared, and another member copies that shared Canvas into a new personal Canvas before editing.
- Good: A project shared Canvas preview opens from `canvas://{canvas_mount_id}` while its VFS mount omits `write`.
- Base: A personal Canvas that has never been published has `shared_canvas_id=null`, `cloned_from_canvas_id=null`, and owner access includes source edit.
- Bad: A project shared Canvas response includes `access.runtime_write_allowed=true`, because runtime write capability would disagree with HTTP and WorkspaceModule permissions.
- Canonical flow: route handler maps `CanvasWithAccess` into generated `CanvasResponse`; frontend reads `canvas.access` to render actions and source editor state.

### 6. Tests Required

- Contract check asserts Canvas scope/access/lineage DTO fields and publish/copy/unpublish request/response DTOs are generated.
- API tests assert scope query parsing, response mapping, personal delete vs shared unpublish decision, and Canvas effective access for update/get/by-mount/runtime routes.
- Application tests assert publish/copy/unpublish deep-copy lineage and access projection.
- VFS tests assert writable personal Canvas includes `write`, read-only project shared Canvas omits `write`, and provider write/delete/rename reject read-only mounts.
- WorkspaceModule tests assert personal and shared Canvas descriptors expose `canvas.bind_data`, bind writes AgentRun-scoped runtime metadata, and Canvas source bindings remain unchanged.
- Frontend service tests assert scoped list query and publish/copy/unpublish endpoints.
- Frontend typecheck asserts Canvas service/UI consume generated DTO aliases without hand-written wire unions.

### 7. Non-canonical / Canonical

#### Non-canonical

```ts
const editable = currentUser.id === canvas.owner_user_id || projectRole === "editor";
```

#### Canonical

```ts
const editable = canvas.access.can_edit_source === true;
```

## Scenario: AgentRun Runtime Frame Resolution Contract

### 1. Scope / Trigger

- Trigger: AgentRun runtime 可以在同一个 delivery session 内采用新的 `AgentFrame` revision，例如 Canvas create/bind/present 写入新的 VFS mount 和 workspace module visibility。
- Scope: backend session-facing frame resolver、AgentRun Workspace projection、Canvas runtime snapshot、Session control view、WorkspacePanel Canvas tab opening。

### 2. Signatures

Backend resolver signature:

```rust
resolve_current_frame_from_delivery_trace_ref(
    runtime_session_id: &str,
    anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    agent_repo: &dyn LifecycleAgentRepository,
    frame_repo: &dyn AgentFrameRepository,
) -> Result<Option<(RuntimeSessionExecutionAnchor, LifecycleAgent, AgentFrame)>, DomainError>
```

Browser-facing projection fields:

```rust
AgentRunWorkspaceView {
    delivery_trace_meta?: RuntimeSessionTraceMeta,
    frame_runtime?: AgentFrameRuntimeView,
    resource_surface?: ResolvedVfsSurface,
    resource_surface_coordinate?: AgentRunResourceSurfaceCoordinateView,
}
```

Database schema:

```sql
runtime_session_execution_anchors(
    runtime_session_id text primary key,
    run_id text not null,
    agent_id text not null,
    launch_frame_id text not null
);

agent_run_delivery_bindings(
    run_id uuid not null,
    agent_id uuid not null,
    runtime_session_id text not null,
    launch_frame_id uuid not null,
    status text not null,
    primary key (run_id, agent_id)
);

-- lifecycle_agents.current_frame_id/current_delivery_* are not part of the runtime frame contract.
```

### 3. Contracts

- Session-facing frame reads start from `runtime_session_id` and resolve through `RuntimeSessionExecutionAnchor`.
- `RuntimeSessionExecutionAnchor.launch_frame_id` is launch evidence; it is not the current workspace surface after runtime adoption.
- `resolve_current_frame_from_delivery_trace_ref` validates anchor -> agent -> run ownership before returning the effective `AgentFrame`.
- `AgentFrameRepository.get_current(agent_id)` is a repository-level revision lookup used inside resolvers or static non-session views. Frontend-facing AgentRun, Canvas, VFS and Session runtime paths must not choose a frame from a raw agent id when a delivery runtime session is available.
- `AgentRunDeliveryBinding` stores the current delivery binding keyed by `run_id + agent_id`. `LifecycleAgent` does not store current delivery or current frame pointers.
- Canvas presentation opens from `workspace_module_presented.presentation_uri = canvas://{canvas_mount_id}`. The runtime surface refresh may happen before opening, but the concrete presentation URI is authoritative for tab creation.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `runtime_session_id` has no execution anchor | Return not found / no runtime surface projection |
| Anchor agent does not belong to anchor run | Treat resolver result as unavailable |
| Effective frame belongs to a different agent | Treat resolver result as unavailable |
| Effective frame has Canvas mount `{canvas_mount_id}` | AgentRun `resource_surface` exposes the Canvas mount and Canvas snapshot uses the same frame |
| `AgentRunView` exposes `current_frame_id` | Contract drift; remove the field and consume `frame_runtime.frame_ref` where a UI needs display-only frame identity |
| Workspace module event has `presentation_uri=canvas://{canvas_mount_id}` while runtime surface is refreshing | Open or activate the Canvas tab and refresh its content after workspace state update |

### 5. Reference Cases

- Canvas presentation flow: `workspace_module_present(canvas:{canvas_mount_id}, preview)` creates/adopts a new frame revision; Agent runtime tools, AgentRun Workspace `resource_surface`, Canvas runtime snapshot, and WorkspacePanel tab all observe the same mount.
- Session control flow: Session control view receives a runtime session id and resolves frame runtime through `resolve_current_frame_from_delivery_trace_ref`.
- Draft/static run flow: A draft/static run view with no delivery runtime may show the latest frame revision via `AgentFrameRepository.get_current(agent_id)` because no session anchor exists.
- Frame freshness: Canvas snapshot and AgentRun resource surface resolve from the current adopted frame revision so late Canvas exposure is visible to both runtime and UI.
- Presentation ordering: WorkspacePanel opens `canvas://{canvas_mount_id}` from the presentation payload while runtime surface refresh catches up.

### 6. Tests Required

- Backend unit test asserts `DeliveryRuntimeSelectionService` returns the effective current frame for the current delivery binding.
- API/session test asserts Canvas/runtime VFS resolution uses the current adopted frame rather than launch frame evidence.
- Contract check asserts `AgentRunView` has no `current_frame_id` field.
- Frontend Workspace module test asserts `workspace_module_presented.presentation_uri` opens the Canvas tab and does not synthesize a Canvas URI from `view_key`.
- Frontend WorkspacePanel/store test asserts concrete `canvas://{canvas_mount_id}` can be opened before the refreshed runtime surface has been rendered.

### 7. Wrong vs Correct

#### Wrong

```rust
let frame = frame_repo
    .get(anchor.launch_frame_id)
    .await?
    .or(frame_repo.get_current(agent.id).await?);
```

#### Correct

```rust
let (_anchor, _agent, frame) = resolve_current_frame_from_delivery_trace_ref(
    runtime_session_id,
    anchor_repo,
    agent_repo,
    frame_repo,
)
.await?
.ok_or_else(|| WorkflowApplicationError::NotFound("runtime frame unavailable".to_string()))?;
```

## Scenario: AgentRun Whole-Run Delete Contract

### 1. Scope / Trigger

- Trigger: 浏览器需要删除 Agent 主页面中的一个主 AgentRun，并让后端统一清理对应 LifecycleRun、LifecycleAgent tree、delivery RuntimeSession trace facts 与 run-owned projection。
- Scope: `DELETE /api/projects/{project_id}/agent-runs/{run_id}`、`agentdash-contracts::workflow::DeleteAgentRunResponse`、AgentRun 删除 application command、AgentRun list projection refresh。

### 2. Signatures

HTTP API:

```text
DELETE /api/projects/{project_id}/agent-runs/{run_id}
```

Response DTO:

```rust
#[serde(rename_all = "snake_case")]
pub struct DeleteAgentRunResponse {
    pub deleted: bool,
    pub project_id: String,
    pub run_id: String,
}
```

Frontend service:

```ts
deleteAgentRun(projectId: string, runId: string): Promise<DeleteAgentRunResponse>
```

Application command:

```rust
pub struct AgentRunDeleteCommand {
    pub project_id: Uuid,
    pub run_id: Uuid,
}
```

### 3. Contracts

- Delete intent is Project-scoped because AgentRun list projection is Project-scoped and deletion must validate both Project edit permission and run ownership.
- The product command target is the whole AgentRun / `LifecycleRun`; child Agent rows are not independent delete targets in this contract.
- RuntimeSession ids are collected from `RuntimeSessionExecutionAnchor` rows and active `AgentRunDeliveryBinding` rows after the run ownership check. `AgentRunDeliveryBinding` supplies current delivery state; `LifecycleAgent` stores identity only. RuntimeSession cleanup serves the AgentRun delete command and is not the browser-facing product action.
- The command rejects active work before any delete side effect. `LifecycleRunStatus::Running`, `SessionExecutionState::Running`, and `SessionExecutionState::Cancelling` all block deletion.
- Successful deletion removes RuntimeSession trace facts first, then deletes the `LifecycleRun`; run-owned lifecycle rows, anchors, mailbox rows, and frame relations rely on existing database cascades.
- Frontend success handling refreshes the AgentRun list projection from the server. If the current route points at the deleted `run_id`, the browser navigates back to the Agent page.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `project_id` or `run_id` is not a UUID | `400 Bad Request` |
| Current user lacks Project edit permission | Project authorization error |
| `run_id` does not exist | `404 Not Found` |
| Run belongs to another Project | `404 Not Found` or equivalent non-disclosing Project ownership error |
| Run status is `running` | `409 Conflict`, no RuntimeSession or LifecycleRun delete |
| Any associated RuntimeSession is `running` or `cancelling` | `409 Conflict`, no RuntimeSession or LifecycleRun delete |
| Associated RuntimeSession is already missing | Continue deleting the AgentRun facts |
| LifecycleRun delete fails after session cleanup | Return the application error; caller refreshes projection before presenting final state |

### 5. Good/Base/Bad Cases

- Good: A completed Project AgentRun with two terminal RuntimeSessions is deleted; both sessions are removed through the session core and the LifecycleRun disappears from the Project AgentRun list after refresh.
- Base: A draft or idle AgentRun with no RuntimeSession deletes only the LifecycleRun and cascaded run-owned rows.
- Bad: A running AgentRun receives a delete request and any RuntimeSession or LifecycleRun row is deleted before the conflict is returned.

### 6. Tests Required

- Application test asserts terminal RuntimeSessions are deleted before LifecycleRun delete and the outcome includes the deleted session ids.
- Application test asserts cross-Project delete returns not found / ownership error without deleting sessions or the run.
- Application test asserts `Running` and `Cancelling` RuntimeSession states return conflict before any delete side effect.
- API check/test asserts the route compiles with `DeleteAgentRunResponse` and Project edit permission path.
- Contract check asserts `DeleteAgentRunResponse` is generated into `workflow-contracts.ts`.
- Frontend test asserts the service calls `/projects/{project_id}/agent-runs/{run_id}` and consumes the generated response type.
- Frontend test asserts the main AgentRun row exposes the delete menu, uses a lightweight danger confirmation, refreshes list projection on success, and protects row-open keyboard/click behavior from nested menu interactions.

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```ts
await deleteSession(runtimeSessionId);
```

#### Canonical

```ts
await deleteAgentRun(projectId, runId);
await refreshProjectAgentRuns(projectId, "agent_run_deleted");
```
