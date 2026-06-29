# Research: Orchestration / gate / launch design

- Query: D2 LifecycleDispatchService owner split, D3 CompanionGate resolver and delivery adapter split, D4 launch command/source single model; focus on first-principles owner convergence and deletion/thinning of duplicate paths.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-design-backlog-review/implement.jsonl` - curated context manifest for the design backlog task.
- `.trellis/tasks/06-30-design-backlog-review/prd.md` - D1-D12 acceptance criteria and required decision-state format.
- `.trellis/tasks/06-30-design-backlog-review/design.md` - common review template and ordering that places D4/D3/D2 as the final integrated items.
- `.trellis/tasks/06-30-design-backlog-review/implement.md` - research dispatch requirements and validation expectations.
- `.trellis/tasks/06-30-module-adversarial-review/followups/design-backlog.md` - canonical backlog entries for D2/D3/D4.
- `.trellis/tasks/06-30-module-adversarial-review/adversarial-review.md` - original issue evidence for LifecycleDispatchService, CompanionGateControlService, and LaunchCommand duplication.
- `.trellis/tasks/06-30-module-adversarial-review/research/05-orchestrated-work-surface.md` - prior orchestration/gate evidence.
- `.trellis/tasks/06-30-module-adversarial-review/research/06-agent-runtime-session-surface.md` - prior AgentRun/RuntimeSession launch evidence.
- `.trellis/spec/backend/workflow/architecture.md` - LifecycleRun, OrchestrationInstance, reducer, dispatch materialization, and RuntimeSession anchor contracts.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - RuntimeSession -> anchor -> Lifecycle control-plane lookup contract.
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession as delivery/trace substrate and launch pipeline invariants.
- `.trellis/spec/backend/session/session-startup-pipeline.md` - LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan source-adapter contract.
- `.trellis/spec/backend/session/runtime-execution-state.md` - mailbox, runtime state, and current surface boundaries.
- `.trellis/spec/backend/session/execution-context-frames.md` - connector-facing ExecutionContext projection boundary.
- `.trellis/spec/backend/session/agentrun-mailbox.md` - mailbox as durable AgentRun delivery queue and source identity contract.
- `.trellis/spec/cross-layer/architecture.md` and `.trellis/spec/cross-layer/frontend-backend-contracts.md` - generated DTO and cross-layer contract owner baseline.
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` - D2 target service.
- `crates/agentdash-application/src/companion/gate_control.rs` - D3 target service.
- `crates/agentdash-api/src/routes/companion_gates.rs` - D3 API entry proving full service construction for a simple human response.
- `crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs` - AgentRun-side D4 duplicated launch model.
- `crates/agentdash-application-runtime-session/src/session/launch/command.rs` - RuntimeSession-side D4 duplicated launch model.
- `crates/agentdash-application-ports/src/frame_launch_envelope.rs` - FrameLaunch port command/source model.
- `crates/agentdash-application/src/runtime_session_agent_run_bridge.rs` - AgentRun -> RuntimeSession launch mapping.
- `crates/agentdash-application-runtime-session/src/session/launch/orchestrator.rs` - RuntimeSession -> FrameLaunchEnvelope provider mapping.
- `crates/agentdash-application/src/frame_construction/mod.rs` - FrameLaunchCommand -> application LaunchCommand reverse mapping.

### Cross-item Convergence

D2/D3/D4 are one integrated owner problem, not three isolated refactors:

- D4 must make launch intent a single command fact. The session spec already defines `LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan`; current code keeps AgentRun, RuntimeSession, and FrameLaunch variants in parallel.
- D3 must make `LifecycleGate` the durable gate fact and move mailbox/session delivery into adapters. Current companion gate code writes gate payload, mailbox result, event notification, runtime trace resolution, and gate resolution in one service.
- D2 must keep lifecycle dispatch as a public facade but stop making it the owner of graph planning, run/orchestration mutation, subject association, agent/frame/session materialization, gate/lineage write, anchor write, and reducer bridge all at once.

Recommended first-principles target:

```text
source adapter
  -> canonical LaunchCommand + optional LaunchPlanningInput
  -> FrameConstructionService builds FrameLaunchEnvelope
  -> LaunchPlanner builds LaunchPlan

LifecycleDispatchFacade
  -> RunOrchestrationStarter
  -> SubjectAssociationWriter
  -> AgentRuntimeMaterializer
  -> Gate/Lineage writer
  -> OrchestrationReducerBridge(NodeStarted)

LifecycleGateResolver
  -> GateTransitionOutcome { gate facts, delivery intents }
  -> Mailbox / session-event adapters consume intents
```

