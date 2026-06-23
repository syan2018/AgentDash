# Agent 执行 Provider 波动重试与重连事件评估

## Goal

评估并规划 Agent 执行期间的 provider 暂态波动处理能力，让模型请求、流式输出或服务商短暂异常不会直接把一次 AgentRun 终止为失败，也不会把暂态错误写成普通 assistant 消息污染后续上下文。

## Requirements

- 梳理当前 AgentDash provider bridge、Agent loop、AgentRun/session 事件链路中 provider 错误的传播方式。
- 优先参考 `references/pi-mono` 中 provider 调用、stream 处理、retry/reconnect 相关实现；`references/codex` 的 retry policy、stream retry 与 `will_retry` 重连消息可作为对照参考。
- 区分 provider 层可判断的暂态错误与 AgentRun 层应表达的运行状态，评估重试能力应落在 Bridge 层、AgentRun 层，或二者分工协作。
- 明确重连事件在运行事件流中的表达方式，使 UI 和持久化事件能展示“正在重连/重试”，而不是把暂态错误当作终态失败。
- 识别半截流式输出、工具调用、上下文污染、重复事件等风险边界，为后续设计和实现拆分提供依据。
- 数据层需要支持失败轮次恢复：当 provider 中途失败、连接异常或其他非连接问题导致当前运行不可继续时，应丢弃/回滚最后一个未稳定完成的轮次，把会话恢复到可以重新开始提交的状态。

## Confirmed Facts

### AgentDash current state

- `crates/agentdash-agent/src/bridge.rs` 的 `BridgeError` 只有 `CompletionFailed` / `EmptyResponse` / `RequestBuildFailed`，没有 retryable/fatal 分类，也没有 server-requested retry delay。
- `crates/agentdash-agent/src/agent_loop/streaming.rs` 遇到 `StreamChunk::Error(error)` 时会构造 `AgentMessage::error_assistant(error.to_string(), false)`，然后仍然通过 `MessageEnd` 结束该 assistant 消息。也就是说 provider 暂态错误当前会进入 agent message 链路，而不是作为可重试运行状态处理。
- `crates/agentdash-agent/src/types.rs` 的 `AgentEvent` 目前没有 retry/reconnect 事件；`AssistantStreamEvent` 也没有 attempt/epoch 信息。
- `crates/agentdash-spi/src/connector/mod.rs` 的 `ConnectorError` 没有 retryable 分类。只有 connector stream 返回 `Err(ConnectorError)` 时，`crates/agentdash-application/src/session/launch/ingestion.rs` 才会把 turn 终止为 failed；PiAgent provider 错误当前多数不会走这条链路。
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` 已经能把 `AgentEvent::ContextCompactionFailed` 映射成 `BackboneEvent::Error(ErrorNotification { will_retry: false, ... })`，说明 PiAgent -> Backbone 的错误事件路径存在，但 provider retry/reconnect 还没有对应事件。
- `crates/agentdash-agent-protocol/src/backbone/event.rs` 已有 `BackboneEvent::Error(codex::ErrorNotification)`；生成到前端的 `ErrorNotification` 包含 `willRetry`。`crates/agentdash-executor/src/connectors/codex_bridge.rs` 会直接透传 Codex bridge 的 `error` notification。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs` 提供 `PlatformEvent::SessionMetaUpdate` 作为平台结构化扩展，但 cross-layer spec 要求平台能力保持结构化 payload，不能把业务语义塞进自由文本。

### pi-mono reference facts

- `references/pi-mono/packages/ai/src/types.ts` 定义了 provider stream 合同：`StreamFunction` 不应因 request/model/runtime 失败直接 throw，而应在返回的 `AssistantMessageEventStream` 中以 `error` 事件结束，最终 `AssistantMessage.stopReason` 为 `"error"` 或 `"aborted"` 并携带 `errorMessage`。
- `references/pi-mono/packages/agent/src/agent-loop.ts` 的 agent loop 在 assistant message 为 `stopReason === "error" || "aborted"` 时正常发出 `turn_end` 和 `agent_end`，为上层 session 自动 retry 留出统一入口。
- `references/pi-mono/packages/coding-agent/src/core/agent-session.ts` 在 `agent_end` 时检查最后一条 assistant 是否是 retryable error；若是，则发 `auto_retry_start`，从 agent 内存状态移除最后一条 error assistant，等待指数退避后调用 `agent.continue()`。
- 同一文件中 `prompt()` 在 `agent.prompt(messages)` 后调用 `waitForRetry()`，并等待 retry 以及后续 agent loop 完整 idle。`agent-session-retry-events.test.ts` 覆盖了 retry 成功、连续失败后成功、耗尽重试、禁用 retry、非 retryable 错误不重试、retry sleep 可取消、retry 后产生 tool call 时 prompt 不提前返回。
- pi-mono 的 session 级 retry 判定排除 context overflow；retryable 文本模式覆盖 overloaded、rate limit、429、500/502/503/504、service unavailable、network/connection/fetch failed、timeout、terminated、ended without 等。
- `references/pi-mono/packages/coding-agent/src/core/settings-manager.ts` 默认 retry enabled，`maxRetries = 3`，`baseDelayMs = 2000`，`maxDelayMs = 60000`。
- `references/pi-mono/packages/ai/src/providers/google-gemini-cli.ts` 在 provider 层处理 429/5xx/network/endpoint fallback，并从 `Retry-After`、`x-ratelimit-reset`、`x-ratelimit-reset-after` 与 body 中提取 retry delay；若 server-requested delay 超过 `maxRetryDelayMs`，则抛出带可读信息的错误交给更高层可见处理。
- 同一 provider 还对空 SSE 响应做有限重试，重试前 `resetOutput()`，测试 `google-gemini-cli-empty-stream.test.ts` 明确断言不会重复发 `start`，最终只产生一次 `done`。
- `references/pi-mono/packages/ai/src/providers/openai-codex-responses.ts` 对 ChatGPT/Codex Responses 也有 429/5xx/network fetch retry；非 retryable 或最终失败后解析 friendly error，最后进入 stream error 事件。

