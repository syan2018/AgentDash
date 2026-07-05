# Session Startup Pipeline

本 appendix 定义当前 session 构建与 prompt launch 的生产主线。目标控制面中，当前 `Session` 语义降级为 `RuntimeSession`；本主线由 `AgentFrame -> FrameLaunchEnvelope -> ExecutionContext` 投影承接，业务入口不得继续把 session owner 当作控制面事实源。模块不变量见 [Session Architecture](./architecture.md)。

## Pipeline

```text
LaunchCommand
  -> FrameLaunchEnvelope
  -> LaunchPlan
  -> PreparedTurn
  -> ConnectorAcceptedTurn
  -> CommittedTurn
  -> AttachedTurn
```

`LaunchCommand` 表达来源意图；`FrameLaunchEnvelope` 是 frame construction 输出的 launch-ready facts；`LaunchPlan` 是单轮启动决策；后续 stage types 表达 accepted 前准备、connector accepted、accepted 后 commit 与 stream attach。`ExecutionContext` 只在 connector 边界投影。

Canonical launch command 类型归属 `agentdash-application-ports::launch`，原因是 HTTP/API、AgentRun mailbox、Workflow/Routine、Companion、Hook resume 与 Local relay 都需要在进入 application 编排前表达同一份启动意图。该 namespace 按 `command.rs` 与 `modifier.rs` 组织：`command.rs` 承载 `LaunchCommand`、`LaunchSource`、`LaunchPromptInput` 与 `LaunchPlanningInput` 等主合同，`modifier.rs` 承载来源差异的 typed modifier。这样来源入口可以共享稳定 DTO/port 合同，session runtime 模块只消费 canonical command 与 launch-ready facts。

## Stage Responsibilities

| 阶段 | 输入 | 输出 | 职责 |
| --- | --- | --- | --- |
| Source adapter | HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 请求 | `LaunchCommand` | 保留来源身份、请求意图、source policy、prompt payload、executor override、follow-up hint；来源专属附加事实进入 typed `LaunchModifier` |
| Frame construction | `LaunchCommand` + runtime session anchor + lifecycle/domain/runtime facts | `FrameLaunchEnvelope` | 解析 `RuntimeSessionExecutionAnchor`、current/pending `AgentFrame`、working dir、VFS、MCP、capability、context bundle/frame、identity、query/audit/inspector projection、resolution trace |
| Launch planning | `LaunchCommand` + `FrameLaunchEnvelope` + runtime facts | `LaunchPlan` | 解析 resolved prompt payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input |
| Turn preparation | `LaunchPlan` | `PreparedTurn` | claim/activate turn，准备 runtime tools、MCP tools、hook runtime、context frames、pending runtime context application 与 connector `ExecutionContext` |
| Connector start | `PreparedTurn` | `ConnectorAcceptedTurn` | 调用 `connector.prompt`，以返回 `ExecutionStream` 作为 accepted 边界；setup 失败时释放 turn/hook 并记录 failed terminal |
| Accepted commit | `ConnectorAcceptedTurn` | `CommittedTurn` | 提交 user message、`TurnStarted`、context/capability projection event、bootstrap meta、runtime command `applied` 与本地 title derivation |
| Stream ingestion | `CommittedTurn` | `AttachedTurn` | spawn `SessionTurnProcessor` 与 stream adapter，并登记 processor tx / adapter abort handle |
| Terminal | connector terminal / stream terminal | terminal event + outbox effect | 持久化终态，清理 active turn，把业务副作用写入 durable outbox |

`Turn` 边界保持很薄：reservation、active、cancel、hook runtime handle、processor/adapter supervision、terminal release。

## Source Adapter Contract

Source adapter 只做来源语义转换，不能预先组装最终运行事实。`LaunchCommand` 的核心字段表达通用 turn launch intent；Routine、Companion、Hook resume、Local relay 等来源差异通过 typed `LaunchModifier` 携带。modifier 是 frame construction / launch planning 的输入事实，不是并列启动路径，原因是所有入口都需要经过同一套 anchor 解析、surface 校验、capability projection 和 accepted 边界。

`backend_selection` 属于 launch planning input。它进入 `LaunchPlanningInput` 后由 planner 解析为 execution placement，原因是 backend 选择是本轮执行位置决策；来源身份、AgentRun owner 与 frame construction facts 继续由 launch identity 和 frame construction pipeline 表达。

