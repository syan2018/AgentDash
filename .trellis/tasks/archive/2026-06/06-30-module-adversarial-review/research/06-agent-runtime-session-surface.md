# Research: Agent Runtime Session Surface

- Query: AgentRun / RuntimeSession / RuntimeGateway / mailbox / conversation control / frame construction adversarial architecture review
- Scope: mixed
- Date: 2026-06-30

## Findings

### Summary

Current code has materially improved since the 06-14 baseline:

- `SessionRuntimeInner` ready gate now checks AgentFrame, anchor, runtime surface query, lifecycle agent, capability, hook target, and `mailbox_runtime_port`; the old mailbox-boundary silent no-op is not present in the current factory.
- `/sessions/{id}/runtime-control` no longer exposes mailbox/action command surfaces; it is now mostly trace/meta/anchor/backlink/read-model.
- `AgentRunSteeringService` is no longer a product-path service; current direct steer residue is under `crates/agentdash-application/src/test_support/`.
- `AgentRunMailboxService` has been split into `delivery`, `controls`, `scheduler`, `policy`, `receipts`, and `target` files.

Residual risks remain around launch command identity, command availability projection, mailbox steering duplication, and the over-wide agent loop delegate.

### Files Found

- `.trellis/spec/backend/session/architecture.md` — Session/AgentRun/RuntimeSession owner contract.
- `.trellis/spec/backend/session/session-startup-pipeline.md` — launch command, frame construction, launch plan, and connector accepted boundary.
- `.trellis/spec/backend/session/runtime-execution-state.md` — runtime registry, turn supervisor, runtime-control, current surface, and mailbox command boundary.
- `.trellis/spec/backend/session/agentrun-mailbox.md` — durable mailbox envelope, scheduler, receipt, and turn boundary contract.
- `.trellis/spec/backend/session/execution-context-frames.md` — connector-facing `ExecutionContext` projection contract.
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` — historical baseline for this domain.
- `crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs` — AgentRun-side session boundary ports and duplicated launch command type.
- `crates/agentdash-application-runtime-session/src/session/launch/command.rs` — RuntimeSession-side duplicated launch command type.
- `crates/agentdash-application-ports/src/frame_launch_envelope.rs` — frame launch port DTOs and `FrameLaunchCommand`.
- `crates/agentdash-application/src/runtime_session_agent_run_bridge.rs` — AgentRun command to RuntimeSession command bridge.
- `crates/agentdash-application/src/frame_construction/mod.rs` — frame launch port request converted back into application AgentRun launch command.
- `crates/agentdash-application-runtime-session/src/session/launch/orchestrator.rs` — runtime launch orchestrator, envelope provider handoff.
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs` — AgentRun workspace snapshot aggregation.
- `crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs` — workspace execution projection.
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs` — conversation execution and command availability projection.
- `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs` — backend command precondition and availability validation.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` — AgentRun workspace and mailbox command route assembly.
- `crates/agentdash-api/src/routes/sessions.rs` — RuntimeSession state/runtime-control route assembly.
- `crates/agentdash-contracts/src/runtime/workflow.rs` — generated-contract source for AgentRun workspace and RuntimeSession control DTOs.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs` — mailbox service dependency surface.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs` — mailbox claim, launch, steer, delegate drain paths.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs` — AgentRuntimeDelegate wrapper for mailbox turn boundaries.
- `crates/agentdash-agent-types/src/runtime/delegate.rs` — over-wide agent runtime delegate trait.
- `crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs` — RuntimeGateway provider registry, surface, and invocation boundary.
- `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs` — workspace-module runtime tool availability and RuntimeGateway dependency handling.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts` — frontend command hook consuming generated conversation commands.
- `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts` — chat UI command/mailbox prop surface.
- `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx` — composer/cancel enablement consumption.

### Code Patterns

