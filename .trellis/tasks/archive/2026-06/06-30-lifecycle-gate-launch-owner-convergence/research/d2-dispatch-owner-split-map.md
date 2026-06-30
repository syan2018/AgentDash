# Research: D2 LifecycleDispatchService owner split map

- Query: D2 LifecycleDispatchService owner split implementation map after D4 launch command convergence and D3 gate resolver convergence.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/prd.md` - task requirement source; D2 acceptance requires facade stability and owner split.
- `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/design.md` - target D2 owner list and graph-backed coordinate invariant.
- `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/implement.md` - D4 -> D3 -> D2 sequencing and validation gates.
- `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/implement.jsonl` - curated specs/research manifest read first for this research.
- `.trellis/tasks/archive/2026-06/06-30-design-backlog-review/research/04-orchestration-gate-launch.md` - prior D2/D3/D4 owner convergence evidence.
- `.trellis/spec/backend/workflow/architecture.md` - LifecycleRun, OrchestrationInstance, reducer, graph-backed dispatch, and AgentCall materialization contracts.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - LifecycleSubjectAssociation and RuntimeSessionExecutionAnchor lookup contract.
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession as delivery/trace substrate and anchor role.
- `.trellis/spec/backend/session/session-startup-pipeline.md` - launch pipeline and plain RuntimeSession dispatch contract.
- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox/delivery boundary; useful for D3/D2 delivery ownership.
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` - current D2 target service and most focused tests.
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_facade.rs` - public facade / port adapter that should remain stable.
- `crates/agentdash-application-lifecycle/src/lifecycle/mod.rs` - current lifecycle module exports.
- `crates/agentdash-application-lifecycle/src/repository_set.rs` - workflow AgentCall materialization adapter that constructs `LifecycleDispatchService`.
- `crates/agentdash-application-workflow/src/orchestration/agent_node_launcher.rs` - ready AgentCall launcher that calls lifecycle materialization then emits `NodeStarted`.
- `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs` - workflow launcher persistence and AgentCall/HumanGate tests.
- `crates/agentdash-application-workflow/src/orchestration/runtime.rs` - reducer contract for `NodeStarted`, trace refs, and ready queue clearing.
- `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs` - current workflow HumanGate direct gate mutation path.
- `crates/agentdash-application-ports/src/lifecycle_materialization.rs` - lifecycle dispatch and workflow node materialization port surface.
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs` - runtime session anchor coordinate fields.
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs` - current delivery binding copied from execution anchor.
- `crates/agentdash-domain/src/workflow/dispatch.rs` - `OrchestrationBindingRefs` and `AgentRuntimeRefs`.
- `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs` - subject association entity and run/agent scoped constructors.

### Current Responsibilities In LifecycleDispatchService

`LifecycleDispatchFacade` is already thin: it stores repository/port dependencies, builds a `LifecycleDispatchService`, and implements `LifecycleDispatchPort` plus `WorkflowAgentNodeMaterializationPort` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_facade.rs:21`, `:58`, `:115`, `:123`). Keep this facade stable.

`LifecycleDispatchService` currently owns a broad dependency set: run, graph, agent, frame, association, gate, lineage, optional anchor, runtime session creation, frame construction, workflow node frame materialization, and graph planner (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:105`).

Public entry dispatching is method-level routing only:

- `dispatch` maps typed `ExecutionIntent` variants to `launch_agent`, `execute_subject`, `start_lifecycle_run`, or `open_interaction_gate` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:275`).
- `launch_agent`, `execute_subject`, and `open_interaction_gate` all call `dispatch_common` and then shape response DTOs (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:300`, `:326`, `:352`).

`start_lifecycle_run` is already the cleanest extraction seed. It plans the workflow graph, creates a control run, activates root orchestration, persists the run, and returns run/orchestration refs without runtime session side effects (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:378`, `:382`, `:387`, `:388`, `:394`).