### Codex reference facts

- `references/codex/codex-rs/codex-client/src/retry.rs` 提供请求级 retry policy：`max_attempts`、`base_delay`、`retry_429`、`retry_5xx`、`retry_transport`，退避为指数 backoff + jitter。
- `references/codex/codex-rs/core/src/responses_retry.rs` 处理 Responses stream retry：retryable stream error 会通知 `Reconnecting... {retry_count}/{max_retries}`，并通过 `notify_stream_error` 形成可见中间状态。
- `references/codex/codex-rs/protocol/src/error.rs` 的 `CodexErr::Stream` 表达 SSE 已建立但在 `response.completed` 前断开；`is_retryable()` 明确区分 usage limit、quota、context window、invalid request 等 fatal 与 stream/timeout/transport/5xx 等 retryable。
- `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs` 将 `EventMsg::StreamError` 映射成 `ErrorNotification { will_retry: true, ... }`。注释说明 stream error 是 retry 的 intermediate state，不更新 turn summary。

## Research Notes

- pi-mono 与 Codex 都不是单层 retry：provider/request 层负责“尚未形成稳定输出”的暂态失败；session/core loop 层负责“运行中断但可继续”的可见重试状态与用户反馈。
- AgentDash 现状更接近 pi-mono 的错误形态：provider 错误会成为 assistant error message。但 AgentDash 还缺少 pi-mono session 层的自动 retry、错误 assistant 从运行内存中移除、prompt 等待 retry 完整结束这几件事。
- 单纯在 Bridge 层无条件重试有重复事件风险：如果已发出 text/reasoning/tool delta，再重新发起 provider 请求，当前 Backbone/前端没有 attempt/epoch 或 replacement 语义来区分旧尝试与新尝试。
- 单纯在 AgentRun/session 层 retry 也不够：provider 层已经能看到 HTTP status、headers、Retry-After、空 SSE 等更精确事实，适合在那里完成首包前的透明 retry 与错误归类。
- `BackboneEvent::Error(ErrorNotification { will_retry: true })` 与 Codex 对齐，适合承载“正在重连/重试”的用户可见中间态；若需要更细的 attempt/maxAttempts/delayMs/provider/source，可能需要新增结构化平台事件或扩展 AgentEvent/Backbone 表达，而不是只写日志。

## Decisions

- 已产生任何可见 assistant delta / reasoning delta / tool delta 后，不做中途 retry。原因是本次 provider stream 已经进入用户可见与持久化事件流，重新请求会制造重复 delta、分叉工具调用或不可解释的上下文替换；当前目标是把这类中断作为本次尝试的失败/取消处理，而不是尝试“接上”或“重放”。
- retry 机制采用 pi agent 风格：provider/bridge 层负责把 provider 暂态失败规范化并在首个可见 delta 前做可控 retry；AgentRun/session 层负责识别可重试失败、移除错误 assistant / 未稳定轮次、退避后重新开始，并确保 prompt/run 等待 retry 完整结束。
- 前端提示采用 Codex 风格：重连/重试作为运行中的系统状态或错误通知表达，类似 `ErrorNotification { will_retry: true }` / `Reconnecting... attempt/max`，不能写成 assistant 消息。
- 最后一个未稳定轮次失败后，数据层必须能恢复到上一稳定边界。恢复目标不是保留失败半截继续拼接，而是让用户或自动 retry 可以从干净状态重新开始；连接问题和其它会话运行炸掉的问题都应走这个恢复原则。

## Open Questions

- 失败轮次恢复的精确稳定边界需要继续从 session event store、projection、turn terminal、frontend feed 聚合中取证：是以 `TurnCompleted` 为提交边界，还是需要额外标记 provider attempt / run attempt 边界。

## Acceptance Criteria

- [ ] 形成一份后续可评审的需求记录，说明 provider 波动、重试、重连事件的目标行为和约束。
- [ ] 记录 `references/pi-mono` 中与 provider retry/reconnect 最相关的参考入口。
- [ ] 记录 `references/codex` 中可借鉴的 retry/reconnect 事件语义对照。
- [ ] 明确失败轮次恢复策略，覆盖 session event store、projection、前端 feed 与下一次 AgentRun 输入上下文。
- [ ] 在进入实现前补充设计与执行计划，覆盖 Bridge 层、Agent loop、AgentRun/session 事件链路的边界。

## Notes

- 本任务当前仅做问题收纳与后续评估入口记录，不在创建阶段展开技术方案。
