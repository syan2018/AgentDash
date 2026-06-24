# Release application crate split 设计

## Position

当前任务已经从 crate split draft 升级为 `codex/release-crate-split-refactor` 分支的重构主轴。设计目标不是继续讨论是否拆分，而是把 application 内部边界改到可以被 Cargo crate graph 强制表达的状态。

核心判断：

- Cargo graph 当前不是主要阻塞；`agentdash-application` 内部 module graph 才是阻塞。
- 先补 ports/facades，再清理 import/public visibility，最后物理移动 crate。
- 允许先锚定 crate 边界并产生 compile errors，再按工作项修复；这个分支用于承载完整迁移。

## Current Facts

### 已完成前置

- `AgentFrameRuntimeTarget` 已归 AgentRun。
- API current surface helper 已是 `agent_run_runtime_surface.rs`。
- RuntimeGateway MCP current-surface port 已在 `agentdash-application-ports`。
- RuntimeGateway MCP access 生产路径依赖 port，不依赖 AgentRun implementation。
- AgentRun current/resource surface query 已存在。
- Canvas / Extension runtime path Project guard 已部分落地。
- accepted launch 的 AgentFrame / Lifecycle writes 已进入 AgentRun launch commit adapter。

### 剩余硬边

| Edge | Cause | Boundary owner |
| --- | --- | --- |
| `session <-> agent_run` | RuntimeSession live hub/adoption/launch/mailbox/effective capability 仍直接串 AgentRun implementation | ports + RuntimeSession implementation adapter |
| `agent_run <-> lifecycle` | Runtime address / lifecycle projector / current frame resolver / AgentFrameBuilder / RuntimeSessionCreator 互相直连 | AgentRun surface ports + Lifecycle projection/materialization ports |
| `runtime_gateway -> mcp_preset/workspace` | setup actions 直接调 helper | `runtime_gateway_setup` ports |
| `api routes/helpers -> implementation DTO` | route/helper import AgentRun/VFS/session internals | AppState-owned facade handles + stable DTO |
| `vfs -> session/lifecycle/canvas` | generic VFS core 与 owner providers 混在一个 facade | VFS core late extraction + owner provider adapters |

## Target Graph

```mermaid
flowchart TD
  API["agentdash-api / agentdash-local / agentdash-mcp"]
  AG["agentdash-application-agentrun"]
  LC["agentdash-application-lifecycle"]
  RS["agentdash-application-runtime-session"]
  RG["agentdash-application-runtime-gateway"]
  VFS["agentdash-application-vfs (late)"]
  Ports["agentdash-application-ports"]
  Core["agentdash-domain / agentdash-spi / protocol/type crates"]

  API --> AG
  API --> LC
  API --> RS
  API --> RG
  API --> VFS
  AG --> Ports
  LC --> Ports
  RS --> Ports
  RG --> Ports
  VFS --> Ports
  Ports --> Core
  AG -. "RuntimeSession delivery/adoption ports" .-> RS
  LC -. "RuntimeSession creation port" .-> RS
  AG -. "Lifecycle projection port" .-> LC
  LC -. "AgentRun materialization/update port" .-> AG
```

Dashed arrows are runtime wiring through ports; implementation crates remain independently movable because the connection is expressed as traits and DTOs.

## Crate Ownership

| Crate | Owns | Extract after |
| --- | --- | --- |
| `agentdash-application-ports` | pure DTO/trait/error for cross-application boundaries: AgentRun surface, RuntimeSession delivery/adoption, Lifecycle projection, RuntimeGateway setup, VFS runtime projection, launch envelope | immediately, before implementation moves |
| `agentdash-application-runtime-gateway` | action registry, actor/context admission, fixed session/setup providers, extension dynamic provider, tool adapter | MCP + setup action backing ports are complete |
| `agentdash-application-runtime-session` | runtime session core/control/eventing/persistence, runtime registry/services, launch substrate after `FrameLaunchEnvelope`, turn processor/supervisor, continuation, terminal/tool result caches, lineage/projection | launch/adoption/mailbox/effective-capability deps are ports |
| `agentdash-application-agentrun` | current/resource surface, effective capability/admission, frame construction/update/launch commit, runtime surface update, mailbox/message delivery, workspace command/read model | no direct Session/Lifecycle implementation imports |
| `agentdash-application-lifecycle` | dispatch/control ledger, subject association, orchestration activation/reducer/scheduler/materialization, terminal callback to reducer, lifecycle projection implementation | RuntimeSession creation and AgentRun frame materialization are ports |
| `agentdash-application-vfs` | generic VFS core, path/types/provider/service/surface/summary/materialization/mutation/search/rewrite/fs tools | owner-specific providers are directional |

