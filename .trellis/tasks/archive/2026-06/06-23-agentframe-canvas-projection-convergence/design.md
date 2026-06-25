# Design: AgentFrame 与 Canvas Workspace Projection 路径收束

## Problem

当前系统存在两个“当前 AgentFrame”事实源：

- Agent runtime adoption 路径：工具写入新 AgentFrame revision 后，`adopt_persisted_agent_frame_revision` 从 `agent_frame_repo.get_current(agent_id)` 采用最新 frame，并更新 active runtime / connector tools / hook runtime。
- AgentRun Workspace 投影路径：`DeliveryRuntimeSelectionService.select_current_delivery` 读取 `LifecycleAgent.current_frame_id`，AgentRun Workspace 和前端 `resource_surface` 由该 frame 派生。

Canvas create/present 正好跨过这两条路径：Agent 工具实际能访问新 frame 上的 `cvs-*` mount，但前端 AgentRun Workspace 可能仍从旧 `LifecycleAgent.current_frame_id` 读取，所以右侧面板和 Canvas runtime snapshot 拿不到同一个 mount。

## Design Direction

AgentFrame revision 应成为 capability / VFS / MCP / workspace module runtime surface 的唯一权威事实源。Current delivery 只负责把 RuntimeSession 绑定到 run / agent / launch evidence，不应再携带或依赖另一份 current frame 指针。

推荐 canonical 解析：

1. RuntimeSession -> `RuntimeSessionExecutionAnchor` 解析 run / agent / launch evidence。
2. Agent -> `AgentFrameRepository.get_current(agent_id)` 解析当前 frame revision。
3. 当前 frame -> capability state / VFS surface / visible workspace module refs。
4. AgentRun Workspace、Canvas runtime snapshot、runtime tool adoption、hook runtime target 都使用同一解析结果。

## Backend Boundaries

- `DeliveryRuntimeSelection` 可以继续返回 `current_frame_id`，但它应来自 canonical frame resolver，而不是 `LifecycleAgent.current_frame_id`。
- `LifecycleAgent` 不应持有运行态 current frame 指针；它的 current delivery binding 保留 runtime session / launch frame / orchestration coordinate / status。
- `adopt_persisted_agent_frame_revision` 应采用调用方指定的 target frame 或明确校验 target frame 是最新 frame，并同步所有使用者可见的 current frame projection。
- `resolve_session_frame_vfs` 与 AgentRun Workspace query 必须共享同一 frame resolver，避免 Canvas snapshot 与 Workspace Panel 再次漂移。
- 数据库迁移应移除 `lifecycle_agents.current_frame_id`，或在同一任务内保证该列不再被读取，并安排后续删除迁移。

## Frontend Boundaries

- `workspace_module_presented` payload 是后端 authoritative presentation；前端应基于 `presentation_uri` 打开/激活 tab。
- WorkspacePanel 的 Canvas `canCreateUri` 只能用于清理历史无效 tab，不能拦截刚收到的 authoritative presentation。
- 用户从 `+` 菜单打开 Canvas 与 agent 触发 present，应收束到同一个 presentation/open helper。菜单候选可以由 project catalog 展示，但点击后不应只做本地 tab 操作。

## Migration Notes

- 如果删除 `lifecycle_agents.current_frame_id`，需要更新：
  - domain entity 与 repository row mapping；
  - insert/update/select SQL；
  - delivery selection / workspace query / command policy；
  - 测试夹具中手动 `set_current_frame` 的路径。
- 如果短期保留列，必须把名称或注释明确为 non-authoritative，并新增测试防止 AgentRun Workspace 读取该列。

## Risks

- 多处测试夹具依赖 `LifecycleAgent.set_current_frame`，删除字段会有较大编译面；但这是必要的，因为该字段继续存在会持续诱导双事实源。
- `AgentFrameRepository.get_current(agent_id)` 的排序/语义必须可靠。如果它只是按 revision 或 created_at 取最新，需要测试覆盖同 revision/异常状态边界。
- Canvas runtime snapshot 可能通过 session construction 路径间接解析旧 launch frame，需要和 AgentRun Workspace 一起验证。

## Recommended MVP

本任务的 MVP 应先完成后端 frame 事实源收束，再处理前端 presentation/open 竞态。否则前端即使重试刷新，也可能持续刷新到旧 frame。
