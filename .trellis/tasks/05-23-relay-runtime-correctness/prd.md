# Relay 运行时正确性修复

## Goal

修复 cloud/local relay prompt 与 session event stream 的关键时序问题，确保本机事件不会因为 sink 注册顺序、backend 断连或重复 forwarder 而丢失、延迟失败或重复投递。

## Confirmed Facts

- `crates/agentdash-application/src/relay_connector.rs` 中 `RelayAgentConnector::prompt` 当前先 `relay_prompt().await`，后 `register_session_sink`。
- `crates/agentdash-api/src/relay/registry.rs` 中 `BackendRegistry::unregister` 当前包含 `retain(|_, _| true)`，不会按 backend 清理 pending。
- `crates/agentdash-local/src/handlers/prompt.rs` 每次 prompt 成功都会 spawn `forward_session_notifications`。

## Requirements

- 在发送 `command.prompt` 前注册 session sink，并在失败、终态、cancel 或 stream end 时清理。
- pending command 必须关联 backend id，backend unregister 时清理该 backend 的 pending request，使等待方立即失败而不是等待 timeout。
- local relay session notification forwarder 必须按 session 去重；同一 relay session 多轮 prompt 不应重复转发同一事件。
- 保持现有 relay JSON 协议形态不变。
- 补充覆盖关键时序的单元测试或集成级测试。
- 将新确认的 relay 时序不变量沉淀到 cross-layer 或 relay 相关 spec。

## Acceptance Criteria

- [x] `RelayAgentConnector::prompt` 在 `relay_prompt` 前完成 sink 注册，且错误路径会 unregister。
- [x] `BackendRegistry` pending map 能按 backend 清理，断连中的 `send_command` 不再等 30s timeout。
- [x] local prompt handler 对同一 session 的 notification forwarder 去重。
- [x] 测试覆盖：response.prompt 前 notification 不丢失。
- [x] 测试覆盖：backend disconnect 后 pending command 立即返回 dropped/offline 类错误。
- [x] 测试覆盖：同一 local relay session 多次 prompt 不重复转发同一 notification。
- [x] 相关 spec 记录 relay prompt/event 的注册与清理顺序。

## Out of Scope

- 不拆 `agentdash-relay/src/protocol.rs`。
- 不改 relay message JSON 兼容形态。
- 不引入 local/cloud fallback 策略。
