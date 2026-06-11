# Research: workflow-orchestration executable plan

- Query: Re-evaluate `reviews/002-workflow-orchestration.md` after completed workflow quick cleanup; produce executable module-level plan for remaining WF-IMPL-002/003/004 and review WF-ARCH-002/003.
- Scope: internal
- Date: 2026-06-11

## Findings

The task-local current pointer was not set (`task.py current --source` returned none), so this research uses the explicit task path supplied in the dispatch prompt: `.trellis/tasks/06-11-review-refactor-quality-sweep`.

Already completed and not replanned:

- WF-IMPL-001, WF-IMPL-005, WF-IMPL-006 are recorded as completed in `fixes/001-workflow-orchestration-quick-cleanup.md`, with commit `c079e519`.
- Current remaining workflow items are WF-IMPL-002, WF-IMPL-003, WF-IMPL-004, plus review WF-ARCH-002/003 as module-level refactor candidates.

Related specs:

- `.trellis/spec/backend/workflow/architecture.md`: compiler blocking diagnostics happen before run/orchestration creation; runtime node key is `orchestration_id + node_path + attempt`; scheduler owns executor start.
- `.trellis/spec/backend/workflow/activity-lifecycle.md`: Function / LocalEffect / HumanGate executor should submit runtime events; BashExec is already modeled as `PlanNodeKind::LocalEffect + ExecutorSpec::Function(BashExec)`.
- `.trellis/spec/backend/architecture.md`: application owns orchestration, infrastructure owns concrete side effects behind SPI ports.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: generated contract drift matters only if DTO shape changes; the proposed capability summary change keeps the existing preflight DTO shape.

External references:

- None. This is internal codebase research only.

### Files Found

| Batch | File | Functions / Types | Current evidence |
| --- | --- | --- | --- |
| A: LocalEffect + capability summary | `crates/agentdash-application/src/workflow/script/builder_document.rs` | `WorkflowScriptStatement`, `WorkflowScriptLocalEffect`, `parse_effect` | `capability_effect` is accepted by the typed parser at lines 299-311. |
| A: LocalEffect + capability summary | `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs` | `ScriptCompileOutput`, `CapabilitySummaryBuilder`, `compile_local_effect`, `plan_metadata` | Compiler returns raw JSON capability summary at lines 111-115; has its own summary builder at lines 197-260; emits `ExecutorSpec::LocalEffect` for `capability_effect` at lines 777-793; writes capability summary into metadata at lines 1215-1235. |
| A: LocalEffect + capability summary | `crates/agentdash-application/src/workflow/script/preflight.rs` | `WorkflowScriptCompileInput`, `WorkflowScriptPreflightOutput`, `extract_workflow_script_capability_summary`, `CapabilitySummaryExtractor` | Preflight owns a second typed summary interpreter at lines 304-392 and passes the summary into the compiler trait input at lines 270-278. |
| A: LocalEffect + capability summary | `crates/agentdash-application/src/workflow/mod.rs` and `script/mod.rs` | re-exports | `extract_workflow_script_capability_summary` is currently re-exported from `workflow::script`; keep the public application import stable while moving implementation. |
| A: LocalEffect + capability summary | `crates/agentdash-api/src/routes/workflows.rs` | `preflight_workflow_script`, `workflow_script_capability_summary_dto` | API returns the typed preflight summary at lines 349-373 and maps it to generated DTO at lines 912-944. No DTO shape change is needed. |
| A: LocalEffect + capability summary | `crates/agentdash-domain/src/workflow/value_objects/script_asset.rs` | `WorkflowScriptCapabilitySummary` | Domain already owns the typed summary shape at lines 118-140. |
| A: LocalEffect + capability summary | `crates/agentdash-spi/src/platform/function_runner.rs` | `FunctionRunner` | Function SPI only supports API request and BashExec at lines 36-48; there is no capability effect executor port. |
| A: LocalEffect + capability summary | `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs` | `drain_ready_nodes`, `run_function_node`, LocalEffect branch | Ready `PlanNodeKind::LocalEffect` enters `launch_function_node` at lines 142-144; `ExecutorSpec::LocalEffect` is converted to `local_effect_capability_not_supported` at lines 552-573. |
| B: root args activation | `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs` | `validate_input_bindings`, `root_arg_keys`, `plan_metadata` | Root bindings are generated from args/schema at lines 1027-1065; concrete `args` are read by `root_arg_keys` at lines 1124-1147; metadata stores concrete args and root bindings at lines 1227-1233. |
| B: root args activation | `crates/agentdash-application/src/workflow/orchestration/runtime.rs` | `activate_orchestration`, `materialize_plan_activation`, `materialize_root_input_bindings` | Activation calls `materialize_root_input_bindings` at line 73; runtime parses `metadata["script"]["args"]` and `root_input_bindings` at lines 83-120. |
| B: root args activation | `crates/agentdash-application/src/workflow/dispatch_service.rs` | `ensure_workflow_graph_orchestration` | Static graph activation calls `activate_orchestration` with no script args at lines 781-801; this path can use default activation input. |
| C: ReadyNode coordinate | `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs` | `ReadyNodeTarget`, `ReadyNodeTarget::next`, `from_node_id`, `from_running_node`, `function_context` | `ReadyNodeTarget` carries run id, orchestration id, node id/path/attempt, plan node, runtime node, and state snapshot at lines 680-691; it clones plan/runtime snapshot in `from_node_id` at lines 713-740 and `from_running_node` at lines 742-793. |
| C: ReadyNode coordinate | `crates/agentdash-domain/src/workflow/dispatch.rs` | `OrchestrationBindingRefs::new` | A run/orchestration/node coordinate pattern already exists for dispatch refs at line 186, but executor launcher uses its own ad hoc DTO. |
| D: launcher split | `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs` | `OrchestrationExecutorLauncher`, `launch_agent_node`, `launch_function_node`, `open_human_gate`, `block_ready_node`, `apply_event` | Launcher owns repository set + function runner at lines 67-82 and all executor kinds in one file; dispatch loop is at lines 119-175. |
| Architecture backlog | `crates/agentdash-application/src/workflow/dispatch_service.rs` | `dispatch_common` | Graph-backed dispatch still creates agent/frame/session/anchor and directly submits `NodeStarted` at lines 330-423. |
| Architecture backlog | `crates/agentdash-application/src/workflow/orchestration/runtime.rs` | `derive_orchestration_status`, `sync_lifecycle_run_status_from_orchestrations` | Runtime reducer has one status aggregation implementation at lines 924-996. |
| Architecture backlog | `crates/agentdash-domain/src/workflow/entity.rs` | `aggregate_orchestration_status` | Domain aggregate has a second status aggregation implementation at lines 260-303. |
| Architecture backlog | `crates/agentdash-application/src/workflow/run.rs` | `select_active_run`, `active_run_status_priority` | Active run selection interprets Ready/Running/Blocked priority separately at lines 3-24. |
| Architecture backlog | `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs` | `status_to_view`, `active_runtime_node_refs` | View projection maps run status and active node status separately at lines 403-412 and 506-530. |

