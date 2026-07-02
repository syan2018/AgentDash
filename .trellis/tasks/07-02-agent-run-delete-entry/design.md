# AgentRun 删除入口技术设计

## Architecture

对外删除入口使用 AgentRun 语义，内部继续以 LifecycleRun 作为持久化事实根。

推荐 HTTP 入口：

```text
DELETE /api/projects/{project_id}/agent-runs/{run_id}
```

选择 Project-scoped endpoint 的原因：

- 当前列表入口已经是 `GET /api/projects/{project_id}/agent-runs`。
- 删除必须校验 Project 编辑权限与 run 归属，路径携带 Project 可以让 API intent 明确。
- 端点仍处在 `agent-runs` 命名空间，避免暴露 `lifecycle-runs` 或 `sessions` 作为产品入口。

API handler 位于现有 `agentdash-api/src/routes/lifecycle_agents.rs` router 内。该模块已经拥有 AgentRun workspace、composer、mailbox、cancel 等 AgentRun workspace 命令入口，新增删除入口时保持同一资源 owner。

删除编排放在 application 层 AgentRun 用例中，而不是在 route 中拼 repository 操作。API route 只负责：

- 解析 `project_id` / `run_id`。
- 校验当前用户对 Project 具备 `ProjectPermission::Edit`。
- 调用 AgentRun 删除 use case。
- 把 application/domain 错误映射成 HTTP 错误。

## Data Flow

删除命令按以下顺序执行：

1. 读取并校验 Project。
2. 读取 `LifecycleRun`，不存在返回 NotFound。
3. 校验 `run.project_id == project_id`，不匹配返回 NotFound 或 Forbidden，不能跨 Project 删除。
4. 查询该 run 下 root / child `LifecycleAgent` 与 delivery RuntimeSession 绑定。
5. 检查 run / agent 的 execution status；`running` 或 `cancelling` 返回 conflict，且不执行部分删除。
6. 收集该 run 下关联的 RuntimeSession ids。
7. 删除关联 RuntimeSession trace facts。
8. 删除 `LifecycleRun`，让 run-owned lifecycle / mailbox / frame relation 数据通过数据库约束级联清理。
9. 返回 `{ deleted: true, project_id, run_id }`。

RuntimeSession 删除必须服务于 AgentRun 删除 use case，而不是成为前端产品入口。清理 RuntimeSession 时可以复用现有 `session_core.delete_session` 语义，但调用方必须先从 AgentRun/Lifecycle control-plane 收集 session ids。

## Contracts

新增返回 DTO 推荐进入 `agentdash-contracts::workflow`：

```rust
pub struct DeleteAgentRunResponse {
    pub deleted: bool,
    pub project_id: String,
    pub run_id: String,
}
```

前端 service：

```ts
deleteAgentRun(projectId: string, runId: string): Promise<DeleteAgentRunResponse>
```

前端调用 `api.delete`：

```text
/projects/{project_id}/agent-runs/{run_id}
```

生成 TypeScript 后由 `packages/app-web/src/generated/workflow-contracts.ts` 或已有 workflow 类型出口消费，不手写 response shape。

## Frontend UI

入口落在 `ActiveAgentRunList` 的主 AgentRun 行：

- 主行右侧增加 `CardMenu` 或等价三点菜单。
- 菜单项为危险操作“删除 AgentRun”。
- 点击菜单项必须 `stopPropagation`，不能触发行点击打开 workspace。
- 使用轻量危险确认做二次确认，确认文案使用主行 `shell.display_title` 或 fallback。优先使用 `ConfirmDialog tone="danger"`；如果使用 `DangerConfirmDialog`，不得传 `expectedValue`，不要求用户重新输入名称。
- 删除中禁用重复提交。
- 成功后调用 `useAgentRunListProjectionStore.getState().refreshProject(projectId, "agent_run_deleted")`。
- 如果当前路由是被删除 run 的 `/agent-runs/{runId}/{agentId}`，导航回 `/dashboard/agent`。
- 失败时在列表区域或行级错误位展示后端错误消息。

子 Agent 行不提供删除入口。MVP 删除的是 whole run，而不是子 Agent subtree。

## Error Semantics

| 条件 | 行为 |
| --- | --- |
| run 不存在 | NotFound |
| project 不存在或无权限 | 现有 Project 权限错误 |
| run 不属于 project | NotFound 或 Forbidden，不能删除 |
| run / delivery 正在 running 或 cancelling | Conflict / BadRequest with clear message |
| 关联 RuntimeSession 中部分不存在 | 删除流程继续清理剩余 facts；最终以 run 删除为准 |
| 删除 LifecycleRun 失败 | 返回错误；前端保留列表并显示错误 |

运行中删除先拒绝的原因是“停止”和“删除”是两个危险动作。第一版保持删除命令确定、可回滚和可解释，后续可以增加显式“停止并删除”命令。

## Persistence And Migration

本任务不预期新增数据库表或字段。

依赖现有外键级联：

- `lifecycle_agents.run_id -> lifecycle_runs(id) ON DELETE CASCADE`
- `runtime_session_execution_anchors.run_id -> lifecycle_runs(id) ON DELETE CASCADE`
- mailbox / command receipt 中 run-owned rows 的 `run_id -> lifecycle_runs(id)` cascade
- session-owned rows 通过 `session_core.delete_session` 删除

如果实现中发现缺少必要的 run -> session 查询 port，可以新增 repository/application query 方法；不应通过 SQL ad hoc 拼在 API route。

## Validation

后端测试：

- 删除不存在 run。
- 跨 Project 删除被拒绝。
- running / cancelling 删除被拒绝且 LifecycleRun 仍存在。
- 非运行态删除会删除 LifecycleRun，并清理关联 RuntimeSession。

前端测试：

- 主 AgentRun 行显示删除菜单。
- 点击删除菜单不触发打开 workspace。
- 确认成功后调用 delete service 并刷新 AgentRun 列表。
- 当前路由指向被删除 run 时导航回 Agent 页面。

跨层验证：

- `pnpm run contracts:check`
- `pnpm run frontend:check`
- 后端相关 package test / check。
