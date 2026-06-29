# Design Backlog Review

## Summary

本评估覆盖 D1-D12。Quick convergence 已经清掉一批错误路径：tool-level grant 不再污染 visible `CapabilityState`，extension schema/workspace/loadability 统一，VFS/local guard rails 落地，mailbox steering 统一 delivery executor，旧 `user_preferences` 迁入 scoped settings。

剩余 Design backlog 的共同方向是 owner 收束：把运行事实、可见 surface、执行准入、delivery intent、workspace directory fact 和 local runtime profile/claim 分别归还给唯一 owner。除 D3 / D4 需要用户确认长期 owner 选择外，其余项可以按推荐方案自行推进实现设计。

## Decision Matrix

| Item | Topic | Decision State | Recommended Owner |
| --- | --- | --- | --- |
| D1 | AgentRun visible capability / admission production boundary | self-decided | `agentdash-application-agentrun` effective capability/admission service |
| D2 | LifecycleDispatchService owner split | self-decided | Lifecycle facade + internal owner services |
| D3 | CompanionGate resolver / delivery adapters | user-decision-required | Shared `LifecycleGateResolver` recommended |
| D4 | Launch command/source single model | user-decision-required | Canonical launch command in application ports recommended |
| D5 | Command availability resolver / command policy | self-decided | `ConversationCommandAvailabilityResolver` |
| D6 | AgentRuntimeDelegate delegate set | self-decided | Agent runtime delegate facets in `agentdash-agent-types` |
| D7 | RuntimeGateway dynamic extension action discovery | self-decided | RuntimeGateway dynamic action catalog |
| D8 | Runtime action availability layer split | self-decided | AgentRun visibility + RuntimeGateway catalog + WorkspaceModule diagnostics |
| D9 | VFS per-mount/path authorization | self-decided | Runtime VFS access policy projection |
| D10 | WorkspacePlacementService directory fact transaction | self-decided | Application-level `WorkspacePlacementService` |
| D11 | Desktop profile/claim/settings down into `agentdash-local` | self-decided | `agentdash-local` local runtime durable facts |
| D12 | Relay prompt typed payload | self-decided | Relay protocol using canonical `UserInputBlock` |

## D1. AgentRun Visible Capability / Admission

Decision State: `self-decided`.

### Boundary

AgentRun owns final visible capability view and execution admission. RuntimeSession/tool assembly consumes this boundary; provider-level `CapabilityState` checks remain declarative exposure checks, not Grant admission.

### Evidence

- `AgentRunEffectiveCapabilityPort::admit_tool` exists in `crates/agentdash-application-ports/src/agent_run_surface.rs`.
- Quick convergence changed runtime projection to frame-scoped grants and stopped mutating visible `CapabilityState`.
- Product tool invocation still does not call `admit_tool`; agent loop execution enters through `delegate.before_tool_call`.

### Convergence

Implement a production `AgentRunEffectiveCapabilityPort` and wire tool invocation admission through the runtime delegate/tool-policy entry. Delete the idea that `RuntimeSessionEffectiveCapabilityPort::execution_capability_state_for_runtime_session` is an authorization boundary.

### Implementation Shape

1. Add product implementation of `AgentRunEffectiveCapabilityPort`.
2. Replace capability-state-only runtime session port with an effective view or direct AgentRun port consumption.
3. Add a tool-admission adapter at launch/prepared-turn construction.
4. Ensure denied admission prevents `tool.execute`.
5. Keep provider `CapabilityState` checks as visible-tool/local invariant checks only.

### Validation

- Tests for visible state unchanged by tool-level grants.
- Frame-scoped admission tests.
- Agent loop test proving denied `admit_tool` prevents execution.
- Static check that production code calls `admit_tool`.

## D2. LifecycleDispatchService Owner Split

Decision State: `self-decided`.

### Boundary

Keep a public Lifecycle dispatch facade, but split internal owners for run/orchestration start, subject association, runtime materialization, relation/gate writing, and reducer bridge.

