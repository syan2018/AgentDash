# Surface / Transition 命名盘点

## 结论

当前不应引入 `RuntimeSurface` 作为新核心名词。它过宽，容易同时覆盖 VFS view、工具可访问面、debug UI 和完整 Agent 运行上下文。

第一轮收束采用以下命名边界：

| 名称 | 保留/新增 | 语义 |
| --- | --- | --- |
| `CapabilitySurface` | 保留 | `CapabilityConfig` / workflow step 解析后的运行期生效投影，当前覆盖 tool clusters、excluded tools、MCP servers、VFS/mount。 |
| `ResolvedVfsSurface` | 保留 | VFS API/前端浏览用的 resolved view，只描述 VFS 访问面，不表达 Agent 完整运行上下文。 |
| `RuntimeContextTransition` | 新增 | 一次 workflow phase / runtime context 切换事务，负责从 before/after `CapabilitySurface`、capability keys、delivery 状态派生事件 payload 与 pending metadata。 |
| `PendingCapabilitySurfaceTransition` | 暂保留 | 持久化字段名仍描述“待应用的 capability surface”。后续若要改名为 `PendingRuntimeContextTransition`，应配套 SessionMeta 字段和 migration 统一处理。 |
| `runtime_surface` DTO 字段 | 暂不动 | API response 中多指 VFS resolved surface 摘要，后续应在前端/API 轮次中评估是否改为 `resolved_vfs_surface`。 |

## 为什么先新增 `RuntimeContextTransition`

原路径中 live apply、pending next turn、applied on next turn 都各自手写 `capability_surface_changed` 事件 payload：

- `workflow/step_activation.rs`：live apply 后推 steering、拼事件、触发 hook。
- `workflow/orchestrator.rs`：没有 live turn 时入队 pending transition，并手写 pending 事件。
- `session/prompt_pipeline.rs`：下一轮 prompt 消费 pending transition，再手写 applied 事件和 hook。

这些入口都在表达同一件事：一次 workflow runtime context transition。差别只在 `apply_mode` 和 delivery 状态。把 payload 构建收束到 `RuntimeContextTransition` 后，后续新增 context/policy/resource budget 维度时，只需要扩展一个值对象，而不是三处 JSON。

第二轮收束后，三条生产路径进一步统一到 `SessionHub` 的 `runtime_context_transition` applier：

| 场景 | 统一入口 |
| --- | --- |
| live apply | `apply_live_runtime_context_transition` |
| 无 live turn 时 pending 入队 | `enqueue_pending_runtime_context_transition` |
| 下一轮 prompt 消费 pending | `apply_pending_runtime_context_transitions_on_turn` |

`replace_current_capability_surface`、`emit_capability_surface_changed`、`emit_capability_changed_hook`、`enqueue_pending_capability_surface_transition` 现在都是 crate 内部低层 primitive；生产代码不得绕开 applier 手写半条链。

## 当前不改名的原因

- `CapabilitySurface` 当前已经覆盖 tool/MCP/VFS，并且事件 delta 也按多维结构表达；它不是单纯工具面，因此暂不改成 `ToolAccessSurface`。
- `PendingCapabilitySurfaceTransition` 已进入 `SessionMeta.pending_capability_surface_transitions` 持久化字段；本轮不做数据库/JSON 字段重命名，避免把第一刀从行为收束扩大成迁移任务。
- `runtime_surface` API 字段分布在 VFS/session response 中，语义更接近 resolved VFS surface；它应在前端/API 专项里改，避免和 workflow transition 收束混在一起。

## 下一步 rename map

1. 若后续确认 `runtime_surface` 只承载 VFS 摘要，改为 `resolved_vfs_surface` 或 `vfs_surface_summary`。
2. 若 pending transition 后续要承载 hook snapshot revision、effective contract、Bundle delta 等非 surface 信息，再把 `PendingCapabilitySurfaceTransition` 升级为 `PendingRuntimeContextTransition`。
3. 若某个结构只描述 MCP/tool list，不要复用 `CapabilitySurface`，应命名为 `ToolAccessSurface` 或 `ToolRuntimeSurface`。
