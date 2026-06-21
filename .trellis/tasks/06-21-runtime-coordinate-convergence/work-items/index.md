# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| RC01 | AgentRun current delivery binding 设计 | design | pending | D02, D03 | 定义 binding 字段、持久化位置、状态与更新时间语义 |
| RC02 | DeliveryRuntimeSelectionService 设计与测试 | design+implementation | pending | D02, D03 | application-level resolver，覆盖 current/run-scoped/launch-primary/subject-latest policy |
| RC03 | raw anchor repository API 降级命名 | implementation | completed | D03 | 已由机械重构任务 M12 完成；raw latest API 命名表达 `updated_at` 排序，不承载业务 latest |
| RC04 | Workspace query 迁移到 unified selection | implementation | blocked_by_RC02 | D02, D03, D15 | AgentRun workspace delivery refs 与 resource surface 来源 |
| RC05 | Cancel / subject control 迁移到 unified selection | implementation | blocked_by_RC02 | D02, D03 | cancel target 不再 global latest 后过滤 run |
| RC06 | Mailbox delivery target 迁移到 unified selection | implementation | blocked_by_RC02 | D02, D03 | mailbox command target 与 workspace/cancel 一致 |
| RC07 | SubjectExecutionView execution history | design+implementation | blocked_by_RC02 | D12 | 增加 runtime attempts/history，latest 从列表派生 |
| RC08 | AgentRun resource surface coordinate contract | design+implementation | blocked_by_RC02 | D15 | DTO 表达 current frame VFS 与 anchor launch frame source |
| RC09 | Task execution surface 收敛 | implementation | ready | D12 | 删除/私有化 narrow TaskExecutionView surface，`task_read execution` 调用 SubjectExecutionView 或移除 execution mode |
