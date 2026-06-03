# Agent 页 Draft 会话启动设计

## Design Summary

本任务将“打开 Agent 输入界面”和“创建 runtime/lifecycle 控制面”拆开：

- 打开 Agent：前端本地 Draft，不持久化。
- 提交首条消息：后端 materialize graphless runtime 控制面，并立即走现有 session launch pipeline。
- 后续消息：继续使用已有 runtime session message API。

Draft 是 UI 准备态，不是 `LifecycleRunStatus::Draft`。原因是后者属于控制面账本状态，一旦创建 run/agent/frame/anchor 就已经产生业务事实；本任务要避免的是未发送消息时产生任何控制面事实。

## Current Flow

```text
AgentTabView
  -> launchProjectAgent(projectId, agentKey)
  -> POST /projects/{id}/agents/{agent}/launch
  -> LifecycleDispatchService.launch_agent(RuntimePolicy::CreateRuntimeSession)
  -> create LifecycleRun / LifecycleAgent / Session / AgentFrame / RuntimeSessionExecutionAnchor
  -> navigate(/session/{runtime_session_id})
```

这个 flow 在用户没有发送消息时也会落完整控制面数据。

## Target Flow

```text
AgentTabView
  -> navigate(/session/new?project_id=...&project_agent_id=...)
  -> SessionPage draft mode(sessionId = null, draftProjectAgent = ...)
  -> user submits first prompt
  -> POST /projects/{id}/agents/{agent}/sessions
  -> validate request
  -> LifecycleDispatchService.launch_agent(RuntimePolicy::CreateRuntimeSession)
  -> LifecycleAgentMessageService.dispatch_user_message(runtime_session_id, first prompt)
  -> response(runtime_session_id, turn_id, run_ref, agent_ref, frame_ref)
  -> navigate(/session/{runtime_session_id}, replace=true)
```

## Frontend Design

### Routing

新增 draft 路由，推荐：

```text
/session/new?project_id={project_id}&project_agent_id={project_agent_id}
```

使用 query 而不是只依赖 route state，原因是页面刷新后仍能重新加载 ProjectAgent summary；同时该 URL 不代表真实 session，不会进入现有 `/session/:sessionId` shortcut 逻辑。

### AgentTabView

- 将“启动 Agent”按钮改为导航 draft route。
- 不再在点击时调用 `launchProjectAgent`。
- 可以继续保留 `selectedAgent` 本地状态用于左侧列表高亮，但不依赖 backend dispatch result。

### SessionPage Draft Mode

`SessionPage` 增加 draft 输入：

```ts
type SessionPageMode =
  | { kind: "runtime"; sessionId: string }
  | { kind: "project_agent_draft"; projectId: string; projectAgentId: string };
```

Draft mode 行为：

- `currentSessionId = null`。
- `useSessionRuntimeState` 不发 runtime-control 查询。
- `SessionChatView` 以 `sessionId={null}` 渲染。
- `customSend` 指向 `materializeProjectAgentSessionAndSend`。
- `agentDefaults` 来自 ProjectAgent summary/config。
- 右侧 WorkspacePanel 可以先保持空 runtime data；如果需要文件引用，则只使用 project/workspace 默认上下文。

### Service Contract

新增前端 service：

```ts
createProjectAgentRuntimeSession(projectId, projectAgentId, request)
```

request 使用 generated contract：

```ts
{
  prompt_blocks: JsonValue[],
  executor_config?: JsonValue
}
```

response 复用或新增 `ProjectAgentSessionStartResponse`，至少包含：

```ts
{
  runtime_session_id: string,
  turn_id: string,
  run_ref,
  agent_ref,
  frame_ref,
  agent,
  subject_ref?
}
```

## Backend Design

### API

新增 route：

```text
POST /projects/{id}/agents/{project_agent_id}/sessions
```

该 endpoint 表达“从 ProjectAgent 创建真实 runtime session 并投递首条消息”。它不是旧 `/launch` 的别名，因为旧 `/launch` 只创建控制面，不代表用户已经提交消息。

