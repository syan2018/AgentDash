# Agent 生命周期模块边界维护设计

## Architecture

本任务维护两条主线：

- Control-plane facts：`LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`RuntimeSessionExecutionAnchor`、`AgentRunDeliveryBinding`、`LifecycleGate`、`AgentRunMailbox`。
- Observable stream：`RuntimeSession` event store、`BackboneEnvelope`、AgentRun journal projection。

Runtime terminal、hook effect、wait result、mailbox wake、frontend refresh 都必须从这两条主线中派生，不创建新的业务事实源。

First-principles boundary:

- `RuntimeSession` owns journal persistence, connector trace, event replay and observable transport only.
- `AgentRun` owns runtime control semantics. Hook policy/effects, terminal convergence, mailbox wake, waiting projection, title/list refresh and resource-surface invalidation all resolve through `run_id + agent_id + frame_id`.
- `delivery_runtime_session_id` is evidence and routing trace. It can appear in effect payloads and invalidation payloads, but it cannot be the owner key of business replay, wait state or frontend command state.

## Wait/Gate Payload Envelope

继续使用 `LifecycleGate.payload_json`，但将 wait declaration 改为 domain-owned typed envelope。建议形态：

```json
{
  "schema_version": 1,
  "wait_policy": {
    "source": {
      "kind": "agent_run_delivery",
      "run_id": "...",
      "agent_id": "...",
      "frame_id": "..."
    },
    "expected_result": {
      "kind": "companion_result",
      "correlation_ref": "..."
    },
    "terminal_policy": {
      "completed": { "status": "failed", "failure_kind": "missing_companion_respond" },
      "failed": { "status": "failed", "failure_kind": "producer_failed" },
      "interrupted": { "status": "interrupted", "failure_kind": "producer_interrupted" }
    },
    "wake_target": {
      "namespace": "companion",
      "target_run_id": "...",
      "target_agent_id": "...",
      "client_command_id": "companion-result:{gate_id}"
    }
  },
  "display": {
    "companion_label": "reviewer"
  }
}
```

Implementation details:

- Domain module owns `GateWaitPolicyEnvelope`, `WaitProducerRef`, `WaitExpectedResult`, `WaitTerminalPolicy`, `WaitWakeTarget`.
- Existing payload metadata such as `companion_label` remains under display/source metadata, not in generic convergence fields.
- `LifecycleGate::waiting_projection()` consumes the same typed envelope where possible.
- Repository JSONB queries can still query `payload_json`, but the path names are envelope-owned and documented in one module.

## Gate Producer Terminal Convergence

Rename / reframe current `wait_obligation` application workflow service as producer terminal convergence:

- Input: `WaitProducerTerminalEvent { producer, terminal_state, terminal_message, source_turn_id, trace_ref }`.
- Lookup: gate repository lists gates whose typed wait policy producer matches.
- Apply: open gate resolves through `LifecycleGateResolver`; already resolved gate only emits idempotent wake intent if needed.
- Output: generic `GateConvergenceIntent`, including `MailboxWakeIntent` and projection invalidation intents.

Companion delivery becomes an adapter that translates `MailboxWakeIntent` into existing companion parent mailbox message format.

## AgentRun Control-Plane Effect Outbox

Migrate `runtime_session_terminal_effects` out of RuntimeSession ownership. The table currently stores AgentRun/Hook control-plane side effects (`hook_effects`、`hook_auto_resume`、`session_terminal_callback`), so the clean target is an AgentRun-owned outbox such as `agent_run_control_effects`.

- Add migration `0053_*` to recreate the table with AgentRun/control-plane naming and owner columns.
- The durable record is scoped by `run_id + agent_id + frame_id`; `delivery_runtime_session_id`, `turn_id`, and `terminal_event_seq` remain trace evidence fields.
- Add typed effect kinds:
  - `agent_run_delivery_convergence`
  - `wait_producer_terminal_convergence`
  - `lifecycle_terminal_convergence`
  - `mailbox_wake_delivery`
  - `hook_effects`
  - `hook_auto_resume_delivery`
  - `hook_runtime_projection_changed`
- Each effect payload contains its own idempotency coordinates.
- Move replay/executor ownership into AgentRun/control-plane application code. RuntimeSession only emits terminal evidence and hands it to an AgentRun control-effect intake port.
- Existing `session_terminal_callback` and composite callback orchestration should be removed in the final state; terminal convergence and hook continuation should not be coordinated by RuntimeSession-owned callback fanout.
- `hook_effects` replay uses AgentRun hook control target (`run_id + agent_id + frame_id`) and optional delivery trace evidence. Hook effect durability is not a reason to keep effect ownership under RuntimeSession.

Execution order:

1. RuntimeSession terminal event persists.
2. RuntimeSession terminal evidence is handed to AgentRun/control-plane effect intake.
3. Enqueue / execute `agent_run_delivery_convergence`.
4. If delivery binding terminal converged, enqueue / execute `wait_producer_terminal_convergence`.
5. Enqueue / execute `lifecycle_terminal_convergence`.
6. Enqueue / execute pending `hook_effects` and `hook_auto_resume_delivery` if the terminal boundary produced hook continuation.
7. Any delivery/gate/mailbox/hook/resource output emits `ControlPlaneProjectionChanged`.

All steps are idempotent. Replaying a step must be harmless if downstream facts already converged.

## Session Residue Excision

The implementation should include a dedicated cleanup phase for known Session-owned business residue:

| Dirty path | Why it is dirty | Target owner |
| --- | --- | --- |
| `agentdash-application-runtime-session/src/session/terminal_effects.rs` | RuntimeSession owns durable replay of hook/control effects | AgentRun control-effect executor |
| `agentdash-spi::session_persistence::SessionTerminalEffectStore` / `TerminalEffectType` | Effect port names make business replay look session-owned | AgentRun control-effect port/SPI |
| `agentdash-api/src/agent_run_terminal_control.rs` `SessionTerminalCallback` fanout | Runtime terminal callback coordinates AgentRun, wait/gate and lifecycle side effects | AgentRun control-effect intake + executor |
| `agentdash-api/src/routes/lifecycle_agents.rs::append_exec_terminal_waiting_items` | API route manufactures waiting projection by reading terminal registry | AgentRun wait/activity or terminal activity projection |
| `agentdash-agent-protocol::PlatformEvent::SessionMetaUpdate` business keys | Free keys are a hidden second event protocol | `ControlPlaneProjectionChanged` |
| `PlatformEvent::MailboxStateChanged { reason }` | Mailbox refresh is typed only by a free reason string and lacks AgentRun projection taxonomy | `ControlPlaneProjectionChanged { projection: mailbox, ... }` |
| `companion/notifications.rs` companion refresh keys | Companion result/request display and refresh are coupled to session meta events | Gate/mailbox snapshot + projection invalidation |
| `workspace_module/surface.rs` `workspace_module_presented` key | Resource surface/workspace panel refresh is encoded as session meta | `ControlPlaneProjectionChanged { projection: resource_surface | workspace }` plus typed presentation payload |
| `controlPlaneModel.ts` event-name allowlist | Frontend derives control-plane refresh from legacy string keys | Generated projection invalidation mapping |
| `useTerminalStore.ts` `projectionKey(eventSeq)` | Event seq is only unique inside one stream, not across AgentRun journals | `{stream_identity}:{event_seq}` |

This cleanup is part of the task goal, not optional polish. The project is pre-release, so these paths should be removed rather than hidden behind compatibility branches.

## Projection Event

Add `PlatformEvent::ControlPlaneProjectionChanged(ControlPlaneProjectionChanged)` in `agentdash-agent-protocol`.

Minimum fields:

- `projection`: string enum generated to TS. Values include `workspace`, `agent_run_list`, `mailbox`, `waiting`, `delivery`, `hook_runtime`, `resource_surface`, `title`.
- `reason`: string enum generated to TS. Values include `mailbox_state_changed`, `wait_resolved`, `delivery_terminal`, `companion_result`, `hook_effect_applied`, `hook_auto_resume_queued`, `workspace_module_presented`, `capability_state_changed`, `context_frame_changed`, `title_changed`.
- `run_id`
- `agent_id`
- optional `frame_id`
- optional `gate_id`
- optional `mailbox_message_id`
- optional `delivery_runtime_session_id`

Backbone remains observable stream. This event is an invalidation hint; frontend must refresh generated snapshots rather than mutate business state directly.

Delete companion-specific projection / refresh event emission as part of the same migration. Existing `companion_dispatch_registered`, `companion_result_available`, `companion_result_returned`, `companion_human_request`, `companion_human_response`, `companion_review_request`, `workspace_module_presented`, `mailbox_state_changed`, `session_meta_updated` and capability/context refresh keys must not remain as AgentRun workspace refresh drivers. If a UI still needs display details, it should read a typed backend projection or a typed payload carried alongside the generic invalidation, not a legacy `SessionMetaUpdate` key.

## Frontend Flow

- `controlPlaneModel` maps `ControlPlaneProjectionChanged` to `AgentRunControlPlaneEffectPlan`.
- Remove `companion_*` session meta event refresh branches.
- Remove `session_meta_updated` / `mailbox_state_changed` / `workspace_module_presented` free-key refresh branches from AgentRun workspace control-plane planning.
- `SessionCompanionRequestCard` should not keep local final truth beyond submit-in-flight UX. After response, trigger refresh and rely on waiting projection / gate result.
- `useTerminalStore` accepts stream identity for `projectOutputEvent` and `projectStateEvent`; dedup key becomes `{stream_identity}:{event_seq}`.

## Migrations and Contracts

- Add migration `0053_*` for AgentRun/control-plane effect outbox recreate and any JSONB envelope indexes needed for wait policy lookup. No new wait declaration table.
- Update `agentdash-agent-protocol` TS exports through existing contract generation.
- Run `pnpm run contracts:check`; commit generated `packages/app-web/src/generated/backbone-protocol.ts` changes.

## Trade-offs

- Flexible payload remains operationally simple but requires strict domain-owned parser and tests.
- Moving hook/control effects to AgentRun/control-plane ownership is a larger migration than reusing the RuntimeSession table, but it removes the misleading runtime-session ownership boundary.
- A single generic projection event keeps frontend refresh simple but makes backend reason/projection taxonomy important.
