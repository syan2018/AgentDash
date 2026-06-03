# RuntimeSession Frame / Assignment Anchor Implement Plan

## Checklist

- [ ] Add structured anchor types in application or domain boundary.
- [ ] Decide anchor storage shape: `runtime_session_execution_anchors` table or equivalent direct repository.
- [ ] Add direct repository/service query for `runtime_session_id -> frame anchor`.
- [ ] Add direct repository/service query for `runtime_session_id -> activity anchor`.
- [ ] Write anchor when `create_runtime_session_for_agent_activity` creates a new activity runtime session.
- [ ] Write per-turn/per-assignment anchor for ContinueRoot / reused runtime session path.
- [ ] Replace `resolve_activity_session_association` implementation with direct anchor service.
- [ ] Update `LifecycleOrchestrator.on_activity_session_terminal` to consume activity anchor.
- [ ] Update `advance_current_activity` to consume activity anchor.
- [ ] Update lifecycle trace endpoint to expose frame anchor consistently.
- [ ] Add tests for exact frame assignment, freeform no-activity session, missing assignment, duplicate assignment conflict.
- [ ] Add tests for reused runtime session with sequential assignments.

## Validation Commands

- [ ] `cargo test -p agentdash-application workflow::session_association`
- [ ] `cargo test -p agentdash-application workflow::orchestrator`
- [ ] `cargo test -p agentdash-infrastructure lifecycle_anchor_repository`
- [ ] `pnpm run contracts:check`

## Risk Points

- Frame revision selection must align with the FrameLaunchEnvelope task; if multiple revisions inherit the same runtime session ref, the selected delivery frame must be deterministic.
- If one runtime session can execute multiple sequential assignments, `runtime_session_id` alone is not enough; include `turn_id` or an explicit active anchor state.
- Removing fallback may expose existing inconsistent test fixtures; fixtures should be corrected to express the intended anchor.
