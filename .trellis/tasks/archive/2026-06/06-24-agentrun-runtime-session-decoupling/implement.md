# AgentRun / RuntimeSession 解耦实施计划

## Dispatch Model

Implementation follows the directed graph in `parallel-dag.md`, not the older linear phase headings alone. Each dispatchable unit has a tracking doc under `work-items/`.

This is a one-shot decoupling migration task. The parent task's decoupling child task map is represented by internal work items and the coverage matrix in `parent-child-coverage.md`; implementation should complete the whole migration in this task rather than spawning more Trellis child tasks.

Safe parallelism:

- `WI-01` and `WI-02` can run in parallel after `WI-00`.
- `WI-03`, `WI-04`, `WI-05`, `WI-08`, and `WI-11` can run in parallel after their dependencies.
- `WI-06` and `WI-07` can run in parallel after `WI-01` / `WI-03`, but they must coordinate on AgentFrame write/adoption semantics.
- `WI-09` and `WI-10` are serial integration gates.

## Phase 0: Prep

- [ ] Complete `work-items/WI-00-baseline-import-contract-inventory.md`.
- [ ] Re-read parent research files.
- [ ] Confirm `parent-child-coverage.md` still maps every parent decoupling goal into this task.
- [ ] Run import hotspot baseline:
  ```powershell
  rg -n "crate::session::|agentdash_application::session::" crates/agentdash-application/src/agent_run crates/agentdash-application/src/lifecycle crates/agentdash-api/src
  rg -n "AgentFrameRuntimeTarget|AgentFrameBuilder|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src crates/agentdash-api/src
  ```
- [ ] Decide exact module names for AgentRun facades before editing production code.

## Phase 1: AgentRun Facade Contracts

- [ ] Complete `work-items/WI-01-agentrun-target-adoption-port.md`.
- [ ] Complete `work-items/WI-02-current-resource-surface-facades.md`.
- [ ] Move or re-own `AgentFrameRuntimeTarget` under AgentRun.
- [ ] Define narrow live adoption port under AgentRun; SessionHub implements it.
- [ ] Stabilize current runtime surface DTO with both launch frame and current surface frame.
- [ ] Add resource surface query facade that wraps `AgentRunLifecycleSurfaceProjector`.
- [ ] Add effective capability/admission facade object or port for runtime consumers.

## Phase 2: RuntimeSession Public Facade

- [ ] Complete `work-items/WI-03-runtime-session-public-facade.md`.
- [ ] Reduce `session/mod.rs` public exports.
- [ ] Remove `AgentFrameHookRuntime` and `WorkflowApplicationError` re-exports from session.
- [ ] Hide session hub internals and surface/capability helper exports.
- [ ] Keep only RuntimeSession substrate use cases public.

## Phase 3: Launch / Commit Boundary

- [ ] Complete `work-items/WI-07-launch-commit-ownership.md`.
- [ ] Move AgentFrame revision write out of `session/launch/commit.rs`.
- [ ] Move LifecycleAgent current delivery binding out of session commit.
- [ ] Move bootstrap status decision out of `session/launch/orchestrator.rs`.
- [ ] Keep connector accepted turn, event commit and stream attach in RuntimeSession.

## Phase 4: Surface Update Unification

- [ ] Complete `work-items/WI-06-surface-update-unification.md`.
- [ ] Route Canvas expose/bind through generic AgentRun update facade.
- [ ] Move Permission frame-writing adapter under AgentRun or behind AgentRun-owned port.
- [ ] Ensure WorkspaceModule surface-changing paths submit typed AgentRun update requests only.
- [ ] Add explicit coverage for currently contract-only update variants or mark them deferred.

## Phase 5: API / RuntimeGateway / VFS Consumers

- [ ] Complete `work-items/WI-04-runtime-gateway-mcp-boundary.md`.
- [ ] Complete `work-items/WI-05-api-current-surface-consumers.md`.
- [ ] Complete `work-items/WI-08-presentation-read-model-cleanup.md`.
- [ ] Complete `work-items/WI-11-canvas-extension-session-project-binding.md`.
- [ ] Rename/move `agentdash-api/src/session_construction.rs` to AgentRun runtime surface adapter.
- [ ] Move resource VFS projection from API helper into application facade.
- [ ] Move VFS AgentRun latest-anchor selection out of route resolver.
- [ ] Add Canvas runtime invoke/bridge Project/session binding guard.
- [ ] Add Extension runtime Project/session mismatch guard.
- [ ] Move Terminal launch target derivation into application runtime placement facade.
- [ ] Keep RuntimeGateway MCP access on query port and add guard tests.
- [ ] Move RuntimeGateway-facing AgentRun surface/MCP access contracts to `agentdash-application-ports`.
- [ ] Add Canvas runtime invoke/bridge Project/session binding guard.
- [ ] Add Extension runtime Project/session mismatch guard.

## Phase 6: Visibility And Import Cleanup

- [ ] Complete `work-items/WI-09-public-visibility-import-cleanup.md`.
- [ ] Tighten application root exports.
- [ ] Tighten `agent_run/frame/mod.rs` exports.
- [ ] Tighten `vfs/mod.rs` exports that leak provider internals.
- [ ] Run import cleanup checks and document remaining exceptions.

## Phase 7: Verification

- [ ] Complete `work-items/WI-10-final-review-gate.md`.
- [ ] Execute `review-gate.md`.
- [ ] Confirm `target-application-state.md`.
- [ ] Confirm `parent-child-coverage.md`.
- [ ] `cargo check -p agentdash-application`
- [ ] `cargo check -p agentdash-api`
- [ ] RuntimeGateway MCP tests.
- [ ] AgentRun runtime surface tests.
- [ ] Permission runtime surface update/adoption tests.
- [ ] Canvas/Extension Project/session mismatch tests.
- [ ] VFS SessionRuntime/AgentRun surface tests.
- [ ] Terminal launch target tests.

## Non-Goals

- No physical crate extraction.
- No new child tasks for decoupling goals covered by this migration.
- No compatibility fallback for old session helper paths.
- No database migration unless a concrete implementation phase changes persisted schema.
