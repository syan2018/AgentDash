# AgentRun / RuntimeSession 解耦实施计划

## Phase 0: Prep

- [ ] Re-read parent research files.
- [ ] Run import hotspot baseline:
  ```powershell
  rg -n "crate::session::|agentdash_application::session::" crates/agentdash-application/src/agent_run crates/agentdash-application/src/lifecycle crates/agentdash-api/src
  rg -n "AgentFrameRuntimeTarget|AgentFrameBuilder|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src crates/agentdash-api/src
  ```
- [ ] Decide exact module names for AgentRun facades before editing production code.

## Phase 1: AgentRun Facade Contracts

- [ ] Move or re-own `AgentFrameRuntimeTarget` under AgentRun.
- [ ] Define narrow live adoption port under AgentRun; SessionHub implements it.
- [ ] Stabilize current runtime surface DTO with both launch frame and current surface frame.
- [ ] Add resource surface query facade that wraps `AgentRunLifecycleSurfaceProjector`.
- [ ] Add effective capability/admission facade object or port for runtime consumers.

## Phase 2: RuntimeSession Public Facade

- [ ] Reduce `session/mod.rs` public exports.
- [ ] Remove `AgentFrameHookRuntime` and `WorkflowApplicationError` re-exports from session.
- [ ] Hide session hub internals and surface/capability helper exports.
- [ ] Keep only RuntimeSession substrate use cases public.

## Phase 3: Launch / Commit Boundary

- [ ] Move AgentFrame revision write out of `session/launch/commit.rs`.
- [ ] Move LifecycleAgent current delivery binding out of session commit.
- [ ] Move bootstrap status decision out of `session/launch/orchestrator.rs`.
- [ ] Keep connector accepted turn, event commit and stream attach in RuntimeSession.

## Phase 4: Surface Update Unification

- [ ] Route Canvas expose/bind through generic AgentRun update facade.
- [ ] Move Permission frame-writing adapter under AgentRun or behind AgentRun-owned port.
- [ ] Ensure WorkspaceModule surface-changing paths submit typed AgentRun update requests only.
- [ ] Add explicit coverage for currently contract-only update variants or mark them deferred.

## Phase 5: API / RuntimeGateway / VFS Consumers

- [ ] Rename/move `agentdash-api/src/session_construction.rs` to AgentRun runtime surface adapter.
- [ ] Move resource VFS projection from API helper into application facade.
- [ ] Move VFS AgentRun latest-anchor selection out of route resolver.
- [ ] Add Canvas runtime invoke/bridge Project/session binding guard.
- [ ] Add Extension runtime Project/session mismatch guard.
- [ ] Move Terminal launch target derivation into application runtime placement facade.
- [ ] Keep RuntimeGateway MCP access on query port and add guard tests.

## Phase 6: Visibility And Import Cleanup

- [ ] Tighten application root exports.
- [ ] Tighten `agent_run/frame/mod.rs` exports.
- [ ] Tighten `vfs/mod.rs` exports that leak provider internals.
- [ ] Run import cleanup checks and document remaining exceptions.

## Phase 7: Verification

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
- No compatibility fallback for old session helper paths.
- No database migration unless a concrete implementation phase changes persisted schema.