### Current Code Patterns

- LocalEffect `capability_effect` is not just unsupported at runtime; it is knowingly produced by script compilation. The parser accepts it, the compiler emits `ExecutorSpec::LocalEffect`, and launcher returns `local_effect_capability_not_supported`.
- A real `capability_effect` executor is not a module-local wiring task. `FunctionRunner` only exposes API and Bash methods, while capability effects would need a defined mapping to runtime gateway / permission / local backend / capability state semantics.
- Capability summary already has the correct typed shape in domain. The bug is not missing types; it is ownership drift: preflight and compiler independently interpret the same builder document.
- Root args are currently mixed into plan metadata. That makes runtime activation depend on private compiler metadata and makes concrete launch args part of the digest input through metadata. First-stage cleanup can stay application-private and avoid domain/contract migration.
- Review WF-ARCH-002 and WF-ARCH-003 are not true architecture backlog by the task threshold. They can be handled as executor-launcher module refactors under 10 files.

## Immediate Implementation Batches

### Batch A: script compiler / preflight convergence

Dependency: first. Batch B also touches `script_compiler.rs`, so keep A and B sequential. Batch C can run in parallel only after A if implementers coordinate file ownership.

Write scope:

- `crates/agentdash-application/src/workflow/script/capability_summary.rs` (new)
- `crates/agentdash-application/src/workflow/script/mod.rs`
- `crates/agentdash-application/src/workflow/script/preflight.rs`
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs`
- `crates/agentdash-application/src/workflow/mod.rs` if re-export wiring requires it

Core changes:

- Move `CapabilitySummaryExtractor` out of `preflight.rs` into `workflow/script/capability_summary.rs` and keep `extract_workflow_script_capability_summary` as the single typed interpreter returning `WorkflowScriptCapabilitySummary`.
- Remove `CapabilitySummaryBuilder` from `script_compiler.rs`. Compiler metadata should serialize the typed `WorkflowScriptCapabilitySummary` with `serde_json::to_value`, not rebuild a raw JSON summary.
- Change `ScriptCompileOutput.capability_summary` from `serde_json::Value` to `WorkflowScriptCapabilitySummary`, or remove it if no non-test caller needs it. Direct compiler tests should assert typed summary fields.
- In the `WorkflowScriptCompiler for ScriptCompiler` adapter, use `input.capability_summary` as the metadata source. Direct `ScriptCompiler::compile` can compute the same typed summary once through the shared extractor.
- Treat `WorkflowScriptEffect::CapabilityEffect` as an unsupported compile-time capability. Add a blocking diagnostic such as `local_effect_capability_not_supported` at `{source_path}.effect.kind` or `{source_path}.effect.capability_key`; do not emit a runnable `ExecutorSpec::LocalEffect` plan for scripts.
- Prefer changing script compile/preflight output so `plan_snapshot` is `None` whenever blocking diagnostics exist. The generated API field is already optional and no frontend code currently consumes script preflight plans.

Risk:

- Existing tests expect `capability_effect` to compile and inspect `output.capability_summary["local_effect_capabilities"]`; they must be updated to expect a blocking diagnostic while still returning the typed summary for UI/preflight visibility.
- If a future script-launch route expected invalid preflight responses to include `plan_snapshot`, it should be corrected now; no such frontend consumer was found.

Validation:

- `cargo test -p agentdash-application workflow::script::preflight`
- `cargo test -p agentdash-application workflow::orchestration::script_compiler`
- `cargo check -p agentdash-api`

### Batch B: root args typed activation input

Dependency: after Batch A due `script_compiler.rs` overlap. Can run before Batch C/D.

Write scope:

- `crates/agentdash-application/src/workflow/orchestration/runtime.rs`
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs`
- `crates/agentdash-application/src/workflow/script/preflight.rs` if compile input drops runtime args
- `crates/agentdash-application/src/workflow/dispatch_service.rs` only if call signatures need explicit default activation options