Old paths that should be deleted or thinned:

- Delete the local `LaunchSource` / `LaunchCommand` / `LaunchModifier` copies in AgentRun and RuntimeSession once callers construct the canonical command directly.
- Delete `FrameLaunchCommand` / `FrameLaunchSource` / `FrameLaunchModifier` as a separate model; `FrameLaunchEnvelopeRequest` should accept the canonical command.
- Delete `runtime_launch_command`, `LaunchCommand::to_frame_launch_command`, and `launch_command_from_frame_launch`; they are evidence of a command loop rather than useful boundaries.
- Thin `CompanionGateControlService` into a compatibility facade over gate resolver + delivery adapters, or delete it after route/tool callers use the narrower services.
- Thin `LifecycleDispatchService::dispatch_common` into a coordinator that composes owner services and result types; it should not contain direct side-effect policy for every owner.

### D4. Launch command/source single model

Decision status: `user-decision-required`.

This is an architectural owner choice. The recommendation is strong, but choosing the canonical crate and the placement of backend selection affects public application ports and follow-up tasks.

#### Code Evidence

- AgentRun defines its own `LaunchSource`, `LaunchCommand`, `LaunchModifier`, and constructors (`crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:159`, `:171`, `:180`, `:333`, `:346`, `:363`, `:376`).
- RuntimeSession defines the same model again (`crates/agentdash-application-runtime-session/src/session/launch/command.rs:11`, `:23`, `:32`, `:197`, `:210`, `:227`, `:240`).
- Frame launch port defines a third source/command/modifier model (`crates/agentdash-application-ports/src/frame_launch_envelope.rs:127`, `:152`, `:160`, `:175`).
- AgentRun maps to RuntimeSession in `runtime_launch_command` (`crates/agentdash-application/src/runtime_session_agent_run_bridge.rs:202`).
- RuntimeSession maps to FrameLaunch in `command.to_frame_launch_command()` before calling the envelope provider (`crates/agentdash-application-runtime-session/src/session/launch/orchestrator.rs:90`, `:92`).
- Frame construction maps the FrameLaunch command back into application `LaunchCommand` (`crates/agentdash-application/src/frame_construction/mod.rs:293`, `:312`, `:319`, `:334`, `:349`).
- `backend_selection` exists on AgentRun/RuntimeSession `UserPromptInput` and is consumed by the RuntimeSession launch planner (`crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:30`; `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:275`, `:293`), but FrameLaunch reconstruction drops it to `None` (`crates/agentdash-application/src/frame_construction/mod.rs:309`).

#### Current Error Path

The current launch path forms a loop:

```text
AgentRun LaunchCommand
  -> RuntimeSession LaunchCommand
  -> FrameLaunchCommand
  -> application/AgentRun LaunchCommand
  -> FrameLaunchEnvelope
```

This makes the frame construction boundary look like a transport DTO consumer, even though specs say it owns launch-ready facts. New launch sources must be added in at least three enums and three mapping functions. Non-isomorphic fields such as `backend_selection` hide contract intent: the value is planner-owned today, but the DTO loop makes it look like frame construction accidentally loses it.

#### Owner / Contract Recommendation

Recommended owner: `agentdash-application-ports::launch_command` or the existing `frame_launch_envelope` port module after renaming. This crate already sits between RuntimeSession launch and FrameConstruction, and can be consumed by AgentRun adapters without making RuntimeSession the domain owner.

Contract shape:

```rust
pub struct LaunchCommand {
    pub source: LaunchSource,
    pub prompt: LaunchPromptInput,
    pub follow_up_session_id: Option<String>,
    pub identity: Option<AuthIdentity>,
    pub modifiers: Vec<LaunchModifier>,
}

pub struct LaunchPromptInput {
    pub input: Option<Vec<UserInputBlock>>,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: Option<AgentConfig>,
}

pub struct LaunchPlanningInput {
    pub backend_selection: Option<BackendSelectionRequest>,
}
```

`LaunchCommand` expresses source intent. `LaunchPlanningInput` expresses planner-only overrides such as backend selection. `FrameLaunchEnvelopeRequest` should carry both when needed, while `FrameConstructionService` consumes only the command fields it owns and records consumed runtime facts in the envelope resolution trace.

#### Serious Choice

- Option A: Canonical model in `agentdash-application-ports` and backend selection split into `LaunchPlanningInput`.
  - Recommendation: choose this.
  - Why: matches the current source-adapter -> frame construction -> planner spec, removes three enum copies, and makes backend placement explicitly planner-owned.