- `RuntimeSession` is still correctly treated as delivery/trace substrate in most current read paths: `get_session_runtime_control` delegates to `presentation_read_model_query.session_runtime_control` and maps only meta, control plane, anchor, run, agent, frame runtime, and associations (`crates/agentdash-api/src/routes/sessions.rs:153`, `crates/agentdash-api/src/routes/sessions.rs:173`; contract shape at `crates/agentdash-contracts/src/runtime/workflow.rs:1419`).
- `AgentRunWorkspaceView` has been narrowed compared with 06-14: top-level `actions` and top-level `mailbox_messages` are gone from the contract; mailbox messages now live under `conversation.mailbox.messages` (`crates/agentdash-contracts/src/runtime/workflow.rs:1147`, `crates/agentdash-contracts/src/runtime/workflow.rs:1167`, `crates/agentdash-contracts/src/runtime/workflow.rs:1195`).
- The workspace route now derives top-level control plane from conversation status instead of maintaining a separate action set (`crates/agentdash-api/src/routes/lifecycle_agents.rs:1021`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1459`).
- RuntimeGateway itself remains a narrow provider registry/invocation boundary: static providers are surfaced by action kind, dynamic providers are only used during `invoke` support lookup (`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:65`, `crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:83`).

### Issue 1 - P1 - Launch command identity is duplicated across three layers and mapped in a loop

- Type: path redundancy / concept fork / abstraction leakage
- Evidence:
  - AgentRun defines `LaunchSource`, `LaunchCommand`, `LaunchModifier`, and source-specific constructors at `crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:159`, `crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:171`, and maps to `FrameLaunchCommand` at `crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:263`.
  - RuntimeSession defines the same source enum, command shape, modifiers, and mapping again at `crates/agentdash-application-runtime-session/src/session/launch/command.rs:11`, `crates/agentdash-application-runtime-session/src/session/launch/command.rs:23`, and `crates/agentdash-application-runtime-session/src/session/launch/command.rs:121`.
  - The frame launch port defines a third source enum and command DTO at `crates/agentdash-application-ports/src/frame_launch_envelope.rs:127` and `crates/agentdash-application-ports/src/frame_launch_envelope.rs:160`.
  - AgentRun -> RuntimeSession conversion is hand-mapped in `runtime_launch_command` (`crates/agentdash-application/src/runtime_session_agent_run_bridge.rs:202`).
  - RuntimeSession launch maps the command to `FrameLaunchCommand` before calling the envelope provider (`crates/agentdash-application-runtime-session/src/session/launch/orchestrator.rs:89`).
  - Frame construction converts `FrameLaunchCommand` back into application `LaunchCommand` (`crates/agentdash-application/src/frame_construction/mod.rs:285`).
- Impact:
  - Adding or changing one launch source requires synchronized edits in at least three enums and three mapping functions.
  - The shapes are not isomorphic: RuntimeSession `UserPromptInput.backend_selection` is used by launch planning (`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:264`) but is absent from `FrameLaunchUserInput`, so frame construction always reconstructs it as `None` (`crates/agentdash-application/src/frame_construction/mod.rs:305`). This is tolerable only while backend placement remains strictly planner-owned; it makes the launch-command contract easy to misunderstand.
  - The current flow is conceptually backwards for a single fact: AgentRun command -> RuntimeSession command -> FrameLaunchCommand -> AgentRun/application command. That makes frame construction look like a consumer of a transport DTO rather than the owner of launch-ready facts.
- Suggested convergence boundary:
  - Keep exactly one domain launch command/source model as the owner, preferably in the application port used between RuntimeSession launch and frame construction.
  - AgentRun and RuntimeSession adapters should construct that command directly, not maintain parallel source enums.
  - If backend placement remains planner-only, encode that intentionally as a separate `LaunchPlanningInput` field rather than relying on a non-isomorphic command DTO.
- 06-14 baseline:
  - This is a new/resurfaced issue after boundary extraction. The old report focused on over-thick SessionHub and mailbox/action projection; it did not identify the three-layer launch command loop.

### Issue 2 - P1 - Command/action availability still has multiple derivation owners

- Type: repeated projection / duplicate fact source / command availability owner drift
- Evidence:
  - `AgentRunWorkspaceQueryService::resolve` reads execution state, steering support, mailbox messages/state, model config, resource surface, then builds both `AgentRunWorkspaceProjection` and `AgentConversationSnapshot` (`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:136`, `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:158`, `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:162`, `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:213`).
  - `AgentRunWorkspaceProjection` independently derives state code, delivery status, active turn, last turn, and runtime command state from `SessionExecutionState` (`crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:12`, `crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:37`, `crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:93`).
  - `conversation_snapshot` independently derives conversation execution status and command enablement from the same execution/frame/mailbox/model facts (`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:596`, `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:623`).
  - `AgentRunWorkspaceCommandPolicyService` re-reads current delivery, execution state, frame, mailbox messages/state, model config, and resolves command availability again before accepting a command (`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:40`, `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:51`, `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:156`).
  - Frontend mostly consumes generated commands, but still carries workspace readiness and local cancel/send gates around those generated commands (`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:250`, `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:559`).
- Impact:
  - Current UI action availability is improved, but the backend still has at least two command availability calculations: conversation snapshot and command policy. If one changes without the other, stale guard rejection and UI enablement diverge.
  - `AgentRunWorkspaceProjection` still exists as a sibling execution projection even though conversation is the user-visible command/control surface. This keeps status naming duplicated (`starting_claimed`, `running_active`, `cancelling`) across projection and conversation code.
  - New command kinds must touch conversation snapshot, command policy, route mapping, contract mapping, and frontend command hook.
- Suggested convergence boundary:
  - Make `AgentConversationSnapshot` / `ConversationCommandAvailabilityResolver` the only command availability owner.
  - Let command policy validate durable precondition equality and current fact freshness by reusing the same resolver output, not by owning a separate query-and-derive path.
  - Keep `AgentRunWorkspaceProjection` only for shell/list status if needed; avoid using it as a second runtime command state model.
- 06-14 baseline:
  - Residual but reduced. The old top-level `actions` / `mailbox_messages` duplication has been removed from the contract, but the backend derivation split remains.

### Issue 3 - P1 - Mailbox steering has two consumption implementations with different terminal/error semantics

- Type: path redundancy / duplicate side-effect path / delivery fact drift
- Evidence:
  - Delegate boundary drain claims messages and calls `consume_as_delegate_steering` (`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:266`, `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:297`).
  - Scheduler route consumes the same conceptual steering delivery through `consume_as_steering` (`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:413`, `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:556`).
  - Both paths re-check active turn and expected turn mismatch (`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:305`, `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:564`).
  - Delegate steering writes `UserInputSubmitted` before marking `Steered`; event write failure marks the message `Failed` and completes receipt as terminal failed (`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:337`).
  - Normal steering calls `session_control.steer_session` first; if later event projection fails, it still marks the message `Steered` with `last_error` (`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:601`, `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:614`, `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:639`).
- Impact:
  - The same mailbox delivery class can be terminal-failed or accepted-with-error depending on whether it was drained inside the agent delegate or delivered through scheduler steering.
  - Receipt semantics, payload cleanup, event history, and frontend mailbox row status can diverge for equivalent user/system steering work.
  - This directly weakens the mailbox contract that durable envelope + scheduler outcome is the single command fact source.
- Suggested convergence boundary:
  - Extract one steering delivery executor that handles active turn validation, expected-turn guard, event emission, receipt completion, status write, and payload cleanup.
  - Delegate path should only choose the output shape (`Vec<AgentMessage>` returned to the loop) and should not own a separate status/receipt/event policy.
- 06-14 baseline:
  - Residual. The old report flagged `consume_as_delegate_steering` and `consume_as_steering` duplication; current code still has both and now shows a concrete error-semantics split.

### Issue 4 - P1 - AgentRuntimeDelegate remains too wide for mailbox turn-boundary work

- Type: module over-thickness / abstraction leakage / horizontal coupling
- Evidence:
  - `AgentRuntimeDelegate` still requires compaction, context transform, tool call policy, after-turn, before-stop, and provider request observer methods in one trait (`crates/agentdash-agent-types/src/runtime/delegate.rs:25`).
  - `AgentRunMailboxRuntimeDelegate` only owns mailbox turn-boundary behavior, but must forward compaction, context transform, tool call, and provider observer calls to an inner delegate (`crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:357`).
  - Runtime launch planner wraps the hook runtime delegate inside the mailbox runtime delegate based on the mailbox port (`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:146`, `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:161`).
  - Mailbox-specific behavior is only in after-turn/before-stop routing and draining (`crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:424`, `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:469`).
- Impact:
  - A mailbox turn-boundary concern must know about compaction, tool call policy, context transform, and provider observers.
  - Adding any new agent-loop delegate hook forces mailbox wrappers to participate even if mailbox has no semantic stake.
  - Hook runtime ordering versus mailbox routing is hidden in wrapper composition rather than expressed as a structured delegate set.
- Suggested convergence boundary:
  - Split `AgentRuntimeDelegate` into smaller traits or a structured delegate set: context transform, tool policy, compaction, turn boundary, provider observer.
  - Mailbox should implement only turn-boundary scheduling/drain.
  - Hook runtime may implement multiple facets, but composition should be explicit in `LaunchPlan` / prepared turn.
- 06-14 baseline:
  - Residual. The over-wide delegate and wrapper forwarding problem remains.

### Issue 5 - P2 - Runtime action availability is split between capability state, workspace-module provider dependency checks, and RuntimeGateway provider support

- Type: action availability owner drift / abstraction leakage
- Evidence:
  - RuntimeGateway `surface_for_actor` lists only registered static providers matching the context action kind (`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:65`), while `invoke` can also use dynamic providers selected by `supports` (`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:90`).
  - Workspace module tool provider first checks `CapabilityState` for `workspace_module_invoke` (`crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:245`), then independently checks RuntimeGateway, extension transport, and runtime backend anchor availability (`crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:292`, `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:311`, `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:353`).
  - If dependencies are missing, the provider still exposes a `workspace_module_invoke` diagnostic tool that executes to an error (`crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:88`, `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:131`, `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:304`).
  - Generic runtime action tool adapter invokes the gateway using a prebuilt `RuntimeActionToolSpec`; it does not own capability/current-surface resolution (`crates/agentdash-application-runtime-gateway/src/runtime_gateway/tool_adapter.rs:16`, `crates/agentdash-application-runtime-gateway/src/runtime_gateway/tool_adapter.rs:97`).
- Impact:
  - The agent-visible action set can say “tool exists” while the real runtime action plane says “provider/dependency unavailable.”
  - RuntimeGateway is invocation authority, but not the single availability projection authority. Workspace-module provider decides when to expose or degrade the tool based on local dependency checks.
  - Dynamic providers can be invokable but invisible to `surface_for_actor`, so any future UI/API that treats gateway surface as action availability will miss actions.
- Suggested convergence boundary:
  - Treat `CapabilityState` as admission input, RuntimeGateway provider support as action availability, and current AgentRun runtime surface as dependency closure.
  - Do not expose diagnostic tools as normal action tools in production launch surfaces; missing runtime dependencies should fail launch readiness or omit the action with a typed diagnostic in conversation/resource diagnostics.
  - If dynamic providers are product actions, include their descriptors in a context-aware availability surface or explicitly keep them out of query surfaces.
- 06-14 baseline:
  - Related but sharper. 06-14 said RuntimeGateway invocation was mostly healthy and dynamic surface manifest could be separate; current evidence shows action availability is still split at the workspace-module/session boundary.

### Baseline Resolution Notes

- 06-14 P1 RuntimeSession runtime-control duplicate mailbox/action projection: resolved enough for this scope. Current contract has no mailbox/action fields (`crates/agentdash-contracts/src/runtime/workflow.rs:1419`), and route maps only read-model fields (`crates/agentdash-api/src/routes/sessions.rs:173`).
- 06-14 P1 SessionRuntimeInner mailbox boundary silent no-op: resolved enough for this scope. Current ready gate explicitly rejects missing `mailbox_runtime_port` (`crates/agentdash-application-runtime-session/src/session/hub/factory.rs:363`).
- 06-14 P2 direct `AgentRunSteeringService`: product path removed; remaining references are test support only (`crates/agentdash-application/src/test_support/agent_run_steering.rs:30`).
- 06-14 P2 mailbox service over-thickness: partially resolved by file split, but the service constructor remains a wide dependency aggregator (`crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs:66`) and steering duplication remains.

### External References

- No external references used. This review is based on internal Trellis specs, 06-14 baseline artifacts, and current business code.

### Related Specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/session/bundle-main-datasource.md`
- `.trellis/spec/cross-layer/architecture.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task; this research used the user-provided task directory and the user-specified single output file.
- No business code was modified and no full test suite was run.
- This was a single-domain review. RuntimeGateway findings were limited to its Agent Runtime Session Surface boundary; extension runtime/workspace-module domain details should be reviewed in that module's own pass.
- I did not find current product-path `AgentRunSteeringService` usage outside test support.
- I did not find the 06-14 runtime-control mailbox/action duplication in current `SessionRuntimeControlView`.
