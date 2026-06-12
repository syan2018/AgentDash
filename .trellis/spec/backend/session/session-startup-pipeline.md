# Session Startup Pipeline

本 appendix 定义当前 session 构建与 prompt launch 的生产主线。目标控制面中，当前 `Session` 语义降级为 `RuntimeSession`；本主线由 `AgentFrame -> FrameLaunchEnvelope -> ExecutionContext` 投影承接，业务入口不得继续把 session owner 当作控制面事实源。模块不变量见 [Session Architecture](./architecture.md)。

## Pipeline

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchPlan
  -> PreparedTurn
  -> ConnectorAcceptedTurn
  -> CommittedTurn
  -> AttachedTurn
```

`LaunchCommand` 表达来源意图；`SessionConstructionPlan` 是构建事实源；`LaunchPlan` 是单轮启动决策；后续 stage types 表达 accepted 前准备、connector accepted、accepted 后 commit 与 stream attach。`ExecutionContext` 只在 connector 边界投影。

## Stage Responsibilities

| 阶段 | 输入 | 输出 | 职责 |
| --- | --- | --- | --- |
| Source adapter | HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 请求 | `LaunchCommand` | 保留来源身份、请求意图、source policy、prompt payload、executor override、follow-up hint |
| Construction | `LaunchCommand` + session/domain/runtime facts | `SessionConstructionPlan` | 解析 owner、workspace、working dir、VFS、MCP、capability、context bundle/frame、identity、query/audit/inspector projection、resolution trace |
| Launch planning | `LaunchCommand` + `SessionConstructionPlan` + runtime facts | `LaunchPlan` | 解析 resolved prompt payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input |
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
| Local relay | workspace root、原始 MCP declaration、relay source identity |

`working_dir` 是 construction 解析结果，不属于用户 prompt input。resolved VFS、resolved MCP、capability state、context bundle 和 connector input 都由 construction/launch 产出。

Task terminal effect 使用 durable lifecycle refs 描述，由 construction/effects 解析。command 边界不传内存 `post_turn_handler` 或其它 trait object。

Routine source metadata 只表达触发来源与当前 execution facts。`routine` VFS mount、Routine memory skill projection 和最终 capability state 在 Construction 阶段形成，原因是 Routine 的跨轮次上下文必须与其它 session owner/runtime facts 一起进入同一份 final VFS projection。

## Construction Contract

`SessionConstructionProvider::build_construction` 直接输出 launch-ready `SessionConstructionPlan`，不是 seed、partial plan 或等待 planner 补齐的中间形态。

目标迁移中，`SessionConstructionPlan` 会降为 `AgentFrameBuilder` 内部的 `AgentFrameConstructionPlan`。业务模块不得把它作为 command input 或 public contract；它只服务 `AgentFrame` revision 与 `FrameLaunchEnvelope` 生成。

`SessionConstructionPlan` 至少覆盖：

- `ResolvedSessionOwner`，owner 解析顺序统一为 `Task -> Story -> Project`。
- workspace 与 typed working directory。`workspace.working_directory` 必须在进入 launch planner 前为 `Some`。
- final VFS、MCP declaration resolution、capability state。
- `SessionContextBundle` 与 continuation/context frames。
- identity、source contract、query/audit/inspector projections。
- resolution trace，用于审计为什么选择某个 owner/workspace/context。

Launch 前必须调用 `SessionConstructionPlan::validate_for_launch()` 或等价 gate：

- 缺少 `workspace.working_directory`、`execution_profile.executor_config`、`surface.vfs`、`projections.capability_state` 时拒绝 launch。
- `projections.capability_state.vfs.active` 必须等于 `surface.vfs`。
- `projections.capability_state.tool.mcp_servers` 必须等于 `projections.mcp_servers`。
- pending runtime command 的 overlay 由 Construction 阶段形成 final capability projection；`requested -> applied` 副作用只能在 connector prompt accepted 后提交。

Construction 可以消费 runtime facts，但这些 facts 一旦进入 `SessionConstructionPlan` 就必须体现在 `resolution` trace 中。LaunchPlanner 不允许再读取 cached profile、hub default VFS、local relay workspace root 或 source MCP declaration 来补齐 VFS/MCP/capability/executor facts。

Context endpoint、权限展示、audit 和 inspector 都投影同一份 `SessionConstructionPlan`。API route 的职责是 auth/permission、DTO 转换、调用 use case、映射 response DTO。

## Capability Projection Normalization

Session runtime surface、VFS、MCP、Skill baseline 与 `CapabilityState` 是同一份 construction projection 的不同维度。

Core entries:

- `derive_session_capability_projection(SessionCapabilityProjectionInput) -> SessionCapabilityProjection`
- `normalize_capability_state_dimensions(&mut CapabilityState, Option<Vfs>, Vec<SessionMcpServer>, &SessionBaselineCapabilities)`
- `build_session_context_plan(...) -> SessionConstructionPlan`

Contract:

- `CapabilityResolver` 只解析 tool / MCP / companion 维度。
- Effective VFS 由 construction finalize 合并 owner/session/runtime-command facts 后确定。
- Skill baseline 与 guidelines 从 effective VFS 派生。
- `CapabilityState.vfs.active` 必须等于 final `plan.surface.vfs`。
- `CapabilityState.tool.mcp_servers` 必须等于 final `plan.projections.mcp_servers`。
- `runtime_surface` 是 query DTO，只从 final `plan.surface.vfs` 生成。
- `SessionConstructionPlan` 的 VFS 投影写入通过 plan helper 集中同步，原因是 `surface.vfs` 服务 launch 装配面，`context_projection.vfs` 服务 query DTO 面，而二者必须跟随 effective capability VFS 保持一致。

## Scenario: MCP Runtime Binding During Construction

### 1. Scope / Trigger

- Trigger: MCP Preset 可以声明运行时绑定；construction、capability resolver、direct/relay/local MCP runtime 都必须消费同一份已解析 MCP server declaration。
- Scope: Project `McpPreset.runtime_binding`、final VFS `main` mount metadata、`mcp_preset_keys`、`mcp:<preset>` capability directive、`SessionConstructionPlan.projections.mcp_servers` 与 `CapabilityState.tool.mcp_servers`。

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

pub struct SessionRuntimeMcpContext<'a> {
    pub vfs: Option<&'a Vfs>,
}

pub fn resolve_preset_mcp_server(
    preset: &McpPreset,
    context: Option<&SessionRuntimeMcpContext<'_>>,
) -> Result<SessionMcpServer, McpRuntimeBindingError>;

pub struct CapabilityResolverInput<'a> {
    pub mcp_candidates: McpCandidates,
    pub mcp_runtime_context: Option<SessionRuntimeMcpContext<'a>>,
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
- `workspace_mount()` must write selected binding facts into metadata using `workspace_id`, `workspace_identity_payload`, `workspace_binding_id`, and `workspace_detected_facts`. The selected binding is the construction fact source because future workspace resolution changes should not require MCP resolver changes.
- `resolve_preset_mcp_server()` returns the runtime result: `SessionMcpServer { name: preset.key, transport: resolved_transport, uses_relay }`.
- `mcp_preset_keys` and `mcp:<preset>` must both resolve through `resolve_preset_mcp_server()` with the same `SessionRuntimeMcpContext` after final VFS exists.
- Request/relay-provided already-resolved `SessionMcpServer` entries are runtime results and must not be re-resolved as presets.
- Projection normalization must keep `CapabilityState.tool.mcp_servers == SessionConstructionPlan.projections.mcp_servers`.
- Duplicate MCP servers are de-duplicated by agent-facing server name after request MCP, capability MCP, and agent preset MCP are merged.
- HTTP/SSE query binding uses a URL parser and replaces existing same-name query values with the runtime fact.
- HTTP/SSE header binding replaces an existing same-name header case-insensitively; final reserved-header validation stays in the rmcp HTTP client layer.
- stdio env binding replaces an existing same-name env var; stdio cwd binding writes `McpTransportConfig::Stdio.cwd`.
- `route_policy.uses_relay(&resolved_transport)` runs after binding so the runtime decision observes the resolved transport.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Runtime binding exists but runtime context is missing | `McpRuntimeBindingError::MissingSessionContext` with preset key |
| Runtime binding exists but `context.vfs` is missing | `McpRuntimeBindingError::MissingSessionContext` with preset key |
| `mount_id` cannot be found in final VFS | `McpRuntimeBindingError::MissingMount` with preset key and mount id |
| Required source path is absent or empty | `McpRuntimeBindingError::MissingRequiredSource` with preset key, rule index, and source path |
| Optional source path is absent or empty | Skip that rule and keep the current transport field |
| Source path resolves to object or array | `McpRuntimeBindingError::InvalidSourceValue` |
| HTTP target is applied to stdio, or stdio target is applied to HTTP/SSE | `McpRuntimeBindingError::TransportMismatch` with target path and transport kind |
| HTTP query target name, header target name, or stdio env target name is blank | `McpRuntimeBindingError::InvalidTarget` |
| HTTP/SSE URL cannot be parsed while applying query binding | `McpRuntimeBindingError::InvalidTarget` |
| stdio cwd binding resolves to blank value | `McpRuntimeBindingError::InvalidTarget` |
| `SessionConstructionPlan::validate_for_launch()` sees MCP projection mismatch | Reject launch before connector prompt |

### 5. Good/Base/Bad Cases

- Good: `workspace.detected_facts.p4.client_name -> http_query.p4_client` resolves from final `main` mount and the direct/relay runtime receives the URL with that query value.
- Good: `workspace.detected_facts.p4.workspace_root -> stdio_cwd` resolves to a non-empty cwd and the local runtime spawns the stdio MCP process in that directory.
- Base: `runtime_binding = None` produces a static `SessionMcpServer` from preset transport and route policy.
- Base: An optional binding source is unavailable during construction; the remaining rules apply and the static value for that target is preserved.
- Boundary mismatch: resolving a runtime-bound preset before final VFS exists gives a declaration without the runtime fact surface.
- Canonical flow: keep preset declarations through owner bootstrap, build final VFS, create runtime MCP context, then resolve all preset-backed MCP servers and normalize capability projection.

### 6. Tests Required

- Domain serialization test covers `McpRuntimeBindingConfig`, source/target tagged unions, and `McpTransportConfig::Stdio.cwd`.
- VFS mount test asserts `workspace_mount()` metadata includes selected binding `workspace_detected_facts` and P4 fields.
- Runtime resolver tests assert HTTP query/header binding, stdio env/cwd binding, optional missing source skip, missing required source diagnostic, non-scalar source failure, transport mismatch, blank target name, invalid URL, and blank cwd.
- Capability resolver test asserts `mcp:<preset>` receives `CapabilityResolverInput.mcp_runtime_context` and resolves the same transport as the agent preset path.
- Session assembler test asserts `mcp_preset_keys` and `mcp:<preset>` for the same preset produce identical resolved `SessionMcpServer`.
- Construction validation test asserts `CapabilityState.tool.mcp_servers == plan.projections.mcp_servers`.
- Direct/relay/local integration tests assert resolved query/header/env/cwd values are the values consumed by runtime clients, not the preset declaration.

### 7. Non-canonical / Canonical

#### Non-canonical

```text
McpPreset(runtime_binding + static transport)
  -> static SessionMcpServer before final VFS
  -> later layers infer workspace facts again
