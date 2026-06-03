# Implement Plan: Lifecycle 控制面重构收口复核

## Phase 0: Baseline Fix

- [ ] Update domain dispatch tests to construct `delivery_runtime_ref` instead of removed `runtime_session_ref` / `trace_ref`.
- [ ] Run `cargo test -p agentdash-domain --lib -- --format terse`.
- [ ] Keep this phase small; it unblocks test compilation and gives later phases a clean signal.

## Phase 1: Anchor Evidence Path

- [ ] Replace hard-coded anchor `activity_key = "entry"` with the actual frame / graph entry activity key.
- [ ] Add regression coverage for a workflow whose entry activity key is not `entry`.
- [ ] Change runtime session association resolution to query `RuntimeSessionExecutionAnchorRepository::find_by_session` first.
- [ ] When anchor has `assignment_id`, resolve assignment directly and validate run/agent/frame consistency.
- [ ] Decide whether JSON contains fallback is still needed in this pre-release branch; if retained, mark it as legacy audit fallback and keep it out of the normal current-data path.
- [ ] Run targeted lifecycle association and dispatch tests.

## Phase 2: Retire Old Construction Concepts

- [ ] Move test-only `RuntimeContextInspectionPlan` / `ResolvedSessionOwner` fixtures behind `#[cfg(test)]` or into test support.
- [ ] Remove `construction_use_case` from production public module exports.
- [ ] Preserve any useful helper logic by moving it into `FrameConstructionService` composers or explicit test fixture modules.
- [ ] Run `cargo check -p agentdash-application` and verify deprecated warning volume is gone or limited to intentional test-only code.

## Phase 3: Frontend Agent-First Read Model

- [ ] Replace `lifecycleStore.primarySessionId(runId)` with an agent/frame delivery runtime selector.
- [ ] Remove `runtime_trace_refs[0]` fallback from sidebar and active session list.
- [ ] Rename `HookSessionRuntimeInfo` to a frame/provenance-aligned name, or split adapter provenance from hook runtime state.
- [ ] Run `pnpm --filter app-web run typecheck`.

## Phase 4: Scoped Artifacts And SessionMeta Narrowing

- [ ] Move compose-stage port output loading to `load_scoped_port_output_map`.
- [ ] If attempt is unavailable at compose time, move assignment/attempt creation earlier enough to supply `ActivityPortArtifactRef`.
- [ ] Introduce or rename a narrow runtime trace launch state so `FrameConstructionService` and `LaunchPlanner` stop consuming full `SessionMeta` as launch facts.
- [ ] Ensure executor config and capability state continue to come from frame surface / lifecycle facts.

## Phase 5: Final Verification

- [ ] `cargo check --workspace`
- [ ] `cargo test -p agentdash-domain --lib -- --format terse`
- [ ] `cargo test -p agentdash-application --lib -- --format terse`
- [ ] `pnpm --filter app-web run typecheck`
- [ ] Residual scans:
  ```bash
  rg "RuntimeContextInspectionPlan|ResolvedSessionOwner" crates/agentdash-application/src --type rust
  rg "runtime_trace_refs\\[0\\]|primarySessionId" packages/app-web/src
  rg "HookSessionRuntimeInfo" packages/app-web/src
  rg "load_port_output_map" crates/agentdash-application/src/session crates/agentdash-application/src/workflow
  rg "executor_session_id" crates/agentdash-application/src/session crates/agentdash-spi crates/agentdash-contracts
  ```
- [ ] Start `pnpm dev` for a minimal launch / active-session-list smoke check after backend changes that affect runtime startup.

## Risk Notes

- Anchor resolver changes touch terminal/tool/hook return paths; add narrow tests before broad refactor.
- Construction type deletion can break old tests in several modules; prefer moving fixtures first, then deleting exports.
- Frontend fallback removal can expose missing `delivery_runtime_ref` in backend projections; validate `LifecycleAgentView` payloads before removing all UI fallback code.
- Port output scoping depends on activity attempt availability; solve the data-flow ordering rather than adding another run-level compatibility branch.