### Evidence

- `LifecycleDispatchService` currently owns graph planning, run/orchestration mutation, agent/session/frame materialization, subject association, gate, lineage, anchor, delivery binding, and reducer writes.
- `start_lifecycle_run` is already a narrower orchestration starter, proving internal split is viable.

### Convergence

Thin `dispatch_common` into orchestration over owner services:

- `RunOrchestrationStarter`
- `SubjectAssociationWriter`
- `AgentRuntimeMaterializer`
- `LifecycleRelationWriter`
- `OrchestrationReducerBridge`

### Implementation Shape

1. Extract run/orchestration starter from existing start path.
2. Extract materializer for agent/session/frame/anchor/delivery.
3. Extract association and relation/gate writers.
4. Extract reducer bridge for `NodeStarted`.
5. Collapse duplicate plain/graph helper names after materialization owns distinctions.

### Validation

- Existing dispatch tests remain green.
- Owner-level unit tests with fake repos.
- Graph-backed regression for matching node coordinate across materialization, reducer and anchor.

## D3. CompanionGate Resolver / Delivery Adapters

Decision State: `user-decision-required`.

### Boundary

`LifecycleGate` should be the durable gate fact. Mailbox/session event delivery should be adapter-owned side effects driven by typed delivery intents.

### Evidence

- `CompanionGateControlService` mixes gate repo, run/frame/agent/anchor/lineage lookup, session notifications, parent mailbox and human response mailbox.
- A simple human response route constructs the full service.
- Gate payload currently stores delivery status blobs, mixing gate fact with mailbox receipt projection.

### Convergence

Recommended shape:

- `LifecycleGateResolver` owns validation and transition.
- `CompanionGateIntentResolver` resolves parent/child/human context.
- `CompanionMailboxDeliveryAdapter` creates mailbox commands from intents.
- `CompanionSessionEventAdapter` injects best-effort notifications.

### User Decision

Use a shared `LifecycleGateResolver` for companion, workflow HumanGate and future routine gates, or do a companion-only split first. Recommendation: shared resolver, because `LifecycleGate` is already a workflow domain fact and this prevents another human-gate language.

### Implementation Shape

1. Add `GateTransitionOutcome` and `GateDeliveryIntent`.
2. Move pure gate transitions into resolver.
3. Move runtime trace/current-frame lookup into context resolver.
4. Move mailbox writes into delivery adapters.
5. Stop writing delivery status blobs into gate payload.
6. Thin `companion_gates.rs` to a narrow human-response use case.

### Validation

- Resolver tests for open/respond/resolve transitions and invalid owner/closed gate cases.
- Adapter tests for human response, parent request and parent response mailbox intents.
- Companion gate route test proving simple human response no longer constructs delivery-only dependencies.
- Static check that gate payload no longer stores mailbox delivery status blobs.

## D4. Launch Command / Source Single Model

Decision State: `user-decision-required`.

### Boundary

Launch intent should have one canonical model. Frame construction consumes launch intent and emits `FrameLaunchEnvelope`; launch planning owns backend placement inputs.

### Evidence

- AgentRun, RuntimeSession and FrameLaunch ports each define source/command/modifier models.
- Bridge code maps AgentRun -> RuntimeSession -> FrameLaunch -> application command.
- `backend_selection` is planner-owned but gets obscured by the command DTO loop.

### Convergence

Recommended model in `agentdash-application-ports`:

- `LaunchCommand`: source intent, prompt input, identity, modifiers.
- `LaunchPlanningInput`: backend selection and planner-only overrides.

Delete duplicate source/command enums and mapping functions.

### User Decision

Choose canonical owner:

- Recommended: application ports. This matches source adapter -> frame construction -> planner.
- Alternative: RuntimeSession owns it, but that makes delivery substrate own source identity.
- Alternative: AgentRun owns it, but non-AgentRun launch sources would depend on AgentRun.

