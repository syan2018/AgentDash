# AgentRun 删除入口

## Goal

在 Agent 主页面提供删除主 AgentRun 的入口，并由后端提供 `agent-runs` 级删除命令。删除语义面向用户可见的 AgentRun 聚合，而不是直接暴露 RuntimeSession 删除能力；RuntimeSession 只作为 delivery / trace evidence 被后端清理流程消费。

用户价值：

- 用户可以从 Agent 主页面清理不再需要的 AgentRun。
- 前端不需要理解 runtime session、lifecycle run、agent tree、mailbox、frame 等底层清理顺序。
- 后端用一个明确的业务命令维护 AgentRun、Lifecycle、RuntimeSession 之间的一致删除语义。

## Background

- 当前后端已有 `DELETE /api/sessions/{id}`，实现位于 `crates/agentdash-api/src/routes/sessions.rs`，只删除 RuntimeSession 及其 session events、terminal effects、runtime commands、lineage、projection、compactions 等 session-owned facts。
- 当前后端没有公开的 `DELETE /api/lifecycle-runs/{id}` 或 `DELETE /api/agent-runs/{id}`；`/lifecycle-runs/{id}` 只暴露查询。
- `LifecycleRunRepository::delete` 可以删除 `lifecycle_runs`，数据库外键会级联删除 `lifecycle_agents`、`lifecycle_gates`、`lifecycle_subject_associations`、agent assignments / lineages、mailbox run-owned rows 等 run-owned 数据。
- `RuntimeSessionExecutionAnchor` 同时以 RuntimeSession 和 LifecycleRun 为外键；删除 session 会清 anchor，但不会删除 lifecycle run。
- 项目规范明确：RuntimeSession 是 delivery / trace substrate，不拥有业务归属、Lifecycle progress 或 Agent effective surface。用户工作台和主列表应以 AgentRun Workspace / Lifecycle control-plane projection 为准。
- 前端 Agent 主页面右侧 `ActiveAgentRunList` 消费 `AgentRunWorkspaceListEntry[]`，主行以 `run_ref + agent_ref` 表达主 AgentRun，子 Agent 递归挂在 `children` 下。
- 前端已有 `deleteSession(id)` service，但这不适合作为 AgentRun 删除入口，因为它会把底层 RuntimeSession 当作聚合根。

## Requirements

- R1. 后端新增对外 `agent-runs` 删除端点，删除目标是主 AgentRun / whole run，而不是 RuntimeSession。
- R2. 删除命令必须先做 Project 编辑权限校验，并确保目标 run 属于当前 Project。
- R3. 删除命令必须拒绝删除正在运行或正在取消的 AgentRun；第一版只允许非运行态 AgentRun 删除。
- R4. 删除命令必须清理目标 run 下关联的 RuntimeSession trace facts，再删除 LifecycleRun，让 run-owned 数据通过数据库约束级联清理。
- R5. 前端 Agent 主页面在主 AgentRun 行提供危险删除入口，并使用应用内确认弹窗；入口不出现在子 Agent 行的 MVP 范围内。
- R6. 删除成功后，前端必须刷新 AgentRun 列表投影；如果当前打开的是被删除的 AgentRun workspace，需要导航回 Agent 页面或安全空状态。
- R7. RuntimeSession 删除接口不作为本次前端入口；前端新增 service 应以 `deleteAgentRun(runId)` 命名并调用 `agent-runs` 端点。
- R8. 新增或修改前后端共享 response DTO 时，必须从 Rust contract 生成 TypeScript，避免前端手写跨层 wire shape。

## Acceptance Criteria

- [ ] 后端存在 `DELETE /api/projects/{project_id}/agent-runs/{run_id}`，并返回结构化删除结果。
- [ ] 删除不存在、无权限或跨 Project 的 run 返回明确错误，不静默成功。
- [ ] 删除 `running` / `cancelling` AgentRun 返回明确冲突错误，不执行部分删除。
- [ ] 删除一个主 AgentRun 会移除对应 AgentRun 列表项，并清理其 lifecycle tree 与相关 runtime session 数据。
- [ ] 前端 Agent 主页面主 AgentRun 行有删除入口，使用危险确认，不误触发打开 AgentRun。
- [ ] 删除成功后 AgentRun 列表刷新；当前 workspace 指向已删除 run 时，前端离开该 workspace 视图。
- [ ] 后端和前端分别有聚焦测试覆盖删除 API / service 或列表交互；跨层 DTO 变更通过 contract check。

## Out of Scope

- 不提供子 Agent 单独删除。
- 不把 RuntimeSession 删除接口接入产品 UI。
- 不新增兼容旧 endpoint 的前端回退逻辑。
- 不做批量删除、归档、恢复站或软删除。
- 不把“停止并删除”合并为隐式动作；运行中删除先明确拒绝。
