# AgentFrame Construction 迁移

## 目标

把 `StepActivation`、`SessionConstructionPlan`、`LaunchPlan`、`HookSessionRuntime`、`CapabilityState`、context / VFS / MCP projection 收束为 `AgentFrame` construction，让 runtime surface 有唯一事实源。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-session-lifecycle-target-anchors-schema`
- 依赖：`06-01-lifecycle-dispatch-service`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B3 AgentFrame Construction。
- 退出贡献：RuntimeSession launch 与 hook/capability/context runtime 都从 `AgentFrame` projection 读取，不再从 business owner、Session 构造计划或 live session maps 读取。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 可以先切断 SessionConstruction / HookSessionRuntime / CapabilityState 的权威地位，即使 connector launch 暂时丢功能。
- `SessionMeta`、live session maps、hook runtime 不能继续作为 effective capability/context/VFS/MCP 的平行事实源。

## 需求

- `StepActivation` 的输出改为 `AgentFrame` delta 或 revision source：procedure、capability、context、VFS、MCP、ports、kickoff/delivery frame。
- `SessionConstructionPlan` 改为内部的 `AgentFrameConstructionPlan`；面向 connector 的 launch 改为从 frame 投影出的 `RuntimeLaunchRequest`。
- `HookSessionRuntime` 改为 `AgentFrameHookRuntime` facet；按 session 索引的 API 只保留 trace adapter 语义。
- `CapabilityState` 与 pending runtime transitions 在 RuntimeSession delivery 前写入 `AgentFrame` revision / transition provenance。
- Context bundle/projection、VFS、MCP、canvas visible surface 归 frame projection 拥有。

## 交付物

- `AgentFrameBuilder` 与内部 `AgentFrameConstructionPlan`。
- `AgentFrame -> RuntimeLaunchRequest -> ExecutionContext` projection。
- frame revision / transition provenance 策略。
- agent/frame scoped hook runtime API。
- `design.md` 与 `implement.md` 中声明的 construction / adapter 分层。

## 不承担

- 不决定 subject association。
- 不推进 Activity terminal / scheduler assignment。
- 不保留 `SessionMeta`、live maps、hook runtime 作为 parallel truth。

## 验收标准

- [ ] 一个 frame revision 能回答 LifecycleAgent 当前使用哪个 procedure、tools、MCP、VFS、context 和 runtime refs。
- [ ] RuntimeSession launch 可以从 frame data 重建，不需要从 session 反查 business owner。
- [ ] Hook runtime advance/resolution 接收 agent/frame refs，且不把 session 当 owner。
- [ ] 业务模块不把 `SessionConstructionPlan`、`LaunchPlan`、connector `ExecutionContext` 或 hook runtime internals 作为 command inputs。
- [ ] 现有 connector execution 仍能拿到所需 `ExecutionContext`，但它只能由 `AgentFrame` 投影生成。
- [ ] `AgentFrame` 满足父任务 `concept-boundaries.md` 中的 ownership 与 corruption checks。
