# Session Capability Projection 链路参考

## 当前链路

```text
owner / session meta / request facts
  -> SessionConstructionPlan(seed)
  -> assembler / bootstrap 分散填充 VFS、MCP、capability_state
  -> finalize 合并 pending runtime patch / VFS overlay
  -> LaunchExecution / ExecutionContext
  -> TurnExecution / SessionProfile 保存运行态快照
```

运行期变化另走一条链：

```text
tool / workflow runtime action
  -> 手写 after CapabilityState
  -> SessionCapabilityService 补一部分派生维度
  -> pending command 或 live transition
  -> connector tool hot update / next turn apply
```

context 查询还有一条旁路：

```text
owner context plan
  -> runtime_surface
  -> finalize 合并 pending overlay
```

因此 `runtime_surface` 可能早于最终 VFS，Skill baseline 也在 bootstrap、inspect、live transition 多处重复派生。

## 不合理之处

- `CapabilityResolver` 只解析 tool / MCP / companion，但 `CapabilityState` 实际还承载 VFS / Skill，调用点需要自己记得补齐依赖。
- `Vfs` 被当成运行态快照到处传递，而不是由 owner、session meta、workflow mount directives、runtime command 等意图统一投影。
- pending runtime command 需要表达“下轮要应用什么变化”，而不是把闭包后的 `CapabilityState` 当成长期事实。
- `runtime_surface` 是 final VFS 的 DTO 投影，却在 context query 中早于 finalize 生成。
- `SessionProfile` / `TurnExecution` 是 connector 与 hot-update 所需缓存；construction facts 需要保持为 owner、session meta、runtime command 等可解释输入。

## 目标链路

```text
durable intents
  - owner / project / story / task facts
  - session meta: visible_canvas_mount_ids
  - workflow / step mount directives
  - runtime command patch or transition
  - request / agent / platform MCP declarations
  -> CapabilityProjectionPipeline
     1. resolve tool / MCP / companion
     2. resolve effective VFS
     3. derive VFS-dependent dimensions: Skill, guidelines
     4. assemble final CapabilityState
     5. produce query-only runtime_surface
  -> SessionConstructionPlan(final)
  -> LaunchExecution / ExecutionContext
```

运行期变化也走同一个 pipeline：

```text
runtime action intent
  -> patch / transition input
  -> CapabilityProjectionPipeline
  -> complete CapabilityState
  -> live transition or pending command
```

## 重构原则

- `CapabilityResolver` 保持纯解析：tool / MCP / companion。
- `CapabilityProjectionPipeline` 负责依赖闭包：VFS -> Skill / guidelines -> final capability state。
- `runtime_surface` 只从 final VFS 生成，不参与状态维护。
- `SessionProfile` / `TurnExecution` 只保存 projection cache，不作为 owner/context 事实源。
- pending command 保存 typed patch：tool / companion 维度、VFS overlay 与 phase metadata；replay 后再由 pipeline 生成闭包 projection。

## 本任务完成态

- `/sessions/{session_id}/context` 的 `runtime_surface` 基于 finalize 后 VFS。
- construction finalize、context inspect、live transition、pending transition 使用同一 capability projection normalizer。
- Skill baseline discovery 只有一个主入口。
- Canvas 可见性通过工具级测试覆盖 meta、VFS、Skill、事件。
- pending runtime command 不再持久化完整 after-state 快照，而是持久化可解释 patch。

## 完整重构覆盖矩阵

### Phase 1: Projection Normalizer

目标：先消灭当前补丁散落。

- 建立 `CapabilityProjectionPipeline` / normalizer。
- 让 construction finalize、context inspect、live transition、pending transition 共用 normalizer。
- `runtime_surface` 后移到 final VFS 之后。
- Skill baseline discovery 只保留一个主入口。
- 删除或替换旧的局部 Skill/VFS 派生路径。

### Phase 2: Runtime Command Intent 化

目标：让 pending runtime command 不再保存完整 after-state 快照作为事实。

- 将 `PendingCapabilityStateTransition.state` 拆为 typed patch / intent。
- patch 至少覆盖 tool directives、mount directives、VFS overlay、MCP delta、phase metadata。
- next-turn launch / context query / live apply 都 replay patch 得到 effective projection。
- repository 与恢复逻辑改为持久化 intent，而不是持久化闭包后的 projection。

### Phase 3: Fact Source 边界收口

目标：清理“cached runtime state 兜底构建事实”的灰区。

- `SessionProfile` / `TurnExecution` 明确只作为 projection cache。
- construction 不再把 cached capability 当常规 VFS/MCP 事实源；恢复场景应通过明确输入与 trace 表达。
- owner/session meta/workflow/runtime command 成为可解释的 durable inputs。
- `validate_for_launch` 扩展为 pipeline 输出 gate，覆盖 VFS、MCP、Skill、runtime surface 的一致性。

### Phase 4: Surface 与 Derived Dimensions 收束

目标：所有派生 DTO 和 VFS 依赖维度都从 final projection 生成。

- `runtime_surface`、guidelines、session_capabilities、tool schema delta 都由 final capability projection 派生。
- Project / Story / Task preview surface 若复用 Session runtime 逻辑，应迁入同一 projection 边界；若语义不同，应显式分层。
- 前端只消费 projection DTO，不自行推断 mount / capability 可见性。
