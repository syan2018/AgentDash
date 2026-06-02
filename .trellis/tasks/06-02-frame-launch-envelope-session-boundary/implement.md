# FrameLaunchEnvelope Implement Plan

## Checklist

- [ ] Introduce `FrameRuntimeSurface`, `FrameLaunchIntent`, `AgentFrameConstructionPlan` / `FrameConstructionResult`, `FrameLaunchEnvelope`.
- [ ] Move `RuntimeLaunchRequest::from_frame` behavior into `FrameRuntimeSurface`.
- [ ] Move command/extras merge out of mutable request and into envelope construction.
- [ ] Move pending runtime transition replay into frame construction.
- [ ] Make envelope fields required where launch cannot proceed without them.
- [ ] Narrow `LaunchPlannerInput` to consume `FrameLaunchEnvelope`.
- [ ] Remove planner fallback checks for owner/context/capability/VFS/MCP.
- [ ] Define and enforce current frame invariant.
- [ ] Split durable frame context summary from launch-time full context payload.
- [ ] Preserve backend execution lease as Session runtime effect fed by envelope hint.
- [ ] Update tests and generated contracts if exposed.

## Validation Commands

- [ ] `cargo test -p agentdash-application session::launch`
- [ ] `cargo test -p agentdash-application workflow::runtime_launch`
- [ ] `cargo test -p agentdash-api session_construction_provider`
- [ ] `cargo check`

## Risk Points

- This task touches high-centrality launch code; do it after anchor task stabilizes trace lookup.
- Avoid mixing connector protocol changes with envelope boundary changes unless unavoidable.
