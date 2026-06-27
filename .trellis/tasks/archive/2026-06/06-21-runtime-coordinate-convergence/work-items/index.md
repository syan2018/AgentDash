# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| RC01 | AgentRun current delivery binding 设计 | design | completed | D02, D03 | current delivery binding 落在 `LifecycleAgent` 粒度，表达当前 runtime session/frame/node/attempt/status/observed_at |
| RC02 | DeliveryRuntimeSelectionService 设计与测试 | design+implementation | completed | D02, D03 | 已实现 LifecycleAgent current delivery binding、migration、Postgres row mapping、dispatch/accepted-turn 写入点与 CurrentDelivery selection；raw anchor ordering 不作为 public business policy |
| RC03 | raw anchor repository API 降级命名 | implementation | completed | D03 | 已由机械重构任务 M12 完成；raw latest API 命名表达 `updated_at` 排序，不承载业务 latest |
| RC04 | Workspace query 迁移到 unified selection | implementation | completed | D02, D03, D15 | Workspace detail/list delivery refs、API route command context、stale guard frame/runtime 校验与 resource surface session evidence 已改用 CurrentDelivery selection；raw anchor latest 只保留为 history/list evidence |
| RC05 | Cancel / subject control 迁移到 unified selection | implementation | completed | D02, D03 | Subject execution cancel、terminal cancel reconcile 与 companion gate/control delivery target 已复用 CurrentDelivery selection；显式 runtime session 只作为 stale 校验 |
| RC06 | Mailbox delivery target 迁移到 unified selection | implementation | completed | D02, D03 | mailbox command target 通过 CurrentDelivery unified selection 解析 current frame/runtime session，已移除 latest anchor fallback |
| RC07 | SubjectExecutionView execution history | design+implementation | completed | D12 | 已增加 `runtime_attempts` history，表达 run / agent / runtime session / frame / orchestration node / attempt / status / observed_at / artifacts，`latest_runtime_node` 与 artifacts 从列表首项派生 |
| RC08 | AgentRun resource surface coordinate contract | design+implementation | completed | D15 | `resource_surface_coordinate.surface_frame_ref` 表达 current frame VFS surface，`source_anchor` 表达 current delivery launch frame / runtime session / orchestration node 证据 |
| RC09 | Task execution surface 收敛 | implementation | completed | D12 | 已删除 narrow TaskExecutionView surface，并从 `task_read` schema/description 移除 execution mode；执行事实继续由 SubjectExecutionView 统一投影 |
