# W4 Backend Commands + Read Models

## 状态

done

## 依赖

- W3 done

## 目标

实现 Run-scoped Task command、Story Task projection read model，并确保 Task runtime 视图统一落到 `SubjectExecutionView`。

## 输入

- W3 contract。
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `crates/agentdash-api/src/routes/stories.rs`
- `crates/agentdash-api/src/routes/lifecycle_views.rs`
- `crates/agentdash-api/src/routes/task_execution.rs`
- `crates/agentdash-application/src/lifecycle/subject_context_assignment.rs`
- `crates/agentdash-application/src/lifecycle/run_view_builder.rs`
- `crates/agentdash-application/src/task/service.rs`
- `crates/agentdash-application/src/task/gateway/repo_ops.rs`
- `crates/agentdash-application/src/task/gateway/artifact_ops.rs`

## 范围

- 提供 Run / AgentRun scoped Task create / update / archive / query command。
- Story projection 从 Story-bound LifecycleRun、linked run 与可选 `story_ref` 推导。
- `SubjectContextAssignmentResolver` 从 owning LifecycleRun 的 Task facts 解析 Task subject context。
- `SubjectExecutionView` 承担 linked runs、latest runtime node、artifacts。
- Assignment link 作为计划层关联，runtime evidence 从 subject association / Agent lineage / runtime anchor 投影。

## 范围边界

- 该节点负责后端 command 与 read model，原因是 W5、W6 和 W7 的并行实现需要统一的服务端入口。
- UI、MCP schema 和 workflow fanout 分别进入 W5、W6 和 W7，原因是这些工作面应消费 W4 的稳定 command / projection。
- Task 查询围绕 run aggregate 展开，原因是当前没有 Project/global Task visibility 的产品需求。

## 验收

- AgentRun workspace 能在 run scope 内创建、推进、归档 Task。
- Story projection tests 覆盖 Story-bound run 可见、linked run 可见、无关 run 排除。
- SubjectExecutionView tests 覆盖 Task subject association 到 latest runtime node / artifacts。
- 旧 `/tasks/{id}/execution` 心智被替换或收口到 `SubjectExecutionView`。

## 产出记录

- 新增 `agentdash_application::task::plan`：提供 run-scoped Task list/create/update/status/archive command、按 Task subject 定位 owning LifecycleRun、Story Task projection read model。
- 新增 API 路由 `routes/task_plan.rs`：提供 `/lifecycle-runs/{run_id}/tasks` 与 `/agent-runs/{run_id}/agents/{agent_id}/tasks` 的 query/create/update/status/archive 入口，响应使用 W3 Task contract DTO。
- `stories.rs` 移除 Story-owned Task CRUD 路由，新增 `/stories/{id}/task-projection`，从 Story-bound run、linked association 与 `story_ref` 生成 projection source 说明。
- `SubjectContextAssignmentResolver` 改为从 owning `LifecycleRun.tasks` 解析 Task subject context；Story context 通过 owning run 的 Story association 或 `story_ref` 可选补齐。
- `SubjectRunContextResolver` / hook owner resolver 改为从 LifecycleRun Task plan 读取 task title/story_ref，不再用 `StoryRepository::find_by_task_id`。
- 旧 Task runtime backwrite 路径已收口：boot projector 不再把 runtime node 状态写回 Story.tasks，artifact/status hook 路径只追加 state_change 事件；旧 `/tasks/{id}/execution` route 不再挂载，SubjectExecutionView 是运行视图入口。
- MCP 仅做 W4 编译适配：注入 lifecycle run / subject association repo，Task MCP 从 run-scoped task plan 读取，Story MCP 的 create/batch create 返回迁移提示，完整 tool schema/agent 行为留给 W6。
- focused tests 覆盖 run-scoped Task command 和 Story projection；静态搜索确认后端/API/MCP 已无 `find_by_task_id`、Story Task CRUD 写入口、Task projection/artifact mutator 调用。

## 风险与交接

- W5 可消费 W4 API：run/agent-run Task plan command 与 Story Task projection。
- W6 需要重新设计 MCP tool schema 和 agent-facing 行为；当前 MCP 只保证后端编译与旧工具不再写 Story-owned Task。
- W7 负责 workflow fanout；当前 Story terminal cancel 只取消 Story subject 自身 latest attached execution，不做 Story→Task 级联。
- `CapabilityScopeCtx::Task` 仍要求 `story_id`，无 Story 的 run-scoped Task 暂以 `Uuid::nil()` 填充；W6/Capability 节点应把 Task scope 的 Story 归属改为可选。
- 工作区存在大量非 W4 文件变更，合流时需由主会话确认哪些来自其它并行节点；W4 没有主动回滚这些变更。
