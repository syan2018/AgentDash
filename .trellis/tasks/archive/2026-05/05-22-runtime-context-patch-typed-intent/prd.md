# Runtime Context Patch Typed Intent 标准化

## Goal

将 runtime command store 中的 `RuntimeContextPatch` 从维度级 replacement 收束为真正的 typed intent，并把前端 Session 右侧栏的上下文展示接入同一份当前 runtime projection 状态。pending runtime command 只表达“下一轮要应用的 runtime context 变化”，final `CapabilityState`、effective VFS、MCP、Skill baseline、guidelines 与 runtime surface 继续由 capability projection pipeline 生成；前端只展示当前 session 已确认的 final projection，不通过一组游离快照推断右侧栏上下文。

## Background

上一轮 `Session Capability Projection Pipeline 收束` 已完成 VFS / Skill / runtime surface 的 final projection 链路，并把 pending command payload 从完整 `CapabilityState` 快照迁移为 `RuntimeContextPatch`。重新 review 后确认还剩一个语义缺口：当前 patch 已经不再保存完整 `CapabilityState`，但 `tool` / `companion` 仍是维度级 replacement，且 `RuntimeContextPatch::from_target_state` 仍从闭包后的 `after_state` 反推 patch。

这会让 runtime command payload 仍然带有 projection cache 的味道。更标准的模型应当从 source intent 入手：

- workflow / lifecycle activation 提供 capability directives、MCP 结果、VFS overlay、mount directives。
- runtime command store 保存这些可解释变化。
- construction / context query / next-turn launch replay intent 后，再由 projection pipeline 生成闭包状态。

用户进一步指出：当前 Session 右侧栏展示的上下文也需要关注，它不应与 session 当前状态脱节，也不应为了刷新展示而生成一堆难以解释的快照。右侧栏应当消费清晰的 current session runtime state：这个 state 由 `/sessions/{id}/context` 等当前投影接口刷新，携带 session identity / source identity / freshness 状态，作为 WorkspacePanel 的唯一上下文输入。

## Confirmed Facts

- 当前 `RuntimeContextPatch` 位于 `crates/agentdash-application/src/session/types.rs`，字段为 `tool: Option<ToolDimension>`、`companion: Option<CompanionDimension>`、`vfs_overlay: Option<Vfs>`、`mount_directives: Vec<MountDirective>`。
- 当前 `RuntimeContextPatch::from_target_state` 会从完整 `CapabilityState` 复制 `tool`、`companion`、`vfs.active`。
- `apply_runtime_context_patch` 会整体替换 `tool` / `companion`，然后合并 VFS overlay 与 mount directives。
- `StepActivation` 已经产出 `capability_state`、`mcp_servers`、`capability_keys`、`lifecycle_vfs`、`mount_directives`；`StepActivationInput` 已经包含 workflow / step capability directives 来源。
- pending transition 当前由 `PendingRuntimeContextTransitionInput { before_state, after_state, ... }` 进入 hub，再通过 `RuntimeContextTransition::to_pending_capability_state_transition` 反推 patch。
- repository 使用 `runtime_commands.payload_json` 保存 payload；当前预研阶段可以调整 payload shape，不需要为旧 JSON shape 保留兼容路径。
- 前端 `SessionPage` 当前用 `loadedSessionContext` 局部 state 保存 `/sessions/{id}/context` 结果，并用 `session_id + source_key` 防止切换 session 后误用旧数据。
- `WorkspacePanel` 当前接收从 `SessionPage` 拆出的 `contextSnapshot`、`runtimeSurface`、`sessionCapabilities`、`hookRuntime`、`workflowRuns` 等字段；它们还不是一个带 loading / freshness / revision 的 current session runtime state。
- Canvas 事件后前端会刷新 session context 再打开右侧栏，但 capability/state 变化与右侧栏当前 state 的关联还没有统一状态机表达。

## Requirements