### Implementation Shape

1. Add canonical `LaunchCommand`, `LaunchSource`, `LaunchModifier` and `LaunchPlanningInput` in the chosen owner.
2. Update `FrameLaunchEnvelopeRequest` and RuntimeSession launch to pass the canonical command directly.
3. Remove AgentRun/RuntimeSession/FrameLaunch duplicate command/source/modifier models.
4. Delete `runtime_launch_command`, `to_frame_launch_command` and `launch_command_from_frame_launch` mapping loops.
5. Keep backend selection as planner input, not launch command identity.

### Validation

- Static check that only one production `LaunchCommand` / `LaunchSource` model remains.
- Focused launch pipeline tests for AgentRun, local relay, workflow/routine and companion launch sources.
- Frame construction tests proving envelope facts and backend planning input remain distinct.

## D5. Command Availability Resolver / Policy

Decision State: `self-decided`.

### Boundary

`ConversationCommandAvailabilityResolver` is the only server-side command availability derivation. Command policy re-resolves and validates the same model.

### Evidence

- Conversation snapshot already owns command list, stale guards and command preconditions.
- Command policy already consumes the resolver.
- Workspace projection still has parallel runtime command state/status fields.
- Frontend still has local workspace-ready gates around command handlers.

### Convergence

Delete/narrow `runtime_command_state` and keep workspace shell status as display-only. Frontend local checks should be UX guards only; semantic command availability comes from backend command objects and stale guard.

### Implementation Shape

1. Remove or rename command-state fields from workspace projection.
2. Move remaining state decisions into resolver.
3. Update frontend handlers to rely on backend command enablement and stale response.
4. Add tests that all command routes require precondition.

### Validation

- Resolver snapshot tests for idle/running/cancelling/terminal command sets.
- Command policy tests for stale guard mismatch, disabled command and terminal rejection.
- Frontend handler tests proving semantic availability comes from backend command objects.
- Static check that `runtime_command_state` is not used as command authority.

## D6. AgentRuntimeDelegate Delegate Set

Decision State: `self-decided`.

### Boundary

Runtime delegate concerns should be facets, not one broad trait.

### Evidence

- `AgentRuntimeDelegate` covers compaction, context transform, tool policy, after-turn, before-stop and provider observers.
- Mailbox adapter only owns turn boundary but forwards unrelated methods.
- Launch planner encodes hook/mailbox ordering by nested wrappers.

### Convergence

Introduce `AgentRuntimeDelegateSet` with facets:

- `RuntimeCompactionDelegate`
- `RuntimeContextTransformDelegate`
- `RuntimeToolPolicyDelegate`
- `RuntimeTurnBoundaryDelegate`
- `RuntimeProviderObserverDelegate`

D1 admission belongs in tool policy; mailbox only implements turn boundary.

### Implementation Shape

1. Add facet traits and delegate set.
2. Update agent loop call sites to invoke relevant facets.
3. Convert hook runtime delegate into facets.
4. Convert mailbox adapter into turn-boundary-only.
5. Make launch/prepared-turn composition explicit.

### Validation

- Agent loop tests for compaction, context transform, tool policy, turn boundary and provider observer facets.
- Mailbox tests proving it only implements turn boundary behavior.
- Ordering tests for hook tool policy and D1 admission short-circuit.
- Static check that broad forwarding wrappers are removed from mailbox delegate code.

## D7. RuntimeGateway Dynamic Action Discovery

Decision State: `self-decided`.

### Boundary

RuntimeGateway owns runtime action discovery because it already owns invocation and actor/context validation.

### Evidence

- `surface_for_actor()` only traverses static providers.
- `invoke()` can route dynamic extension providers.
- Extension dynamic provider has a marker descriptor, while concrete extension actions are discovered elsewhere.
- WorkspaceModule and frontend use Project extension runtime projection as action availability.

### Convergence

