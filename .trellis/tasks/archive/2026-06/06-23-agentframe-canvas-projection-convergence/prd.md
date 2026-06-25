# 收束 AgentFrame 与 Canvas Workspace Projection 路径

## Goal

修复 Agent runtime 实际采用的 AgentFrame 与前端 AgentRun Workspace / Canvas Panel 读取的 AgentFrame 可能错位的问题，确保 Canvas 创建、暴露、展示、绑定数据预览都从同一个 frame/runtime surface 事实源出发。

用户可见目标：Agent 已能访问的 Canvas mount，Workspace Panel 和 Canvas runtime snapshot 也必须立即、稳定、同源地访问到；不能出现聊天流显示 Workspace Module 已展示，但右侧面板仍提示无可打开 Canvas 或预览无法读取 session VFS 的状态。

## Confirmed Facts

- Canvas 工具路径会通过 `expose_canvas_mount_revision_and_adopt` 写入新的 AgentFrame revision，把 `cvs-<mount_id>` 追加到 VFS，并追加 `visible_workspace_module_ref = canvas:<mount_id>`。
- `adopt_persisted_agent_frame_revision` 当前会同步 active runtime 的 turn cache、connector tools 和 hook runtime target，但未同步 `LifecycleAgent.current_frame_id`。
- AgentRun Workspace 查询路径通过 `DeliveryRuntimeSelectionService.select_current_delivery` 读取 `LifecycleAgent.current_frame_id`，再用该 frame 生成前端 `resource_surface`。
- Canvas runtime snapshot 通过 `session_id` 调 `resolve_session_frame_vfs` 获取 session frame VFS；如果该解析同样落到旧 frame，Canvas 预览即使 tab 打开也拿不到 Agent 已能访问的 mount / binding 数据。
- 前端 WorkspacePanel 还存在第二层不同步：`workspace_module_presented` 事件打开 tab、`+` 菜单候选、Canvas runtime snapshot 分别依赖 presentation payload、project workspace module catalog、runtime surface，当前没有统一为一条 presentation/open/read 路径。

## Requirements

- AgentFrame runtime adoption 后，AgentRun Workspace 投影必须读取同一个生效 frame，不得继续读取旧 `LifecycleAgent.current_frame_id`。
- 移除或废弃 `LifecycleAgent.current_frame_id` 这条冗余事实路径；如果迁移无法一次物理删除，运行时读路径也必须不再以它作为权威来源。
- Current delivery / runtime session anchor / AgentFrame revision 的关系必须有单一 canonical 解析函数，供 AgentRun Workspace、Canvas runtime snapshot、hook/runtime tool adoption 复用。
- Canvas workspace module 展示必须以 `WorkspaceModulePresentation.presentation_uri = canvas://{mount_id}` 为 canonical open contract，前端所有打开入口都应收束到同一解析/打开流程。
- 前端不得用旧 runtime surface 在收到 authoritative presentation event 时否决打开；runtime surface 刷新与 tab 打开必须有明确顺序或可重试机制。
- 需要保留数据库迁移处理，移除字段或改变字段语义时同步 migration 和 repository contract。
- 需要覆盖 Agent 可访问 mount 但前端 workspace/canvas 读取旧 frame 的回归测试。

## Acceptance Criteria

- [ ] `LifecycleAgent.current_frame_id` 不再作为 AgentRun Workspace / Canvas runtime snapshot 获取当前 frame 的事实源；对应字段被删除或明确降级为非运行路径。
- [ ] Canvas create/present 后，Agent runtime、AgentRun Workspace `resource_surface`、Canvas runtime snapshot 都解析到包含 `cvs-<mount_id>` 的同一有效 VFS surface。
- [ ] `workspace_module_presented` 后 WorkspacePanel 能稳定打开 `canvas://{mount_id}` tab，不依赖旧 `activeCanvasMountIds` 竞态判断。
- [ ] WorkspacePanel `+` 菜单与自动展示事件使用同一 Canvas presentation/open 语义，不再出现同一会话中“已展示”和“无可打开 Canvas”矛盾。
- [ ] 后端测试覆盖 frame adoption 后 workspace projection 不落旧 frame。
- [ ] 前端测试覆盖 presentation event 在 runtime surface 刷新前到达时仍能最终打开 Canvas tab。
- [ ] 数据库 migration 与 repository mapping 与最终字段模型一致。

## Out Of Scope

- 不做兼容旧字段的长期双读逻辑；本项目尚未上线，应直接收束到正确事实源。
- 不改变 Canvas authoring 文件模型、VFS provider 能力范围或 Workspace Module 对外 tool schema，除非它们被证明依赖错误 frame 路径。

## Decisions

- `LifecycleAgent.current_frame_id` 已按冗余事实路径处理；本任务推荐物理删除字段与迁移列。只有在实现过程中发现数据库迁移顺序存在硬性约束时，才允许先移出全部运行时读路径，并在同一任务内记录明确的删除迁移方案。
