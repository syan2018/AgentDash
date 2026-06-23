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
- 每个 Agent turn / provider attempt 需要记录可展示的执行耗时。优先补齐 Codex `Turn.durationMs` 对齐字段，并让前端 turn segment 能稳定展示“已处理 x 秒/分钟”。
- 需要表达 provider 已连接但尚未吐出首个可见 delta 的运行状态。该状态用于告诉用户 Agent 已进入模型等待/思考阶段，区别于本地启动中、重连中、工具执行中和已经输出内容。

## Confirmed Facts

### AgentDash current state

- `crates/agentdash-agent/src/bridge.rs` 的 `BridgeError` 只有 `CompletionFailed` / `EmptyResponse` / `RequestBuildFailed`，没有 retryable/fatal 分类，也没有 server-requested retry delay。
- `crates/agentdash-agent/src/agent_loop/streaming.rs` 遇到 `StreamChunk::Error(error)` 时会构造 `AgentMessage::error_assistant(error.to_string(), false)`，然后仍然通过 `MessageEnd` 结束该 assistant 消息。也就是说 provider 暂态错误当前会进入 agent message 链路，而不是作为可重试运行状态处理。
- `crates/agentdash-agent/src/types.rs` 的 `AgentEvent` 目前没有 retry/reconnect 事件；`AssistantStreamEvent` 也没有 attempt/epoch 信息。
- `crates/agentdash-spi/src/connector/mod.rs` 的 `ConnectorError` 没有 retryable 分类。只有 connector stream 返回 `Err(ConnectorError)` 时，`crates/agentdash-application/src/session/launch/ingestion.rs` 才会把 turn 终止为 failed；PiAgent provider 错误当前多数不会走这条链路。
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` 已经能把 `AgentEvent::ContextCompactionFailed` 映射成 `BackboneEvent::Error(ErrorNotification { will_retry: false, ... })`，说明 PiAgent -> Backbone 的错误事件路径存在，但 provider retry/reconnect 还没有对应事件。
- `crates/agentdash-agent-protocol/src/backbone/event.rs` 已有 `BackboneEvent::Error(codex::ErrorNotification)`；生成到前端的 `ErrorNotification` 包含 `willRetry`。`crates/agentdash-executor/src/connectors/codex_bridge.rs` 会直接透传 Codex bridge 的 `error` notification。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs` 提供 `PlatformEvent::SessionMetaUpdate` 作为平台结构化扩展，但 cross-layer spec 要求平台能力保持结构化 payload，不能把业务语义塞进自由文本。
- Codex `Turn` wire shape 已包含 `startedAt`、`completedAt`、`durationMs`；AgentDash 当前 `build_turn_started_envelope` 填 `started_at`，但 `duration_ms` 为 `None`，turn terminal 主要通过 `Platform(SessionMetaUpdate key="turn_terminal")` 表达。
- 前端 `useSessionFeed.segmentByTurn` 已读取 `turn_completed.payload.turn.durationMs`，`SessionChatViewParts` 已能展示“已处理 {duration}”。当前 PiAgent terminal 不稳定产出 `TurnCompleted` 时，这条展示链路不会完整发挥作用。

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
- Codex app-server `TurnStartedNotification` / `TurnCompletedNotification` 均承载 `Turn`；TUI 使用 turn lifecycle 维护 running 状态和 final separator duration。
- Codex TUI 有 `Thinking` 状态头，并在 stream error 时暂存原状态、显示 `Reconnecting... n/m`，重连成功后恢复状态头。Codex 没有 public `stream_connected` / `waiting_for_first_token` / durable `thinking` 一等事件；这些更多由 `TurnStarted/InProgress` 到首个 delta 之间推断，最终 TTFT 只在完成事件中记录。
- Codex `ErrorNotification.will_retry` 是二值字段，attempt/max/delay/provider/source 没有结构化 public DTO 字段，主要藏在 message/details 里。

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
- turn elapsed time 对齐 Codex `Turn.durationMs`。AgentDash 应在 turn start 记录毫秒时间戳，terminal 时计算 duration，并让持久化/NDJSON/前端 feed 使用同一事实。
- “已连接，等待首字/思考中”应是运行状态事件，不是 assistant 文本。若 Codex 没有 app-server 一等事件，可使用 AgentDash 结构化 platform/provider status 事件承载，并在前端用 Codex Thinking/Reconnecting 的交互风格展示。
- 失败轮次恢复采用 append-only rollback/stable-boundary marker + projection filter，不优先物理删除 `session_events` 尾部。原因是当前 `SessionEventStore` 没有 truncate API，`event_seq` 是 NDJSON resume 游标，且 `SessionMeta.save` 使用 `GREATEST(last_event_seq)`，普通 meta save 不能回退 head。
- 前端当前 `rawEvents` reducer 是 append-only，且没有 rewind/truncate/replacement 语义；后端丢弃最后失败轮次时，必须发显式 `session_rewound/session_rebuilt` 类事件或触发 full rehydrate，不能只改变后端历史读取结果。
- 协议落点采用类型安全的一等 `PlatformEvent` variants：`ProviderAttemptStatus` 表达连接、等待首字、重连与 retry 生命周期；`SessionRewound` 表达失败轮次恢复。原因是项目处于预研期，直接补正确协议比把关键业务状态塞进 `SessionMetaUpdate` 自由 key 更利于 Rust/TS 类型同步与前端 reducer 测试。
- provider/runtime failure 写入 terminal failed/lost/interrupted 与 recovery marker 后，会话应恢复到可再次提交的状态。失败诊断保留给 UI，但下一次 prompt 不继承半截 provider 输出，也不因为已恢复的失败轮次继续停在 mailbox paused 状态。
- 后续实现按 `implement.md` 的 parallel execution plan 拆成 protocol、bridge/provider retry、agent loop boundary、session recovery、frontend feed、integration check 六条可独立验证的工作流。

## Open Questions

- 当前没有阻塞 planning 的 open question。进入实现前只需要用户评审 `design.md` 与 `implement.md`，确认 append-only recovery marker、类型安全事件协议、mailbox 恢复行为和测试范围。

## Acceptance Criteria

- [ ] 形成一份后续可评审的需求记录，说明 provider 波动、重试、重连事件的目标行为和约束。
- [ ] 记录 `references/pi-mono` 中与 provider retry/reconnect 最相关的参考入口。
- [ ] 记录 `references/codex` 中可借鉴的 retry/reconnect 事件语义对照。
- [ ] 明确失败轮次恢复策略，覆盖 session event store、projection、前端 feed 与下一次 AgentRun 输入上下文。
- [ ] turn terminal 或等价事件携带可展示 `durationMs`，前端 turn segment 能显示每轮已执行时间。
- [ ] 连接成功等待首字/思考中、重连中、retry 成功/耗尽等状态在事件流中可观测，且不进入 assistant message。
- [ ] 在进入实现前补充设计与执行计划，覆盖 Bridge 层、Agent loop、AgentRun/session 事件链路的边界。
- [ ] 并行推进方案清晰记录每条 workstream 的范围、依赖、产物和验证方式，便于后续 Trellis sub-agents 分工。

## Notes

- 本任务当前仅做问题收纳与后续评估入口记录，不在创建阶段展开技术方案。
