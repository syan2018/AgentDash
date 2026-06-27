# Research: session-runtime-crate-review

- Query: 复核 release crate split draft 中 RuntimeSession/session 相关拆分方案，给出未来 module/crate 分组、public exports 处理、RuntimeSession crate 依赖规则、physical extraction 前置迁移批次，以及 draft 缺口/错误。
- Scope: internal
- Date: 2026-06-25

## Findings

### Files found

- `.trellis/tasks/06-24-release-crate-boundary-review/prd.md` - 上游边界 review 的目标与 acceptance criteria。
- `.trellis/tasks/06-24-release-crate-boundary-review/design.md` - RuntimeSession/AgentRun/Lifecycle/RuntimeGateway 目标边界假设。
- `.trellis/tasks/06-24-release-crate-boundary-review/implement.md` - 上游 child task / batch 顺序。
- `.trellis/tasks/06-24-release-crate-boundary-review/research/01-session-runtime-inventory.md` - session 文件清单、外部调用点和迁移建议。
- `.trellis/tasks/06-24-release-crate-boundary-review/research/05-crate-split-coupling-map.md` - crate split 依赖图、batch 和风险。
- `.trellis/tasks/06-24-release-crate-split-draft/design.md` - 当前 crate split draft。
- `.trellis/tasks/06-24-release-crate-split-draft/implement.md` - 当前未来 wave checklist。
- `.trellis/spec/backend/session/architecture.md` - Session 目标语义是 RuntimeSession delivery/trace substrate。
- `.trellis/spec/backend/runtime-gateway.md` - RuntimeGateway Session MCP action 必须通过 AgentRun/Lifecycle current runtime surface query。
- `.trellis/spec/backend/directory-structure.md` - backend crate/layer 依赖方向和 `agentdash-application-ports` 角色。
- `crates/agentdash-application/src/lib.rs` - application root 仍 broad-public 暴露 39 个模块。
- `crates/agentdash-application/src/session/mod.rs` - RuntimeSession facade 与当前 public exports。
- `crates/agentdash-application/src/session/**` - RuntimeSession launch、turn、event、projection、live coordination、hook、capability transition 实现。
- `crates/agentdash-application/src/agent_run/runtime_surface.rs` - current/resource runtime surface query 与 RuntimeGateway MCP surface port adapter。
- `crates/agentdash-application/src/agent_run/runtime_surface_update.rs` - AgentRun surface update 与 active runtime adoption port。
- `crates/agentdash-application/src/agent_run/frame/launch_commit.rs` - accepted launch control-plane commit adapter。
- `crates/agentdash-application/src/runtime_gateway/mcp_access.rs` - RuntimeGateway MCP access 已通过 ports crate 的 MCP surface query。
- `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs` - 已存在 gateway-facing MCP current surface port/DTO。
- `crates/agentdash-api/src/agent_run_runtime_surface.rs` - API current surface adapter；旧 `session_construction.rs` 已不存在。

### Related specs

- `.trellis/spec/backend/session/architecture.md` 明确 Session 只拥有 turn/tool/event/resume/debug/projection/trace lineage，不拥有 business ownership、permission scope、Lifecycle progress 或 Agent effective surface。
- `.trellis/spec/backend/runtime-gateway.md` 要求 `RuntimeSessionMcpAccess` 消费 AgentRun current surface query port，不进入 `SessionHub`，也不直接持有 `AgentFrame` 或 current frame resolver。
- `.trellis/spec/backend/directory-structure.md` 要求 Interface -> Application -> Domain 依赖向内，`agentdash-application-ports` 只承载 application 边界 port、轻量 DTO/error。

### Code patterns