`materialize_workflow_agent_node` is a second public use case. It loads the run, validates the orchestration binding, resolves the plan node and lifecycle surface identity, creates a workflow `LifecycleAgent`, creates/attaches a RuntimeSession, materializes a workflow AgentFrame, writes a `RuntimeSessionExecutionAnchor`, binds `LifecycleAgent.current_delivery`, and returns `AgentRuntimeRefs` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:411`, `:425`, `:439`, `:443`, `:447`, `:471`, `:491`, `:501`, `:518`). It deliberately does not apply `NodeStarted`; `AgentNodeLauncher` does that after materialization (`crates/agentdash-application-workflow/src/orchestration/agent_node_launcher.rs:110`, `:131`).

Graph-backed `dispatch_common` currently owns the full side-effect chain:

- graph planning and run/orchestration creation or reuse (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:541`, `:546`, `:547`, `:553`);
- agent creation/reuse (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:554`);
- subject association (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:555`);
- RuntimeSession creation/attach (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:563`);
- initial frame construction (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:566`);
- lineage write (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:571`);
- gate opening (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:583`);
- execution anchor and agent delivery binding (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:591`, `:602`);
- `NodeStarted` reducer event and updated run persistence (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:615`, `:629`);
- response facts and `SubjectExecutionRef` assembly (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:631`, `:639`).

Plain dispatch repeats the same materialization/association/relation/anchor pattern without orchestration planning or reducer bridge (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:670`, `:674`, `:675`, `:676`, `:684`, `:687`, `:692`, `:704`, `:713`).

Helper responsibilities are already visible but local to the thick service:

- run resolution: `resolve_or_create_run` and `resolve_or_create_plain_run` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:750`, `:778`);
- association writer: `create_subject_association` chooses agent-scoped `task/story` vs run-scoped subjects and writes the repo row (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:805`);
- agent materialization: `resolve_or_create_agent`, explicit reuse validation, and `create_agent` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:824`, `:846`, `:876`);
- frame materialization: duplicated plain/graph wrappers both delegate to `construct_launch_anchor_frame` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:889`, `:898`, `:907`);
- launch anchor frame construction consumes `AgentRunFrameConstructionPort::DispatchLaunchAnchor` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:920`, `:936`, `:938`);
- workflow AgentCall frame construction consumes `WorkflowAgentNodeFrameMaterializationPort` and passes `orchestration_id + node_path + attempt` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:955`, `:972`, `:980`);
- runtime session creation consumes `RuntimeSessionCreationPort` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1000`, `:1015`);
- gate opening still calls `LifecycleGate::open` directly (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1032`, `:1043`).

Graph/orchestration helpers are pure enough to move first:

