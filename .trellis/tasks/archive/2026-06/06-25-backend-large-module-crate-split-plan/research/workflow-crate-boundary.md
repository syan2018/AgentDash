# Research: workflow crate boundary

- Query: review AgentDash backend `workflow` crate/module boundary and produce executable split recommendation
- Scope: internal
- Date: 2026-06-25

## Findings

### Conclusion

`workflow` should be extracted into an independent application crate named `agentdash-application-workflow`.

The target crate should own workflow definition/catalog, builtin workflow templates, graph/script compiler/preflight, orchestration activation/reducer, and orchestration executor launcher. It should not own plain lifecycle dispatch, lifecycle read/view projection, lifecycle VFS provider, lifecycle surface projector, session terminal callback registration, or generic AgentRun workspace/query behavior.

The new crate should depend on `agentdash-application-ports`, not directly on `agentdash-application-agentrun`. Current code already exposes the important AgentRun-facing seams as ports: workflow graph planning, runtime session creation, workflow agent node frame materialization, AgentRun frame construction, lifecycle surface projection, and AgentRun runtime/resource surface query. Directly depending on `agentdash-application-agentrun` would couple workflow scheduling to concrete AgentRun services and risks a cycle once AgentRun keeps depending on lifecycle/domain workflow facts.

Recommended dependency direction:

```text
agentdash-application
  -> agentdash-application-workflow
  -> agentdash-application-lifecycle
  -> agentdash-application-ports
  -> agentdash-domain

agentdash-application-agentrun
  -> agentdash-application-ports
  -> agentdash-domain
```

But this should be achieved in two steps: first move pure definition/compiler/catalog plus runtime reducer into `agentdash-application-workflow`; then move executor launcher after cutting the current `LifecycleDispatchService` call from `AgentNodeLauncher`.

The most important precondition is to invert `AgentNodeLauncher -> LifecycleDispatchService::materialize_workflow_agent_node`. Today workflow executor code imports lifecycle service directly, so moving it as-is would force `agentdash-application-workflow -> agentdash-application-lifecycle`. That is acceptable only if lifecycle no longer imports workflow. The cleaner final shape is a new port, likely in `agentdash-application-ports`, for workflow agent node materialization. Lifecycle can implement that port or the existing materialization service can move behind it. Then workflow calls the port, and lifecycle can still call workflow runtime/launcher without a cycle.

### Files found

