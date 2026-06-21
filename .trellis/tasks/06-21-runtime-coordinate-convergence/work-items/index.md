# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| RC01 | AgentRun current delivery binding 设计 | design | completed | D02, D03 | current delivery binding 落在 `LifecycleAgent` 粒度，表达当前 runtime session/frame/node/attempt/status/observed_at |
| RC02 | DeliveryRuntimeSelectionService 设计与测试 | design+implementation | completed | D02, D03 | 已实现 LifecycleAgent current delivery binding、migration、Postgres row mapping、dispatch/accepted-turn 写入点与 DeliveryRuntimeSelectionService；不迁移 consumers |
| RC03 | raw anchor repository API 降级命名 | implementation | completed | D03 | 已由机械重构任务 M12 完成；raw latest API 命名表达 `updated_at` 排序，不承载业务 latest |
| RC04 | Workspace query 迁移到 unified selection | implementation | implementation_ready | D02, D03, D15 | RC02 后第一批 consumer；workspace delivery refs、stale guard 与 resource surface current source 改用 selection |
| RC05 | Cancel / subject control 迁移到 unified selection | implementation | blocked_by_RC04 | D02, D03 | cancel target 不再使用 route context/global latest，复用 CurrentDelivery selection |
| RC06 | Mailbox delivery target 迁移到 unified selection | implementation | blocked_by_RC04 | D02, D03 | mailbox command target 与 workspace/cancel 一致，移除 latest anchor fallback |
| RC07 | SubjectExecutionView execution history | design+implementation | implementation_ready | D12 | 增加 runtime attempts/history，latest 从列表派生，涉及 contract DTO；建议在 RC04-RC06 target migration 后进入 |
| RC08 | AgentRun resource surface coordinate contract | design+implementation | blocked_by_RC04_RC07 | D15 | DTO 表达 current frame VFS 与 anchor launch frame source |
| RC09 | Task execution surface 收敛 | implementation | completed | D12 | 已删除 narrow TaskExecutionView surface，并从 `task_read` schema/description 移除 execution mode；执行事实继续由 SubjectExecutionView 统一投影 |