- `ensure_workflow_graph_orchestration` reuses existing orchestration by role + plan digest or activates a new orchestration (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1084`).
- `orchestration_entry_binding` derives the entry runtime coordinate from ready node / entry node (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1107`).
- `ensure_orchestration_node_binding` validates an explicit binding exists in a run (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1138`).

### Related Helper And Launcher Patterns

`AgentNodeLauncher` is the scheduler-side contract to preserve: it derives `OrchestrationBindingRefs` from the ready coordinate, calls `WorkflowAgentNodeMaterializationPort`, then builds `OrchestrationRuntimeEvent::NodeStarted` using the same coordinate and returned runtime session id (`crates/agentdash-application-workflow/src/orchestration/agent_node_launcher.rs:101`, `:110`, `:124`, `:131`). D2 should not move reducer mutation into lifecycle materialization for this path, or the launcher would lose the "materialize first, then mark started" boundary.

`OrchestrationRuntimeEvent::NodeStarted` is intentionally scoped by node path and attempt; `apply_orchestration_event_to_run` chooses the orchestration instance by `orchestration_id` (`crates/agentdash-application-workflow/src/orchestration/runtime.rs:178`, `:266`). Applying `NodeStarted` sets the runtime node to Running, writes `executor_run_ref`, appends matching `RuntimeTraceRef`, and removes the ready node (`crates/agentdash-application-workflow/src/orchestration/runtime.rs:303`, `:317`, `:321`, `:328`).

`RuntimeSessionExecutionAnchor::new_orchestration_dispatch` stores the same coordinate on launch evidence (`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:68`, `:83`, `:84`, `:85`). `LifecycleAgentCurrentDeliveryBinding::from_anchor` copies those coordinate fields into current delivery (`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:141`, `:149`, `:150`, `:151`).

`AgentRuntimeRefs` also carries the same `OrchestrationBindingRefs` coordinate (`crates/agentdash-domain/src/workflow/dispatch.rs:177`, `:197`, `:206`). This makes `orchestration_id + node_path + attempt` the shared coordinate across result refs, frame materialization input, anchor, current delivery, reducer event, and ready queue clearing.

Current D3/D4 code is not yet converged in this workspace scan:

- Launch command duplication still exists in AgentRun, RuntimeSession, and frame launch ports (`crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:159`, `crates/agentdash-application-runtime-session/src/session/launch/command.rs:11`, `crates/agentdash-application-ports/src/frame_launch_envelope.rs:160`).
- Workflow HumanGate still mutates `gate.payload_json` and calls `gate.resolve` directly (`crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:120`, `:130`, `:131`).
- Companion gate code still contains delivery-payload helpers and direct `gate.resolve` calls, based on grep hits in `crates/agentdash-application/src/companion/gate_control.rs:467`, `:526`, `:1443`, `:1453`.

### Proposed File / Module Layout

Keep public API compatibility:

```text
crates/agentdash-application-lifecycle/src/lifecycle/
  dispatch_service.rs        # public facade/coordinator; keeps LifecycleDispatchService name
  dispatch_facade.rs         # unchanged public port adapter shape
  dispatch/
    mod.rs                   # private module exports used by dispatch_service.rs
    plan.rs                  # DispatchPlan, DispatchFacts, MaterializedAgentRuntime, small request/result DTOs
    run_orchestration_starter.rs
    agent_runtime_materializer.rs
    subject_association_writer.rs
    lifecycle_relation_writer.rs
    orchestration_reducer_bridge.rs
```

`RunOrchestrationStarter` should own graph planning, run creation/reuse, plain/control topology selection, `ensure_workflow_graph_orchestration`, lifecycle-start-only flow, and entry binding derivation. It should depend on `LifecycleRunRepository`, `WorkflowGraphRepository`, and optional `WorkflowGraphPlanningPort`. It should return a context like `PreparedRunOrchestration { run, orchestration_binding, workflow_graph?, plan_snapshot? }`. It should absorb `start_lifecycle_run`, `plan_workflow_graph`, `resolve_or_create_run`, `resolve_or_create_plain_run`, `ensure_workflow_graph_orchestration`, `orchestration_entry_binding`, and `ensure_orchestration_node_binding`.

`AgentRuntimeMaterializer` should own `LifecycleAgent`, RuntimeSession, AgentFrame, anchor, and delivery binding materialization. It should depend on `LifecycleAgentRepository`, `RuntimeSessionCreationPort`, `RuntimeSessionExecutionAnchorRepository`, `AgentRunFrameConstructionPort`, and `WorkflowAgentNodeFrameMaterializationPort`. Inputs should include `run`, `DispatchPlan` or narrower runtime source fields, optional `OrchestrationBindingRefs`, optional workflow node materialization facts, and `frame_created_by_id`. It should return `MaterializedAgentRuntime { agent, frame_id, runtime_session_ref, runtime_refs }` and should be the only owner of `RuntimeSessionExecutionAnchor::new_dispatch`, `RuntimeSessionExecutionAnchor::new_orchestration_dispatch`, and `bind_current_delivery_from_anchor` in lifecycle dispatch.

`SubjectAssociationWriter` should own `LifecycleSubjectAssociation` writes and `SubjectExecutionRef` assembly. It should depend only on `LifecycleSubjectAssociationRepository`. It should absorb `create_subject_association`, `association_role_from_source`, and the `task/story` agent-scoped decision. It should expose one method that returns both the association row and optional `SubjectExecutionRef` so `dispatch_service.rs` stops constructing response refs itself.

`LifecycleRelationWriter` should own lineage and gate opening. It should depend on `AgentLineageRepository` and the D3 gate opening/resolver port, not directly on `LifecycleGate::open` once D3 is done. It should absorb the duplicated `AgentLineage::new(...)` blocks and `create_gate`. A narrow command shape should be enough: `write_relations(run, agent, frame_id, parent_agent_id, agent_policy, gate_policy) -> RelationWriteResult { gate_ref }`.

`OrchestrationReducerBridge` should own `NodeStarted` event construction, reducer application, and updated run persistence for graph-backed dispatch. It should depend on `LifecycleRunRepository`. Its core method should be close to `mark_node_started(run, binding, runtime_session_ref) -> LifecycleRun`, and must reject missing runtime session ids before applying the reducer. It should verify that `MaterializedAgentRuntime.runtime_refs.orchestration_binding == binding` before persisting. The workflow scheduler can keep its current reducer path in `AgentNodeLauncher`; this bridge is for lifecycle dispatch's graph-backed entry dispatch path.

`dispatch_service.rs` should become a coordinator that composes these owners, keeps the public methods, maps response DTOs, and emits high-level diagnostics. `dispatch_common` should no longer directly contain repository write policy for every owner.

### Recommended Extraction Order After D4 / D3

1. Confirm D4 and D3 are complete with the stage grep gates before starting D2. Current scan still shows duplicate launch models and direct gate mutation, so D2 should wait.
2. Add the `lifecycle/dispatch/` module and move internal DTOs/helpers with no behavior change. Keep `LifecycleDispatchService` constructor and public methods intact.
3. Extract `RunOrchestrationStarter` first. `start_lifecycle_run` is already narrow and gives a low-risk first slice; keep lifecycle-start tests green.
4. Extract `SubjectAssociationWriter`. It is small, has clear repo ownership, and is independent of D3/D4.
5. Extract `LifecycleRelationWriter` after D3 exposes the shared resolver/opening port. This avoids baking the current direct `LifecycleGate::open` shape into a new owner.
6. Extract `AgentRuntimeMaterializer`. Start with plain dispatch and then workflow node materialization; keep anchor + delivery binding together so current delivery cannot drift from anchor evidence.
7. Extract `OrchestrationReducerBridge` last. At that point materialization returns a stable runtime ref and coordinate, so the bridge can assert coordinate equality before applying `NodeStarted`.
8. Collapse `create_initial_frame` / `create_plain_initial_frame` and other duplicate helper names only after materializer owns the plain/graph distinction.

### Tests And Regressions To Protect Coordinate Consistency

Keep these existing tests as non-negotiable regression coverage:

- `dispatch_service::agent_launch_creates_plain_surface_without_orchestration_binding` verifies plain run, no orchestration binding, RuntimeSession, association, and plain anchor (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1908`).
- `dispatch_service::story_root_launch_creates_agent_scoped_story_association` protects `task/story` agent-scoped association behavior (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1953`).
- `dispatch_service::subject_execution_initializes_orchestration_node_and_anchor_binding` protects graph-backed entry dispatch: result refs, node Running, ready queues empty, executor ref, trace ref, anchor orchestration id, node path, and attempt (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1994`).
- `dispatch_service::dispatch_resolves_workflow_graph_by_key_inside_service` protects graph lookup + entry `NodeStarted` behavior (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:2073`).
- `dispatch_service::lifecycle_run_start_intent_initializes_root_orchestration_state` protects lifecycle start as orchestration-only with node Ready and no agent side effect (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:2141`).
- `dispatch_service::lifecycle_run_start_rejects_blocking_compiler_diagnostics_without_creating_run` protects planning failure before run creation (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:2214`).
- `dispatch_service::reuse_existing_with_parent_agent_id_resumes_explicit_agent` protects explicit agent reuse (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:2263`).
- `runtime::orchestration_runtime_node_started_updates_executor_ref_and_ready_queue` protects reducer semantics (`crates/agentdash-application-workflow/src/orchestration/runtime.rs:1200`).
- `runtime::orchestration_runtime_node_started_refreshes_lifecycle_run_as_running` protects run-level status refresh (`crates/agentdash-application-workflow/src/orchestration/runtime.rs:1242`).
- `executor_launcher` AgentCall tests protect scheduler materialization followed by `NodeStarted`, frame VFS surface, anchor coordinate, and procedure contract forwarding (`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:1333`, `:1389`).

