# 清理 session hub 错误归属路径设计草案

## Design Goal

`session/hub` 保留为进程内 Session live runtime coordination 边界；所有需要回答“某个 runtime session 当前 AgentRun/Lifecycle runtime surface 是什么”的路径迁出到 AgentRun/Lifecycle control-plane。

这次重构以 `mcp.list_tools` 的 Canvas idle 调用缺少 runtime backend anchor 为触发点，但设计目标覆盖更宽的错误归属链路：

```text
RuntimeGateway MCP action
  -> SessionCapabilityService
  -> SessionRuntimeInner(session/hub)
  -> active turn cache 或手写 idle AgentFrame fallback
```

目标链路：

```text
RuntimeGateway MCP action
  -> RuntimeSessionMcpAccess implementation
  -> AgentRunRuntimeSurfaceQuery
  -> RuntimeSessionExecutionAnchor
  -> AgentRun/Lifecycle runtime address
  -> closed current runtime surface
```

## Research Synthesis

四份研究输入共同确认：

- 现有代码没有一个统一的 AgentRun/Lifecycle 粒度 closed runtime surface query。已有的 `resolve_current_frame_from_delivery_trace_ref` 只解决 `runtime_session_id -> anchor -> agent -> current AgentFrame` 的内部查找；`resolve_session_frame_vfs` 只返回 VFS/frame，并把 backend anchor 继续交给 active-turn-only helper；hub MCP discovery idle 分支只拼了 MCP/VFS/capability，缺 backend anchor/identity。
- RuntimeGateway MCP provider 本身边界是干净的：provider 只做 action input/output 与 actor/context admission，错误归属在 `RuntimeSessionMcpAccess` 当前由 `SessionCapabilityService` 实现并委托 hub。
- Prompt tool assembly 与 active turn tool refresh 是合法 live runtime coordination：它们消费已闭合的 `ExecutionContext`，并更新 connector tools / active turn cache / hook runtime。它们不应被当成 current surface query 来迁走。
- API current surface consumers 已经超过 Canvas：Extension runtime、VFS surface `SessionRuntime` source、Terminal spawn 都通过 `resolve_session_frame_vfs` 或同类 backend anchor helper 间接依赖旧路径。
- Surface-changing mutation 与 query 必须分离：Canvas create/present、WorkspaceModule Canvas exposure、Permission grant adoption 是 update/use case；Canvas snapshot、Extension target、VFS session source、Terminal target、MCP list/call 是 current surface query。

因此本任务的第一阶段不是“改 hub 的 idle 分支补 anchor”，而是建立 AgentRun/Lifecycle 粒度的 current runtime surface query facade，再迁移所有 query consumer；第二阶段再把 Canvas/Permission/WorkspaceModule 的 update/adoption 旧入口赶出 `SessionCapabilityService`。

## Boundary Model

### 0. AgentFrame Exposure Boundary

当前 `AgentFrame` 的实际暴露面偏大，原因不是 `AgentFrame` 本身抽象错误，而是 application 仍是单 crate，`agentdash_domain::workflow::{AgentFrame, AgentFrameRepository}` 与 `agent_run::frame::*` 很容易被横向模块直接 import。

本任务采用模块边界先行的收束方式，即使暂不拆 crate，也按以下规则设计端口：

| Area | 是否可直接持有 `AgentFrame` | 理由 |
| --- | --- | --- |
| `agent_run::frame::*` construction / surface / runtime_launch | Yes | 这里是 frame revision 的构造、typed 读取、launch closure 内聚实现。 |
| AgentRun/Lifecycle runtime surface query facade | Internal only | facade 内部可读 current frame revision，但对外返回 run/runtime-address 粒度的 closed surface。 |
| Runtime surface update use case | Internal only | Canvas/Permission/WorkspaceModule 等 surface-changing effect 可通过 update service 写 frame revision，但业务模块不直接写。 |
| session launch / accepted commit / hook runtime binding | Limited internal | 这些路径负责把已闭合 frame surface 投递到 connector 或同步 hook target，属于 launch/coordination 内部实现。 |
| RuntimeGateway providers | No | Provider 只处理 action input/output 与 actor/context admission，通过 port 消费 closed surface。 |
| API current-surface consumers | No | Canvas snapshot、Extension runtime、VFS surface、Terminal target 应消费 query facade，不返回或缓存 `AgentFrame`。 |
| `session/hub` idle query / MCP discovery | No | Hub 不回答 current AgentRun surface，也不读 idle frame revision 拼 surface。 |
| Presentation read models | Transitional | Lifecycle/session view 可读 frame 形成 UI projection，但不得复用为 runtime action/query 路径；后续可迁到专门 read-model facade。 |

