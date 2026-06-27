# 实施计划

1. 在 contracts 中新增 AgentRun workspace DTO 和 command request/response 字段。
2. 在 API routes 中新增 `/agent-runs/{run_id}/{agent_id}/workspace` 和 command endpoints。
3. 将 sessions runtime-control 的组装逻辑抽成 AgentRunWorkspace builder，避免复制两套状态判断。
4. 新增 Project Agent `/agent-runs` materialization endpoint。
5. 调整 AgentRunMessage delivery port 的 connector error 映射。
6. 运行 contract generation/check。
7. 增加 API focused tests：
   - workspace endpoint 通过 run/agent 返回 delivery runtime ref。
   - invalid config 返回 400。

## Validation

- contract generation/check
- `cargo test -p agentdash-api agent_run_workspace`
- `cargo test -p agentdash-application agent_message`

## Implementation Notes

- 已在 `agentdash-contracts::workflow` 新增 `AgentRunWorkspaceView`、`AgentRunWorkspaceShell`、`RuntimeSessionTraceMeta`、`AgentRunCommandReceipt` 与 `AgentRunAcceptedRefs`，并同步 generated TypeScript contracts。
- 已将 `AgentRunMessageRequest`、`AgentRunSteeringRequest`、`EnqueuePendingMessageRequest` 与 `CreateProjectAgentSessionRequest` 收束为必填 `client_command_id`。
- 已新增 `/agent-runs/{run_id}/agents/{agent_id}/workspace`、message、steering、pending-message 与 cancel routes；handler 以 run / agent public identity 解析 delivery runtime ref，workspace shell/action status 从 `SessionExecutionState` 与 `LifecycleAgent.status` 投影。
- 已新增 `/projects/{project_id}/agents/{project_agent_id}/agent-runs` materialization route，并让 start result 返回 command receipt 与 accepted refs。
- 已将 `ConnectorError::InvalidConfig` 经 `WorkflowApplicationError` 映射为 BadRequest，使 AgentRun message delivery 保留 400 语义；同时修复一处既有 clippy `needless_borrow` 以保持本任务质量门可通过。
- `cargo test -p agentdash-api agent_run_workspace` 当前没有匹配测试用例；本子任务通过 `cargo check` 与 contract check 覆盖 API/contract 编译面，后续前端/receipt 子任务继续补行为测试。