### Contracts

在 `agentdash-contracts/src/project_agent.rs` 增加：

- `CreateProjectAgentSessionRequest`
- `ProjectAgentSessionStartResult`

`ProjectAgentSessionStartResult` 可以和 `LifecycleAgentMessageResponse` 对齐，同时带回 ProjectAgent summary，便于前端更新 store。

### Application Service

建议新增应用服务，例如：

```rust
ProjectAgentSessionStartService
```

职责：

1. 解析 ProjectAgent context。
2. 构造 graphless `AgentLaunchIntent`：
   - `source = ExecutionSource::ProjectAgent`
   - `subject_ref = Project`
   - `run_policy = CreateLinkedRun`
   - `agent_policy = Create`
   - `context_policy = Isolated`
   - `capability_policy = Baseline`
   - `runtime_policy = CreateRuntimeSession`
3. 调用 `LifecycleDispatchService.launch_agent`。
4. 将 `project_agent_id` 写回 `LifecycleAgent`。
5. 使用 `LifecycleAgentMessageService.dispatch_user_message` 投递首条消息。
6. 返回 refs。

把这段逻辑从 route 中抽出，原因是 route 应只做 auth、DTO 映射和错误映射，首条消息 materialize 是业务 use case。

### Failure Handling

需要区分两个失败窗口：

- dispatch 前校验失败：不创建任何数据。
- dispatch 成功但 connector accepted 前失败：如果 `sessions.last_event_seq == 0`，应清理本次创建的 RuntimeSession 与 LifecycleRun。
- connector accepted 后失败：保留 session 和 lifecycle 控制面，因为已有真实用户提交和执行证据。

实现清理需要补齐 repository 能力或数据库约束：

- `RuntimeSessionExecutionAnchorRepository` 增加 `delete_by_session` 或 `delete_by_run`。
- `lifecycle_run_repo.delete(run_id)` 依赖 cascade 清理 `lifecycle_agents`、`agent_frames`、`lifecycle_subject_associations`。
- 新增递增 migration 为 `runtime_session_execution_anchors` 增加到 `sessions(id)`、`lifecycle_runs(id)`、`lifecycle_agents(id)`、`agent_frames(id)` 的外键，原因是 anchor 是 runtime trace 到控制面事实的索引，不应在任一侧删除后孤立存在。

## Database / Migration Notes

当前 `runtime_session_execution_anchors` 只有主键和索引，没有到 `sessions` 的外键。本任务不是数据库 baseline squash / reset / merge，必须新增下一号 migration，例如当前仓库只有 `0001_init.sql` 时新增：

```text
crates/agentdash-infrastructure/migrations/0002_runtime_session_anchor_fks.sql
```

目标 FK：

- `runtime_session_id REFERENCES sessions(id) ON DELETE CASCADE`
- `run_id REFERENCES lifecycle_runs(id) ON DELETE CASCADE`
- `agent_id REFERENCES lifecycle_agents(id) ON DELETE CASCADE`
- `launch_frame_id REFERENCES agent_frames(id) ON DELETE CASCADE`

这让失败清理和手动删除更接近事实关系。

## Compatibility

项目未上线，不做旧 API/旧数据兼容。

保留现有真实 session message API：

```text
POST /lifecycle-agents/by-runtime-session/{runtime_session_id}/messages
```

原因是它适合“已经 materialize 的 runtime session”继续发送，不适合“首条消息前的 Draft”。

## Trade-offs

- 前端本地 Draft 比数据库 Draft 更少持久化污染，但刷新页面只能从 URL/query 和 ProjectAgent config 恢复草稿上下文，未提交输入不保证持久化。
- 新增首条消息 materialize API 比复用旧 `/launch` 多一个 contract，但能把“用户真的提交消息”作为控制面创建边界。
- 失败清理增加 repository / FK 工作量，但能避免把 connector setup 失败变成新的空数据来源。