| 来源 | `LaunchCommand` 应携带 |
| --- | --- |
| HTTP prompt | request DTO、auth identity、prompt payload、executor override |
| Task service | task id、phase/override/additional prompt source hint、task source identity |
| Workflow orchestrator | workflow/lifecycle source identity、activity activation intent |
| Routine executor | routine source identity、execution id、trigger source、entity key，系统身份来自 `AuthIdentity::system_routine(routine.id)` |
| Companion dispatch / parent resume | parent agent/frame refs、dispatch/slice/gate/source policy；parent session id 只作为 trace provenance |
| Hook auto-resume | hook trigger identity、resume intent、follow-up hint |
| Local relay | workspace root、已解析 MCP runtime server、relay source identity |

`working_dir` 是 frame construction 解析结果，不属于用户 prompt input。resolved VFS、resolved MCP、capability state、context bundle 和 connector input 都由 frame construction / launch 产出。

Task terminal effect 使用 durable lifecycle refs 描述，由 launch effects 解析。command 边界不传内存 `post_turn_handler` 或其它 trait object。

Routine source metadata 只表达触发来源与当前 execution facts。`routine` VFS mount、Routine memory skill projection 和最终 capability state 在 frame construction 阶段形成，原因是 Routine 的跨轮次上下文必须与 runtime facts 一起进入同一份 final VFS projection。

ProjectAgent AgentRun start 是两层 receipt：外层 `project_agent_start` receipt 只证明 AgentRun owner、runtime anchor 和 draft mailbox envelope 已 durable materialize；首条用户输入的投递结果由 `AgentRunMessageCommandResponse` 表达。外层 start 不以 `SessionLaunchService::launch_command` 完成作为 accepted 边界，原因是启动入口需要先形成可恢复的 AgentRun workspace，再让 mailbox scheduler 根据 runtime state 投递首条消息。前端只能把 start response 当作可导航的 AgentRun projection，不能把 `turn_id` 或 route success 当作 connector accepted。

## Frame Construction Contract

`SessionConstructionProvider::build_frame_construction` 直接输出 launch-ready `FrameLaunchEnvelope`，不是 seed、partial plan 或等待 planner 补齐的中间形态。生产实现是 `FrameConstructionService::construct_launch_envelope`。

`FrameConstructionService` 通过 `RuntimeSessionExecutionAnchor` 反查 `LifecycleRun` / `LifecycleAgent` / current `AgentFrame`，先选择 owner surface composer：ProjectAgent、LifecycleNode 或 ExistingSurface；再把 companion 等 source modifier 应用到 owner surface 上生成 envelope。ProjectAgent / owner bootstrap 路径由 `workflow::frame_construction::owner_bootstrap` 组合 owner surface，原因是该路径产出写入 `AgentFrame` 的 VFS、MCP、capability、context bundle 与 execution profile。modifier 只能补充或约束来源语义，不能替代 owner route，原因是 AgentRun 的 workspace、权限、VFS/MCP、capability 与 context 必须先有明确控制面 owner。业务模块不得绕过该服务自行组装 connector facts。

`FrameLaunchEnvelope` 至少覆盖：

- `FrameSurfaceDraft`，由 construction pipeline 汇总 capability、VFS、MCP、context bundle summary 与 execution profile surface，并作为写入 `AgentFrame` revision 的 typed handoff。
- `FrameLaunchSurface`，由 `FrameSurfaceDraft` 在 envelope 构造边界校验生成，字段为 non-optional，是 launch planner、turn preparation 与 connector projection 的唯一 runtime surface 读取入口。
- `FrameRuntimeSurface`，只来自 `AgentFrame` 持久化 surface。
- `FrameLaunchIntent`，只来自 `LaunchCommand` / composer launch extras。
- workspace 与 typed working directory。`working_directory` 必须在进入 launch planner 前解析完成。
- final VFS、MCP runtime server resolution、capability state。
- `SessionContextBundle` 与 continuation/context frames。
- identity、source contract、query/audit/inspector projections。
- resolution trace，用于审计为什么选择某个 owner/workspace/context。

Launch 前必须在 `FrameLaunchEnvelope` 构造边界完成等价 gate：

