Active task: .trellis/tasks/07-11-agent-runtime-debug-regression-tracking

You are already the Trellis implement agent. Implement directly; do not spawn another trellis-implement or trellis-check agent.

Implement ARD-004 only. The confirmed chain is:

1. `create_project_agent_run` calls Lifecycle dispatch with an effective execution profile.
2. `AgentRunLaunchAnchorFrameConstructionAdapter` persists a launch-anchor AgentFrame carrying the execution profile but no canonical Project/workspace/VFS/capability surface.
3. First `AgentRunProductDelivery` provisions the Runtime immediately.
4. `BusinessFrameSurfaceQuery` projects missing frame VFS as an empty VFS.
5. `AgentFrameNativeSurfaceCompiler` rejects it with `AgentRun VFS has no usable default mount`.

The old runtime-session owner bootstrap used to materialize the Project workspace later during connector launch; after WP08 cutover, the new Runtime admission requires the immutable AgentFrame Business Surface before binding. Fix the ordering and ownership, not the error check.

Requirements:

- Materialize and persist the canonical ProjectAgent owner Business Surface before Runtime provision.
- Reuse/extract the existing Project/workspace/VFS owner composition logic where ownership is correct; do not duplicate a reduced VFS builder if the canonical composer can be exposed cleanly.
- Preserve the per-run effective execution profile in the same final current AgentFrame revision.
- The Runtime surface compiler must remain a strict consumer/validator. Do not synthesize cwd/VFS there.
- No process-cwd, empty-directory, arbitrary-backend, compatibility, or silent fallback.
- Add a regression test that follows the real Lifecycle launch boundary with a Project/workspace mount and proves the current frame has a usable default mount before product delivery/provision.
- Add a precise negative test for a genuinely unavailable Project workspace/mount if the current canonical VFS contract permits such a state.
- Do not touch unrelated existing changes. Do not commit or push.
- Run scoped format, tests, check, and clippy appropriate to the changed crates. Report exact commands and results.

Inspect the archived PR task under `.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/`, especially workstreams 03 and 08, when validating ownership.
