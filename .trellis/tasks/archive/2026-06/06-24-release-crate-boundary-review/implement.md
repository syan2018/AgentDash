# 调研执行计划

## Phase 1: Task Bootstrap And Dispatch

- [x] Create Trellis task.
- [x] Capture PRD, target boundary, review record contract and workstreams.
- [x] Dispatch direct parallel reviewers with `multi_agent_v1` workers.
- [x] Verify each reviewer writes to its assigned `research/*.md` file.

## Phase 2: Parallel Module Review

- [x] Session runtime inventory reviewer completes `research/01-session-runtime-inventory.md`.
- [x] AgentRun/Lifecycle reviewer completes `research/02-agentrun-lifecycle-surface.md`.
- [x] API/RuntimeGateway reviewer completes `research/03-api-runtime-gateway-consumers.md`.
- [x] Business surface update reviewer completes `research/04-business-surface-update-paths.md`.
- [x] Crate split reviewer completes `research/05-crate-split-coupling-map.md`.

## Phase 3: Synthesis

- [x] Merge reviewer findings into `research/00-index.md`.
- [x] Update `design.md` with first-round boundary verdict, accepted/rejected split candidates and dependency direction.
- [x] Update `implement.md` with child task map and execution batches.
- [x] Identify existing active tasks that should be linked or superseded by child tasks.

## Phase 4: Planning Review Gate

- [x] Review artifacts for evidence-backed conclusions.
- [x] Ask the user for any product/scope decision that code evidence cannot answer. No blocking product decision remains for the research handoff; implementation approval belongs to child tasks.
- [ ] Keep task in `planning` until implementation child tasks are approved.

## Reviewer Output Requirements

Each reviewer should write one markdown report and include:

- Files inspected.
- Confirmed coupling facts with paths.
- Boundary verdict for each important file or group.
- Proposed target owner/module/crate.
- Release split risk and suggested child tasks.

## Validation Commands

These are research validation commands, not code quality gates:

```powershell
rg --files crates/agentdash-application/src/session
rg -n "SessionCapabilityService|SessionRuntimeInner|RuntimeSessionExecutionAnchor|AgentFrameRepository|AgentFrame|resolve_session_frame_vfs|get_current_runtime_backend_anchor|adopt_persisted" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-domain/src
cargo metadata --no-deps --format-version 1
```

If later implementation begins, create or activate child tasks and run the relevant Rust checks there.

## Child Task Map

1. `agentrun-current-surface-facade`
   - Scope: settle `AgentRunRuntimeSurfaceQueryPort`, runtime surface DTOs, resource surface facade and effective capability/admission facade as public application boundary.
   - Gate: `cargo test -p agentdash-application agent_run::runtime_surface runtime_gateway::mcp_access`.

2. `runtime-session-substrate-facade`
   - Scope: tighten `session/mod.rs`, move `AgentFrameRuntimeTarget` ownership to AgentRun, keep RuntimeSession public surface limited to delivery/trace/turn/event/resume/debug/persistence.
   - Gate: `cargo check -p agentdash-application`; production API no longer imports session planner/surface internals.

3. `launch-commit-agentrun-boundary`
   - Scope: move AgentFrame write, LifecycleAgent current delivery binding and bootstrap status decisions out of session launch commit/orchestrator into AgentRun/Lifecycle launch/commit adapters.
   - Gate: lifecycle dispatch/session launch tests and `cargo check -p agentdash-application`.

4. `runtime-gateway-port-boundary`
   - Scope: move gateway-facing AgentRun surface/MCP access contracts to `agentdash-application-ports`; keep providers behind RuntimeGateway facade.
   - Gate: `cargo test -p agentdash-application runtime_gateway`.

5. `vfs-resource-surface-facade`
   - Scope: move AgentRun resource surface query out of API `session_construction.rs`, preserve launch frame/current surface frame ids, clean VFS AgentRun source latest-anchor selection from route layer.
   - Gate: `cargo check -p agentdash-api`; VFS surface resolver tests if present.

6. `canvas-extension-session-project-binding`
   - Scope: add explicit Canvas/Extension runtime route validation that path Project/Canvas Project matches current runtime session surface Project before Gateway/provider invocation.
   - Gate: route/application tests for mismatched project rejection.

7. `application-public-visibility-cleanup`
   - Scope: reduce `pub mod` / `pub use` exposure in application root, `session/mod.rs`, `agent_run/frame/mod.rs` and `vfs/mod.rs`.
   - Gate: `cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp`.

8. `physical-crate-extraction-wave-1`
   - Scope: expand `agentdash-application-ports`, then extract RuntimeSession substrate and RuntimeGateway if import graph is clean.
   - Gate: `cargo metadata --no-deps --format-version 1`; `cargo check --workspace`.

9. `physical-crate-extraction-wave-2`
   - Scope: extract AgentRun and Lifecycle crates after ports prevent cycles.
   - Gate: workspace check plus AgentRun/Lifecycle targeted tests.

## Existing Task Relationship

- `.trellis/tasks/06-23-session-hub-boundary-cleanup` should remain the concrete session/hub cleanup implementation input. This parent task supersets it as release split tracking and should absorb its evidence, not duplicate its code work.
- `.trellis/tasks/06-23-agentrun-runtime-surface-projection-convergence` is the nearest existing implementation task for AgentRun runtime surface query/update. The proposed `agentrun-current-surface-facade` child should align with or replace that implementation plan after review.
- `.trellis/tasks/06-19-lifecycle-mount-architecture-cleanup` informs the VFS/resource surface child task, especially AgentRun lifecycle mount/resource browser behavior.
- `.trellis/tasks/06-14-module-overdesign-review` remains a broader architecture background task. This task is the concrete release-facing tracker for module/crate split boundaries.
- `.trellis/tasks/06-23-session-hub-boundary-cleanup` and `.trellis/tasks/06-23-agentrun-runtime-surface-projection-convergence` both mention old `SessionCapabilityService` language; current code has moved toward `AgentRunRuntimeSurfaceQueryPort`, `CurrentSurfaceRuntimeMcpAccess`, and `AgentRunRuntimeSurfaceUpdateService`, so follow-up child tasks should use current symbol names.