- 缺少 `working_directory`、`executor_config`、`vfs`、`capability_state` 时拒绝 launch。
- `launch_surface.capability_state.vfs.active` 必须等于 `launch_surface.vfs`。
- `launch_surface.capability_state.tool.mcp_servers` 必须等于 `launch_surface.mcp_servers`。
- `FrameLaunchEnvelope` 不保留与 typed surface 并列的 executor/capability/VFS/MCP 字段；launch planner、turn preparation 与 MCP tool assembly 只读取 `FrameLaunchSurface`，原因是 AgentFrame revision 与 construction draft 应成为 launch 面的同一事实闭包。
- `CapabilityState.tool.mcp_servers` 是 capability/draft projection，用于 runtime command replay、tool policy 关联和工具装配快照；AgentRun 当前可执行 MCP surface 的事实源是 `AgentFrame.surface.mcp_surface` / `FrameSurfaceDraft.mcp_servers`。旧 split columns 只作为 repository projection，不能作为新的写源。
- pending runtime command 的 overlay 由 frame construction 形成 final capability projection；`requested -> applied` 副作用只能在 connector prompt accepted 后提交。

Frame construction 可以消费 runtime facts，但这些 facts 一旦进入 `FrameLaunchEnvelope` 就必须体现在 `resolution_trace` 中。LaunchPlanner 不允许再读取 cached profile、hub default VFS、local relay workspace root 或 source MCP server 来补齐 VFS/MCP/capability/executor facts。

Context endpoint、权限展示、audit 和 inspector 都从 `AgentFrame` / `FrameLaunchEnvelope` 同源投影。API route 的职责是 auth/permission、DTO 转换、调用 use case、映射 response DTO。

## Scenario: ProjectAgent Backend Requirement

### 1. Scope / Trigger

- Trigger: ProjectAgent preset 可以声明 backend 是否为硬性运行依赖。
- Scope: `AgentPresetConfig.backend_requirement`、ProjectAgent preset editor、AgentRun mailbox launch planning、backend execution placement、ProjectAgent owner frame construction、runtime MCP / workspace module / backend-bound capability projection。

### 2. Signatures

```rust
#[serde(rename_all = "snake_case")]
pub enum AgentBackendRequirement {
    Required,
    Optional,
}

pub struct AgentPresetConfig {
    pub backend_requirement: Option<AgentBackendRequirement>,
    // other preset fields omitted
}

pub struct LaunchPlanningInput {
    pub backend_selection: Option<BackendSelectionInput>,
    pub backend_requirement: Option<AgentBackendRequirement>,
    pub authorized_backend_ids: Vec<String>,
}
```

Frontend ProjectAgent config 使用同名 snake_case 字段：

```ts
type AgentBackendRequirement = "required" | "optional";

interface AgentPresetConfig {
  backend_requirement?: AgentBackendRequirement;
}
```

### 3. Contracts

- `backend_requirement` 缺省按 `required` 解释，原因是存量 ProjectAgent 默认依赖 backend-bound workspace / local capability。
- ProjectAgent mailbox launch planning 必须从 ProjectAgent preset config 解析 requirement，并写入持久化 mailbox `launch_planning_input`，原因是排队后实际 launch 需要消费同一份运行环境要求。
- `required` 模式下，没有授权 backend、没有在线可用 executor、或 workspace binding backend 不可用，必须返回用户可见 unavailable / connection failure。
- `optional` 模式下，自动 backend placement 解析失败可以产出无 backend placement 的 launch；显式 `backend_selection` 仍按授权、在线状态和 executor 可用性校验。
- ProjectAgent frame construction 只有在存在真实在线 backend-bound workspace 时才构造 backend-bound `main` surface。
- 无 backend anchor 的 ProjectAgent surface 不投影 relay MCP、workspace module、terminal、extension host 等 backend-bound capability，原因是 runtime surface 必须真实表达当前可执行能力。

### 4. Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| config 缺少 `backend_requirement` | 解析为 `required` |
| `backend_requirement = required` 且 Project 无授权 backend | `ConnectorError::ConnectionFailed`，API 映射为 unavailable |
| `backend_requirement = required` 且无在线 executor | `ConnectorError::ConnectionFailed`，API 映射为 unavailable |
| `backend_requirement = optional` 且自动选择无可用 backend | 允许 launch，无 backend placement |
| `backend_requirement = optional` 且显式 backend 离线或未授权 | 返回用户可见错误 |
| 无 backend anchor 的 ProjectAgent frame | 不包含 backend-bound `main` workspace、relay MCP、workspace module 和本机 runtime capability |

### 5. Good/Base/Bad Cases