## Port Modules

| Port module | Purpose |
| --- | --- |
| `agent_run_surface` | `AgentRunRuntimeAddress`, current surface DTO/error/trait, resource surface DTO/error/trait, terminal/runtime placement DTOs. |
| `runtime_gateway_mcp_surface` | Existing reduced MCP current-surface DTO/trait for RuntimeGateway. |
| `runtime_session_delivery` | RuntimeSession creation request/result, delivery command refs, turn/message delivery traits. |
| `runtime_surface_adoption` | Active runtime adoption target/trait currently represented by AgentRun target + SessionHub adopter. |
| `frame_launch_envelope` | Launch-ready handoff traits/DTOs needed by RuntimeSession launch without depending on AgentRun implementation. |
| `lifecycle_surface_projection` | message stream ref, orchestration node evidence/projection, lifecycle mount projection trait. |
| `lifecycle_materialization` | Lifecycle dispatch/materialization facade used by AgentRun project-agent / workspace command surfaces. |
| `agent_frame_materialization` | AgentRun-owned frame construction/update boundary consumed by Lifecycle AgentCall materialization. |
| `runtime_gateway_setup` | MCP probe, workspace detect, detect-git, browse-directory, discover-by-identity backing traits. |
| `vfs_surface_runtime` | API/local implemented runtime projection facts consumed by VFS summary. |

## Reference Rules

### API / Local / MCP

- Route modules keep auth, DTO mapping, path parsing and error mapping.
- Bootstrap/AppState may instantiate concrete services and wire ports.
- Current/resource/runtime surface helpers consume AppState-owned facade handles.
- Presentation/debug read-models may expose trace/frame views, but they are separate from RuntimeGateway/current-surface DTOs.

### RuntimeGateway

- Gateway owns registry, action kind, actor/context validation, provider dispatch and action input/output validation.
- Session MCP providers consume `RuntimeSessionMcpAccess`; production access consumes `RuntimeGatewayMcpSurfaceQueryPort`.
- Setup providers consume `runtime_gateway_setup` backing ports.
- Extension providers consume Project installation / runtime transport ports and current surface admission results supplied by caller/facade.

### AgentRun

- Current surface query starts at `runtime_session_id`, follows `RuntimeSessionExecutionAnchor`, loads run/agent/current frame and returns closed DTO.
- DTOs carry both `launch_evidence_frame_id` and `current_surface_frame_id`; VFS/capability/MCP use current surface frame.
- Resource surface starts from current AgentFrame typed VFS and consumes Lifecycle projection through a port.
- Surface-changing modules submit typed update requests. AgentRun owns AgentFrame revision write, live adoption invocation and effective capability/admission.

### Lifecycle

- Lifecycle owns run/agent/control ledger, subject association, orchestration runtime, reducer and scheduler.
- Lifecycle creates or receives RuntimeSession delivery evidence through creation ports, then writes anchor and current delivery binding.
- AgentCall materialization consumes AgentRun frame materialization/update port instead of passing frame builder internals.
- Terminal callback resolves anchor/node coordinate and applies `OrchestrationRuntimeEvent` reducer.

### RuntimeSession

- RuntimeSession owns delivery/trace/turn/event stream/connector continuation/runtime registry/active turn/live sync.
- RuntimeSession implements delivery/adoption ports and consumes launch envelope/commit ports.
- Launch substrate consumes closed launch facts; frame construction and accepted launch control-plane writes are outside RuntimeSession implementation.

### VFS

- AgentRun resource surface is an AgentRun facade because it starts from current AgentFrame typed VFS and Lifecycle projection facts.
- Generic VFS core owns provider dispatch, path normalization, summary, materialization and mutation mechanics.
- Owner providers stay with their owner or adapters until dependency direction is clean.