Core changes:

- Add application-private typed activation input, for example `OrchestrationActivationInput { root_args: Option<Value>, root_input_bindings: Vec<RootInputBinding> }`, plus `RootInputBinding { node_id, port }`.
- Keep `activate_orchestration(role, source_ref, plan_snapshot)` as the no-args/default path for static graph callers, and add `activate_orchestration_with_input(...)` for script activation/tests.
- Remove `materialize_root_input_bindings` JSON metadata parsing from `runtime.rs`; materialization should read the typed activation input.
- Remove concrete `args` and `root_input_bindings` from `plan_metadata`. `args_schema`, log markers, capability summary, and provenance can remain metadata.
- Make `root_arg_keys` use `document.args_schema` as the compile-time contract, not concrete runtime `args`. Concrete args should be launch/activation input, not plan-shaping data.
- Expose compiler-produced typed `root_input_bindings` on `ScriptCompileOutput` for script launch code/tests to pass into activation.

Risk:

- Scripts that relied on undeclared runtime args to satisfy entry inputs will become compile errors. That is correct for the current project rule: no compatibility fallback during pre-release.
- Digest changes are expected because concrete args no longer live in metadata. That is desirable; plan identity should reflect static plan shape, not activation values.

Validation:

- `cargo test -p agentdash-application workflow::orchestration::runtime`
- `cargo test -p agentdash-application workflow::orchestration::script_compiler`
- `cargo test -p agentdash-application workflow::dispatch_service`

### Batch C: ReadyNode coordinate/view cleanup

Dependency: independent of A/B by behavior, but should run before Batch D.

Write scope:

- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs`
- Optional new file: `crates/agentdash-application/src/workflow/orchestration/ready_node.rs`
- `crates/agentdash-application/src/workflow/orchestration/mod.rs` if the new module is public inside orchestration

Core changes:

- Introduce module-local typed coordinate, for example `RuntimeNodeCoordinate { run_id, orchestration_id, node_path, attempt }`.
- Split current `ReadyNodeTarget` into a small coordinate plus short-lived `ReadyNodeView` / `RunningNodeView`. Keep plan/runtime snapshots scoped to the function that needs them instead of passing one large DTO through every executor path.
- Replace ad hoc JSON coordinate detail construction with a helper that serializes the typed coordinate to the existing `"orchestration_node_coordinate.v1"` detail shape.
- In `function_context`, build context from a fresh view and the current state snapshot rather than a long-lived target field.

Risk:

- Behavior should not change. Risk is accidental drift in `node_id` vs `node_path` lookup and human gate conflict checks.

Validation:

- `cargo test -p agentdash-application workflow::orchestration::executor_launcher`
- `cargo test -p agentdash-application workflow::orchestrator`

### Batch D: executor launcher service split

Dependency: after Batch C.

Write scope:

- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs`
- Optional new files under `crates/agentdash-application/src/workflow/orchestration/`: `agent_node_launcher.rs`, `function_node_runner.rs`, `human_gate_launcher.rs`, `ready_node.rs`
- `crates/agentdash-application/src/workflow/orchestration/mod.rs`