- Good: cloud-native ProjectAgent 配置 `optional`，Project 当前没有在线 backend，AgentRun 仍创建并投递，runtime surface 只包含云端可用能力。
- Good: ProjectAgent 配置 `optional` 且用户显式选择某个 backend，目标 backend 离线时返回 unavailable 提示。
- Base: 存量 ProjectAgent config 没有该字段，前后端均展示并执行 `required`。
- Boundary: optional 只表达“可无 backend placement”；一旦 surface 声明了 backend-bound 能力，该能力必须来自真实 backend anchor。

### 6. Tests Required

- Domain test: `AgentPresetConfig` 解析缺省值为 `required`，并能保存 / 读取 `optional`。
- Runtime placement test: `required` 无可用 backend 返回 connection failure；`optional` 自动选择失败允许无 placement；显式离线 backend 仍失败。
- Frame construction test: 无 backend anchor 时 relay MCP 和 workspace module capability 不进入 runtime surface。
- AgentRun mailbox test: ProjectAgent config 的 `backend_requirement` 进入持久化 `LaunchPlanningInput`。
- Frontend test: ProjectAgent preset 表单缺省展示 `required`，并能保存 `optional`。

### 7. Boundary vs Canonical

#### Boundary

```text
ProjectAgent(optional)
  -> 无 backend placement
  -> runtime surface 仍携带 backend-bound main / relay MCP / workspace module
```

#### Canonical

```text
ProjectAgent(optional)
  -> 无 backend placement
  -> runtime surface 只保留不依赖本机 backend 的能力
```

## Capability Projection Normalization

Session runtime surface、VFS、MCP、Skill baseline 与 `CapabilityState` 是同一份 frame construction projection 的不同维度。

Core entries:

- `derive_session_capability_projection(SessionCapabilityProjectionInput) -> SessionCapabilityProjection`
- `normalize_capability_state_dimensions(&mut CapabilityState, Option<Vfs>, Vec<RuntimeMcpServer>, &SessionBaselineCapabilities)`
- `FrameConstructionService::construct_launch_envelope(...) -> FrameLaunchEnvelope`

Contract:

- `CapabilityResolver` 只解析 tool / MCP / companion 维度。
- Effective VFS 由 frame construction 合并 frame/session/runtime-command facts 后确定。
- Skill baseline 与 guidelines 从 effective VFS 派生。
- `CapabilityState.vfs.active` 必须等于 `FrameLaunchEnvelope.launch_surface.vfs`。
- `CapabilityState.tool.mcp_servers` 必须等于 `FrameLaunchEnvelope.launch_surface.mcp_servers`。
- `runtime_surface` 是 query DTO，只从 `FrameLaunchEnvelope.launch_surface.vfs` / `AgentFrame.surface.vfs_surface` 生成；split `agent_frames.*_json` columns 只作为 read projection / migration materialization path。
- `AgentFrame` 的 VFS / MCP / capability surface 通过 `AgentFrameBuilder::with_surface_draft` 集中写入，原因是 launch 装配面、query DTO 面和 capability replay 必须跟随 effective capability VFS 保持一致。
- `FrameSurfaceDraft` 是 construction 到 `AgentFrameBuilder` / `FrameLaunchEnvelope` 的显式交接结构。`FrameLaunchSurface` 是从该 draft 校验得到的 launch-ready typed surface，原因是 construction validation、launch planning、connector projection 和 query surface 必须观察同一份 typed handoff，且 planner 不应读取 optional draft 字段。

## AgentFrame Surface Document

`AgentFrame.surface` 是 AgentFrame revision 的 canonical runtime surface document，包含 capability state、context slice、VFS surface、MCP surface、execution profile、visible canvas mounts 与 visible workspace module refs。Repository 写入时从 `AgentFrameSurfaceDocument` 投影到 split columns；读取时优先使用 `surface`，缺失时才从 split columns 物化 document。这样迁移期可以保留既有查询投影，同时让新的写路径只有一个 surface source。

Frame construction、accepted launch commit、runtime surface update 和 fork materialization 写 AgentFrame revision 时必须调用 surface document / surface draft 写入路径。直接把 `effective_capability_json`、`vfs_surface_json` 或 `mcp_surface_json` 当作并列写源会造成 launch planner、runtime query 和 context delivery 读取不同事实。

## Scenario: MCP Runtime Binding During Frame Construction