```

#### Canonical

```text
McpPreset(runtime_binding + static transport)
  + SessionRuntimeMcpContext(final VFS main mount)
  -> resolve_preset_mcp_server(...)
  -> SessionMcpServer(resolved transport)
  -> CapabilityState.tool.mcp_servers == plan.projections.mcp_servers
```

## LaunchPlan And Stage Contracts

`LaunchPlanner::plan` 返回 `LaunchPlan`。planner 输入由 `LaunchPlanningDeps`、`LaunchCommand`、`SessionConstructionPlan` 与 runtime facts 组成。

`LaunchPlan` 承载或引用：

- resolved prompt payload
- `SessionConstructionPlan`
- lifecycle / restore / hook / follow-up plan
- pending runtime command apply plan
- terminal effect plan
- connector input projection
- launch trace

Connector input 的 working directory、executor config、MCP、VFS、identity、capability state 和 context frame 都从 final construction 与 `LaunchPlan` 投影生成。launch stages 执行计划时沿用 construction 事实，保持 owner、context、VFS、MCP 与 capability 的单一来源。

`PreparedTurn` 汇总 connector accepted 前的 turn runtime projection、tools、context frames、hook runtime handle 与 connector-facing `ExecutionContext`。

`connector.prompt` 返回 `ExecutionStream` 是 launch accepted 边界。accepted 之前允许做 turn claim、active runtime projection、hook `SessionStart` context preparation 和 connector context assembly；accepted 之后才提交 user message、`TurnStarted`、context/capability projection event、bootstrap meta、runtime command `applied` 与本地 title derivation。connector setup 失败时释放 turn runtime 并记录失败终态。

`TurnCommitter::commit` 消费 `ConnectorAcceptedTurn`，原因是 accepted 后事实只有在 connector 已接收本轮 prompt 后才有业务意义。`StreamIngestionAttacher::attach` 消费 `CommittedTurn`，原因是 processor/adapter supervision 依赖 accepted 后事实已经落库。

LaunchPlanner 处理 runtime-only planning：

- resolved prompt payload
- lifecycle / restore / hook / follow-up
- requested runtime command apply plan
- terminal effect plan
- connector input projection

## Pending Runtime Commands

Runtime context / capability transition 的控制面事实源是 `AgentFrameTransitionRecord` / `agent_frame_transitions`。`SessionRuntimeCommandStore` 只承担 runtime delivery outbox：它用 runtime session 作为投递目标，并引用 frame transition fact。Projection 只服务查询、apply-once 与失败恢复。

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

## Graphless RuntimeSession Dispatch

普通 Agent 会话进入 `LifecycleRun(topology=graphless)` 过程归属模型，避免 RuntimeSession 与 lifecycle 控制面形成两套事实源。

Contract:

- `POST /sessions` 创建 project-scoped 业务会话，必须先校验调用者对 `project_id` 有 `Edit` 权限。
- 新入口提交 graphless `ExecutionIntent(subject_ref=Project, agent_policy=create/reuse, runtime_policy=create_runtime_session)`，由 dispatch 创建或复用 `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`RuntimeSession` 和 `RuntimeSessionExecutionAnchor`。
- Project 归属写入 `LifecycleSubjectAssociation`，runtime trace/delivery refs 写入 `RuntimeSessionExecutionAnchor`，原因是 Session shell 只承载消息流，业务控制面反查需要稳定索引。
- 显式 workflow launch 会创建或复用 `LifecycleRun.orchestrations[]` 中的 `OrchestrationInstance`，并把 runtime session trace anchor 绑定到 `orchestration_id + node_path + attempt`。

## Ready Gate

云端 `AppState::new_with_plugins` 返回前必须完成 session 主链路依赖绑定：

- runtime tool provider
- MCP relay provider
- terminal callback
- session construction provider
- context audit bus

Ready gate 的职责是保证运行期看到完整依赖图。
