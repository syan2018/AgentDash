# W4 Backend Commands + Read Models

## 状态

pending

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

- 待填写。

## 风险与交接

- W5 / W6 / W7 从此节点后可并行。
- 下游只消费 W4 暴露的 command / read model，不自行绕过 repository 或 contract。