- Option B: RuntimeSession owns `LaunchCommand`, AgentRun and frame construction import/re-export it.
  - Tradeoff: fewer files initially, but makes RuntimeSession the owner of source identity even though specs say it is delivery/trace substrate.
- Option C: AgentRun owns `LaunchCommand`, RuntimeSession imports it.
  - Tradeoff: aligns with AgentRun workspace commands but incorrectly makes non-AgentRun launch sources such as local relay/routine/workflow depend on AgentRun.

#### Implementation Slices

1. Introduce canonical `LaunchCommand`, `LaunchSource`, `LaunchModifier`, and `LaunchPromptInput` in application ports. Add constructor helpers there or near source adapters.
2. Change `FrameLaunchEnvelopeRequest.command` to the canonical type. Remove `FrameLaunchCommand` and its parallel source/modifier types.
3. Change RuntimeSession launch orchestrator to pass the canonical command directly to the envelope provider; remove `to_frame_launch_command`.
4. Change AgentRun runtime boundary to use/re-export the canonical command. Remove AgentRun-local source/modifier enums after callers compile.
5. Delete `runtime_launch_command` bridge mapping and make `SessionLaunchBridge` pass the canonical command through.
6. Move `backend_selection` out of command identity into planner input, or explicitly encode it as `LaunchPlanningInput` on the RuntimeSession launch entry. The planner remains the only consumer.
7. Add a grep/static test that only one `pub enum LaunchSource` and one `pub struct LaunchCommand` exist outside tests.

#### Verification Strategy

- Targeted grep: `rg -n "pub enum LaunchSource|pub struct LaunchCommand|FrameLaunchCommand|to_frame_launch_command|launch_command_from_frame_launch|runtime_launch_command" crates`.
- Unit tests in RuntimeSession launch command/planner for each source and modifier.
- Frame construction tests assert companion/routine/local relay modifiers still produce the same envelope intent and resolution trace.
- `cargo check -p agentdash-application-ports -p agentdash-application-runtime-session -p agentdash-application-agentrun -p agentdash-application` is enough for implementation validation; no full Rust workspace compile is required for the research/design step.

### D3. CompanionGate resolver and delivery adapters

Decision status: `user-decision-required`.

The split itself is self-decided by code shape, but the scope is a product/architecture choice: companion-only resolver versus a shared lifecycle gate resolver that also covers workflow HumanGate/routine gates.

#### Code Evidence

- `CompanionGateControlService` owns gate, run, frame, agent, anchor, lineage, session notification delivery, parent mailbox delivery, and human response mailbox delivery (`crates/agentdash-application/src/companion/gate_control.rs:346`).
- The same service handles direct human response (`:417`), child result completion (`:537`), parent request opening (`:727`), and parent response resolution (`:905`).
- Human response delivery is performed before resolving the gate and stores mailbox delivery results back into gate payload (`crates/agentdash-application/src/companion/gate_control.rs:487`, `:526`, `:527`).
- Parent request opens a `LifecycleGate`, then delivers to parent mailbox and writes delivery status into gate payload (`crates/agentdash-application/src/companion/gate_control.rs:791`, `:836`, `:875`).
- Parent response resolution validates runtime/frame ownership, delivers to child mailbox, then resolves the gate (`crates/agentdash-application/src/companion/gate_control.rs:1054`, `:1092`, `:1093`).
- API `POST /companion-gates/{gate_id}/respond` constructs the full service, including runtime session eventing and mailbox delivery, for a simple human response (`crates/agentdash-api/src/routes/companion_gates.rs:51`, `:60`, `:70`).

#### Current Error Path

Gate state and delivery mechanics are mixed in one payload:

```text
load LifecycleGate
  -> resolve runtime frame / lineage / current delivery
  -> create mailbox or inject event
  -> write delivery status into gate.payload_json
  -> sometimes gate.resolve(...)
  -> best-effort event notification
```

This makes it hard to answer "who changed the gate" without also understanding mailbox delivery, runtime trace resolution, and notification behavior. Delivery failures mutate gate payload as failure diagnostics, so gate payload becomes both durable decision fact and delivery receipt projection.

#### Owner / Contract Recommendation

Recommended owner split:

- `LifecycleGateResolver`: owns gate validation, transition, and resulting durable gate fact.
- `CompanionGateIntentResolver`: resolves companion-specific parent/child/human context into typed delivery intents, but does not execute delivery.
- `CompanionMailboxDeliveryAdapter`: converts delivery intents into AgentRun mailbox envelopes and returns mailbox refs/results.
- `CompanionSessionEventAdapter`: injects session notifications as best-effort side effects after durable gate/mailbox facts exist.