- `RuntimeContextPatch` 必须表达 typed intent，而不是维度级闭包状态。
- 生产路径必须移除 `RuntimeContextPatch::from_target_state` 或等价的 full-state 反推 patch 逻辑。
- workflow / lifecycle pending transition 必须从 `StepActivation` 或等价 source intent 构造 patch。
- patch schema 必须显式表达：
  - tool capability directives 或已归约的 runtime tool intent；
  - MCP server 变化，至少能表达本轮 runtime command 的 effective MCP replace / set；
  - companion 变化，至少能表达 agent candidate replace / set；
  - VFS overlay；
  - mount directives。
- phase metadata 继续由 `PendingCapabilityStateTransition` 承载，包括 `run_id`、`lifecycle_key`、`phase_node`、`capability_keys`、`source_turn_id`、`created_at`。
- replay 必须以 construction base projection 为输入，应用 typed intent 后再交给 capability projection normalizer 补齐 VFS / Skill / MCP / runtime surface 派生维度。
- context query、next-turn launch、pending apply event 都必须使用同一 replay 入口。
- live transition 可以继续消费 final `after_state` 服务 connector hot update，但写入 pending command 的路径必须保存 typed intent。
- Session 右侧栏必须消费一个清晰的 current session runtime state，而不是多个互相独立的局部快照。
- current session runtime state 必须携带 session id、owner/source key、加载状态、错误状态与最后成功投影，避免 session 切换、owner binding 切换或运行态事件刷新时展示旧上下文。
- `WorkspacePanel` / context tab / VFS tab 继续只消费 final projection DTO；其输入应来自统一 state 容器或 hook，而不是在页面层散落拆分。
- capability/runtime 事件触发右侧栏刷新时，应表现为对当前 state 的 invalidate/refetch，不生成新的长期快照事实源。
- 相关 spec 必须更新，让 future agent 明确 runtime command store 保存 intent，`CapabilityState` 是 projection 输出。

## Acceptance Criteria

- [x] `RuntimeContextPatch` schema 不再直接包含 `ToolDimension` / `CompanionDimension` replacement 字段。
- [x] 生产代码不再调用 `RuntimeContextPatch::from_target_state`，该函数被删除。
- [x] `PendingRuntimeContextTransitionInput` 或其构造路径携带 typed patch / intent，不再只靠 `after_state` 生成 persisted payload。
- [x] workflow lifecycle pending activation 的 patch 来自 activation source 数据，并能 replay 得到与当前 final `CapabilityState` 等价的 tool / MCP / companion / VFS 结果。
- [x] `apply_runtime_context_patch` 与 `replay_runtime_context_patch` 入口只应用 typed intent，并保持 VFS overlay + mount directives 幂等。
- [x] `/sessions/{id}/context` 与 next-turn launch 继续通过 requested runtime commands replay 出 final projection。
- [x] repository 测试覆盖新 payload JSON，断言 payload 中没有 `tool` / `companion` replacement，也没有 `state` 字段。
- [x] runtime hub 测试覆盖 pending transition 的 event/context frame 使用 replay 后的 final projection。
- [x] `SessionPage` 右侧栏上下文改为统一 current session runtime state / hook / store，移除散落的 `loadedSessionContext` 形态。
- [x] session 切换、owner binding 切换、context 刷新失败时，右侧栏不会展示上一个 session 的 runtime surface / capabilities。
- [x] `canvas_presented` / `capability_state_changed` 等 runtime 更新后，右侧栏通过当前 state 刷新到最新 projection。
- [x] 前端测试覆盖 stale context 不展示，既有 projection 测试覆盖 VFS tab 使用最新 `runtime_surface`。
- [x] 相关 Rust 聚焦测试通过。
- [x] 相关前端 typecheck / 聚焦测试通过。

## Out of Scope

- 不修改 runtime command 数据库表结构；继续使用 `payload_json` 容器。
- 不重写 capability resolver、VFS provider、Skill loader 或 runtime surface resolver。
- 不改变 live connector hot update 的外部行为。
- 不重设计右侧栏 Tab 交互；本任务只收束其 session runtime data source。

## Open Questions

- 暂无阻塞规划的问题。实现中如发现 tool directives 无法完整表达当前 replacement 语义，优先新增明确的 `SetEffectiveToolProjection` / `SetMcpServers` 这类 typed intent，而不是恢复 full `CapabilityState` payload。
