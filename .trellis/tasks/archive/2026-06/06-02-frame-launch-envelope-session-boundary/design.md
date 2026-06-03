# FrameLaunchEnvelope Design

## Type Split

目标拆分：

```text
AgentFrame
  -> FrameRuntimeSurface
LaunchCommand
  -> FrameLaunchIntent
Frame construction
  -> AgentFrameConstructionPlan / FrameConstructionResult
  -> FrameLaunchEnvelope
Session planner
  -> ConnectorLaunchInput / ExecutionContextProjection / LaunchPlan
```

### FrameRuntimeSurface

只表达 frame revision 上已经持久化的 surface：

- agent/frame/procedure refs
- graph instance/activity refs
- capability surface
- context slice
- VFS surface
- MCP surface
- execution profile
- runtime delivery refs

### FrameLaunchIntent

只表达本轮 prompt / command intent：

- prompt blocks / env
- identity
- source reason tag
- follow-up hint
- terminal effect intent
- backend selection intent

### FrameLaunchEnvelope

Frame construction 输出的 launch-ready envelope：

- frame ref / agent ref / run ref
- typed capability state
- typed VFS and default working directory
- typed MCP servers
- executor config
- context bundle / continuation frame
- prompt blocks / env / identity
- terminal hook effect binding
- backend placement input facts
- base capability state and applied transition trace

context bundle 需要拆分 durable summary 与 launch payload：`AgentFrame.context_slice_json` 保存摘要，envelope 携带本轮 connector 需要的 full context bundle / context frames seed。

## Boundary Rule

`SessionLaunchOrchestrator` 在进入 `LaunchPlanner` 前必须已经拥有 envelope。`LaunchPlanner` 不做 owner / workspace / capability / VFS / MCP 推导，只做 turn 运行计划。backend execution lease 的 claim / release / fail 仍留在 Session launch，因为它是 per-turn runtime effect；但 planner 只消费 envelope 给出的 backend hint。

## Current Frame Invariant Options

推荐规则：

```text
AgentFrameBuilder.build()
  -> create next revision
  -> caller updates LifecycleAgent.current_frame_id in same service transaction boundary
```

`find_by_runtime_session` 用于 trace lookup；业务 current frame 以 `LifecycleAgent.current_frame_id` 或 repository current revision 的单一规则为准。实现前需要选定并写入 spec。

## Affected Areas

- `workflow/runtime_launch.rs`
- `workflow/frame_builder.rs`
- `session/construction_provider.rs`
- `api/bootstrap/session_construction_provider.rs`
- `session/launch/planner.rs`
- `session/launch/orchestrator.rs`
- `session/launch/plan.rs`
- `session/assembler.rs`
- `workflow/activity_activation.rs`
- `permission/service.rs`

## Validation

- Envelope construction rejects missing executor / working dir / capability before Session planner.
- Session launch planner accepts only launch-ready envelope.
- Frame transition replay tests assert effective surface is persisted before connector start.
- Current frame invariant tests assert every new effective frame can be resolved by the chosen current rule.
