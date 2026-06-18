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

## Stage Responsibilities

| 阶段 | 输入 | 输出 | 职责 |
| --- | --- | --- | --- |
| Source adapter | HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 请求 | `LaunchCommand` | 保留来源身份、请求意图、source policy、prompt payload、executor override、follow-up hint |
| Frame construction | `LaunchCommand` + runtime session anchor + lifecycle/domain/runtime facts | `FrameLaunchEnvelope` | 解析 `RuntimeSessionExecutionAnchor`、current/pending `AgentFrame`、working dir、VFS、MCP、capability、context bundle/frame、identity、query/audit/inspector projection、resolution trace |
| Launch planning | `LaunchCommand` + `FrameLaunchEnvelope` + runtime facts | `LaunchPlan` | 解析 resolved prompt payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input |
| Turn preparation | `LaunchPlan` | `PreparedTurn` | claim/activate turn，准备 runtime tools、MCP tools、hook runtime、context frames、pending runtime context application 与 connector `ExecutionContext` |
| Connector start | `PreparedTurn` | `ConnectorAcceptedTurn` | 调用 `connector.prompt`，以返回 `ExecutionStream` 作为 accepted 边界；setup 失败时释放 turn/hook 并记录 failed terminal |
| Accepted commit | `ConnectorAcceptedTurn` | `CommittedTurn` | 提交 user message、`TurnStarted`、context/capability projection event、bootstrap meta、runtime command `applied` 与本地 title derivation |
| Stream ingestion | `CommittedTurn` | `AttachedTurn` | spawn `SessionTurnProcessor` 与 stream adapter，并登记 processor tx / adapter abort handle |
| Terminal | connector terminal / stream terminal | terminal event + outbox effect | 持久化终态，清理 active turn，把业务副作用写入 durable outbox |

`Turn` 边界保持很薄：reservation、active、cancel、hook runtime handle、processor/adapter supervision、terminal release。

## Source Adapter Contract

Source adapter 只做来源语义转换，不能预先组装最终运行事实。

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

`FrameConstructionService` 通过 `RuntimeSessionExecutionAnchor` 反查 `LifecycleRun` / `LifecycleAgent` / current `AgentFrame`，再按 companion、lifecycle node、ProjectAgent 或 existing frame surface 路径生成 envelope。ProjectAgent / owner bootstrap 路径由 `workflow::frame_construction::owner_bootstrap` 组合 owner surface，原因是该路径产出写入 `AgentFrame` 的 VFS、MCP、capability、context bundle 与 execution profile。业务模块不得绕过该服务自行组装 connector facts。

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
- `CapabilityState.tool.mcp_servers` 是 capability/draft projection，用于 runtime command replay、tool policy 关联和工具装配快照；AgentRun 当前可执行 MCP surface 的事实源仍是 `AgentFrame.mcp_surface_json` / `FrameSurfaceDraft.mcp_servers`。
- pending runtime command 的 overlay 由 frame construction 形成 final capability projection；`requested -> applied` 副作用只能在 connector prompt accepted 后提交。

Frame construction 可以消费 runtime facts，但这些 facts 一旦进入 `FrameLaunchEnvelope` 就必须体现在 `resolution_trace` 中。LaunchPlanner 不允许再读取 cached profile、hub default VFS、local relay workspace root 或 source MCP server 来补齐 VFS/MCP/capability/executor facts。

Context endpoint、权限展示、audit 和 inspector 都从 `AgentFrame` / `FrameLaunchEnvelope` 同源投影。API route 的职责是 auth/permission、DTO 转换、调用 use case、映射 response DTO。

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
- `runtime_surface` 是 query DTO，只从 `FrameLaunchEnvelope.launch_surface.vfs` / `AgentFrame.vfs_surface_json` 生成。
- `AgentFrame` 的 VFS / MCP / capability surface 通过 `AgentFrameBuilder::with_surface_draft` 集中写入，原因是 launch 装配面、query DTO 面和 capability replay 必须跟随 effective capability VFS 保持一致。
- `FrameSurfaceDraft` 是 construction 到 `AgentFrameBuilder` / `FrameLaunchEnvelope` 的显式交接结构。`FrameLaunchSurface` 是从该 draft 校验得到的 launch-ready typed surface，原因是 construction validation、launch planning、connector projection 和 query surface 必须观察同一份 typed handoff，且 planner 不应读取 optional draft 字段。

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

Runtime tools 与 MCP tools 通过 `session::tool_assembly::assemble_tools_for_execution_context` 统一装配。launch preparation 和 hub runtime capability refresh 都必须委托该 helper，原因是 connector prompt 前工具面与运行中 capability refresh 必须观察同一份 `ExecutionContext` / `CapabilityState` / MCP discovery 语义；hub 不得从旧 runtime profile 或 active turn cache 回填缺失 VFS/MCP/capability facts。

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

- `POST /sessions` 创建 project-scoped 业务会话，必须先校验调用者对 `project_id` 有 `Edit` 权限。
- 新入口提交 plain `ExecutionIntent(subject_ref=Project, agent_policy=create/reuse, runtime_policy=create_runtime_session)`，由 dispatch 创建或复用 `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`RuntimeSession` 和 `RuntimeSessionExecutionAnchor`。
- Project 归属写入 `LifecycleSubjectAssociation`，runtime trace/delivery refs 写入 `RuntimeSessionExecutionAnchor`，原因是 Session shell 只承载消息流，业务控制面反查需要稳定索引。
- 显式 workflow launch 会创建或复用 `LifecycleRun.orchestrations[]` 中的 `OrchestrationInstance`，并把 runtime session trace anchor 绑定到 `orchestration_id + node_path + attempt`。

## Ready Gate

云端 `AppState::new_with_plugins` 返回前必须完成 session 主链路依赖绑定：

- runtime tool provider
- MCP relay provider
- terminal callback
- session launch envelope provider
- context audit bus

Ready gate 的职责是保证运行期看到完整依赖图。
