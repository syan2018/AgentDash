# AgentRun 工作台 API 与投递合同

Parent: `06-11-session-model-delivery-state-chain`

## Goal

建立 AgentRun Workspace 的后端/API 合同，让前端以 `run_id + agent_id` 打开交互工作台，并由服务端解析 delivery RuntimeSession、current AgentFrame、actions、pending messages 和 execution profile。

## Dependencies

- 依赖 `06-11-agentrun-runtime-trace-meta-convergence` 确定 `SessionMeta` trace metadata 与 AgentRun Workspace shell/list/status projection 边界。
- 后续 `agentrun-delivery-command-receipts` 依赖本任务确定 `client_command_id` 字段名和 command response shape。
- 后续 `agentrun-workspace-frontend-route-state` 依赖本任务生成的 TypeScript contracts。

## Requirements

- 新增或重命名 DTO：`AgentRunWorkspaceView`，以 AgentRun 为主体返回 workspace 所需数据。
- `AgentRunWorkspaceView` 包含 `AgentRunWorkspaceShell`，工作台 title/status/list shell 不从 `SessionMeta` 暴露为 public fact。
- RuntimeSession trace metadata 使用独立 `RuntimeSessionTraceMeta` 或等价字段组，只保留 trace/delivery ledger 信息。
- 新增 AgentRun workspace query endpoint：`GET /agent-runs/{run_id}/{agent_id}/workspace`。
- 新增 AgentRun command endpoints：messages、steering、pending messages、cancel，路径以 run_id + agent_id 为 public identity。
- 新增 Project Agent materialization endpoint：`POST /projects/{project_id}/agents/{project_agent_id}/agent-runs`。
- command request DTO 包含必填 `client_command_id`。
- command response 返回 command state 和 accepted refs，足够前端在 transport recovery 后恢复 workspace。
- `ConnectorError::InvalidConfig` 在 AgentRun command API 中保持 BadRequest 语义。
- RuntimeSession id 只作为 `delivery_runtime_ref` 或 trace metadata 返回。

## Acceptance Criteria

- [ ] generated Rust/TypeScript contracts 包含 `AgentRunWorkspaceView`。
- [ ] generated Rust/TypeScript contracts 明确区分 `AgentRunWorkspaceShell` 与 RuntimeSession trace metadata。
- [ ] Project Agent start 和 AgentRun message request 均包含 `client_command_id`。
- [ ] AgentRun workspace endpoint 能通过 run_id + agent_id 返回 delivery runtime ref、current frame runtime 和 execution profile。
- [ ] AgentRun message endpoint 内部解析 delivery RuntimeSession 后复用 launch 服务。
- [ ] connector invalid config 经 API 返回 400。
- [ ] 原 session runtime-control 可作为内部/过渡实现细节，但前端交互合同使用 AgentRun Workspace 命名。
