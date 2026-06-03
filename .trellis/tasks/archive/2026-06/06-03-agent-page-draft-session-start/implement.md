# Agent 页 Draft 会话启动实现计划

## Implementation Checklist

- [ ] 阅读相关规范：
  - `.trellis/spec/backend/architecture.md`
  - `.trellis/spec/backend/session/architecture.md`
  - `.trellis/spec/backend/session/session-startup-pipeline.md`
  - `.trellis/spec/backend/workflow/architecture.md`
  - `.trellis/spec/backend/workflow/lifecycle-run-link.md`
  - `.trellis/spec/frontend/architecture.md`
  - `.trellis/spec/frontend/type-safety.md`
  - `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- [ ] 后端 contract：
  - 在 `agentdash-contracts/src/project_agent.rs` 增加首条消息 materialize request/response DTO。
  - 更新 `generate_ts.rs` export。
  - 运行 contracts generate/check。
- [ ] 后端 route：
  - 在 `crates/agentdash-api/src/routes/project_agents.rs` 增加 `POST /projects/{id}/agents/{project_agent_id}/sessions`。
  - route 只负责权限、DTO、service 调用和 response 映射。
- [ ] 后端 application service：
  - 抽出 ProjectAgent graphless launch 的构造逻辑，避免 route 重复拼 use case。
  - 实现“dispatch + 写回 project_agent_id + 首条消息投递”的主路径。
  - 首条消息投递复用 `LifecycleAgentMessageService` 和 `SessionLaunchLifecycleAgentMessageDeliveryPort`。
- [ ] 失败清理：
  - 为 `RuntimeSessionExecutionAnchorRepository` 增加必要 delete 方法，或通过新清理 service 先删 anchor。
  - 确保 connector accepted 前失败且 `last_event_seq == 0` 时删除 RuntimeSession 与本次 LifecycleRun。
  - connector accepted 后失败保留证据。
- [ ] migration：
  - 新增下一号 migration，例如 `0002_runtime_session_anchor_fks.sql`，为 `runtime_session_execution_anchors` 增加到 `sessions`、`lifecycle_runs`、`lifecycle_agents`、`agent_frames` 的外键。
  - 不修改 `0001_init.sql`；本任务不是数据库 baseline squash / reset / merge。
  - 同步 Postgres repository 行为与测试 fixtures。
  - 运行 `pnpm run migration:guard`。
- [ ] 前端 service/type：
  - 更新 generated TS。
  - 在 `packages/app-web/src/services/project.ts` 增加 `createProjectAgentRuntimeSession` mapper。
  - 更新 `projectStore` 或直接 service 调用，保持 store 中 Agent summary 可刷新。
- [ ] 前端 route：
  - 在 `App.tsx` 增加 `/session/new` wrapper。
  - `AgentTabView` 点击启动只导航 draft route，不调用旧 `/launch`。
- [ ] `SessionPage` draft mode：
  - 支持 `sessionId=null` + draft project/agent params。
  - 加载 ProjectAgent summary/default executor。
  - 为 Draft 提供 `customSend`，提交成功后 `navigate(/session/{id}, { replace: true })`。
  - runtime-control、stream、title edit、run detail 等真实 session 功能只在 runtime mode 启用。
- [ ] UI 状态：
  - Draft 顶栏展示 Agent 名称和“待发送”状态。
  - 未发送离开页面不触发任何清理请求，因为没有持久化数据。
- [ ] 测试：
  - 后端测试：ProjectAgent draft open 不涉及 route，首条消息 materialize 创建 refs 并调用 delivery。
  - 后端测试：prompt 校验失败不创建数据。
  - 后端测试：pre-accepted failure 清理空 session/run/anchor。
  - 前端测试：Agent 启动按钮只导航 `/session/new...`。
  - 前端测试：Draft 首条消息成功后 replace 到 `/session/{runtime_session_id}`。

## Validation Commands

按改动范围逐步执行：

```powershell
pnpm run contracts:check
pnpm --filter @agentdash/app-web typecheck
cargo check -p agentdash-contracts -p agentdash-domain -p agentdash-application -p agentdash-api -p agentdash-infrastructure
```

如修改前端测试：

```powershell
pnpm --filter @agentdash/app-web test -- --run
```

如新增后端单测：

```powershell
cargo test -p agentdash-application workflow::agent_message
cargo test -p agentdash-api project_agents
```

实际命令需按 repo 当前 package 名和测试模块名微调。

## Risky Files

- `packages/app-web/src/App.tsx`：新增 route，需避免影响现有 `/session/:sessionId`。
- `packages/app-web/src/pages/SessionPage.tsx`：runtime mode 与 draft mode 分支要清晰，避免空 session 触发 runtime-control 查询。
- `packages/app-web/src/features/agent/agent-tab-view.tsx`：启动行为从 API call 改为导航。
- `crates/agentdash-api/src/routes/project_agents.rs`：现有 `/launch` 路由仍可能被其它入口使用，新增 route 不应误改旧语义。
- `crates/agentdash-application/src/workflow/dispatch_service.rs`：graphless dispatch 是共享入口，首条消息 materialize service 应复用而不是改坏 Task/Story/Routine 路径。
- `crates/agentdash-application/src/workflow/agent_message.rs`：继续发送真实 session 的路径不能回归。
- `crates/agentdash-infrastructure/migrations/*`：只能新增递增 migration；新增 FK 后要确认 delete 顺序和 repository cleanup 不冲突。

## Rollback Points

- 如果 Draft route 前端改动异常，可以先保留新后端 API，临时让 AgentTabView 回到旧 `/launch` 路径，但不要合并为最终行为。
- 如果 FK 增加导致测试 fixture 暂时失败，优先修正 fixture 创建顺序；不要移除 FK 规避事实关系。
- 如果 pre-accepted cleanup 复杂度超出本任务，可先实现主路径并把 cleanup 作为同任务后续 checklist 未完成项，不应把任务标记完成。

## Review Gate Before Start

- PRD、design、implement 经用户确认。
- 明确 `/session/new` 是否采用 query 参数 route。
- 明确旧 `/projects/{id}/agents/{agent}/launch` 是保留内部语义还是在本任务中标记废弃。