## Parallel Lanes

| Lane | Work items | Parallel notes |
| --- | --- | --- |
| A | `01-ports-boundary-expansion` | Single owner for `agentdash-application-ports`; other lanes consume after first scaffold commit. |
| B | `02-runtime-gateway-setup-boundary`, `06-api-consumer-facade-cleanup` | Can run after ports scaffold; edit mostly RuntimeGateway/API. |
| C | `03-agentrun-surface-facade`, `07-vfs-resource-surface-boundary` | Share AgentRun resource surface; coordinate DTO names before coding. |
| D | `04-runtime-session-substrate-boundary`, `05-agentrun-lifecycle-boundary` | Highest conflict lane; run with explicit file ownership and frequent checkpoint commits. |
| E | `08-public-visibility-cleanup` | Runs after facade consumers are mostly moved; uses compile errors and grep gates. |
| F | `09-physical-crate-extraction-runtime`, `10-physical-crate-extraction-control-plane-vfs` | Runs after import graph has target direction. |

## Extraction Waves

### Wave 0: Mainline Setup

- Branch created and bound to task.
- Work item files and manifests created.
- Current baseline commands captured.

### Wave 1: Ports Only

- Add named port modules.
- Keep implementations in existing crates.
- Use compile errors to expose DTO dependency shape early.

### Wave 2: Import Cleanup

- RuntimeGateway setup consumes ports.
- Lifecycle consumes RuntimeSession creation and AgentRun materialization ports.
- AgentRun consumes RuntimeSession delivery/adoption and Lifecycle projection ports.
- SessionHub/runtime builder consume launch/adoption/mailbox/effective-capability ports.
- API helpers consume facade handles.

### Wave 3: Runtime Crate Extraction

- Extract RuntimeGateway.
- Extract RuntimeSession.
- Rewire API/local/MCP composition root.

### Wave 4: Control Plane Extraction

- Extract AgentRun.
- Extract Lifecycle.
- Keep workflow runtime/reducer with Lifecycle unless a later design separates workflow definition/compiler.

### Wave 5: VFS Core Extraction

- Extract generic VFS core after owner-specific providers are directional.
- Keep lifecycle/canvas/routine/skill providers with owners or adapter crates as needed.

## Static Gates

```powershell
cargo metadata --no-deps --format-version 1
rg -n "use crate::(mcp_preset|workspace)::" crates/agentdash-application/src/runtime_gateway -g '*.rs'
rg -n "crate::session::(plan|runtime_commands|types|hub|Session.*Service|LaunchCommand)" crates/agentdash-application/src/agent_run -g '*.rs'
rg -n "AgentFrameBuilder" crates/agentdash-application/src/lifecycle crates/agentdash-application/src/workflow/orchestration -g '*.rs'
rg -n "crate::lifecycle::.*AgentRunRuntimeAddress|crate::lifecycle::surface::surface_projector|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src/agent_run crates/agentdash-application/src/session -g '*.rs'
rg -n "AgentRunRuntimeSurfaceQuery::new|AgentRunRuntimeSurfaceQueryDeps|runtime_surface_query\\(" crates/agentdash-api/src -g '*.rs'
rg -n "agentdash_application::session::(construction|plan|types|hub)|agentdash_application::agent_run::frame|agentdash_application::vfs::ResolvedVfsSurfaceSource|agentdash_application::vfs::build_surface_summary" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
```

## Compile Gates

阶段提交可以红灯；每个 wave 的收敛点需要记录能跑到哪里：

```powershell
cargo check -p agentdash-application-ports
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local -p agentdash-mcp
cargo check --workspace
```

Targeted tests:

```powershell
cargo test -p agentdash-application runtime_gateway::mcp_access
cargo test -p agentdash-application runtime_gateway::session_actions
cargo test -p agentdash-application runtime_gateway::extension_actions
cargo test -p agentdash-application agent_run::runtime_surface
cargo test -p agentdash-application agent_run::runtime_surface_update
cargo test -p agentdash-application agent_run::permission_runtime_surface_update
cargo test -p agentdash-api agent_run_runtime_surface
```
