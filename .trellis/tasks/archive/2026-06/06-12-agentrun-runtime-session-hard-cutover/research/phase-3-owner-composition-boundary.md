# Research: Phase 3 Owner Composition Boundary

- Query: Phase 3: Move Owner Composition Out Of Session Layer migration boundary for `session::assembler`.
- Scope: internal
- Date: 2026-06-12

## Findings

### Files Found

- `.trellis/tasks/06-12-agentrun-runtime-session-hard-cutover/prd.md` - hard cutover requirements; requires owner bootstrap composition to leave session layer.
- `.trellis/tasks/06-12-agentrun-runtime-session-hard-cutover/design.md` - target boundary: frame construction owns owner/activity/companion composer output into `FrameSurfaceDraft`.
- `.trellis/tasks/06-12-agentrun-runtime-session-hard-cutover/implement.md` - Phase 3 execution plan and validation commands.
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession target role: launch stages, delivery, trace, event, connector continuation, persistence.
- `.trellis/spec/backend/session/session-startup-pipeline.md` - startup pipeline and FrameConstruction contract; still contains transition wording that Phase 1/2/4 must converge.
- `.trellis/spec/backend/session/execution-context-frames.md` - connector-facing `ExecutionContext` projection contract.
- `.trellis/spec/backend/session/runtime-execution-state.md` - runtime registry, active turn, delivery command, persistence boundaries.
- `.trellis/spec/backend/workflow/architecture.md` - LifecycleRun / LifecycleAgent / AgentFrame / RuntimeSession target vocabulary.
- `.trellis/spec/backend/runtime-gateway.md` - RuntimeGateway MCP source boundary: active execution snapshot or current AgentFrame.
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` - capability resolver and `CapabilityState.tool.mcp_servers` projection contract.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - AgentRun workspace and RuntimeSession trace DTO split.
- `.trellis/spec/frontend/type-safety.md` - frontend generated DTO consumption boundary; not directly edited by Phase 3 unless contracts drift.
- `.trellis/tasks/archive/2026-06/06-12-agentrun-runtime-session-surface-convergence/design.md` - previous convergence design; notes old fixture fields were retained before hard cutover.
- `.trellis/tasks/archive/2026-06/06-12-agentrun-runtime-session-surface-convergence/implement.md` - previous phase status; identifies leftover projection fixture and transitional launch fields.
- `crates/agentdash-application/src/session/assembler.rs` - current mixed composer file; contains owner bootstrap, lifecycle node, companion, MCP normalization, and tests.
- `crates/agentdash-application/src/session/assembly_builder.rs` - builder that turns composed VFS/capability/MCP/context into `FrameSurfaceDraft` and `AssemblyLaunchExtras`.
- `crates/agentdash-application/src/session/construction.rs` - test-only `RuntimeContextInspectionPlan` and old fixture projection fields.
- `crates/agentdash-application/src/session/hub/facade.rs` - production/test-facing bridge that still calls `surface_draft_or_fixture_projection`.
- `crates/agentdash-application/src/session/launch/plan.rs` - launch planner tests construct envelopes through old `RuntimeContextInspectionPlan` fixture helper.
- `crates/agentdash-application/src/session/hub/tests.rs` - behavior tests plus old construction fixtures around MCP/capability/runtime update behavior.
- `crates/agentdash-application/src/workflow/frame_construction/mod.rs` - current FrameConstructionService; still creates `SessionRequestAssembler`.
- `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs` - ProjectAgent composer currently delegates owner composition back into session.
- `crates/agentdash-application/src/workflow/frame_construction/composer_companion.rs` - companion composer currently delegates to session assembler; adjacent but not owner bootstrap.
- `crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs` - lifecycle node composer currently delegates to session composer; adjacent but not owner bootstrap.
- `crates/agentdash-application/src/workflow/runtime_launch.rs` - current `FrameLaunchEnvelope` still has transitional parallel surface fields.
- `crates/agentdash-application/src/session/mod.rs` - public re-export surface currently exposes owner composer types from session.
- `crates/agentdash-application/src/workspace_module/tools.rs` - test helper still writes `construction.projections.capability_state`.

### Target Boundary

The target model is already clear in task artifacts and specs:

- `AgentFrame` is the executable surface fact source.
- `FrameConstructionService` should produce launch-ready frame surface handoff.
- `RuntimeSession` should remain a delivery/trace/runtime substrate.
- Owner bootstrap composition is business surface composition, so it belongs under `workflow/frame_construction` or an `AgentFrame` composer boundary.

Current code violates that boundary because `FrameConstructionService` creates a `SessionRequestAssembler` and ProjectAgent construction delegates owner bootstrap to session-owned types/functions: `FrameConstructionService::assembler()` returns `SessionRequestAssembler` at `crates/agentdash-application/src/workflow/frame_construction/mod.rs:167`, and `composer_project_agent.rs` imports `OwnerBootstrapSpec` / `OwnerScope` from `crate::session` at `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:10` before calling `.compose_owner_bootstrap_to_frame(...)` at `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:80`.

### Owner Bootstrap Surface Composition To Move

These are owner bootstrap surface composition and should move to `workflow/frame_construction` ownership, preferably into a new module such as `workflow/frame_construction/composer_owner.rs` or `workflow/frame_construction/owner_composer.rs`.

- `OwnerScope`: owner/domain scope, capability scope, project id, and mount target selection. It is defined in `session::assembler` at `crates/agentdash-application/src/session/assembler.rs:227` and is not a runtime session concept.
- `OwnerBootstrapSpec`: complete owner composition input containing owner, identity, subject context, executor config, user input, agent MCP/tool directives/skills/VFS grants, workspace module visibility, active workflow, audit key, and caller agent. It is defined at `crates/agentdash-application/src/session/assembler.rs:283`.
- `OwnerPromptLifecycle`: owner composer input deciding bootstrap/rehydrate/plain bundle behavior. It is defined at `crates/agentdash-application/src/session/assembler.rs:332` and currently mapped from `SessionPromptLifecycle` inside frame construction at `crates/agentdash-application/src/workflow/frame_construction/mod.rs:240`.
- `owner_audit_lifecycle` and `resolve_owner_audit_trigger`: owner bundle audit policy, defined at `crates/agentdash-application/src/session/assembler.rs:357` and `crates/agentdash-application/src/session/assembler.rs:365`; this belongs with owner context bundle composition because it is keyed by owner lifecycle and bundle availability.
- `build_owner_context_contribution`: builds Story/Project owner context contributions at `crates/agentdash-application/src/session/assembler.rs:386`; it is owner context composition, not runtime delivery.
- `build_owner_session_plan_contribution`: builds owner `SessionPlan` fragments from VFS/MCP/executor at `crates/agentdash-application/src/session/assembler.rs:422`; despite the `SessionPlan` name, it contributes context bundle content for the owner launch surface.
- `owner_scope_phase`: maps owner scope to context build phase at `crates/agentdash-application/src/session/assembler.rs:480`; this is context bundle construction policy.
- `prepare_owner_bootstrap_vfs`: builds/updates owner VFS, applies access grants, lifecycle mounts, canvas mounts, and skill asset projection at `crates/agentdash-application/src/session/assembler.rs:549`; this is frame surface composition.
- `resolve_owner_capabilities`: resolves owner capability state from agent directives, workflow directives, companion candidates, presets, and final VFS runtime binding context at `crates/agentdash-application/src/session/assembler.rs:631`; this is frame capability surface composition.
- `apply_skill_baseline`: derives skill baseline from VFS and identity at `crates/agentdash-application/src/session/assembler.rs:699`; it contributes to `CapabilityState.skill.skills` in the frame surface.
- `build_owner_context_bundle`: composes owner context bundle and session plan fragments at `crates/agentdash-application/src/session/assembler.rs:721`; this is frame/context handoff.
- `normalize_owner_bootstrap_mcp_projection`: merges request MCP, agent preset MCP, resolver output, and platform scoped MCP into runtime MCP declarations at `crates/agentdash-application/src/session/assembler.rs:1516`; this is MCP surface normalization for `FrameSurfaceDraft`.
- `compose_owner_bootstrap`: orchestrates VFS, capability, workspace module skill projection, MCP, context bundle, audit, workspace defaults, and `SessionAssemblyBuilder` output at `crates/agentdash-application/src/session/assembler.rs:772`; this is the main owner composer and should no longer be `pub(in crate::session)`.
- `compose_owner_bootstrap_to_frame`: converts owner composition into `AgentFrameBuilder + AssemblyLaunchExtras` at `crates/agentdash-application/src/session/assembler.rs:889`; this should become a frame construction composer helper or disappear behind a direct `FrameConstructionService` call.
- `AgentLevelMcp`: used by ProjectAgent composer as owner agent MCP input (`composer_project_agent.rs:96`); if only owner bootstrap uses it, move with owner composer. If other non-owner paths use it, keep a neutral capability/composer module, not session.
- Owner bootstrap-specific tests in `assembler.rs`: MCP projection tests at `crates/agentdash-application/src/session/assembler.rs:1635`, `crates/agentdash-application/src/session/assembler.rs:1665`, and `crates/agentdash-application/src/session/assembler.rs:1707`; audit trigger tests at `crates/agentdash-application/src/session/assembler.rs:1854`, `crates/agentdash-application/src/session/assembler.rs:1866`, and `crates/agentdash-application/src/session/assembler.rs:1878`. These should move with the owner composer module.

Recommended destination shape:

```text
workflow/frame_construction/
  owner_composer.rs
    OwnerFrameComposer or compose_owner_bootstrap_to_frame(...)
    OwnerBootstrapSpec
    OwnerScope
    OwnerPromptLifecycle
    owner VFS/capability/MCP/context helpers
  composer_project_agent.rs
    ProjectAgent-specific lookup + subject assignment + calls owner_composer