- `crates/agentdash-application/src/workflow/mod.rs` - application workflow facade; re-exports catalog, builtins, graph resolver/planner, script/preflight, compiler, and lifecycle orchestration runtime.
- `crates/agentdash-application/src/workflow/catalog.rs` - CRUD/upsert/validation services for `AgentProcedure` and `WorkflowGraph`.
- `crates/agentdash-application/src/workflow/definition.rs` - builtin workflow bundle/template JSON loading.
- `crates/agentdash-application/src/workflow/graph_resolver.rs` - resolves `WorkflowGraphRef` by id/key.
- `crates/agentdash-application/src/workflow/graph_planner.rs` - implements `WorkflowGraphPlanningPort` using resolver + compiler.
- `crates/agentdash-application/src/workflow/orchestration/compiler.rs` - static `WorkflowGraph` to `OrchestrationPlanSnapshot` compiler.
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs` - typed workflow script builder document to `OrchestrationPlanSnapshot` compiler.
- `crates/agentdash-application/src/workflow/script/*` - workflow script typed builder document, capability summary, preflight service.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs` - orchestration activation, reducer events, apply functions.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs` - drains ready nodes and launches AgentCall/Function/LocalEffect/HumanGate.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs` - materializes AgentCall nodes through lifecycle dispatch service.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/function_node_runner.rs` - Function/API/Bash/LocalEffect execution adapter over `FunctionRunner`.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/human_gate_launcher.rs` - human gate open/decision bridge.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/ready_node.rs` - ready/running node coordinate helpers.
- `crates/agentdash-application-lifecycle/src/lifecycle/orchestrator.rs` - session terminal and `complete_lifecycle_node` bridge into orchestration reducer and launcher.
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` - plain lifecycle/AgentRun dispatch plus workflow graph orchestration bootstrap and workflow agent-node materialization.
- `crates/agentdash-api/src/routes/workflows.rs` - API caller for workflow graph/procedure CRUD, script preflight, lifecycle run start/continue, drain, human decisions.

### Code patterns

- `agentdash-application::workflow` is currently a facade. It re-exports lifecycle runtime APIs from `agentdash_application_lifecycle::workflow::orchestration` (`crates/agentdash-application/src/workflow/orchestration/mod.rs:3`, `crates/agentdash-application/src/workflow/orchestration/mod.rs:7`) and exposes them again at `workflow/mod.rs` (`crates/agentdash-application/src/workflow/mod.rs:19`).
- `application` workflow catalog is definition/catalog responsibility, not lifecycle runtime. `ActivityLifecycleCatalogService` validates graph references to `AgentProcedureRepository` and writes `WorkflowGraphRepository` (`crates/agentdash-application/src/workflow/catalog.rs:20`, `crates/agentdash-application/src/workflow/catalog.rs:32`, `crates/agentdash-application/src/workflow/catalog.rs:70`). `WorkflowCatalogService` only upserts `AgentProcedure` (`crates/agentdash-application/src/workflow/catalog.rs:146`, `crates/agentdash-application/src/workflow/catalog.rs:154`).
- `ApplicationWorkflowGraphPlanner` already implements an application-port boundary, resolving graph and compiling to a plan (`crates/agentdash-application/src/workflow/graph_planner.rs:1`, `crates/agentdash-application/src/workflow/graph_planner.rs:28`, `crates/agentdash-application/src/workflow/graph_planner.rs:39`). This belongs in the workflow crate.
- Script preflight is pure workflow application logic over `WorkflowScriptEvaluator` SPI and compiler trait (`crates/agentdash-application/src/workflow/script/preflight.rs:14`, `crates/agentdash-application/src/workflow/script/preflight.rs:80`, `crates/agentdash-application/src/workflow/script/preflight.rs:207`). It belongs in the workflow crate.
- Orchestration runtime reducer is pure workflow/lifecycle aggregate manipulation over domain value objects (`crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs:37`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs:179`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs:266`). It should move into workflow before the launcher.
- `OrchestrationExecutorLauncher` is the scheduler/executor facade and owns workflow-specific execution results (`crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:32`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:58`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:76`). It currently depends on lifecycle `RepositorySet` and `WorkflowApplicationError` (`crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:21`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:22`) and drains ready nodes by loading/updating `LifecycleRunRepository` (`crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:157`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:398`).
- `AgentNodeLauncher` creates a concrete `LifecycleDispatchService` and calls `materialize_workflow_agent_node` (`crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs:12`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs:138`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs:152`). This is the main cycle risk if moved directly.
- Existing ports already express the right AgentRun/lifecycle-facing seams: `WorkflowGraphPlanningPort` (`crates/agentdash-application-ports/src/workflow_graph_planning.rs:43`), `WorkflowAgentNodeFrameMaterializationPort` (`crates/agentdash-application-ports/src/workflow_agent_frame_materialization.rs:30`), and `RuntimeSessionCreationPort` (`crates/agentdash-application-ports/src/runtime_session_delivery.rs:50`).
- `LifecycleDispatchService` is mixed: it owns generic lifecycle dispatch and AgentRun control plane creation (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:87`, `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:99`), but it also plans workflow graphs and creates orchestration instances (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:355`, `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:608`, `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:1041`). Keep generic dispatch in lifecycle; move graph planning/activation helper ownership to workflow or behind workflow ports.
- `LifecycleOrchestrator` is lifecycle/session bridge code. It converts session terminal or `complete_lifecycle_node` input into `OrchestrationRuntimeEvent`, applies reducer, then drains ready nodes (`crates/agentdash-application-lifecycle/src/lifecycle/orchestrator.rs:1`, `crates/agentdash-application-lifecycle/src/lifecycle/orchestrator.rs:111`, `crates/agentdash-application-lifecycle/src/lifecycle/orchestrator.rs:204`, `crates/agentdash-application-lifecycle/src/lifecycle/orchestrator.rs:307`). This can remain in lifecycle while depending on workflow crate.
- API route downstream caller imports workflow catalog/preflight/launcher from `agentdash_application::workflow` and lifecycle commands from `agentdash_application_lifecycle` (`crates/agentdash-api/src/routes/workflows.rs:20`, `crates/agentdash-api/src/routes/workflows.rs:24`). It constructs catalog services at graph create/update/validate (`crates/agentdash-api/src/routes/workflows.rs:208`, `crates/agentdash-api/src/routes/workflows.rs:273`, `crates/agentdash-api/src/routes/workflows.rs:305`), uses script preflight/compiler (`crates/agentdash-api/src/routes/workflows.rs:347`, `crates/agentdash-api/src/routes/workflows.rs:353`), and invokes launcher for human decisions (`crates/agentdash-api/src/routes/workflows.rs:542`).
- Root `RepositorySet` currently knows both `agentdash-application-agentrun` and `agentdash-application-lifecycle`, and converts to each specialized repository set (`crates/agentdash-application/src/repository_set.rs:3`, `crates/agentdash-application/src/repository_set.rs:4`, `crates/agentdash-application/src/repository_set.rs:95`, `crates/agentdash-application/src/repository_set.rs:144`). A workflow crate should get its own narrower repository set or constructor input rather than reuse lifecycle's broad `RepositorySet`.

### What to include in `agentdash-application-workflow`

Include immediately:

- `crates/agentdash-application/src/workflow/catalog.rs`
- `crates/agentdash-application/src/workflow/definition.rs`
- `crates/agentdash-application/src/workflow/builtins/*.json`
- `crates/agentdash-application/src/workflow/graph_resolver.rs`
- `crates/agentdash-application/src/workflow/graph_planner.rs`
- `crates/agentdash-application/src/workflow/orchestration/compiler.rs`
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs`
- `crates/agentdash-application/src/workflow/script/*`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs`

Include after port inversion:

- `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/function_node_runner.rs`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/human_gate_launcher.rs`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/ready_node.rs`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/mod.rs`

Do not include:

- `crates/agentdash-application-lifecycle/src/lifecycle/orchestrator.rs` - lifecycle/session callback bridge.
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` as a whole - generic LifecycleRun/AgentRun dispatch remains lifecycle. Extract only workflow activation helper or expose a workflow node materialization port.
- `crates/agentdash-application-lifecycle/src/lifecycle/run_command_service.rs` - lifecycle run command facade; it may call workflow launcher.
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs`, `projection.rs`, `vfs_provider.rs`, `surface/*` - lifecycle read model, VFS and surface projection remain lifecycle/AgentRun-facing.
- `crates/agentdash-application/src/frame_construction/*` - AgentRun frame construction/materialization implementation should stay with AgentRun/application composition, consumed by workflow through `WorkflowAgentNodeFrameMaterializationPort`.
- `crates/agentdash-application/src/hooks/*` - hooks are a separate planned split; active workflow snapshot can later depend on workflow/lifecycle read ports.

### Direct AgentRun dependency decision

Do not make `agentdash-application-workflow` depend on `agentdash-application-agentrun`.

Reasons:

- Workflow needs AgentRun control-plane effects, not AgentRun workspace implementation. Current code already calls `RuntimeSessionCreationPort` and `WorkflowAgentNodeFrameMaterializationPort` for the concrete effects (`crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:94`, `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:95`).
- `agentdash-application-agentrun` already depends on domain workflow facts and application ports (`crates/agentdash-application-agentrun/Cargo.toml:10`, `crates/agentdash-application-agentrun/Cargo.toml:15`). Adding `workflow -> agentrun` would make later AgentRun-to-workflow surface/query reuse risky.
- The workspace spec says Workspace Module and running Agent collaboration should use AgentRun semantic ports and keep runtime session id as an adapter trace coordinate, not a business abstraction (`.trellis/spec/backend/directory-structure.md:79`).

If workflow needs more AgentRun behavior, add ports under `agentdash-application-ports`, for example:

- `WorkflowAgentNodeMaterializationPort`: input `run_id`, `orchestration_binding`, runtime policy, frame creator id, optional workflow contract; output runtime refs and delivery runtime ref. This replaces the direct `LifecycleDispatchService` construction in `AgentNodeLauncher`.
- Optional `OrchestrationRunRepositorySet` or constructor input struct in the workflow crate, containing only lifecycle run, graph/procedure, agent/frame/gate/lineage/anchor repos, runtime session creation, workflow node frame materialization, inline file repo for terminal output, and function runner.

### Lifecycle relationship after split

Lifecycle should call workflow for:

- starting a graph-backed lifecycle run: plan/activate root orchestration, then persist the resulting `LifecycleRun`;
- continuing/draining a lifecycle run: call `OrchestrationExecutorLauncher::drain_ready_nodes`;
- terminal callback / `complete_lifecycle_node`: convert delivery/tool evidence into `OrchestrationRuntimeEvent`, call workflow reducer, persist run, then drain;
- human gate decision route/service: call workflow launcher decision API after lifecycle authorization/load.

Workflow should call lifecycle only through ports, not concrete lifecycle services, for:

- materializing a workflow AgentCall node into a LifecycleAgent/AgentFrame/RuntimeSession/anchor;
- creating runtime sessions and workflow node frames;
- writing updated `LifecycleRun` aggregate through `LifecycleRunRepository`.

Avoid cycle by making `agentdash-application-lifecycle -> agentdash-application-workflow` the concrete crate dependency. Any callback from workflow to lifecycle must be a trait object from `agentdash-application-ports`, with lifecycle/application composition root providing the implementation.

### Migration steps

1. Add `crates/agentdash-application-workflow` and workspace dependency entries. Dependencies should start with `agentdash-domain`, `agentdash-application-ports`, `agentdash-spi`, and shared utility crates already used by current workflow files (`chrono`, `serde`, `serde_json`, `uuid`, `thiserror`, `sha2`, `async-trait`, `tokio` if tests require it).
2. Move pure workflow files from `agentdash-application/src/workflow` into the new crate. Introduce a local `WorkflowApplicationError` or move the shared error type to a small port/error module, because current pure files import `crate::lifecycle::WorkflowApplicationError`.
3. Move orchestration `runtime.rs` into the new crate and update compiler tests that import `super::super::runtime`.
4. Update `agentdash-application/src/workflow/mod.rs` to re-export from `agentdash-application-workflow` while removing local moved modules. This keeps current API route imports stable during the same refactor pass.
5. Change `agentdash-application-lifecycle` to depend on `agentdash-application-workflow` for runtime activation/reducer APIs. Replace `crate::workflow::orchestration::*` imports in lifecycle dispatch/orchestrator/subject execution control with the new crate path.
6. Add `WorkflowAgentNodeMaterializationPort` in `agentdash-application-ports` or reuse/extend `LifecycleDispatchPort` if it can express the exact AgentCall materialization contract without importing lifecycle concrete types.
7. Move executor launcher and private launchers into `agentdash-application-workflow`. Replace direct `LifecycleDispatchService` construction with the materialization port.
8. Add a workflow-specific repository/input set and conversion from root `agentdash-application::RepositorySet`. Avoid using lifecycle's broad `RepositorySet` in workflow public constructors.
9. Update API route imports to either keep using `agentdash_application::workflow::*` facade or import direct `agentdash_application_workflow::*`. Direct import is cleaner after the crate exists, but keeping a facade can reduce route churn during the split.
10. Remove `crates/agentdash-application-lifecycle/src/workflow` after all downstream imports resolve. Keep lifecycle-facing bridge code in `lifecycle/*`.

### Risk files

- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` - highest cycle risk because it imports workflow reducer/activation and is also called by workflow `AgentNodeLauncher`.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs` - direct concrete lifecycle service construction must be inverted.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs` - constructor currently takes lifecycle `RepositorySet` and lifecycle error type.
- `crates/agentdash-application-lifecycle/src/lifecycle/orchestrator.rs` - lifecycle bridge should remain but imports must switch to new workflow crate.
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs` - imports `super::runtime::RootInputBinding`; will need path adjustment after runtime moves.
- `crates/agentdash-application/src/workflow/graph_planner.rs`, `graph_resolver.rs`, `catalog.rs` - depend on `crate::lifecycle::WorkflowApplicationError`.
- `crates/agentdash-application/src/shared_library/seed.rs`, `install.rs`, `publish.rs` - consume builtin workflow template types/functions from `crate::workflow`.
- `crates/agentdash-api/src/routes/workflows.rs` - downstream caller for catalog, preflight, launcher, lifecycle commands, and DTO mapping.
- `Cargo.toml` workspace members/dependencies and `crates/*/Cargo.toml` - dependency direction must be checked carefully to avoid cycles.

### Validation commands

Focused commands:

```bash
cargo check -p agentdash-application-workflow
cargo check -p agentdash-application-lifecycle
cargo check -p agentdash-application
cargo test -p agentdash-application-workflow workflow
cargo test -p agentdash-application-lifecycle orchestration
cargo test -p agentdash-application workflow
pnpm run contracts:check
```

Full backend gate:

```bash
pnpm run backend:check
pnpm run backend:clippy
pnpm run backend:test
```

Only run `pnpm run migration:guard` if a later implementation touches schema/migration files. This split should not require a database migration by itself.

### Related specs

- `.trellis/spec/backend/architecture.md` - backend dependency direction and application/domain/infrastructure boundaries.
- `.trellis/spec/backend/directory-structure.md` - crate role table and `agentdash-application-ports` purpose.
- `.trellis/spec/backend/workflow/architecture.md` - workflow vocabulary, runtime invariants, module boundary table, compiler/runtime contract.
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - semantic executor launcher contract and validation requirements.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - lifecycle subject association and runtime session trace lookup.
- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox and turn boundary contract, relevant to direct AgentRun dependency decision.
- `.trellis/spec/backend/runtime-gateway.md` - AgentRun runtime surface query boundary.

### External references

No external references used. This review is based on current repository code, Cargo manifests, package scripts, and Trellis specs.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task, so the user-provided task path was used as the explicit output location.
- No implementation was performed and no business code/spec files were modified.
- Some broad `rg` outputs were truncated by tool output limits; conclusions above rely on targeted line-number reads for the relevant public APIs and dependency edges.
- The final crate split should be coordinated with the parallel `skill` split noted in the task PRD, because `agentdash-application-lifecycle` currently depends on `agentdash-application-skill` and workflow/lifecycle surface projection may share skill projection facts.