Dynamic providers participate in `RuntimeGateway::surface_for_actor()`. Extension provider emits concrete action descriptors from enabled installations. The marker `extension.runtime_action` stops being actor-visible.

### Implementation Shape

1. Add dynamic discovery contract.
2. Make ExtensionRuntimeActionProvider emit concrete descriptors.
3. Reuse the same resolver for catalog and invoke.
4. Make WorkspaceModule operations come from Gateway action catalog.
5. Update frontend bridge to consume session runtime action surface or rely on backend denial.

### Validation

- RuntimeGateway surface tests for concrete enabled extension action descriptors.
- Invoke/catalog consistency tests for disabled actions, missing context and artifact/readiness failures.
- Static check that actor-visible `extension.runtime_action` marker expectations are gone.

## D8. Runtime Action Availability Layers

Decision State: `self-decided`.

### Boundary

Split availability into three non-overlapping owners:

- AgentRun effective capability: visibility.
- RuntimeGateway action catalog: executable action support.
- WorkspaceModule/Extension presentation: readiness diagnostics.

### Evidence

- WorkspaceModule tool provider gates by `CapabilityState`.
- RuntimeGateway invokes by dynamic provider support.
- WorkspaceModule descriptor maps manifest runtime actions directly.
- Missing runtime dependencies appear as diagnostic tools.

### Convergence

Keep diagnostics, but source action operations from RuntimeGateway catalog and use explicit readiness enum. Missing gateway/channel/backend/artifact is a typed diagnostic, not launch readiness failure.

### Implementation Shape

1. Feed WorkspaceModule runtime operations from RuntimeGateway action catalog instead of raw manifest projection.
2. Preserve AgentRun effective capability as visibility owner only.
3. Introduce explicit readiness diagnostics for runtime gateway, channel, backend and artifact conditions.
4. Update frontend bridge to consume session runtime action surface or accept backend typed denial.
5. Split naming so capability visibility, renderer loadability and invocation readiness do not share one `available` meaning.

### Validation

- WorkspaceModule list/describe tests for Gateway-backed operations and UI-only tab presentation.
- Frontend bridge tests for session action surface or backend denial.
- Diagnostic tests for missing gateway/channel/backend/artifact without treating them as launch readiness failures.

## D9. VFS Per-Mount / Path Authorization

Decision State: `self-decided`.

### Boundary

Project VFS preset grants are not generic VFS authorization. Generic VFS admission should be a runtime access policy compiled from AgentRun/frame/Permission facts and enforced after VFS path normalization.

### Evidence

- `AgentVfsAccessGrant` only has mount id and mount capabilities.
- Current grant pruning skips non-project mounts.
- Mount capabilities express provider support, not runtime admission.
- Tool-level capability checks cannot answer mount/path questions.

### Convergence

Introduce `RuntimeVfsAccessPolicy`:

- mount id / surface ref
- path pattern
- operations
- source

Effective access is tool capability enabled + mount supports operation + policy admits normalized path.

### Implementation Shape

1. Rename/narrow current Project VFS grant.
2. Define runtime VFS access policy and path patterns.
3. Compile project grants into policy.
4. Enforce policy in VFS tool resolution.
5. Extend PermissionGrant projection to emit VFS path rules.

### Validation

- Policy compiler tests preserving current Project VFS grant behavior.
- VFS tool tests for normalized path allow/deny across read, write, search and shell.
- Permission tests proving tool-level grants do not expand mount/path access.
- Mount discovery tests clarifying provider capability versus effective authorization.

## D10. WorkspacePlacementService

Decision State: `self-decided`.

### Boundary

`workspace.detect -> WorkspaceDirectoryFact -> inventory/binding` is one placement transaction owned by application layer.

### Evidence

- `WorkspaceDirectoryFact` helpers already exist.
- Manual register route directly invokes detect and upserts inventory.
- bind-discovered route separately invokes detect, validates identity and writes inventory/binding.
- workspace create/update routes also derive binding shape.
- Duplicate `invoke_workspace_detect` helpers exist in routes.