- 当前 `session/mod.rs` 已比上游 research 更收敛：`baseline_capabilities`、`bootstrap`、`dimension`、`hub`、`runtime_builder`、`runtime_commands`、`runtime_control`、`runtime_services`、`tool_assembly`、`turn_processor` 等多为 `pub(crate)`，但仍公开 `context`、`continuation`、`control`、`core`、`eventing`、`launch`、`persistence`、`stall_detector`、`terminal_cache`、`types` 和大量 `pub use`（`crates/agentdash-application/src/session/mod.rs:3`, `crates/agentdash-application/src/session/mod.rs:10`, `crates/agentdash-application/src/session/mod.rs:17`, `crates/agentdash-application/src/session/mod.rs:73`, `crates/agentdash-application/src/session/mod.rs:81`, `crates/agentdash-application/src/session/mod.rs:118`）。
- `AgentFrameRuntimeTarget` 已不在 `session::types`，当前位于 AgentRun 归属下，表达 frame revision + delivery runtime session 的 live adoption target（`crates/agentdash-application/src/agent_run/runtime_target.rs:4`, `crates/agentdash-application/src/agent_run/runtime_target.rs:9`）。
- `SessionRuntimeInner` 仍直接持有 VFS service、AgentFrame repo、RuntimeSessionExecutionAnchor repo、LifecycleAgent repo、PermissionGrant repo 和 AgentRun mailbox adapter（`crates/agentdash-application/src/session/hub/mod.rs:53`, `crates/agentdash-application/src/session/hub/mod.rs:87`, `crates/agentdash-application/src/session/hub/mod.rs:88`, `crates/agentdash-application/src/session/hub/mod.rs:90`, `crates/agentdash-application/src/session/hub/mod.rs:92`, `crates/agentdash-application/src/session/hub/mod.rs:93`）。这是 RuntimeSession 物理拆 crate 的主要阻塞。
- `SessionRuntimeBuilder` 仍公开 `active_runtime_surface_adopter()` 并返回 AgentRun adoption trait object（`crates/agentdash-application/src/session/runtime_builder.rs:143`）。目标形态应改为 composition root 把 RuntimeSession live adapter 注入 AgentRun port，RuntimeSession crate 不认识 AgentRun implementation。
- `AgentRunRuntimeSurfaceQuery` 已从 `runtime_session_id` 经 anchor/run/agent/current frame 解析 current surface（`crates/agentdash-application/src/agent_run/runtime_surface.rs:53`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:68`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:217`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:295`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:352`）。
- `AgentRunResourceSurfaceQuery` 已存在，并以 runtime surface + lifecycle projector 生成 resource surface（`crates/agentdash-application/src/agent_run/runtime_surface.rs:84`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:106`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:160`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:195`）。
- RuntimeGateway MCP access 已不依赖 AgentRun implementation type，而是依赖 `agentdash-application-ports::runtime_gateway_mcp_surface::RuntimeGatewayMcpSurfaceQueryPort`（`crates/agentdash-application/src/runtime_gateway/mcp_access.rs:7`, `crates/agentdash-application/src/runtime_gateway/mcp_access.rs:22`, `crates/agentdash-application/src/runtime_gateway/mcp_access.rs:42`）。
- `agentdash-application-ports` 已包含 `runtime_gateway_mcp_surface` 模块（`crates/agentdash-application-ports/src/lib.rs:4`），其 DTO/port 覆盖 RuntimeGateway MCP surface with backend（`crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs:18`, `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs:28`, `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs:60`）。
- AgentRun current surface query 实现了 RuntimeGateway MCP surface port，因此 RuntimeGateway extraction 的第一道 port 已完成（`crates/agentdash-application/src/agent_run/runtime_surface.rs:746`）。
- 旧 `crates/agentdash-api/src/session_construction.rs` 已不存在。当前 API adapter 是 `agent_run_runtime_surface.rs`，并已提供 current surface、backend-required current surface、project guard 和 resource VFS facade（`crates/agentdash-api/src/agent_run_runtime_surface.rs:33`, `crates/agentdash-api/src/agent_run_runtime_surface.rs:54`, `crates/agentdash-api/src/agent_run_runtime_surface.rs:75`, `crates/agentdash-api/src/agent_run_runtime_surface.rs:89`, `crates/agentdash-api/src/agent_run_runtime_surface.rs:108`, `crates/agentdash-api/src/agent_run_runtime_surface.rs:122`）。
- Accepted launch 的 AgentFrame/Lifecycle writes 已迁到 AgentRun adapter：文件注释声明 RuntimeSession owns delivery events/stream attach，adapter owns accepted AgentFrame revision、LifecycleAgent delivery binding、owner-bootstrap status（`crates/agentdash-application/src/agent_run/frame/launch_commit.rs:1`, `crates/agentdash-application/src/agent_run/frame/launch_commit.rs:3`）。Session commit 现在只调用 adapter（`crates/agentdash-application/src/session/launch/commit.rs:107`）。
- 但 Session launch deps 仍从 session 内部仓储构造 `AgentRunAcceptedLaunchCommitAdapter`（`crates/agentdash-application/src/session/launch/deps.rs:53`, `crates/agentdash-application/src/session/launch/deps.rs:144`），orchestrator 仍从 adapter 查询 bootstrap 状态并标记完成（`crates/agentdash-application/src/session/launch/orchestrator.rs:84`, `crates/agentdash-application/src/session/launch/orchestrator.rs:268`）。这说明职责位置已改善，但 crate 依赖还没有倒置成 port。

### Future module/crate grouping for session internals

| Group | Current files | Target owner | Notes |
| --- | --- | --- | --- |
| Delivery/session core | `core.rs`, `control.rs`, `runtime_control.rs`, `runtime_services.rs`, `runtime_registry.rs`, `runtime_builder.rs` | RuntimeSession crate internal facade | Public API 应只保留 `SessionCoreService`、`SessionControlService`、`SessionRuntimeService` 等 use case facade；builder/registry/services 作为 composition/internal。 |
| Trace/event substrate | `eventing.rs`, `persistence.rs`, `branching.rs`, `compaction_checkpoint.rs`, `transcript_restore.rs`, `title_service.rs`, `title_generator.rs` | RuntimeSession crate | `SessionMeta`、`PersistedSessionEvent` 等底层 DTO 已在 SPI，RuntimeSession crate 可以 re-export 或包一层 facade，但不要把业务 surface 混进来。 |
| Turn processing | `turn_processor.rs`, `turn_supervisor.rs`, `stall_detector.rs`, `continuation.rs`, `post_turn_handler.rs`, `terminal_effects.rs`, `terminal_cache.rs`, `tool_result_cache.rs` | RuntimeSession crate | 保留 active turn、stream supervision、terminal/tool result cache、continuation；对外只暴露 turn/steer/cancel/read-model 入口。 |
| Runtime command / transition | `runtime_commands.rs`, `runtime_transition_service.rs`, `hub/runtime_context_transition.rs` | Split: command store in RuntimeSession; AgentFrame transition intent in AgentRun port | Pending delivery command 可留 RuntimeSession；`AgentFrameTransitionRecord` 和 capability/context transition 语义应归 AgentRun runtime surface update/adoption port。 |
| Connector context/projection | `context.rs`, `context_frame.rs`, `context_projector.rs`, `context_usage_*`, `*_context_frame.rs`, `memory_context_frame.rs` | RuntimeSession projection crate/module with AgentRun input ports | `ExecutionContext` 是 connector-facing projection，可留 RuntimeSession；但 capability/VFS/MCP effective facts 必须来自 AgentRun `FrameLaunchEnvelope` 或 current surface DTO。 |
| Launch substrate | `launch/plan.rs`, `launch/planner.rs`, `launch/preparation.rs`, `launch/connector_start.rs`, `launch/ingestion.rs`, `launch/service.rs`, session-side part of `launch/commit.rs` | RuntimeSession crate | 从 `FrameLaunchEnvelope` 到 Prepared/Accepted/Attached turn 属 RuntimeSession；`LaunchCommand` source business intent 和 accepted AgentFrame/Lifecycle commit 不属于 RuntimeSession。 |
| Launch commit/control-plane side effects | `agent_run/frame/launch_commit.rs`; session `launch/deps.rs` wires it | AgentRun/Lifecycle facade/port | 当前 adapter 已在 AgentRun；前置条件是让 Session 依赖 `AcceptedLaunchCommitPort` / `BootstrapStatusPort`，不再构造 AgentRun adapter 或持有 repos。 |
| Live coordination | `hub/mod.rs`, `hub/factory.rs`, `hub/facade.rs`, `hub/tool_builder.rs`, `hub/hook_dispatch.rs` | RuntimeSession internal live adapter, split into tool/hook/transition services | 保留 connector live refresh、active turn cache、runtime registry sync；移除 direct AgentFrame/Lifecycle/Permission repo knowledge。 |
| Agent frame adoption | `agent_run/runtime_surface_update.rs` trait + `session/hub/tool_builder.rs` impl | Port in AgentRun/application-ports; implementation in RuntimeSession | Trait/DTO owner 应是 AgentRun/application-ports；RuntimeSession crate 只实现 live adapter，不定义 business target。 |
| Capability/dimension/projection | `dimension/*.rs`, `baseline_capabilities.rs`, session `types.rs` capability records | AgentRun runtime_capability / capability crate; RuntimeSession only stores transition evidence | `session/mod.rs` 继续 re-export `CapabilityState` 和 capability effect records 会让 RuntimeSession 像 capability owner，应拆出。 |
| VFS/resource surface | `prompt_vfs.rs`, `path_policy.rs`, session launch use of VFS, AgentRun `AgentRunResourceSurfaceQuery` | AgentRun resource surface facade + VFS provider module | RuntimeSession launch 可以消费 closed VFS in envelope；resource browser/current surface query 归 AgentRun，不归 RuntimeSession。 |
| Hook runtime delivery | `hooks_service.rs`, `hook_delegate.rs`, `hook_events.rs`, `hook_injection_sink.rs`, `hook_messages.rs` | Split: hook delivery adapter in RuntimeSession; hook target/surface policy in AgentRun/Hook facade | Hook execution trace 可留 RuntimeSession；AgentFrameHookRuntimeTarget 和 owner policy 不应由 RuntimeSession public facade 暴露。 |

### Public exports verdict

Keep as RuntimeSession public facade:

- `SessionCoreService`, `SessionEventingService`, `SessionControlService`, `SessionRuntimeService`, `SessionLaunchService` as narrow use-case services while API/local still compose runtime sessions (`crates/agentdash-application/src/session/mod.rs:73`, `crates/agentdash-application/src/session/mod.rs:74`, `crates/agentdash-application/src/session/mod.rs:76`, `crates/agentdash-application/src/session/mod.rs:81`, `crates/agentdash-application/src/session/mod.rs:104`).
- `SessionPersistence`, store traits and persisted event/meta DTOs only as RuntimeSession storage contract; long term source of truth remains `agentdash-spi/src/session_persistence.rs` (`crates/agentdash-spi/src/session_persistence.rs:304`, `crates/agentdash-spi/src/session_persistence.rs:531`, `crates/agentdash-spi/src/session_persistence.rs:953`).
- `SessionTurnSteerCommand`, `TurnEvent`, `SessionExecutionState`, `UserPromptInput` as delivery/turn protocol DTOs if API/AgentRun mailbox still needs them (`crates/agentdash-application/src/session/mod.rs:73`, `crates/agentdash-application/src/session/mod.rs:117`, `crates/agentdash-application/src/session/types.rs:25`, `crates/agentdash-application/src/session/types.rs:209`).
- Session lineage/branching/read-model exports can remain public only if they are presentation/query facades, not construction internals (`crates/agentdash-application/src/session/mod.rs:60`).

Migrate out of session public facade:

- `LaunchCommand`, `LaunchSource`, `LaunchModifier`, `LocalRelayLaunchPayload` should move toward AgentRun/Lifecycle/API intake or a runtime-session delivery command port; they encode business source intent, not pure RuntimeSession facts (`crates/agentdash-application/src/session/mod.rs:81`).
- `RuntimeCommandRecord`, `RuntimeDeliveryCommand`, `AgentFrameTransitionRecord` should split: delivery command records stay RuntimeSession; AgentFrame transition records move to AgentRun/runtime-surface contract (`crates/agentdash-application/src/session/mod.rs:100`).
- `CapabilityState`, `CapabilityDimensionKey`, capability contribution/effect/transition records should stop being re-exported by `session`; ownership belongs to AgentRun runtime capability/capability surface (`crates/agentdash-application/src/session/mod.rs:118`, `crates/agentdash-application/src/session/types.rs:6`).
- `SessionRuntimeTransitionService` should become an AgentRun-facing port or internal RuntimeSession adapter; it currently imports AgentRun runtime capability projection and should not be a broad session API (`crates/agentdash-application/src/session/mod.rs:106`).
- `SessionRuntimeBuilder` should be composition-root/internal, not a public RuntimeSession crate API, because it exposes cross-boundary wiring such as `active_runtime_surface_adopter()` (`crates/agentdash-application/src/session/mod.rs:99`, `crates/agentdash-application/src/session/runtime_builder.rs:143`).
- `HookRuntimeDelegate`, `SessionHookService`, `build_hook_trace_envelope`, `TerminalHookEffectBinding` should be split into hook delivery facade vs AgentRun/hook owner target; avoid exporting hook owner policy through RuntimeSession (`crates/agentdash-application/src/session/mod.rs:77`, `crates/agentdash-application/src/session/mod.rs:79`, `crates/agentdash-application/src/session/mod.rs:93`).
- `local_workspace_vfs` belongs to project/workspace/VFS bootstrap, not RuntimeSession public surface (`crates/agentdash-application/src/session/mod.rs:98`).

Privatize behind RuntimeSession crate internals:

- `hub`, `dimension`, `tool_assembly`, `runtime_builder`, `runtime_services`, `runtime_commands`, `runtime_control`, `post_turn_handler`, `terminal_effects`, `title_generator`, `title_service`, `tool_result_cache`, `turn_processor`, `plan` should not be route-level or cross-crate public modules. Some are already `pub(crate)`; physical extraction should keep them crate-private and expose typed facades only (`crates/agentdash-application/src/session/mod.rs:20`, `crates/agentdash-application/src/session/mod.rs:29`, `crates/agentdash-application/src/session/mod.rs:40`, `crates/agentdash-application/src/session/mod.rs:43`, `crates/agentdash-application/src/session/mod.rs:53`).
- `session::types` should be split into smaller modules. Keeping all protocol, capability, prompt, meta and execution state types under one public `types` module recreates a catch-all substrate (`crates/agentdash-application/src/session/mod.rs:58`, `crates/agentdash-application/src/session/mod.rs:118`).

### RuntimeSession crate dependency rules

Allowed dependencies for a future RuntimeSession crate:

- `agentdash-domain` for repository traits/value objects that are true RuntimeSession trace anchors or generic domain IDs. Use sparingly; AgentFrame/Lifecycle repositories should normally arrive through higher-level ports.
- `agentdash-spi` for connector traits, hook traits, `SessionPersistence` DTOs/stores, `CapabilityState` as serialized connector-facing data only.
- `agentdash-agent-types` / `agentdash-agent-protocol` for tool/input/content/stream protocol DTOs used at connector delivery boundary.
- `agentdash-application-ports` for backend transport, MCP discovery, runtime-session delivery/adoption ports, and narrow gateway-facing/query ports.
- `agentdash-relay` or protocol crates only for delivery transport DTOs already used at connector/runtime boundary.
- `tokio`, `async-trait`, `serde`, `uuid`, `chrono`, tracing and similar infrastructure-neutral libraries.

Forbidden dependencies for a future RuntimeSession crate:

- AgentRun implementation crate/module. RuntimeSession may implement AgentRun-owned ports, but it must not import AgentRun services/builders/query implementation.
- Lifecycle implementation crate/module. RuntimeSession must not query `LifecycleAgentRepository` to decide bootstrap/progress; Lifecycle/AgentRun should pass closed launch facts or use ports.
- RuntimeGateway implementation crate. Gateway can depend on ports and RuntimeSession MCP access ports; RuntimeSession must not know provider registry/action admission.
- API/interface crates, route DTOs, `AppState`, auth/project permission helpers.
- Business modules: Canvas, Permission, WorkspaceModule, Companion, Task, Workflow reducer implementation, ExtensionRuntime, MCP preset management.
- Concrete VFS provider/service crate if VFS is split. RuntimeSession launch may consume a closed `Vfs` or a VFS provider port; it should not own resource-surface query.
- Repository set/composition root modules that aggregate all domain repositories.

### Preconditions before physical extraction

1. Update draft baseline to current code: `AgentFrameRuntimeTarget` move is already done, old `session_construction.rs` is gone, RuntimeGateway MCP surface port already exists, and accepted launch commit writes already live in AgentRun adapter.
2. Introduce the remaining ports before moving files:
   - RuntimeSession delivery launch/turn/event port.
   - RuntimeSession live adoption implementation port consumed by AgentRun runtime surface update.
   - Accepted launch commit/bootstrap status port consumed by RuntimeSession launch, implemented by AgentRun/Lifecycle.
   - Frame launch envelope provider port moved out of AgentRun implementation if RuntimeSession crate must consume it.
   - AgentRun current/resource surface DTO/port moved or mirrored into `agentdash-application-ports` if AgentRun implementation will be extracted.
3. Replace `SessionRuntimeInner` direct AgentFrame/Lifecycle/Permission/Mailbox repo fields with ports or move the repo-owning logic fully to AgentRun/Lifecycle adapters. Current direct fields are the main extraction blocker (`crates/agentdash-application/src/session/hub/mod.rs:87`, `crates/agentdash-application/src/session/hub/mod.rs:88`, `crates/agentdash-application/src/session/hub/mod.rs:90`, `crates/agentdash-application/src/session/hub/mod.rs:92`).
4. Change session launch deps so RuntimeSession receives `AcceptedLaunchCommitPort` and `BootstrapStatusPort`; it should not construct `AgentRunAcceptedLaunchCommitAdapter` from repos (`crates/agentdash-application/src/session/launch/deps.rs:144`).
5. Keep `FrameLaunchEnvelope` as the only launch handoff, but move the provider trait to a neutral port if RuntimeSession crate will depend on it. Current provider is in AgentRun frame module (`crates/agentdash-application/src/agent_run/frame/launch_envelope_provider.rs:66`).
6. Split `session::types` and stop re-exporting capability/effect records from RuntimeSession facade. AgentRun/capability owns effective runtime surface; RuntimeSession only stores delivery/projection evidence.
7. Keep RuntimeGateway extraction behind its existing MCP surface port and verify production `mcp_access` stays free of SessionHub/AgentFrame references; there is already a test for that invariant (`crates/agentdash-application/src/runtime_gateway/mcp_access.rs:463`).
8. Normalize API route imports to `agent_run_runtime_surface` and application facades. Existing Canvas/Extension/Terminal routes already use the new helper in several places; future cleanup should remove any new direct `session` construction/surface imports.
9. Reduce application root broad `pub mod` exposure before using crate boundaries as architecture enforcement (`crates/agentdash-application/src/lib.rs:1`, `crates/agentdash-application/src/lib.rs:30`, `crates/agentdash-application/src/lib.rs:39`).
10. Only after imports express the target graph, extract in this order: ports expansion -> RuntimeGateway/RuntimeSession -> AgentRun/Lifecycle -> VFS later.

### Proposed migration batches

Batch A: draft correction and boundary audit.

- Mark completed: `AgentFrameRuntimeTarget` ownership moved to AgentRun; RuntimeGateway MCP surface port exists; API current surface helper renamed to `agent_run_runtime_surface.rs`; accepted launch commit writes live in AgentRun adapter.
- Add an `rg` gate for stale references to `session_construction.rs` and `session::AgentFrameRuntimeTarget`.

Batch B: port ownership.

- Add `runtime_session_delivery` and `runtime_session_launch_commit` ports to `agentdash-application-ports`.
- Move or mirror AgentRun current/resource surface DTOs needed by API/RuntimeGateway/external crates into ports.
- Keep implementations in `agentdash-application` during this batch.

Batch C: RuntimeSession facade cleanup.

- Split `session::types`.
- Keep only delivery/trace/turn/event/projection facade exports.
- Make hub/builder/tool/hook/transition internals crate-private.
- Stop exporting capability projection/effect records from `session`.

Batch D: invert launch/live coordination.

- RuntimeSession launch depends on neutral launch envelope and accepted-launch-commit ports.
- AgentRun/Lifecycle implement commit/bootstrap/status/adoption ports.
- `SessionRuntimeInner` loses direct AgentFrame/Lifecycle/Permission repos unless those repos are strictly behind runtime-session-owned ports.

Batch E: API and resource surface consolidation.

- Treat `agent_run_runtime_surface.rs` as the canonical API adapter.
- Ensure Canvas/Extension/Terminal/VFS routes use project-checked current surface/resource surface helpers.
- Keep session routes limited to session timeline/stream/turn control, not resource/current surface selection.

Batch F: physical extraction.

- Extract RuntimeGateway after it consumes only ports.
- Extract RuntimeSession after it no longer imports AgentRun/Lifecycle/business implementation.
- Extract AgentRun and Lifecycle after their mutual links are port-mediated.
- Defer VFS crate until generic VFS provider/service is separated from AgentRun resource surface.

### Draft gaps or errors

- The draft still says Wave 1 should add gateway-facing MCP/current surface contracts, but `runtime_gateway_mcp_surface` already exists in `agentdash-application-ports` and production RuntimeGateway MCP access uses it.
- The draft references `session_construction.rs`; current API helper is `agent_run_runtime_surface.rs`.
- The draft treats moving `AgentFrameRuntimeTarget` out of `session` as future work; current code has already moved it to `agent_run/runtime_target.rs`.
- The draft says launch commit AgentFrame/Lifecycle writes must move out of session; the write owner already moved to `AgentRunAcceptedLaunchCommitAdapter`. Remaining work is dependency inversion: session still constructs/uses that adapter through repo fields.
- The draft does not name the actual remaining blocker inside `SessionRuntimeInner`: direct AgentFrame/Lifecycle/Permission/Mailbox/VFS service fields.
- The draft does not distinguish RuntimeGateway MCP surface port, AgentRun current surface query port, and AgentRun resource surface query. These are now three separate surfaces and should have separate crate/port decisions.
- The draft under-specifies public export cleanup. The main public hazards are `session::types`, capability re-exports, launch command/source, runtime transition records, hook runtime exports, and `SessionRuntimeBuilder`.
- The draft should include a "no business module dependency" rule for RuntimeSession crate. Without that, Canvas/Permission/WorkspaceModule/VFS references can follow live adoption code into the extracted crate.

### External references

- No web references used.
- No package version lookup required; this review is source/spec based.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned `Current task: (none)`. The research file was written under the user-specified task path `.trellis/tasks/06-24-release-crate-split-draft`.
- `crates/agentdash-api/src/session_construction.rs` was not found; current replacement appears to be `crates/agentdash-api/src/agent_run_runtime_surface.rs`.
- This review did not modify production code, Cargo workspace files, specs, or git state.
- No database migration was inspected because the task is research-only and crate boundary planning does not require schema changes.
