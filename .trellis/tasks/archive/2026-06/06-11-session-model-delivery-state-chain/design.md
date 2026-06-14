# 设计

## Target State

AgentRun Workspace 是唯一可交互工作台：

- URL identity: `run_id + agent_id`
- control identity: LifecycleRun + LifecycleAgent
- runtime surface: current AgentFrame
- delivery adapter: RuntimeSessionExecutionAnchor 指向的 RuntimeSession
- trace identity: RuntimeSession
- workspace shell: AgentRun Workspace projection

页面读取 AgentRun Workspace 后获得 delivery runtime ref，再执行 message、steer、enqueue、cancel。RuntimeSession 仍然存在于 API 和仓储中，但它只作为 delivery/trace ref 出现在 workspace view 内。

## Route Model

目标前端 route：

- `/agent-runs/new?project_id=...&project_agent_id=...`
- `/agent-runs/:runId/:agentId`
- `/run/:runId`
- `/agent/:agentId` 继续作为轻量 inspector 或被后续任务吸收到 workspace 链接中
- RuntimeSession trace 如需页面入口，使用 `/runtime-sessions/:runtimeSessionId/trace`

当前 `/session/new` 和 `/session/:sessionId` 交互入口由 frontend child 移除。所有 active list、run page、subject page、ProjectAgent launch 入口改用 AgentRun refs 导航。

## Route Identity Rationale

`LifecycleRun` 是运行账本和拓扑容器；`AgentRun` 在合同中由 `AgentRunRefDto { run_id, agent_id }` 表达。一个 run 可以包含多个 LifecycleAgent/AgentRun，所以 canonical workspace route 使用完整 AgentRunRef。

`agent_id` 决定 workspace 内 current frame、execution profile、delivery runtime ref、pending messages/actions 的归属。若未来需要短入口，`/agent-runs/:runId` 只能作为 exact-one-agent resolver：当 run 下只有一个可交互 agent 时解析并进入 canonical route。

## API Model

新增或重命名合同：

```text
AgentRunWorkspaceView {
  run_ref
  agent_ref
  shell
  current_frame_ref?
  delivery_runtime_ref?
  delivery_trace_meta?
  control_plane
  frame_runtime?
  subject_associations
  actions
  pending_messages
}

AgentRunWorkspaceShell {
  display_title
  title_source
  delivery_status
  last_turn_id?
  updated_at
}

RuntimeSessionTraceMeta {
  runtime_session_ref
  event_seq
  executor_session_id?
  trace_title?
  trace_title_source?
}
```

目标 endpoints：

- `GET /agent-runs/{run_id}/{agent_id}/workspace`
- `POST /agent-runs/{run_id}/{agent_id}/messages`
- `POST /agent-runs/{run_id}/{agent_id}/steering`
- `GET/POST/DELETE /agent-runs/{run_id}/{agent_id}/pending-messages...`
- `POST /agent-runs/{run_id}/{agent_id}/cancel`
- `POST /projects/{project_id}/agents/{project_agent_id}/agent-runs`

服务端通过 run_id + agent_id 校验项目权限、agent 属于 run、delivery runtime ref 存在、current frame 可投递。命令 endpoints 内部仍可复用 session launch/control 服务，但 runtime id 是解析结果，不是 public workspace identity。

## Command Receipts

每个用户投递命令携带 `client_command_id`。服务端创建 command receipt：

- scope: project_agent_start 或 agent_run_message
- scope refs: project/agent 或 run/agent
- request_digest: canonical JSON digest
- status: pending, accepted, terminal_failed
- accepted refs: runtime_session_id, run_id, agent_id, frame_id, turn_id
- error: terminal failure message

同一 scope + `client_command_id`：

- accepted: 返回已接受 refs/workspace state
- pending: 返回 command state，前端刷新 workspace 或继续等待
- terminal_failed: 返回已记录错误
- digest mismatch: 409 Conflict

## Launch Boundary

`connector.prompt` 返回 `ExecutionStream` 是 accepted boundary。

目标顺序：

1. resolve AgentRun + current AgentFrame + delivery RuntimeSession
2. build launch plan and pending frame surface in memory
3. prepare connector context
4. call connector
5. accepted commit:
   - user input submitted
   - turn started
   - current frame revision if execution profile/capability/context changed
   - session meta running
   - command receipt accepted
6. attach stream ingestion

Connector preparation/start failure records failure state and clears claimed runtime/hook state, while current frame remains stable.

## HookRuntime Refresh

HookRuntime is frame-scoped. Runtime registry lookup must validate the cached runtime against the current HookControlTarget:

- same RuntimeSession
- same run_id
- same agent_id
- same frame_id

On mismatch, rebuild from provider `resolve_runtime_hook_target` and replace the registry entry before hook dispatch. The mismatch error remains useful for true data corruption, but normal frame transitions should refresh before reaching that error.

## Frontend State Model

`AgentRunWorkspaceState` is derived from `AgentRunWorkspaceView`:

- workspace key: `agentrun:${runId}:${agentId}:${frameId ?? "no-frame"}`
- draft key: `draft:${projectId}:${projectAgentId}`
- executor source: draft ProjectAgent executor or frame_runtime.execution_profile
- delivery runtime id: internal command adapter id
- display shell: workspace response shell, not `SessionMeta`

`useExecutorConfig` must separate authoritative workspace source from recent local preferences. Workspace source wins on key changes and accepted command responses. Local storage remains useful for recent executor/model choices in new drafts, but it must not override an AgentRun frame execution profile.

## Runtime Trace Meta Boundary

`SessionMeta` 继续服务 RuntimeSession trace/delivery ledger：event seq、executor_session_id、trace title provenance 和 terminal trace summary。AgentRun Workspace 不读取 `SessionMeta` 作为页面标题、侧栏列表或 command enablement 的事实源。

工作台 shell 由 `run_id + agent_id` 投影：display title 来自 ProjectAgent/subject/user workspace title，delivery status 来自 AgentRun delivery projection 或 command receipt，last turn/activity 来自 delivery trace refs 与 command result。RuntimeSession trace 页面可以展示 trace meta，但不参与工作台导航身份。

## Work Breakdown

1. Runtime trace meta child separates RuntimeSession trace meta from AgentRun workspace shell/list/status projection.
2. API contract child establishes AgentRun Workspace public shape and route contract.
3. Command receipt child adds durable idempotency and recovery semantics.
4. Launch/Hook child fixes accepted boundary and frame-scoped runtime cache.
5. Frontend child switches routes, page naming, workspace state, executor hydration and command ids.

API contract depends on the meta boundary. Frontend child depends on API contract. Command receipt and launch/hook children can proceed in parallel after API field names are fixed, then frontend consumes their result.
