# AgentRun 投递命令幂等与失败恢复

Parent: `06-11-session-model-delivery-state-chain`

## Goal

为 Project Agent materialization 和 AgentRun `send_next` 建立 durable command receipt，保证 transport failure 后重试不会创建额外 RuntimeSession、turn 或 AgentFrame revision。

## Dependencies

- 依赖 `06-11-agentrun-workspace-api-contract` 确定 request/response 字段。
- 与 `06-11-launch-frame-hook-atomicity` 协调 accepted boundary：receipt accepted 必须发生在 launch accepted commit。
- `SessionMeta` 保持 RuntimeSession trace-head projection；receipt 不写入 `SessionMeta`。

## Requirements

- `client_command_id` 在 Project Agent start 和 AgentRun message 中必填。
- 服务端记录 command scope、scope refs、client_command_id、request digest、status、accepted refs、terminal error。
- receipt 不复用 `session_runtime_commands`，后者仍是 runtime context/frame transition delivery outbox。
- 同一 scope + command id + digest 重试返回既有 command state 或 accepted refs。
- 同一 scope + command id + 不同 digest 返回 409。
- Project Agent start 重试不会创建第二个 LifecycleRun/RuntimeSession。
- AgentRun message 重试不会创建第二个 turn/frame。
- terminal failure 可被前端读取并展示为同一个命令的失败。

## Acceptance Criteria

- [ ] 新增 forward migration 保存 command receipts。
- [ ] memory/test persistence 实现与 Postgres 行为一致。
- [ ] duplicate Project Agent start command 返回同一个 run/agent/runtime/turn。
- [ ] duplicate AgentRun message command 返回同一个 turn/frame。
- [ ] digest mismatch 返回 conflict。
- [ ] transport recovery 路径可通过 command state 或 workspace refresh 恢复。
- [ ] `SessionMeta` 未新增 `client_command_id`、request digest 或 retry/conflict 字段。