这条边界相当于在单 crate 内先建立 “准 crate boundary”：外层模块只依赖 query/update port，不能因为 Rust 可见性方便就把 `AgentFrameRepository` 当全局事实源。

### 1. Session Hub

Hub 只处理 live runtime coordination：

- `SessionRuntimeRegistry` / active turn state。
- connector live session 与 active turn transition。
- active turn 内 connector tool refresh。
- hook runtime delivery binding cache。
- session launch / stream ingestion / turn supervision 中必须依赖进程内 runtime 的协调逻辑。
- pending runtime command 在 turn 边界的应用协调。

Hub 不作为以下问题的事实源：

- runtime session 当前 AgentRun/Lifecycle runtime surface revision 是哪个。
- current runtime surface 的 VFS / MCP / capability / backend anchor 是什么。
- Canvas、Permission、WorkspaceModule 是否应写 AgentFrame revision。
- RuntimeGateway `mcp.*` action 应暴露哪些 MCP tools。
- API route 如何解析 session VFS / backend target。

### 2. AgentRun Runtime Surface Query

新增 application 层 query facade，命名：

```rust
agent_run::runtime_surface::AgentRunRuntimeSurfaceQuery
```

这个 facade 的对外语义以 AgentRun / Lifecycle runtime address 为粒度，不向 RuntimeGateway、API route 或 Canvas/Extension/VFS consumer 暴露 `AgentFrame` 对象。`AgentFrame` 是 facade 内部的 revision storage 与审计 provenance，不是跨模块 API。

候选对外结构：

```rust
pub struct AgentRunRuntimeSurface {
    pub runtime_session_id: String,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_address: AgentRunRuntimeAddress,
    pub surface_revision: RuntimeSurfaceRevision,
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
    pub active_turn_id: Option<String>,
}
```

Query 输入只需要 delivery/runtime identity 和可选用途 label：

```rust
pub async fn current_runtime_surface(
    &self,
    runtime_session_id: &str,
    purpose: RuntimeSurfaceQueryPurpose,
) -> Result<AgentRunRuntimeSurface, RuntimeSurfaceQueryError>
```

`purpose` 用于诊断和未来 admission，不让 caller 自行拼 `component` 字符串。

#### Surface Closure

Facade 内部可以从 current `AgentFrame` revision 读取 typed surface 后闭合，但这个细节不出现在 RuntimeGateway/API consumer contract 中：

```text
AgentFrame typed surface
  -> capability_state
  -> VFS
  -> MCP servers
  -> runtime backend anchor from VFS default mount
```

实现选择：

- 提取共享的 VFS -> `RuntimeBackendAnchor` helper，例如 `runtime_backend_anchor_from_vfs(...)`，让 launch closure 与 current surface query 使用同一派生规则。
- query facade 内部可定义 `ClosedRuntimeSurface::from_revision(...)` 或等价私有结构闭合 capability/VFS/MCP/backend；但 query path 不直接暴露 launch-only 语义或 `AgentFrame` 给 consumer。
- 如果内部 current frame revision 缺少 closure 字段，应返回 typed query error，而不是落到 hub active-turn-only fallback。

### 3. RuntimeGateway MCP Access

`RuntimeSessionMcpAccess` 的实现迁出 `SessionCapabilityService`。

候选新类型：

```rust
pub struct CurrentSurfaceRuntimeMcpAccess {
    surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    mcp_tool_discovery: Arc<dyn McpToolDiscovery>,
}
```

`list_mcp_tools(session_id)`：

```text
surface_query.current_runtime_surface(session_id, McpToolDiscovery)
  -> capability/admission projection
  -> McpToolDiscoveryRequest {
       servers: surface.mcp_servers,
       capability_state: surface.capability_state,
       call_context: RelayMcpCallContext {
         session_id,
         turn_id: surface.active_turn_id,
         backend_anchor: Some(surface.runtime_backend_anchor),
         vfs: Some(surface.vfs),
         identity: ...
       }
     }
  -> RuntimeMcpToolDescriptor[]
```

`call_mcp_tool(session_id, input)` 复用同一 tool entries，并调用 `execute_runtime_mcp_tool`。

RuntimeGateway 的依赖边界：

