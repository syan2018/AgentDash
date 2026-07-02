# AgentRun 删除入口实施计划

## Checklist

1. 后端合同
   - 在 `agentdash-contracts::workflow` 增加 `DeleteAgentRunResponse`。
   - 将 DTO 加入 TypeScript 生成入口。
   - 运行或准备运行 `pnpm run contracts:check`。

2. 后端删除用例
   - 在 application AgentRun 边界新增删除 service / method。
   - 输入包含 `project_id`、`run_id` 和必要 repository/session cleanup ports。
   - 读取 LifecycleRun 并校验 Project 归属。
   - 查询 run 下 agents / delivery runtime refs / anchors，收集 RuntimeSession ids。
   - 判断 running / cancelling 状态并返回 conflict。
   - 删除 RuntimeSession trace facts。
   - 删除 LifecycleRun。

3. 后端 API
   - 在 `lifecycle_agents::router()` 增加：
     ```text
     DELETE /projects/{project_id}/agent-runs/{run_id}
     ```
   - Route 只做身份、Project 编辑权限、参数解析、DTO 映射和错误映射。
   - 返回 generated contract DTO。

4. 前端 service
   - 新增 `deleteAgentRun(projectId, runId)`，调用 `/projects/{project_id}/agent-runs/{run_id}`。
   - 返回 generated `DeleteAgentRunResponse` 类型。

5. 前端列表入口
   - 在 `ActiveAgentRunList` 主行增加危险菜单项。
   - 使用轻量危险确认；优先 `ConfirmDialog tone="danger"`，或使用不带 `expectedValue` 的 `DangerConfirmDialog`。
   - 删除成功后刷新 `useAgentRunListProjectionStore`。
   - 当前路由匹配被删除 run 时导航回 `/dashboard/agent`。
   - 子 Agent 行不加删除入口。

6. 测试与验证
   - 后端 API/use-case 测试覆盖正常删除、跨 Project、running/cancelling 拒绝。
   - 前端测试覆盖菜单、确认、刷新、导航。
   - 运行必要检查命令。

## Validation Commands

按实际触达范围运行：

```powershell
pnpm run contracts:check
pnpm run frontend:check
cargo check -p agentdash-api
cargo test -p agentdash-api
```

如果新增或修改 application crate 测试，补跑对应 `cargo test -p <crate>`。

## Risk Points

- 不能让 API route 直接拼多 repository 删除逻辑；跨聚合删除必须进入 application 用例。
- 不能把 RuntimeSession 删除 endpoint 接到前端产品 UI。
- 删除 RuntimeSession 和 LifecycleRun 的顺序要避免外键约束或残留 projection；设计顺序是先删 session trace facts，再删 run。
- running / cancelling 拒绝必须在任何删除副作用前完成。
- 前端菜单点击必须阻止行点击，否则用户会同时打开 workspace。
- 删除确认不能要求用户重新输入名称；这是普通清理动作，不是需要输入匹配的重危险操作。
- 删除成功后必须刷新服务端 projection，不能只本地 filter。

## Rollback Points

- 后端端点可单独回滚，不影响现有 `GET /projects/{project_id}/agent-runs` 和 AgentRun workspace 路由。
- 前端入口可单独隐藏或移除，后端删除命令仍可保留为未暴露 API 能力。
- 如果 contract 生成影响过大，保留 DTO 在 `agentdash-contracts::workflow`，但前端只消费 generated 类型，不落手写过渡类型。