Contract shape:

```rust
pub enum LifecycleGateCommand {
    RespondHuman { gate_id, payload },
    OpenParentRequest { child_runtime_session_id, turn_id, payload, wait },
    ResolveParentRequest { request_id, parent_runtime_session_id, resolved_turn_id, payload },
    CompleteChildResult { child_runtime_session_id, resolved_turn_id, request_id, payload },
}

pub struct GateTransitionOutcome {
    pub gate: LifecycleGate,
    pub transition: GateTransitionKind,
    pub delivery_intents: Vec<GateDeliveryIntent>,
    pub notification_intents: Vec<GateNotificationIntent>,
}
```

Gate payload should store request/decision facts and stable delivery refs, not mailbox delivery status blobs. Mailbox status remains in mailbox rows/command receipt. Session event notification remains best-effort and should not decide whether a gate is resolved.

#### Serious Choice

- Option A: Companion-only resolver now; workflow HumanGate remains separate.
  - Tradeoff: fastest implementation, but it keeps a second human-gate language in workflow orchestration.
- Option B: Shared `LifecycleGateResolver` for companion, workflow HumanGate, and future routine gates; companion supplies adapters.
  - Recommendation: choose this.
  - Why: `LifecycleGate` is already a workflow domain fact, and specs treat orchestration HumanGate decisions as runtime node commands rather than route-local payloads. Shared resolver prevents another gate state language.
- Option C: Keep `CompanionGateControlService` public and only move methods to submodules.
  - Tradeoff: low churn but does not solve ownership; gate transitions and delivery policy would still share a facade-level mutable payload.

#### Implementation Slices

1. Introduce `GateTransitionOutcome` and `GateDeliveryIntent` types. Keep `CompanionGateControlService` as a facade initially.
2. Move pure gate validation/transition for `respond`, `open_parent_request`, and `resolve_parent_request` into `LifecycleGateResolver`.
3. Move runtime trace/current-frame/lineage lookup into a companion context resolver that returns typed parent/child/human targets.
4. Move mailbox calls behind delivery adapters. Gate resolver returns intents; adapters create mailbox envelopes and return delivery refs.
5. Stop writing delivery status blobs into `gate.payload_json`. Store only stable request/decision facts and optional mailbox refs.
6. Move session event injection behind notification adapter and treat failures as diagnostics only.
7. Convert `companion_gates.rs` to call the narrow human-response use case instead of constructing the full control service.

#### Verification Strategy

- Unit tests for gate resolver transitions with in-memory gate repo: open -> resolved, invalid owner, closed gate, malformed payload.
- Adapter tests for human response, parent request, and parent response delivery intents creating the expected mailbox command.
- API test for human gate response proving route no longer wires parent/child delivery dependencies.
- Targeted grep: `rg -n "gate\\.payload_json.*delivery|with_parent_mailbox_delivery_payload|with_human_mailbox_delivery_payload|gate\\.resolve|deliver_.*mailbox" crates/agentdash-application/src/companion`.
- No large compile required for design. Implementation should use focused `cargo check -p agentdash-application -p agentdash-api`.

### D2. LifecycleDispatchService internal owner split

Decision status: `self-decided`.

The code and specs point to one design: keep the public dispatch facade but split internal owners. No serious product choice is needed.

#### Code Evidence

- `LifecycleDispatchService` currently holds run, graph, agent, frame, association, gate, lineage, anchor, runtime session creation, frame construction, workflow node frame materialization, and graph planning dependencies (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:105`).
- `start_lifecycle_run` is already a narrow orchestration starter: it plans a graph, creates a run, ensures root orchestration, and persists the run without runtime session side effects (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:378`).
- `materialize_workflow_agent_node` creates agent identity, runtime session, workflow node frame, execution anchor, and delivery binding in one method (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:411`, `:491`).
- `dispatch_common` handles graph planning, run/orchestration update, agent creation, subject association, runtime session, frame, lineage, gate, anchor, delivery binding, and `NodeStarted` reducer application (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:529`, `:592`, `:615`).
- Plain dispatch has a similar agent/session/frame/lineage/gate/anchor path but without orchestration reducer (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:670`, `:714`).
- Frame creation is already delegated through frame construction ports (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:920`, `:955`), and gate creation is isolated enough to become a small owner (`:1032`).

#### Current Error Path

The current graph-backed path is:

```text
dispatch_common
  -> plan graph
  -> create/update LifecycleRun + OrchestrationInstance
  -> create/reuse LifecycleAgent
  -> create subject association
  -> create/attach RuntimeSession
  -> construct AgentFrame
  -> create lineage
  -> create LifecycleGate
  -> upsert RuntimeSessionExecutionAnchor
  -> bind agent delivery
  -> apply NodeStarted reducer
  -> update LifecycleRun
```