Core changes:

- Keep `OrchestrationExecutorLauncher::drain_ready_nodes` as the scheduler-facing facade.
- Move AgentCall-specific repository writes/session/frame/anchor creation into an `AgentNodeLauncher`.
- Move API/Bash function running, output mapping, and function context construction into a `FunctionNodeRunner`.
- Move HumanGate open/decision helpers into a `HumanGateLauncher`.
- Keep reducer writes through one small `apply_event` helper so terminal materialization remains centralized.

Risk:

- Medium refactor risk because `executor_launcher.rs` has dense tests and mocks. Keep behavior-preserving and do not combine with LocalEffect or root-args changes.

Validation:

- `cargo test -p agentdash-application workflow::orchestration::executor_launcher`
- `cargo test -p agentdash-application workflow::orchestration`

## Architecture Backlog

### WF-ARCH-001 / task ARCH-002: ready node startup has two real entry paths

Keep as architecture backlog.

Why it exceeds quick-fix scope:

- `dispatch_common` still creates agent/frame/runtime session/anchor and submits `NodeStarted` itself at `crates/agentdash-application/src/workflow/dispatch_service.rs:330-423`.
- The launcher also owns ready queue scheduling and executor start at `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs:119-175`.
- Unifying this changes the runtime fact source for dispatch, scheduler, session delivery, frame/anchor creation, and `NodeStarted` persistence. This is more than a file split and should have a separate design.

Suggested direction:

- Dispatch service should eventually ensure run/orchestration and delegate all ready-node start side effects to one scheduler/launcher port, or the launcher should become the only scheduler-facing startup service.

### WF-ARCH-004 / task ARCH-003: lifecycle status aggregation facts are duplicated

Keep as architecture backlog.

Why it exceeds quick-fix scope:

- Runtime reducer status aggregation lives in `runtime.rs:924-996`.
- Domain aggregate has another aggregation in `entity.rs:260-303`.
- Active run selection interprets status priority in `run.rs:3-24`.
- View projection separately finds active runtime node refs in `lifecycle_run_view_builder.rs:506-530`.
- Fixing this properly means defining one status projector contract and moving all consumers to it. That crosses domain/application projection and active selection semantics.

Suggested direction:

- Define a shared lifecycle/orchestration status projector consumed by reducer, aggregate refresh, active run selection, and view projection.

### Future real `capability_effect` executor

Keep only if product scope requires `capability_effect` support.

Why it exceeds quick-fix scope:

- Current `FunctionRunner` SPI only supports API request and BashExec.
- A real capability effect executor needs a declared capability-key namespace, permission policy, runtime gateway/local execution mapping, output contract, and failure semantics.
- The immediate issue is not deferred: scripts should fail compile/preflight until this architecture exists.

## Non-Deferred Review Items

- WF-IMPL-002 should not be deferred. The quick fix is compile/preflight blocking for `capability_effect`, not a fake runtime executor. It is module-local and avoids shipping a plan that can only fail at runtime.
- WF-IMPL-003 should not be deferred. It is a single-interpreter cleanup across about 4-5 application files with no public DTO shape change.
- WF-IMPL-004 should not be deferred as a whole. Full public plan-extension design can wait, but first-stage typed activation input removes runtime reverse-reading of private metadata in about 3-4 files.
- Review WF-ARCH-002 (`OrchestrationExecutorLauncher` overwide) should be downgraded to module-level refactor. Splitting executor-kind services is under 10 files and does not change database, contracts, or cross-layer protocol.
- Review WF-ARCH-003 (`ReadyNodeTarget` naked coordinate/snapshot DTO) should be downgraded to module-level refactor. A typed coordinate/view cleanup is local to orchestration launcher code and should be planned before or with the launcher split.

## Caveats / Not Found

- No git commands were used. Completed commit state was inferred from task records (`review-index.md` and `fixes/001-workflow-orchestration-quick-cleanup.md`) plus current source code.
- I did not find frontend consumers of `preflightWorkflowScript` beyond the service wrapper and generated type, so making invalid preflight omit `plan_snapshot` appears low-risk.
- I did not find an existing capability-effect executor port. The closest side-effect SPI is `FunctionRunner`, and it only covers API request and BashExec.