### Convergence

Create application-level `WorkspacePlacementService` with explicit intents:

- ManualRegisterInventory
- BindDiscovered
- CreateOrUpdateWorkspace
- SyncCandidateInventory
- AdvancedBindingOnly

Routes become auth/DTO adapters.

### Implementation Shape

1. Move detect invocation to placement service.
2. Wrap existing directory fact helpers.
3. Convert manual register route.
4. Convert bind-discovered batch.
5. Convert workspace create/update hydration.
6. Remove route-local detect helpers.

### Validation

- Placement service tests for manual register, bind-discovered, sync, create/update and advanced binding-only intents.
- Identity mismatch and inactive access error tests with stable error classes.
- API adapter tests proving routes only handle auth/DTO mapping.
- Static check that route-local `invoke_workspace_detect` helpers are removed.

## D11. Desktop Profile / Claim / Settings Into agentdash-local

Decision State: `self-decided`.

### Boundary

`agentdash-local` owns local runtime durable facts and enrollment. Tauri shell owns OS/window/tray/autostart adapters.

### Evidence

- Tauri main still defines runtime start/profile/settings DTOs and file IO.
- Tauri main performs desktop ensure HTTP claim and response validation.
- `agentdash-local` already owns machine identity, runner claim and desktop runner lifecycle.
- Frontend bridge still normalizes auto-connect profile defaults.

### Convergence

Move desktop profile/settings/claim DTOs and logic into `agentdash-local` modules. Tauri commands forward to local library functions.

### Implementation Shape

1. Move DTOs to `agentdash-local`.
2. Move profile/settings load/save/delete.
3. Move desktop access-token ensure client and response validation.
4. Build `LocalRuntimeConfig` from local claim/profile code.
5. Reduce Tauri main to shell concerns.

### Validation

- `agentdash-local` tests for profile/settings roundtrip, claim response validation and runtime config projection.
- Tauri command compile check proving commands delegate to local library functions.
- Frontend local runtime bridge tests for profile load/save/start without claim-internal assumptions.
- Regression test that standalone runner claim remains unchanged.

## D12. Relay Prompt Typed Payload

Decision State: `self-decided`.

### Boundary

Relay prompt should use canonical `UserInputBlock`; ACP ContentBlock conversion belongs only at ACP/model adapter edges.

### Evidence

- RuntimeSession and AgentRun launch already use `Vec<UserInputBlock>`.
- Relay app port and wire payload still carry raw JSON `prompt_blocks`.
- Cloud converts canonical input to ACP JSON; local parses ACP JSON back to canonical input.

### Convergence

Replace relay prompt `prompt_blocks` with typed `input: Vec<UserInputBlock>`. Delete paired relay ACP conversions.

### Implementation Shape

1. Change relay/app transport DTOs.
2. Pass typed input directly from cloud to local.
3. Remove conversion helpers and replace tests with typed relay serialization tests.
4. Audit remaining `ContentBlock` references.

### Validation

- Relay protocol serde tests for text, image, skill and mention `UserInputBlock` values.
- Cloud/local handler tests proving typed input passes through without ACP JSON conversion.
- Static check that relay/app transport/local prompt code no longer uses `prompt_blocks`.
- Agent protocol conversion tests remain the only model-boundary `UserInputBlock` to model content checks.

## Cross-Item Order

Recommended implementation order:

1. D12 typed relay prompt payload.
2. D7 dynamic RuntimeGateway action catalog.
3. D8 action availability split.
4. D1 production admission wiring.
5. D5 command availability cleanup.
6. D6 delegate set split.
7. D9 VFS access policy.
8. D10 WorkspacePlacementService.
9. D11 desktop local ownership.
10. D4 launch command model.
11. D3 shared gate resolver.
12. D2 lifecycle dispatch internal owner split.

The last three are sequenced last because they affect transaction boundaries and public control-plane language.