### 1. Scope / Trigger

- Trigger: MCP Preset 可以声明运行时绑定；frame construction、capability resolver、direct/relay/local MCP runtime 都必须消费同一份已解析 `RuntimeMcpServer`。
- Scope: Project `McpPreset.runtime_binding`、final VFS `main` mount metadata、`mcp_preset_keys`、`mcp:<preset>` capability directive、`FrameLaunchEnvelope.launch_surface.mcp_servers` 与 `CapabilityState.tool.mcp_servers`。

### 2. Signatures

```rust
pub struct McpRuntimeBindingConfig {
    pub mount_id: Option<String>, // default: "main"
    pub bindings: Vec<McpRuntimeBindingRule>,
}

pub struct McpRuntimeBindingRule {
    pub source: McpRuntimeBindingSource,
    pub target: McpRuntimeBindingTarget,
    pub required: bool,
}

pub enum McpRuntimeBindingSource {
    VfsRootRef,
    VfsBackendId,
    WorkspaceId,
    WorkspaceBindingId,
    WorkspaceIdentity { path: Vec<String> },
    WorkspaceDetectedFact { path: Vec<String> },
}

pub enum McpRuntimeBindingTarget {
    HttpQuery { name: String },
    HttpHeader { name: String },
    StdioEnv { name: String },
    StdioCwd,
}

pub struct McpRuntimeBindingContext<'a> {
    pub vfs: Option<&'a Vfs>,
}

pub fn resolve_preset_mcp_server(
    preset: &McpPreset,
    context: Option<&McpRuntimeBindingContext<'_>>,
) -> Result<RuntimeMcpServer, McpRuntimeBindingError>;

pub struct CapabilityResolverInput<'a> {
    pub mcp_candidates: McpCandidates,
    pub mcp_runtime_context: Option<McpRuntimeBindingContext<'a>>,
    // other capability inputs omitted
}
```

Workspace mount metadata consumed by the resolver:

```json
{
  "workspace_id": "...",
  "workspace_identity_kind": "p4_workspace",
  "workspace_identity_payload": {},
  "workspace_binding_id": "...",
  "workspace_detected_facts": {
    "p4": {
      "client_name": "...",
      "server_address": "...",
      "stream": "...",
      "workspace_root": "...",
      "user_name": "..."
    }
  }
}
```

### 3. Contracts

- `McpPreset.runtime_binding` stores only the reusable binding declaration. It never stores resolved workspace values.
- The final VFS mount metadata is the canonical source for workspace/binding/detected facts. Runtime binding reads the mount selected by `mount_id.unwrap_or("main")`.
- `workspace_mount()` must write selected binding facts into metadata using `workspace_id`, `workspace_identity_payload`, `workspace_binding_id`, and `workspace_detected_facts`. The selected binding is the frame construction fact source because future workspace resolution changes should not require MCP resolver changes.
- `resolve_preset_mcp_server()` returns the runtime result: `RuntimeMcpServer { name: preset.key, transport: resolved_transport, uses_relay }`.
- `mcp_preset_keys` and `mcp:<preset>` must both resolve through `resolve_preset_mcp_server()` with the same `McpRuntimeBindingContext` after final VFS exists.
- Request/relay-provided already-resolved `RuntimeMcpServer` entries are runtime results and must not be re-resolved as presets.
- Projection normalization must keep `CapabilityState.tool.mcp_servers == FrameLaunchEnvelope.launch_surface.mcp_servers`.
- Duplicate MCP servers are de-duplicated by agent-facing server name after request MCP, capability MCP, and agent preset MCP are merged.
- HTTP/SSE query binding uses a URL parser and replaces existing same-name query values with the runtime fact.
- HTTP/SSE header binding replaces an existing same-name header case-insensitively; final reserved-header validation stays in the rmcp HTTP client layer.
- stdio env binding replaces an existing same-name env var; stdio cwd binding writes `McpTransportConfig::Stdio.cwd`.
- `route_policy.uses_relay(&resolved_transport)` runs after binding so the runtime decision observes the resolved transport.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Runtime binding exists but runtime binding context is missing | `McpRuntimeBindingError::MissingRuntimeBindingContext` with preset key |
| Runtime binding exists but `context.vfs` is missing | `McpRuntimeBindingError::MissingRuntimeBindingContext` with preset key |
| `mount_id` cannot be found in final VFS | `McpRuntimeBindingError::MissingMount` with preset key and mount id |
| Required source path is absent or empty | `McpRuntimeBindingError::MissingRequiredSource` with preset key, rule index, and source path |
| Optional source path is absent or empty | Skip that rule and keep the current transport field |
| Source path resolves to object or array | `McpRuntimeBindingError::InvalidSourceValue` |
| HTTP target is applied to stdio, or stdio target is applied to HTTP/SSE | `McpRuntimeBindingError::TransportMismatch` with target path and transport kind |
| HTTP query target name, header target name, or stdio env target name is blank | `McpRuntimeBindingError::InvalidTarget` |
| HTTP/SSE URL cannot be parsed while applying query binding | `McpRuntimeBindingError::InvalidTarget` |
| stdio cwd binding resolves to blank value | `McpRuntimeBindingError::InvalidTarget` |
| envelope projection sees MCP mismatch | Reject launch before connector prompt |

