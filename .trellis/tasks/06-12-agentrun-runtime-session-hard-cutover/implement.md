# Hard Cutover Implementation Plan

## Status

Phase 1 implementation complete. Launch planning now reads a non-optional `FrameLaunchSurface`
from `FrameLaunchEnvelope`; the old parallel envelope surface fields and transition sync helper
are removed. Phase 2/3 remain pending for projection-field cleanup and owner composition
relocation.

## Phase 0: Context And Specs

Status: completed

- [x] Read relevant specs:
  - `.trellis/spec/backend/session/architecture.md`
  - `.trellis/spec/backend/session/session-startup-pipeline.md`
  - `.trellis/spec/backend/session/execution-context-frames.md`
  - `.trellis/spec/backend/session/runtime-execution-state.md`
  - `.trellis/spec/backend/workflow/architecture.md`
  - `.trellis/spec/backend/runtime-gateway.md`
  - `.trellis/spec/backend/capability/tool-capability-pipeline.md`
  - `.trellis/spec/cross-layer/frontend-backend-contracts.md`
  - `.trellis/spec/frontend/type-safety.md`
- [x] Confirm current branch and clean/dirty state before implementation.
- [x] Write `implement.jsonl` and `check.jsonl` manifests.

## Phase 1: Launch Surface Single Source

Status: completed

Owner: first `trellis-implement` subagent.

Goal: delete parallel launch surface fields from `FrameLaunchEnvelope`.

Tasks:

- [x] Introduce launch-ready typed surface helper/wrapper if `FrameSurfaceDraft` optional fields would force fallback logic.
- [x] Remove `FrameLaunchEnvelope.executor_config`.
- [x] Remove `FrameLaunchEnvelope.capability_state`.
- [x] Remove `FrameLaunchEnvelope.vfs`.
- [x] Remove `FrameLaunchEnvelope.mcp_servers`.
- [x] Remove `sync_transitional_fields_from_surface_draft`.
- [x] Replace fallback `launch_*` accessors with direct typed surface reads or non-optional launch surface access.
- [x] Update construction, planner, plan tests, orchestrator test helpers, and build factories to use complete typed launch surface.
- [x] Delete tests whose only purpose is transition-field synchronization.

Validation:

- [x] `cargo check -p agentdash-application`
- [x] `cargo test -p agentdash-application frame_surface`
- [x] `cargo test -p agentdash-application session::launch`
- [x] `rg -n "sync_transitional_fields_from_surface_draft|\\.executor_config|\\.capability_state|\\.vfs|\\.mcp_servers" crates/agentdash-application/src/workflow/runtime_launch.rs crates/agentdash-application/src/session/launch`

Note: this broad grep still reports legitimate `FrameLaunchSurface`, `ExecutionContext` and
`CapabilityState` field access. It reports no `sync_transitional_fields_from_surface_draft` and no
parallel `FrameLaunchEnvelope` surface fields.

Commit target:

- `refactor(session): 单一化 FrameLaunch surface`

## Phase 2: Delete Old Projection Fixtures

Status: pending

Owner: second `trellis-implement` subagent after Phase 1 lands.

Goal: remove `RuntimeContextInspectionPlan` projection compatibility and old tests.

Tasks:

- [ ] Remove `ConstructionProjections.mcp_servers`.
- [ ] Remove `ConstructionProjections.capability_state`.
- [x] Remove `RuntimeContextInspectionPlan::surface_draft_or_fixture_projection`.
- [ ] Update `apply_session_assembly` to write only `frame_surface_draft`.
- [ ] Rewrite behavior tests to construct real `FrameSurfaceDraft` / launch surface directly.
- [ ] Delete tests that only assert fixture fallback or stale projection compatibility.
- [ ] Update specs to describe direct typed handoff only.

Validation:

- [ ] `cargo check -p agentdash-application`
- [ ] `cargo test -p agentdash-application session::hub`
- [ ] `cargo test -p agentdash-application session::launch`
- [ ] `rg -n "surface_draft_or_fixture_projection|projections\\.mcp_servers|projections\\.capability_state" crates .trellis/spec`

