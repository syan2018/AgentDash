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