```

The owner composer will still depend on existing session-neutral or historically session-named helpers such as `build_session_context_bundle`, `SessionContextBundle`, `SessionPlan` fragment builders, and capability projection helpers until those are separately renamed. That dependency is acceptable if ownership is inverted: workflow/frame construction calls those lower-level utilities directly; session no longer owns the business surface composition API.

### Functions That Should Stay In Session

These functions/types are not owner bootstrap surface composition and should remain in session for Phase 3 unless a later task explicitly moves broader composers:

- Launch stages and planner code in `session/launch/*`: `LaunchPlan`, turn preparation, connector start, accepted commit, stream attach. The spec says pipeline after `FrameLaunchEnvelope` remains session launch/runtime delivery.
- Runtime registry, turn supervisor, hub, eventing, persistence, runtime commands, terminal effects, pending queue, continuation, compaction, and lineage modules under `session/`. These are RuntimeSession delivery/trace boundaries.
- `RuntimeContextInspectionPlan` may remain test-only in `session/construction.rs` only if it no longer carries owner surface fixture fields. It is currently `#[cfg(test)]` via `session/mod.rs:12`; its old fields are Phase 2 cleanup, not Phase 3 owner composition.
- `SessionAssemblyBuilder` can temporarily remain if lifecycle/companion composers still use it. It currently owns generic VFS/capability/MCP/context-to-`FrameSurfaceDraft` conversion (`crates/agentdash-application/src/session/assembly_builder.rs:101`, `crates/agentdash-application/src/session/assembly_builder.rs:350`, `crates/agentdash-application/src/session/assembly_builder.rs:403`). For Phase 3, move it only if doing so does not collide with Phase 1/2. If moved, it should become a neutral frame construction helper because `project_assembly_to_frame` writes `FrameSurfaceDraft` to `AgentFrameBuilder` at `crates/agentdash-application/src/session/assembly_builder.rs:410`.
- Companion and lifecycle composer functions can stay for this phase if the task is interpreted narrowly as owner bootstrap only:
  - `compose_lifecycle_node_to_frame`, `compose_lifecycle_node_with_audit`, and `LifecycleNodeSpec` at `crates/agentdash-application/src/session/assembler.rs:908`, `crates/agentdash-application/src/session/assembler.rs:1057`, and `crates/agentdash-application/src/session/assembler.rs:1285`.
  - `compose_companion_to_frame`, `compose_companion`, `compose_companion_with_workflow_to_frame`, `compose_companion_with_workflow`, `CompanionSpec`, and `CompanionWorkflowSpec` at `crates/agentdash-application/src/session/assembler.rs:934`, `crates/agentdash-application/src/session/assembler.rs:1265`, `crates/agentdash-application/src/session/assembler.rs:963`, `crates/agentdash-application/src/session/assembler.rs:1350`, `crates/agentdash-application/src/session/assembler.rs:1299`, and `crates/agentdash-application/src/session/assembler.rs:1334`.

Important nuance: `design.md` says the eventual workflow/frame construction target can own owner/activity/companion composers. The Phase 3 checklist says "owner bootstrap-facing types/functions", so the minimum safe migration should move Project/Story owner bootstrap first and leave lifecycle/companion paths stable. Moving lifecycle/companion at the same time widens imports and test churn without being required to satisfy the owner bootstrap acceptance criterion.

### Current Code Patterns

- `FrameConstructionService` is already the production entry point for launch envelope construction: `construct_launch_envelope` starts at `crates/agentdash-application/src/workflow/frame_construction/mod.rs:85`.
- `FrameConstructionService` still manufactures a session assembler at `crates/agentdash-application/src/workflow/frame_construction/mod.rs:167`, which keeps the owner composition dependency inverted.
- ProjectAgent frame construction resolves project, agent config, workspace, subject assignment, executor config, and identity in `composer_project_agent.rs`, then delegates actual owner surface assembly into `session::assembler` (`crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:51`, `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:54`, `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:57`, `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:72`, `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:80`).
- `SessionRequestAssembler` is broad infrastructure injection, not inherently runtime session state: it stores `VfsService`, `CanvasRepository`, `BackendAvailability`, `RepositorySet`, `PlatformConfig`, audit bus, skill discovery, and companion parent facts provider at `crates/agentdash-application/src/session/assembler.rs:96`. Owner composer can receive a narrower deps struct from `FrameConstructionService`.
- Owner VFS composition combines project VFS mounts, existing VFS, owner mount target, access grants, active workflow lifecycle mount, canvas mounts, lifecycle skill projection, and companion system skill assets at `crates/agentdash-application/src/session/assembler.rs:549`.
- Owner capability composition builds workflow/tool/companion contributions and calls `CapabilityResolver::resolve_checked` with `McpRuntimeBindingContext { vfs }` at `crates/agentdash-application/src/session/assembler.rs:631`.
- Owner context bundle composition converts MCP declarations to runtime servers, resolves Story workspace-declared sources, emits owner contribution and session plan fragments, then calls `build_session_context_bundle` at `crates/agentdash-application/src/session/assembler.rs:721`.
- Owner bootstrap composition currently sets workspace module visibility on `CapabilityState`, normalizes MCP projection, decides effective context bundle by owner prompt lifecycle, audits bundle fragments, builds `SessionAssemblyBuilder`, and returns a prepared assembly at `crates/agentdash-application/src/session/assembler.rs:772`.
- `SessionAssemblyBuilder` already has the desired handoff abstraction: `to_surface_draft` builds `FrameSurfaceDraft` at `crates/agentdash-application/src/session/assembly_builder.rs:350`, and `project_assembly_to_frame` writes it into `AgentFrameBuilder` and `AssemblyLaunchExtras` at `crates/agentdash-application/src/session/assembly_builder.rs:403`.
- `FrameLaunchEnvelope` still has parallel transitional fields documented at `crates/agentdash-application/src/workflow/runtime_launch.rs:99`, `crates/agentdash-application/src/workflow/runtime_launch.rs:101`, `crates/agentdash-application/src/workflow/runtime_launch.rs:103`, and `crates/agentdash-application/src/workflow/runtime_launch.rs:105`. That is Phase 1 conflict surface, not Phase 3 owner composer work.
- `FrameLaunchEnvelope::launch_capability_state` and `launch_vfs` still fallback to transitional fields at `crates/agentdash-application/src/workflow/runtime_launch.rs:125` and `crates/agentdash-application/src/workflow/runtime_launch.rs:134`; Phase 1 must remove fallback semantics.
- `RuntimeContextInspectionPlan::surface_draft_or_fixture_projection` builds or patches `FrameSurfaceDraft` from fixture fields at `crates/agentdash-application/src/session/construction.rs:290`; Phase 2 must delete it.

### Minimum Safe Write Sequence

The minimum sequence assumes Phase 1 and Phase 2 may be in flight. Avoid editing their hot files until they land.

1. Wait for or verify Phase 1 cleanup if possible.
   - Conflict files: `crates/agentdash-application/src/workflow/runtime_launch.rs`, `crates/agentdash-application/src/workflow/frame_construction/mod.rs`, `crates/agentdash-application/src/session/launch/plan.rs`.
   - Reason: Phase 1 removes `FrameLaunchEnvelope.executor_config/capability_state/vfs/mcp_servers`, `sync_transitional_fields_from_surface_draft`, and fallback accessors. Phase 3 should not build against old parallel fields.

2. Wait for or verify Phase 2 cleanup if possible.
   - Conflict files: `crates/agentdash-application/src/session/construction.rs`, `crates/agentdash-application/src/session/assembly_builder.rs`, `crates/agentdash-application/src/session/assembler.rs` tests, `crates/agentdash-application/src/session/hub/facade.rs`, `crates/agentdash-application/src/session/launch/plan.rs`, `crates/agentdash-application/src/session/hub/tests.rs`, `crates/agentdash-application/src/workspace_module/tools.rs`.
   - Reason: Phase 2 deletes old projection fixture fields and rewrites tests to direct `FrameSurfaceDraft`. Phase 3 should not move tests/helpers that Phase 2 is about to delete.

3. Introduce workflow-owned owner composer module.
   - Add `workflow/frame_construction/owner_composer.rs` or equivalent.
   - Move `OwnerScope`, `OwnerBootstrapSpec`, `OwnerPromptLifecycle`, owner audit helpers, owner context contribution helpers, owner VFS/capability/context/MCP helpers, `compose_owner_bootstrap`, and `compose_owner_bootstrap_to_frame`.
   - Give it a deps struct sourced from `FrameConstructionService` instead of `SessionRequestAssembler`.
   - Keep function names initially if needed for small diff, but export from `workflow::frame_construction` rather than `session`.

4. Update ProjectAgent composer to call workflow-owned owner composer directly.
   - Replace `use crate::session::{AgentLevelMcp, OwnerBootstrapSpec, OwnerScope}` in `composer_project_agent.rs:10`.
   - Replace `svc.assembler().compose_owner_bootstrap_to_frame(...)` at `composer_project_agent.rs:79`-`80` with `owner_composer::compose_owner_bootstrap_to_frame(svc.owner_composer_deps(), ...)` or a `FrameConstructionService` method.

5. Shrink or remove `FrameConstructionService::assembler()`.
   - If lifecycle/companion still use it, keep a narrower helper for those paths only.
   - The acceptance check should show no owner bootstrap path enters `session::assembler`.

6. Update `session/mod.rs` re-exports.
   - Remove `OwnerBootstrapSpec`, `OwnerPromptLifecycle`, and `OwnerScope` from session re-exports (`crates/agentdash-application/src/session/mod.rs:62`).
   - Keep companion/lifecycle exports only if their code remains in session.
   - Re-export owner composer types from `workflow/frame_construction` only where needed.

7. Move or rewrite owner-specific tests with the module.
   - Move MCP projection and audit lifecycle unit tests to owner composer tests.
   - If `SessionAssemblyBuilder` remains in session, tests can import it as an implementation helper, but assertions should be about `FrameSurfaceDraft`/`AssemblyLaunchExtras`, not `RuntimeContextInspectionPlan.projections`.

8. Update specs only after implementation settles.
   - Session spec should state why session owns launch/delivery/event/persistence only.
   - Workflow spec should state why frame construction owns owner surface composition.
   - Avoid recording old wrong structure except as necessary migration caveat in task research.

### Phase 1 / Phase 2 Conflict Map

- `workflow/runtime_launch.rs`: Phase 1 owns this. Phase 3 should avoid modifying `FrameLaunchEnvelope` fields/accessors except to compile after Phase 1.
- `workflow/frame_construction/mod.rs`: Phase 1 may edit `build_envelope_from_frame`; Phase 3 edits imports/deps/assembler helper. Coordinate carefully.
- `session/launch/plan.rs`: Phase 1 and Phase 2 both likely edit fixture envelope construction; Phase 3 should avoid broad rewrites here.
- `session/construction.rs`: Phase 2 owns old `ConstructionProjections.mcp_servers/capability_state` and `surface_draft_or_fixture_projection`; Phase 3 should not preserve or move those fields.
- `session/assembly_builder.rs`: Phase 2 may update `apply_session_assembly`; Phase 3 may want to move `project_assembly_to_frame`/`AssemblyLaunchExtras`. Keep this move last or skip if Phase 2 is active.
- `session/assembler.rs`: Phase 3 owns owner composer extraction, but Phase 2 owns `apply_session_assembly_tests` and fixture assertions near `crates/agentdash-application/src/session/assembler.rs:2111`. Split edits by region if humans/subagents overlap.
- `session/hub/tests.rs` and `workspace_module/tools.rs`: Phase 2 fixture cleanup; Phase 3 should not edit unless owner composer migration breaks imports.

### Tests To Preserve Or Rewrite

Preserve/rewrite behavior tests:

- Owner MCP normalization behavior:
  - `owner_bootstrap_mcp_projection_grants_agent_preset_without_directive` at `crates/agentdash-application/src/session/assembler.rs:1635`.
  - `owner_bootstrap_mcp_projection_dedupes_by_source_priority` at `crates/agentdash-application/src/session/assembler.rs:1665`.
  - `owner_bootstrap_mcp_projection_maps_platform_scoped_server_to_platform_capability` at `crates/agentdash-application/src/session/assembler.rs:1707`.
  - Reason: these protect MCP runtime declaration merge, dedupe priority, and platform scoped capability mapping. Move with `normalize_owner_bootstrap_mcp_projection`.
- Owner audit lifecycle behavior:
  - `owner_bootstrap_audit_trigger_requires_effective_bundle` at `crates/agentdash-application/src/session/assembler.rs:1854`.
  - `owner_rehydrate_audit_trigger_maps_to_composer_rebuild` at `crates/agentdash-application/src/session/assembler.rs:1866`.
  - `owner_plain_lifecycle_never_emits_owner_audit` at `crates/agentdash-application/src/session/assembler.rs:1878`.
  - Reason: these protect context audit emission policy for bootstrap vs rehydrate vs plain.
- Companion bundle slicing tests at `crates/agentdash-application/src/session/assembler.rs:1795`, `crates/agentdash-application/src/session/assembler.rs:1806`, `crates/agentdash-application/src/session/assembler.rs:1832`, and `crates/agentdash-application/src/session/assembler.rs:1843`.
  - Reason: preserve if companion code remains in session; move only if companion composer is moved in a later/wider phase.
- Hub/runtime behavior tests:
  - `build_tools_filters_relay_mcp_with_initial_capability_state` at `crates/agentdash-application/src/session/hub/tests.rs:494`.
  - `replace_current_capability_state_updates_active_turn_capability_state` at `crates/agentdash-application/src/session/hub/tests.rs:1042`.
  - `pending_capability_state_transition_applies_on_next_prompt_and_clears_meta` at `crates/agentdash-application/src/session/hub/tests.rs:1403`.
  - Reason: these protect active turn, tool assembly, runtime command replay, and delivery semantics; rewrite fixture setup to complete `FrameSurfaceDraft` if Phase 2 removes projection fields.
- Launch planner behavior tests in `session/launch/plan.rs` around `LaunchPlanner::plan` should be preserved but rewritten to construct `FrameLaunchEnvelope` from complete typed surface rather than `RuntimeContextInspectionPlan` fixture helper (`crates/agentdash-application/src/session/launch/plan.rs:428` and `crates/agentdash-application/src/session/launch/plan.rs:446`).
- Runtime launch tests in `workflow/runtime_launch.rs` should be preserved if they assert launch-ready typed surface, but transitional-field sync tests should be removed after Phase 1 (`crates/agentdash-application/src/workflow/runtime_launch.rs:157`, test starts at `crates/agentdash-application/src/workflow/runtime_launch.rs:176`, `crates/agentdash-application/src/workflow/runtime_launch.rs:203`, `crates/agentdash-application/src/workflow/runtime_launch.rs:219`).

Delete or rewrite old fixture/compatibility shell tests:

- `apply_session_assembly_tests` under `crates/agentdash-application/src/session/assembler.rs:2111` are mostly fixture merge semantics for `RuntimeContextInspectionPlan`; only `prepared_surface_is_handed_off_as_frame_surface_draft` at `crates/agentdash-application/src/session/assembler.rs:2164` is conceptually valuable. Rewrite that as a direct `SessionAssemblyBuilder`/owner composer to `FrameSurfaceDraft` test, or move it with `project_assembly_to_frame`.
- Tests that assert `projections.mcp_servers` or `projections.capability_state` merge/clear behavior should be deleted, because hard cutover requires those fields to disappear. Current direct references include `crates/agentdash-application/src/session/assembler.rs:2142`, `crates/agentdash-application/src/session/assembler.rs:2201`, and `crates/agentdash-application/src/session/assembler.rs:2206`.
- `RuntimeContextInspectionPlan::surface_draft_or_fixture_projection` tests should be deleted or rewritten to require `frame_surface_draft` directly. The helper is the exact old compatibility shell Phase 2 removes (`crates/agentdash-application/src/session/construction.rs:290`).
- Test helpers in `session/hub/tests.rs` and `workspace_module/tools.rs` that populate `construction.projections.capability_state` / `mcp_servers` should be rewritten to populate `FrameSurfaceDraft` directly (`crates/agentdash-application/src/session/hub/tests.rs:229`, `crates/agentdash-application/src/session/hub/tests.rs:469`, `crates/agentdash-application/src/session/hub/tests.rs:470`, `crates/agentdash-application/src/workspace_module/tools.rs:1486`).

### External References

- No external references were needed. This is an internal boundary migration driven by Trellis task artifacts, local specs, and current Rust code.

### Related Specs

- `.trellis/spec/backend/session/architecture.md` - RuntimeSession is delivery/trace substrate; AgentFrame is surface fact source.
- `.trellis/spec/backend/session/session-startup-pipeline.md` - frame construction to launch pipeline; needs target-state cleanup after Phase 1/2/3.
- `.trellis/spec/backend/session/execution-context-frames.md` - connector projection stays runtime/launch stage.
- `.trellis/spec/backend/session/runtime-execution-state.md` - runtime registry, active turn, delivery command, persistence stay in session.
- `.trellis/spec/backend/workflow/architecture.md` - AgentFrame and RuntimeSession vocabulary.
- `.trellis/spec/backend/runtime-gateway.md` - MCP runtime action reads active snapshot/current AgentFrame.
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` - capability/MCP projection semantics.

## Caveats / Not Found

- The current code still contains Phase 1 transitional `FrameLaunchEnvelope` fields and fallback accessors. Phase 3 implementation should avoid taking dependencies on them.
- The current code still contains Phase 2 old projection fixture fields (`ConstructionProjections.mcp_servers`, `ConstructionProjections.capability_state`) and `surface_draft_or_fixture_projection`. Phase 3 tests should not preserve those shells.
- I did not find a separate existing `workflow/frame_construction` owner composer module; current ProjectAgent composer delegates owner surface composition back to `session::assembler`.
- I did not find production code outside `composer_project_agent.rs` that uses `OwnerBootstrapSpec`/`OwnerScope`; remaining references are session exports and tests, so the owner type move should be contained.
- The source comment in `session/assembler.rs` still describes five startup paths owned by `SessionRequestAssembler` (`crates/agentdash-application/src/session/assembler.rs:5`). That comment will become stale once owner composition moves; update it only in implementation/spec phases, not in this research pass.