Commit target:

- `refactor(session): 删除旧 projection fixture`

## Phase 3: Move Owner Composition Out Of Session Layer

Status: pending

Owner: third `trellis-implement` subagent after Phase 1/2 land.

Goal: remove session module ownership of owner bootstrap surface composition.

Tasks:

- [ ] Move owner bootstrap-facing types/functions from `session::assembler` into workflow/frame construction composer ownership.
- [ ] Keep session module focused on launch stages, runtime registry, eventing, persistence and delivery.
- [ ] Ensure `FrameConstructionService` calls the new composer directly instead of returning through session-owned owner composition APIs.
- [ ] Remove or shrink `SessionRequestAssembler` so it no longer owns Project/Story owner bootstrap composition.
- [ ] Keep behavior for owner bootstrap, lifecycle node, companion and routine launch intact.
- [ ] Update module docs/specs to describe frame construction ownership.

Validation:

- [ ] `cargo check -p agentdash-application`
- [ ] `cargo test -p agentdash-application session::launch`
- [ ] `cargo test -p agentdash-application workflow::frame`
- [ ] `cargo test -p agentdash-application agent_message`
- [ ] `rg -n "compose_owner_bootstrap|OwnerBootstrapSpec|OwnerScope|SessionRequestAssembler" crates/agentdash-application/src/session crates/agentdash-application/src/workflow`

Commit target:

- `refactor(workflow): 接管 AgentFrame owner composition`

## Phase 4: Cross-Boundary Review And Spec Convergence

Status: pending

Owner: `trellis-check` subagent.

Goal: verify hard cutover across runtime gateway, AgentRun workspace, persistence and frontend contracts.

Tasks:

- [ ] Check no public AgentRun workspace DTO regressed to SessionRuntime control DTO.
- [ ] Check SessionRuntime control remains runtime trace/detail only.
- [ ] Check runtime gateway MCP source reads active execution snapshot or current AgentFrame.
- [ ] Check RuntimeSession persistence remains trace/delivery only.
- [ ] Update specs with target-state wording only.
- [ ] Delete stale task/research comments that describe transition fields as current behavior.

Validation:

- [ ] `git diff --check`
- [ ] `pnpm run backend:clippy`
- [ ] `cargo check -p agentdash-application`
- [ ] `pnpm run contracts:check`
- [ ] `pnpm run migration:guard`
- [ ] `cargo test -p agentdash-application session::launch`
- [ ] `cargo test -p agentdash-application session::hub`
- [ ] `cargo test -p agentdash-application runtime_gateway`
- [ ] `cargo test -p agentdash-application capability`
- [ ] `cargo test -p agentdash-application mcp_preset`
- [ ] `cargo test -p agentdash-application mcp`
- [ ] `cargo test -p agentdash-executor mcp`
- [ ] `cargo test -p agentdash-local relay_mcp_servers`

## Parallelization Strategy

- Phase 1 is the critical path because Phase 2 and Phase 3 depend on the final launch surface shape.
- While Phase 1 runs, another subagent may perform read-only research on owner composition move targets and write notes under this task directory.
- Phase 2 and Phase 3 should be sequential unless Phase 1 yields a small clean diff; their write sets overlap in `session::assembler`, `assembly_builder`, `construction`, and frame construction.
- Final `trellis-check` runs after all implementation phases and may self-fix spec/test drift.

## Risk Areas

- `FrameSurfaceDraft` optional fields may require a launch-ready wrapper to avoid panic-prone accessors.
- Some tests currently rely on partial construction fixtures; delete fixture-only tests and preserve behavior tests with real surface construction.
- Moving owner composition can cause broad import churn. Prefer a new workflow/frame_construction composer module and scoped re-exports over large unrelated renames.
- Frontend checks may still be blocked if `packages/app-web/node_modules` is missing; record environment failure rather than changing dependencies.