### 5. Good/Base/Bad Cases

- Good: `workspace.detected_facts.p4.client_name -> http_query.p4_client` resolves from final `main` mount and the direct/relay runtime receives the URL with that query value.
- Good: `workspace.detected_facts.p4.workspace_root -> stdio_cwd` resolves to a non-empty cwd and the local runtime spawns the stdio MCP process in that directory.
- Base: `runtime_binding = None` produces a static `RuntimeMcpServer` from preset transport and route policy.
- Base: An optional binding source is unavailable during frame construction; the remaining rules apply and the static value for that target is preserved.
- Boundary mismatch: resolving a runtime-bound preset before final VFS exists gives a declaration without the runtime fact surface.
- Canonical flow: keep preset declarations through owner bootstrap, build final VFS, create runtime MCP context, then resolve all preset-backed MCP servers and normalize capability projection.

### 6. Tests Required

- Domain serialization test covers `McpRuntimeBindingConfig`, source/target tagged unions, and `McpTransportConfig::Stdio.cwd`.
- VFS mount test asserts `workspace_mount()` metadata includes selected binding `workspace_detected_facts` and P4 fields.
- Runtime resolver tests assert HTTP query/header binding, stdio env/cwd binding, optional missing source skip, missing required source diagnostic, non-scalar source failure, transport mismatch, blank target name, invalid URL, and blank cwd.
- Capability resolver test asserts `mcp:<preset>` receives `CapabilityResolverInput.mcp_runtime_context` and resolves the same transport as the agent preset path.
- Session assembler test asserts `mcp_preset_keys` and `mcp:<preset>` for the same preset produce identical resolved `RuntimeMcpServer`.
- Frame construction validation test asserts `CapabilityState.tool.mcp_servers == FrameLaunchEnvelope.launch_surface.mcp_servers`.
- Direct/relay/local integration tests assert resolved query/header/env/cwd values are the values consumed by runtime clients, not the preset declaration.

### 7. Non-canonical / Canonical

#### Non-canonical

```text
McpPreset(runtime_binding + static transport)
  -> static RuntimeMcpServer before final VFS
  -> later layers infer workspace facts again
```

#### Canonical

```text
McpPreset(runtime_binding + static transport)
  + McpRuntimeBindingContext(final VFS main mount)
  -> resolve_preset_mcp_server(...)
  -> RuntimeMcpServer(resolved transport)
  -> CapabilityState.tool.mcp_servers == FrameLaunchEnvelope.launch_surface.mcp_servers
```

## LaunchPlan And Stage Contracts

`LaunchPlanner::plan` 返回 `LaunchPlan`。planner 输入由 `LaunchPlanningDeps`、`LaunchCommand`、`FrameLaunchEnvelope` 与 runtime facts 组成。

`LaunchPlan` 承载或引用：

- resolved prompt payload
- `FrameLaunchEnvelope`
- lifecycle / restore / hook / follow-up plan
- pending runtime command apply plan
- terminal effect plan
- connector input projection
- launch trace

Connector input 的 working directory、executor config、MCP、VFS、identity、capability state 和 context frame 都从 `FrameLaunchEnvelope.launch_surface` / `FrameRuntimeSurface` 与 `LaunchPlan` 投影生成。launch stages 执行计划时沿用 frame surface handoff，保持 context、VFS、MCP 与 capability 的单一来源。

`PreparedTurn` 汇总 connector accepted 前的 turn runtime projection、tools、context frames、hook runtime handle 与 connector-facing `ExecutionContext`。