Add focused D2 tests during extraction:

- `AgentRuntimeMaterializer` unit: for graph binding `(orchestration_id, "agent", 1)`, assert `WorkflowAgentNodeFrameMaterializationInput.orchestration_id/node_path/attempt`, `AgentRuntimeRefs.orchestration_binding`, `RuntimeSessionExecutionAnchor.orchestration_id/node_path/node_attempt`, and `LifecycleAgent.current_delivery` all match exactly.
- `AgentRuntimeMaterializer` unit: for plain dispatch, assert `AgentRuntimeRefs.orchestration_binding == None`, anchor orchestration fields are `None`, and current delivery is copied from the plain anchor.
- Extend `subject_execution_initializes_orchestration_node_and_anchor_binding`: also assert `anchor.runtime_session_id == delivery_runtime_ref.to_string()`, `anchor.launch_frame_id == result.runtime_refs.frame_ref`, and `agent.current_delivery` matches the anchor's runtime session + coordinate.
- Add direct `LifecycleDispatchService::materialize_workflow_agent_node` test in application-lifecycle. It should assert materialization writes an anchor/current delivery and returns matching refs, but does not mutate the run's runtime node state; `AgentNodeLauncher` remains the reducer owner for scheduler-driven AgentCall.
- `OrchestrationReducerBridge` unit: with a valid graph run and binding, assert the persisted run has node Running, executor ref `RuntimeSession`, trace ref `RuntimeSession`, and both `activation.ready_node_ids` and `dispatch.ready_node_ids` no longer contain that node.
- `OrchestrationReducerBridge` negative unit: missing runtime session ref returns internal error before `run_repo.update`; binding to a missing node returns reducer error and does not persist a partial run.
- `LifecycleRelationWriter` unit after D3: gate opening calls the shared resolver/opening port and returns `gate_ref`; no mailbox delivery status blob is written into gate payload.
- Static regression after split: `rg -n "apply_orchestration_event_to_run|RuntimeSessionExecutionAnchor::new_orchestration_dispatch|RuntimeSessionExecutionAnchor::new_dispatch|create_subject_association|create_gate|resolve_or_create_runtime_session" crates/agentdash-application-lifecycle/src/lifecycle` should show those responsibilities only in the new owner modules and tests, not in coordinator code.

### External References

No external references were used. This research is based on task artifacts, Trellis specs, prior Trellis research, and repository source inspection.

### Related Specs

- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell. The output path was taken from the explicit user assignment: `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/research/`.
- No git operation was run because the research sub-agent instructions forbid git operations. I therefore did not inspect dirty state directly. The relevant workspace-risk caveat is source-level: current scan still shows D4/D3 old shapes, so D2 should not be implemented until D4/D3 are actually converged in the working tree.
- No tests or cargo checks were run; this was targeted read/search research only.
- No business code was modified.