- RuntimeGateway 不依赖 `AgentFrameRepository`、`LifecycleAgentRepository` 或 `AgentFrame` 类型。
- RuntimeGateway 只依赖 `RuntimeSessionMcpAccess`，该 access 再依赖一个窄 `AgentRunRuntimeSurfaceQueryPort`。
- active turn 时，runtime action query 以 AgentRun/Lifecycle current committed surface 为事实源；active turn transient 只补充 `active_turn_id`、可能的 in-memory identity/hook context。connector live refresh 仍可使用 hub active turn snapshot。

### Scope Boundary

本任务实施范围硬收以下路径：

- RuntimeGateway MCP session action backing access。
- API current surface consumers：Canvas snapshot、Extension runtime、VFS `SessionRuntime`、Terminal launch target。
- `session/hub` idle query / MCP discovery / active-turn-only backend anchor 对外暴露。
- Canvas、WorkspaceModule、Permission 的 surface-changing business update/adoption 旧路径。

以下路径作为 transitional，不阻塞本任务：

- Lifecycle/session presentation read-model 中的 `AgentFrameRefDto` / `AgentFrameRuntimeView`。
- Test-only in-memory frame repository 与 fixture。
- Frame construction / launch / hook runtime 内部持有 `AgentFrame` 的实现细节。

### 4. Business Consumers

所有 current runtime surface consumer 应共用 AgentRun/Lifecycle runtime surface query：

| Consumer | 当前路径 | 目标路径 |
| --- | --- | --- |
| Canvas runtime invoke | `RuntimeGateway.invoke` -> MCP access -> hub | RuntimeGateway -> new MCP access -> AgentRun runtime surface query |
| Canvas runtime snapshot VFS | `resolve_session_frame_vfs` | AgentRun runtime surface query / resource context |
| Extension runtime action/channel | `resolve_session_frame_vfs` + active-turn anchor helper | AgentRun runtime surface query |
| VFS surface `SessionRuntime` source | `resolve_session_frame_vfs` | AgentRun runtime surface query |
| Terminal/session VFS launch target | `resolve_session_frame_vfs` + active-turn-only backend anchor helper | AgentRun runtime surface query where it is a query path |
| WorkspaceModule Canvas exposure | `SessionCapabilityService` update/adopt helpers | runtime surface update use case |
| Permission grant adoption | route/service direct adoption path | runtime surface update use case |

### 5. Update/Adoption Paths

Query facade 只回答 current surface，不负责写 AgentFrame revision。

会改变 surface 的路径进入 runtime surface update use case：

```text
business event
  -> typed update request
  -> AgentRun runtime surface update service
  -> AgentFrame revision write
  -> active runtime adoption primitive
```

本任务首要目标是清出 hub 错误归属和 MCP/query consumer。Permission/WorkspaceModule/Canvas update paths 可以分阶段实施，但必须在本任务 design 中完成迁移矩阵和边界定义。

## Migration Matrix Draft

| Old Path | Current Responsibility | Target Owner | Action |
| --- | --- | --- | --- |
| `SessionRuntimeInner::discover_runtime_mcp_tool_entries` | MCP action backing discovery + active/idle surface assembly | RuntimeGateway MCP access + AgentRun runtime surface query | Split: action backing migrates out; active live refresh helper stays if needed |
| `SessionCapabilityService impl RuntimeSessionMcpAccess` | RuntimeGateway MCP provider backing access | New `CurrentSurfaceRuntimeMcpAccess` over query port | Remove trait impl from capability service |
| `SessionRuntimeInner::get_current_runtime_backend_anchor` | active-turn-only anchor lookup | AgentRun runtime surface query | Keep only internal active-turn helper if needed; API consumers migrate |
| `resolve_session_frame_vfs` | API session -> frame -> VFS + active-turn anchor | API adapter over AgentRun runtime surface query | Replace or turn into thin adapter over query facade |
| API direct `AgentFrame` current-surface helpers | `session_construction.rs`, VFS/Extension/Terminal consumers expose frame-shaped result | AgentRun runtime surface query / read-model facade | Remove `AgentFrame` from runtime consumer result types |
| `resolve_current_frame_from_delivery_trace_ref` direct callers | Many modules use lifecycle helper as convenient current surface lookup | Query/update/read-model facades | Keep helper internal to lifecycle/AgentRun implementation; stop exposing it as general-purpose runtime query |
| Canvas `/runtime-invoke` | RuntimeGateway actor/context assembly | Canvas API route | Keep; fix downstream MCP access |
| Canvas `/runtime-snapshot` | VFS binding via session frame helper | Canvas runtime resource + runtime surface query | Migrate VFS/session context source |
| Extension runtime invoke/channel | backend target + workspace context | Extension API route + runtime surface query | Replace frame helper and active anchor helper |
| VFS surface resolver `SessionRuntime` | session VFS query | VFS API + runtime surface query | Replace frame helper |
| WorkspaceModule Canvas exposure helpers | business mutation + frame write/adopt | Runtime surface update use case | Migrate/update, then private/delete helper |
| Permission route adoption | grant state + active runtime adoption | Permission service + runtime surface update use case | Remove route-level adoption |
| Presentation lifecycle/session views | UI projection from current frame | Read-model facade | Keep separate from runtime surface query; no action/provider reuse |

