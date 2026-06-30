# Lifecycle gate launch owner convergence implementation plan

## Stage 0. Planning Gate

- [ ] User reviews and approves `prd.md`, `design.md`, and `implement.md`.
- [ ] Curate `implement.jsonl` and `check.jsonl` with real spec/research entries.
- [ ] Run `python ./.trellis/scripts/task.py start 06-30-lifecycle-gate-launch-owner-convergence` only after approval.

## Stage 1. D4 Canonical Launch Command

- [ ] Add canonical launch command module in `agentdash-application-ports`.
- [ ] Create `agentdash-application-ports/src/launch/` with `mod.rs`, `command.rs`, and `modifier.rs`; do not add root-level launch files.
- [ ] Move source-specific launch payloads out of `frame_launch_envelope.rs` into `launch/modifier.rs`.
- [ ] Change `FrameLaunchEnvelopeRequest` to accept canonical `LaunchCommand`.
- [ ] Add `LaunchPlanningInput` and move `backend_selection` into planner-only input.
- [ ] Convert AgentRun launch boundary and RuntimeSession launch command path to pass canonical command directly.
- [ ] Remove AgentRun-local and RuntimeSession-local production launch command/source/modifier types.
- [ ] Remove FrameLaunch command/source/modifier production model and mapping functions.
- [ ] Remove or repurpose `session/launch/command.rs` so RuntimeSession does not keep a command wrapper; move runtime-specific outcome/result types to `service.rs` or `outcome.rs`.
- [ ] Update tests for AgentRun, Companion, Workflow/Routine, Hook resume, and Local relay launch sources.

Validation:

```powershell
rg -n "pub enum LaunchSource|pub struct LaunchCommand|FrameLaunchCommand|to_frame_launch_command|launch_command_from_frame_launch|runtime_launch_command" crates
Get-ChildItem crates/agentdash-application-ports/src | Where-Object { $_.Name -like "*launch*" }
Get-ChildItem crates/agentdash-application-ports/src/launch | Select-Object Name
cargo check -p agentdash-application-ports -p agentdash-application-runtime-session -p agentdash-application-agentrun -p agentdash-application
```

Exit criteria:

- Static grep proves only the canonical production model remains.
- `backend_selection` is only planner input.
- Ports launch intent files are grouped under `src/launch/`; `frame_launch_envelope.rs` has no command/source/modifier definitions.
- AgentRun and RuntimeSession imports show launch command ownership comes from `agentdash_application_ports::launch`.
- Focused compile passes before D3 starts.

## Stage 2. D3 Shared LifecycleGateResolver

- [ ] Add shared resolver types: `LifecycleGateResolver`, `GateTransitionOutcome`, `GateDeliveryIntent`, `GateNotificationIntent`.
- [ ] Move pure gate validation/transition from Companion gate control into resolver.
- [ ] Add Companion context resolver for parent/child/human runtime context lookup.
- [ ] Move mailbox delivery into delivery adapters that consume resolver intents.
- [ ] Move session notification into notification adapter and treat failures as diagnostics, not gate transition facts.
- [ ] Convert Workflow HumanGate to call shared resolver.
- [ ] Thin `CompanionGateControlService` into facade over resolver + adapters.
- [ ] Update API route for simple human response to call the narrow use case instead of constructing full delivery service graph where possible.

Validation:

```powershell
rg -n "gate\\.payload_json.*delivery|with_parent_mailbox_delivery_payload|with_human_mailbox_delivery_payload|gate\\.resolve\\(" crates/agentdash-application/src/companion crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs
cargo check -p agentdash-application -p agentdash-application-workflow -p agentdash-api
```

Exit criteria:

- Resolver tests cover open/respond/resolve, closed gate, invalid owner, malformed payload.
- Adapter tests cover human response, parent request, parent response delivery intents.
- Workflow HumanGate no longer bypasses shared resolver.

## Stage 3. D2 LifecycleDispatchService Owner Split

- [ ] Extract `RunOrchestrationStarter` from lifecycle start, graph planning, run/orchestration creation and reuse.
- [ ] Extract `AgentRuntimeMaterializer` for LifecycleAgent, RuntimeSession, AgentFrame, anchor, and delivery binding materialization.
- [ ] Extract `SubjectAssociationWriter`.
- [ ] Extract `LifecycleRelationWriter`; gate opening calls the D3 resolver/opening port.
- [ ] Extract `OrchestrationReducerBridge` for `NodeStarted` reducer and updated run persistence.
- [ ] Reduce `dispatch_common` to a coordinator over these owners.
- [ ] Collapse duplicate plain/graph helper names once materialization owns the distinction.

Validation:

```powershell
rg -n "async fn dispatch_common|apply_orchestration_event_to_run|RuntimeSessionExecutionAnchor::new_orchestration_dispatch|create_subject_association|create_gate" crates/agentdash-application-lifecycle/src/lifecycle
cargo check -p agentdash-application-lifecycle -p agentdash-application-workflow -p agentdash-application-runtime-session
```

Exit criteria:

- Existing dispatch tests still cover plain dispatch, graph-backed subject execution, lifecycle start, workflow node materialization, and gate opening.
- New owner-level tests cover run starter, runtime materializer, subject writer, relation/gate writer, reducer bridge.
- Graph-backed regression proves materialization refs, anchor refs, `NodeStarted`, and ready queue clearing use the same `orchestration_id + node_path + attempt`.

## Final Check

- [ ] Run all stage static gates again.
- [ ] Run focused cargo checks from all stages.
- [ ] Run contract/codegen checks if any exported DTO changes require generated TS updates.
- [ ] Run migration guard if a DB migration was introduced.
- [ ] Update `.trellis/spec/` only for final owner contracts learned during implementation.
- [ ] Commit in stage-sized commits using the project commit format.
