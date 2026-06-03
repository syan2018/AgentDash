# FrameLaunchEnvelope 收敛 Session 启动解析

## Goal

把 Session launch 中关于 owner、context、capability、VFS、MCP、execution profile、runtime delivery 的长流程解析上提到 Frame construction，形成 launch-ready `FrameLaunchEnvelope`。Session runtime 只消费 envelope 并管理 turn / connector / stream / terminal 生命周期。

## User Value

- Session 逻辑职责变窄，避免 RuntimeSession 在业务控制面中继续承担事实解析。
- Frame revision 能完整说明一次 runtime session prompt 的执行 surface。
- Capability grant、Frame hot update、Agent reuse、workflow activity launch 都能通过统一 Frame envelope 进入 connector。

## Confirmed Facts

- `RuntimeLaunchRequest` 当前既是 `AgentFrame` 投影，又被 construction / planner 当作 mutable optional bag 补齐。
- `LaunchPlanner` 仍校验并读取 `working_directory / executor_config / typed_capability_state` 等基础事实。
- `AppStateSessionConstructionProvider` 已经负责从 runtime session 找 frame，再根据 story/task/lifecycle/project agent/companion 等路径 compose frame。
- Session launch orchestrator 在 plan 后还会把 initial capability state 写入新的 frame revision。

## Requirements

- 定义 `FrameLaunchEnvelope`，字段必须 launch-ready，不能依赖 Session planner 补齐基础控制面事实。
- Frame construction 负责 owner/context/capability/VFS/MCP/execution profile/runtime delivery refs 的解析与 frame revision 写入。
- Session planner 只处理 prompt resolution、turn lifecycle、hook runtime attach、connector accepted、backend lease、terminal effect 等运行职责。
- 明确 `LifecycleAgent.current_frame_id`、frame current revision、runtime delivery frame 的唯一权威规则。
- Frame transition replay 应发生在 Frame construction / envelope 生成阶段，并产出 effective capability surface。
- 去除或重命名 `RuntimeLaunchRequest` 中混合层级的字段，推荐拆为 `FrameLaunchIntent`、`AgentFrameConstructionPlan / FrameConstructionResult`、`FrameLaunchEnvelope`、`ConnectorLaunchInput / ExecutionContextProjection`。

## Acceptance Criteria

- [ ] `RuntimeLaunchRequest` 不再作为跨 construction / planner 的半成品 optional bag。
- [ ] `LaunchPlanner` 不再负责补齐 owner/context/capability/VFS/MCP 基础事实。
- [ ] Frame construction 输出的 envelope 包含非 optional working directory、executor config、capability state、VFS、MCP、context bundle。
- [ ] 新 frame revision 写入后 current frame invariant 明确且测试覆盖。
- [ ] pending runtime capability transition replay 归属 Frame construction。
- [ ] backend execution lease 仍留在 Session launch，但 planner 消费 envelope 的 backend hint，不直接用 raw VFS 推导业务事实。
- [ ] Session runtime tests 证明 launch 仍正确处理 turn claim、connector accepted、stream attach、terminal cleanup。

## Out Of Scope

- 不在本任务中重做 terminal callback anchor；依赖 child anchor task 的结果。
- 不在本任务中重做 lifecycle artifact scope。
- 不改变 connector 协议本身，除非 envelope 到 connector input 映射需要同步命名。

## Dependency Notes

- 建议在 `runtime-session-frame-assignment-anchor` 后实施，以复用稳定的 session-to-frame anchor。
- 可与 `scoped-lifecycle-artifacts` 并行规划，但实现时应避免同时大改 session assembler 与 artifact loader。