Exact function names and additional rows are captured in the research files listed at the end of this design.

## Data Flow Details

### Canvas `mcp.list_tools`

Target flow:

```text
iframe window.agentdash.invoke("mcp.list_tools", {})
  -> parent CanvasRuntimePreview
  -> POST /canvases/{id}/runtime-invoke
  -> RuntimeInvocationRequest {
       actor: UserCanvas { session_id, canvas_id },
       context: Session { session_id, project_id }
     }
  -> RuntimeGateway.invoke
  -> McpListToolsProvider
  -> CurrentSurfaceRuntimeMcpAccess.list_mcp_tools(session_id)
  -> AgentRunRuntimeSurfaceQuery.current_runtime_surface(session_id)
  -> McpToolDiscoveryRequest with backend_anchor
```

Acceptance test should assert the request reaches discovery with a backend anchor in idle/non-active-turn setup.

### Active Turn Tool Refresh

Target flow remains separate:

```text
capability/runtime transition during active turn
  -> hub active turn cache
  -> assemble tool surface for ExecutionContext
  -> connector.update_session_tools
```

This path can stay in `session/hub` because it mutates live connector state.

### Query vs Mutation

Use this rule during migration:

- If caller asks “what is current VFS/MCP/backend/capability for this runtime session?” -> AgentRun runtime surface query.
- If caller says “Canvas/Permission/WorkspaceModule changed runtime surface” -> runtime surface update use case.
- If caller says “active connector needs live tools refreshed now” -> session hub.

## Test Plan

Minimum tests:

- RuntimeGateway MCP provider test using a fake AgentRun runtime surface query port with backend anchor.
- Application test for idle runtime session current surface resolution from `RuntimeSessionExecutionAnchor` and internal current `AgentFrame` revision, while the public result remains AgentRun/Lifecycle scoped.
- Static/import boundary check or focused code review checklist proving RuntimeGateway and API current-surface consumers no longer import `AgentFrame`, `AgentFrameSurfaceExt`, or `resolve_current_frame_from_delivery_trace_ref` directly.
- Canvas runtime invoke test or route-level test for `mcp.list_tools` without active turn.
- Regression test that `runtime_mcp_tool_discovery` receives non-empty backend anchor in idle branch.
- Extension runtime target selection test migrated from `resolve_session_frame_vfs` helper to AgentRun runtime surface query.
- Grep/static check or focused unit test ensuring API consumers no longer call active-turn-only `get_current_runtime_backend_anchor`.

Potential validation commands:

```powershell
cargo test -p agentdash-application runtime_gateway::session_actions
cargo test -p agentdash-application session::hub
cargo test -p agentdash-api canvases
cargo test -p agentdash-api extension_runtime
cargo check -p agentdash-api
```

Exact commands should be finalized after research.

## Risks

- Active turn snapshot may contain in-flight pending runtime commands not yet reflected in persisted surface revision. Design must explicitly decide whether runtime action query should observe committed AgentRun surface only or live active turn overlay.
- Moving `RuntimeSessionMcpAccess` may require dependency injection changes in `AppState` bootstrap, because current AppState wires RuntimeGateway before late session construction provider is installed.
- `SessionCapabilityService` currently acts as a convenient facade for unrelated services; splitting it may reveal hidden dependency cycles.
- Extension runtime and VFS surface routes currently share `resolve_session_frame_vfs`; replacing it must preserve project permission checks.
- Some helpers are `pub(crate)` only because API and application live in different crates; moving query ownership may require introducing a narrow port or API adapter.

## Research Inputs

- `research/session-hub-inventory.md`
- `research/runtime-gateway-mcp-boundary.md`
- `research/business-consumer-migration-map.md`
- `research/agentframe-exposure-inventory.md`

These research files are incorporated into the synthesis and migration matrix above. Before implementation starts, use them to finalize exact type names and convert the draft matrix into a reviewed checklist.