Runtime tools 与 MCP tools 通过 `session::tool_assembly::assemble_tool_surface_for_execution_context`
统一装配。该 surface 同时产出 `DynAgentTool` 执行表与 `RuntimeToolSchemaEntry` PromptText
schema 表：前者写入 `ExecutionTurnFrame.assembled_tools` 并交给 connector，后者留在 application
层生成 runtime ContextFrame 的 `tool_schema_delta`。launch preparation 和 hub runtime capability
refresh 都必须委托该 helper，原因是 connector prompt 前工具面、运行中 capability refresh、MCP
discovery metadata 与 Agent 可见 ToolSchema 文本必须观察同一份 `ExecutionContext` /
`CapabilityState` / MCP discovery 语义；hub 不得从先前 runtime profile 或 active turn cache 回填缺失
VFS/MCP/capability facts。

`connector.prompt` 返回 `ExecutionStream` 是 launch accepted 边界。accepted 之前允许做 turn claim、active runtime projection、hook `SessionStart` context preparation 和 connector context assembly；accepted 之后才提交 user message、`TurnStarted`、context/capability projection event、bootstrap meta、runtime command `applied` 与本地 title derivation。connector setup 失败时释放 turn runtime 并记录失败终态。

`TurnCommitter::commit` 消费 `ConnectorAcceptedTurn`，原因是 accepted 后事实只有在 connector 已接收本轮 prompt 后才有业务意义。`StreamIngestionAttacher::attach` 消费 `CommittedTurn`，原因是 processor/adapter supervision 依赖 accepted 后事实已经落库。

LaunchPlanner 处理 runtime-only planning：

- resolved prompt payload
- lifecycle / restore / hook / follow-up
- requested runtime command apply plan
- terminal effect plan
- connector input projection

## Pending Runtime Commands

Runtime context / capability transition 的控制面事实源是 `AgentFrameTransitionRecord` / `agent_frame_transitions`。`SessionRuntimeCommandStore` 只承担 runtime delivery outbox：它用 runtime session 作为投递目标，并引用 frame transition fact。Projection 只服务查询、apply-once 与失败恢复，不保存 AgentRun 当前 surface truth。

状态流：

```text
requested -> applied
requested -> failed
```

connector.prompt accepted 后再标记 applied；connector.prompt 失败时保留 requested/failed 事实供下一轮恢复。

Payload contract:

- persisted delivery payload type: `RuntimeDeliveryCommand`
- delivery payload 持有 `frame_transition_id` 与 `target_frame_id`
- frame transition fact 持有 `RuntimeCapabilityTransition { declarations, effects }`
- replay entry: `RuntimeCommandRecord::pending_capability_state_transition()` -> `replay_runtime_capability_transitions(base_state, transitions) -> RuntimeCapabilityReplay`
- frame transition 语义是 intent，不是 full `CapabilityState` projection
- 写入 delivery outbox 前必须通过 `CapabilityDimensionRegistry::validate_transition`，并校验 delivery target 与 frame transition target 一致

## Plain RuntimeSession Dispatch

普通 Agent 会话进入 `LifecycleRun(topology=plain)` 过程归属模型，避免 RuntimeSession 与 lifecycle 控制面形成两套事实源。

Contract:

- 产品入口提交 plain `ExecutionIntent(subject_ref=Project, agent_policy=create/reuse, runtime_policy=create_runtime_session)`，由 dispatch 创建或复用 `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`RuntimeSession` 和 `RuntimeSessionExecutionAnchor`。保留的 RuntimeSession create route 只作为内部诊断/测试入口，并必须先校验调用者对 `project_id` 有 `Use` 权限。
- Project 归属写入 `LifecycleSubjectAssociation`，runtime trace/delivery refs 写入 `RuntimeSessionExecutionAnchor`，原因是 RuntimeSession 只承载 trace 和 delivery refs，业务控制面反查需要稳定索引。
- 显式 workflow launch 会创建或复用 `LifecycleRun.orchestrations[]` 中的 `OrchestrationInstance`，并把 runtime session trace anchor 绑定到 `orchestration_id + node_path + attempt`。

## Ready Gate

云端 `AppState::new_with_plugins` 返回前必须完成 session 主链路依赖绑定：

- runtime tool provider
- MCP relay provider
- terminal callback
- session launch envelope provider
- context audit bus

Ready gate 的职责是保证运行期看到完整依赖图。