This path is correct in broad order but too thick as an owner. It also creates the execution anchor and delivery binding before `NodeStarted`; if the reducer fails, the system can have delivery evidence for a node still not transitioned to running.

#### Owner / Contract Recommendation

Keep `LifecycleDispatchService` / `LifecycleDispatchFacade` as the public use-case facade. Split internal owners:

- `RunOrchestrationStarter`: graph planning, run creation/reuse, `ensure_workflow_graph_orchestration`, and lifecycle-start-only flow.
- `SubjectAssociationWriter`: subject association creation and `SubjectExecutionRef`.
- `AgentRuntimeMaterializer`: agent creation/reuse, runtime session creation/attach, frame construction/materialization, execution anchor, delivery binding. It returns `MaterializedAgentRuntime`.
- `LifecycleRelationWriter`: lineage write and gate opening. Gate opening should later call the D3 resolver/opening port.
- `OrchestrationReducerBridge`: builds and applies `OrchestrationRuntimeEvent::NodeStarted` after materialization, and persists the updated `LifecycleRun`.

Result contracts:

```rust
pub struct MaterializedAgentRuntime {
    pub runtime_refs: AgentRuntimeRefs,
    pub delivery_runtime_ref: Option<Uuid>,
    pub node_start: Option<OrchestrationRuntimeEvent>,
}

pub struct DispatchAssemblyResult {
    pub run_id: Uuid,
    pub runtime_refs: AgentRuntimeRefs,
    pub delivery_runtime_ref: Option<Uuid>,
    pub gate_ref: Option<Uuid>,
    pub subject_execution_ref: Option<SubjectExecutionRef>,
}
```

`LifecycleDispatchService::dispatch_common` becomes orchestration of these owners, not the place that encodes every side-effect policy.

#### Implementation Slices

1. Extract `RunOrchestrationStarter` from `start_lifecycle_run`, `plan_workflow_graph`, `resolve_or_create_run`, and `ensure_workflow_graph_orchestration`. Keep tests for lifecycle start unchanged.
2. Extract `AgentRuntimeMaterializer` for plain dispatch and workflow node materialization. It should consume `RuntimePolicy`, source identity, and orchestration binding, then return refs and optional `NodeStarted`.
3. Extract `SubjectAssociationWriter` and `LifecycleRelationWriter` for association, lineage, and gate open. Gate writer should expose a narrow command so D3 can replace its internals.
4. Extract `OrchestrationReducerBridge` and move `apply_orchestration_event_to_run` out of `dispatch_common`.
5. Reorder graph-backed validation so `NodeStarted` can be built and checked against the orchestration binding before committing anchor/delivery where possible. If a single DB transaction is later introduced, these owner steps are the natural unit-of-work participants.
6. Collapse duplicate plain/graph helper names (`create_initial_frame` and `create_plain_initial_frame`) once materialization owns the distinction.
7. Keep public facade methods and response types stable for callers; the change is an internal owner split.

#### Verification Strategy

- Existing dispatch service tests should still cover plain dispatch, graph-backed subject execution, lifecycle start, workflow node materialization, and gate opening.
- Add unit tests per extracted owner with fake repos.
- Add graph-backed regression: materialization returns runtime refs with `orchestration_id + node_path + attempt`, reducer writes `NodeStarted`, ready queue is cleared, and anchor refs match the same coordinate.
- Targeted grep: `rg -n "apply_orchestration_event_to_run|RuntimeSessionExecutionAnchor::new_orchestration_dispatch|create_gate|create_subject_association|resolve_or_create_runtime_session" crates/agentdash-application-lifecycle/src/lifecycle`.
- Implementation check should use focused package checks such as `cargo check -p agentdash-application-lifecycle -p agentdash-application-workflow`; no full workspace compile is needed for the design research.

## External References

No external references used. This research is based on repository code, Trellis specs, and prior Trellis research only.

## Related Specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/cross-layer/architecture.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell. This file uses the explicit task/output path provided by the user: `.trellis/tasks/06-30-design-backlog-review/research/04-orchestration-gate-launch.md`.
- No business code was modified.
- No Rust compile or full test suite was run; this was targeted read/search research only.
- I did not review every workflow HumanGate API path in depth. D3's recommendation to share a gate resolver is based on `LifecycleGate` ownership and workflow specs; implementation should read `agentdash-application-workflow/src/orchestration/human_gate_launcher.rs` before changing HumanGate behavior.
