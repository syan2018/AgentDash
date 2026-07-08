# Companion subagent 工具卡片与协议收束 - Implement

## Checklist

- [x] 读取相关 spec：frontend architecture/type-safety/component/design、cross-layer frontend-backend contracts、backend session/runtime execution state。
- [x] 后端收敛 `CompanionRequestTool` 的 subagent dispatch result：
  - [x] 异步返回文本移除 runtime session id。
  - [x] wait completed 返回文本移除 runtime session id。
  - [x] wait timed out 返回文本移除 runtime session id。
  - [x] details 改为产品级 `kind/child(agent_id)/journal/mailbox/status`，不把 `run_id` 作为 Agent 可见必填字段。
- [x] 后端收敛 `CompanionRequestTool` 的 parent/human result，避免 Agent 可见 details 暴露 parent/child/delivery runtime session id。
- [x] 后端收敛 `CompanionRespondTool` 的 parent/child resolve result，避免默认 details 暴露 delivery runtime session id。
- [x] 明确 journal access ref 沿用 `lifecycle://agent-runs/{agent_id}/sessions/messages`，因为父子 Agent 天然处于同一个 lifecycle run 下。
- [x] 前端新增 Companion subagent presentation parser。
- [x] 前端新增/改造 Companion subagent dispatch card，用当前 lifecycle/workspace 上下文加 `agent_id` 打开 child AgentRun workspace。
- [x] 前端从 AgentRun list projection 向 session card 传入 child shell 状态，用 child `agent_id` 更新标题、状态和 fallback 跳转。
- [x] 后端在 `companion_request target=sub wait=true` 派发 child 后通过 tool update delta 推送 `companion_subagent_dispatch` details，让运行中的卡片从 pending 衔接到 child agent/journal。
- [x] PiAgent stream mapper 对 `companion_subagent_dispatch` 工具结果保留结构化 details content item，让完成态和 delta 都能把 child/journal/status/result_preview 传到前端卡片。
- [x] 前端 Companion subagent dispatch card 展示 `result_preview`，让等待完成的 card 能显示 child agent 回传结果。
- [x] 前端从普通 tool burst 中分离 subagent dispatch。
- [x] 对 `collabAgentToolCall spawnAgent` 增加 AgentDash 解析策略，保留 raw protocol refs 但默认使用产品 refs。
- [x] 更新相关测试。
- [x] 更新 spec，记录 agent id / journal、lifecycle workspace context 与 delivery runtime session 的坐标边界。

## Validation Commands

- `pnpm --filter app-web test -- companion`
- `pnpm --filter app-web test -- session`
- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-api lifecycle_agents`
- 如改动 generated contracts：`cargo run -p agentdash-contracts --bin generate_contracts_ts`

## Validation Results

- `cargo test -p agentdash-application companion`：通过，61 passed。
- `cargo fmt --check -p agentdash-application`：通过。
- `cargo fmt --check -p agentdash-executor`：通过。
- `pnpm --filter app-web test -- companion`：通过，1 file / 6 tests。
- `cargo test -p agentdash-executor companion_subagent_dispatch_result_preserves_structured_details_for_ui`：通过，1 passed。
- `cargo test -p agentdash-executor tool_result_image_content_uses_data_url_for_codex_protocol`：通过，1 passed。
- `cargo test -p agentdash-executor stream_mapper`：通过，2 passed。
- `pnpm --filter app-web test -- session`：通过，25 files / 215 tests。
- `pnpm --filter app-web run typecheck`：通过。
- `git diff --check`：通过。

## Risk Points

- `AgentToolResult.details` 可能被模型、前端或 tool result cache 同时消费，字段收敛需要覆盖文本和 JSON details。
- `collabAgentToolCall` 是 Codex 上游协议形状，AgentDash 扩展解析应与 raw 字段并存，避免破坏上游 item 反序列化。
- AgentRun projection 刷新存在时序，前端卡片需要在 child projection 尚未出现时仍能用 `agent_id` 显示已派发状态，并在 workspace context 可用时提供跳转。

## Start Gate

- PRD 已明确 journal access URI 沿用当前 lifecycle URI。
- `implement.jsonl` / `check.jsonl` 需要保持真实 spec/research 条目后再 `task.py start`。
