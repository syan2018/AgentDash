# Research: frontend retry feed

- Query: 前端 session stream/feed 对 ErrorNotification.willRetry、PlatformEvent、TurnStarted/TurnCompleted、系统事件卡、rawEvents 的处理；评估如何以 Codex 风格展示 provider reconnect/retry、elapsed time、已连接等待首字/思考状态；后端丢弃最后失败轮次时前端 feed/store 如何同步或重建。
- Scope: mixed
- Date: 2026-06-23

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-23-agent-provider-retry-reconnect/prd.md` | 当前任务需求与已确认事实，明确 retry/reconnect 不能写成 assistant 消息，失败轮次需恢复到上一稳定边界。 |
| `.trellis/spec/frontend/architecture.md` | 前端不创建第二套业务事实源，运行态以后端 projection / event stream 为准。 |
| `.trellis/spec/frontend/state-management.md` | RuntimeSession 终态通过 `Platform(SessionMetaUpdate key="turn_terminal")` 统一进入前端，AgentRun workspace 监听后刷新权威 snapshot。 |
| `.trellis/spec/frontend/hook-guidelines.md` | `useSessionStream` / `streamTransport` / `useSessionFeed` 的 NDJSON 与 feed 聚合契约，`rawEvents` 是事实源。 |
| `.trellis/spec/frontend/design-language.md` | UI 改动应使用语义 token、有限 radius、已有 primitive 风格。 |
| `.trellis/spec/backend/session/streaming-protocol.md` | Session NDJSON envelope、`x-stream-since-id` 续传与前端消费要求。 |
| `packages/app-web/src/features/session/model/streamTransport.ts` | 会话 NDJSON transport，负责网络连接、续传游标与自动重连。 |
| `packages/app-web/src/features/session/model/useSessionStream.ts` | session stream hook，先历史 hydrate，再接增量流；维护 `isConnected/isLoading/isReceiving/error/rawEvents`。 |
| `packages/app-web/src/features/session/model/sessionStreamReducer.ts` | stream reducer，把事件累积为 `rawEvents` 与可渲染 `entries`。 |
| `packages/app-web/src/features/session/model/useSessionFeed.ts` | feed 聚合与 `segmentByTurn`，从 `rawEvents` 派生 turn 分段和 duration。 |
| `packages/app-web/src/features/session/model/platformEvent.ts` | 从 `PlatformEvent` 统一提取 event type / data / message。 |
| `packages/app-web/src/features/session/model/systemEventPolicy.ts` | 系统事件可见性与 feed boundary 策略。 |
| `packages/app-web/src/features/session/ui/SessionEntry.tsx` | 单条 feed entry 渲染，error/platform/reasoning/agent message 的当前 UI 分派点。 |
| `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx` | 系统事件卡渲染与事件文案映射。 |
| `packages/app-web/src/features/session/ui/SessionChatView.tsx` | 页面层消费 feed、rawEvents、连接状态、turn lifecycle，并展示状态栏/错误横幅。 |
| `packages/app-web/src/features/session/ui/SessionChatViewModel.ts` | 从 raw BackboneEvent 提取 turn lifecycle 和 projection refresh key。 |
| `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx` | 状态条、stream 空态、turn section 折叠与 duration 展示。 |
| `packages/app-web/src/features/session/ui/SessionList.tsx` | 较老/通用 session list，对断线重连有 sticky 提示。 |
| `packages/app-web/src/generated/backbone-protocol.ts` | 生成的 BackboneEvent、ErrorNotification、PlatformEvent、TurnStarted/TurnCompleted 类型。 |
| `packages/app-web/src/generated/session-contracts.ts` | 生成的 Session NDJSON envelope 与 event page response 类型。 |
| `crates/agentdash-agent-protocol/src/backbone/event.rs` | 后端 BackboneEvent enum，错误复用 Codex `ErrorNotification`，平台扩展走 `PlatformEvent`。 |
| `crates/agentdash-application/src/session/hub_support.rs` | 后端 turn_started 与 `turn_terminal` 平台事件构造/解析。 |
| `crates/agentdash-application/src/session/launch/ingestion.rs` | 后端 stream adapter 从 connector stream 读取 notification，终态事件结束 turn processor。 |
| `crates/agentdash-application/src/session/launch/commit.rs` | accepted-after-commit 边界；user/start/context/runtime facts 已提交后才 attach stream。 |
| `crates/agentdash-executor/src/connectors/codex_bridge.rs` | Codex connector 透传 `turn/started` 和 `turn/completed`。 |
| `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` | PiAgent 已有 ErrorNotification 路径，但当前 `will_retry=false`。 |
| `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs` | Codex 把 stream retry 中间态映射成 `ErrorNotification { will_retry: true }`，不更新 turn summary。 |
| `references/codex/codex-rs/core/src/responses_retry.rs` | Codex stream retry 生成 `Reconnecting... {retry_count}/{max_retries}` 并通知 stream error。 |
| `references/codex/codex-rs/tui/src/chatwidget/protocol.rs` | Codex TUI 对 `will_retry=true` 错误采用临时状态处理，不当作普通历史错误。 |
| `references/pi-mono/packages/coding-agent/test/suite/agent-session-retry-events.test.ts` | pi-mono retry 事件覆盖 attempt、成功/失败、禁用、不可重试、取消 sleep、retry 后继续 tool loop。 |

### Current Stream And Feed Data Flow

- `useSessionStream` 明确把 `rawEvents` 作为事实源，`entries` 只是基于事件流派生出来的显示状态。见 `packages/app-web/src/features/session/model/useSessionStream.ts:1` 和 `packages/app-web/src/features/session/model/useSessionStream.ts:5`。
- stream hook 的状态出口包含 `rawEvents`、`isConnected`、`isLoading`、`isReceiving`、`error`、`reconnect`。见 `packages/app-web/src/features/session/model/useSessionStream.ts:37`。
- 初始化会用历史 page hydrate，再用 `lastAppliedSeq` 建立 NDJSON 续传连接。见 `packages/app-web/src/features/session/model/useSessionStream.ts:200`、`packages/app-web/src/features/session/model/useSessionStream.ts:202`、`packages/app-web/src/features/session/model/useSessionStream.ts:215`、`packages/app-web/src/features/session/model/useSessionStream.ts:218`。
- 每条事件先经 `dispatchSessionPlatformEvent` 拦截 terminal output/state，剩余事件进入 React state 管道。见 `packages/app-web/src/features/session/model/useSessionStream.ts:125`、`packages/app-web/src/features/session/model/useSessionStream.ts:128`。
- reducer 用 `event_seq <= lastAppliedSeq` 跳过重复事件，并把接受的事件追加到 `rawEvents`，再更新 `entries`。见 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:294`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:298`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:299`。
- NDJSON transport 对 `connected` 更新本地 `sinceId`，对 `event` 解析后推进 `sinceId` 并回调 `onEvent`。见 `packages/app-web/src/features/session/model/streamTransport.ts:246`、`packages/app-web/src/features/session/model/streamTransport.ts:261`、`packages/app-web/src/features/session/model/streamTransport.ts:264`。
- 网络层 reconnect 是 transport 生命周期，不是 provider retry 事件：`scheduleReconnect` 把 lifecycle 设为 `reconnecting`，按 800ms 到 8000ms 指数退避重连。见 `packages/app-web/src/features/session/model/streamTransport.ts:8`、`packages/app-web/src/features/session/model/streamTransport.ts:279`、`packages/app-web/src/features/session/model/streamTransport.ts:280`。

### Current Error Handling

- 生成类型里 `ErrorNotification` 已有 `willRetry` 字段：`{ error, willRetry, threadId, turnId }`。见 `packages/app-web/src/generated/backbone-protocol.ts:154`。
- 前端目前没有消费 `willRetry`。检索 `willRetry|will_retry` 仅命中生成类型，session UI 的 error 分支只展示 `event.payload.error.message`。见 `packages/app-web/src/features/session/ui/SessionEntry.tsx:141`、`packages/app-web/src/features/session/ui/SessionEntry.tsx:147`。
- reducer 把 `error` 事件作为普通 display entry 追加。见 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:249`。
- feed 聚合把 `error` 归为 hard boundary，会截断工具 burst 并成为独立条目。见 `packages/app-web/src/features/session/model/useSessionFeed.ts:153`、`packages/app-web/src/features/session/model/useSessionFeed.ts:157`。
- 当前策略下，`ErrorNotification.willRetry=true` 会被当成红色错误行进入 feed 历史，和 Codex “retry 中间态不进普通历史 cell” 不一致。
- PiAgent 现有 context compaction failure 会同时 emit `Platform(SessionMetaUpdate key="context_compaction_failed")` 和 `ErrorNotification { will_retry: false }`。见 `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:861`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:875`、`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:883`。

### Current Platform/System Event Rendering

- `PlatformEvent` 生成类型包括 `executor_session_bound`、`source_session_title_updated`、`hook_trace`、`session_meta_update`、`terminal_output`、`terminal_state_changed`、`mailbox_state_changed`。见 `packages/app-web/src/generated/backbone-protocol.ts:269`。
- `platformEvent.ts` 把 `executor_session_bound` 映射为同名 event type，`hook_trace` 映射为 `hook_event`，`session_meta_update` 直接使用 `data.key`。见 `packages/app-web/src/features/session/model/platformEvent.ts:15`、`packages/app-web/src/features/session/model/platformEvent.ts:19`、`packages/app-web/src/features/session/model/platformEvent.ts:22`。
- `session_meta_update` 的 `value` 是对象时作为 data，字符串时包装成 `{ message }`；message 提取只读 `value.message`。见 `packages/app-web/src/features/session/model/platformEvent.ts:54`、`packages/app-web/src/features/session/model/platformEvent.ts:73`。
- 系统事件可见白名单在 `RENDERABLE_SYSTEM_EVENT_TYPES`，包括 `executor_session_bound`、`turn_interrupted`、`turn_failed`、`hook_event`、companion 事件、workspace module 事件和 `context_frame` 等。见 `packages/app-web/src/features/session/model/systemEventPolicy.ts:16`。
- 由于后端终态统一 key 是 `turn_terminal`，`extractPlatformEventType` 返回的是 `turn_terminal`，它不在当前 renderable system event 白名单内；因此 `Platform(SessionMetaUpdate key="turn_terminal")` 当前不会显示系统事件卡。见 `packages/app-web/src/features/session/model/platformEvent.ts:22`、`packages/app-web/src/features/session/model/systemEventPolicy.ts:117`。
- renderable platform event 会成为 hard boundary；不可渲染 platform event 是 neutral。见 `packages/app-web/src/features/session/model/systemEventPolicy.ts:125`、`packages/app-web/src/features/session/model/systemEventPolicy.ts:126`。
- `SessionEntry` 中 platform 先走 task event，再走 renderable system event，否则返回 null。见 `packages/app-web/src/features/session/ui/SessionEntry.tsx:157`、`packages/app-web/src/features/session/ui/SessionEntry.tsx:162`、`packages/app-web/src/features/session/ui/SessionEntry.tsx:172`。
- 系统事件卡的通用分支使用中性色 `info` badge，默认 message 来自 `EVENT_TYPE_DEFAULT_MESSAGES` 或 `value.message`。见 `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:299`、`packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:303`。
- `SessionSystemEventCard` 虽然有 `turn_failed` / `turn_interrupted` label 和默认文案，但这些只有当 event type 本身是 `turn_failed` 或 `turn_interrupted` 时才会生效；当前 `turn_terminal` key 不会命中。见 `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:78`、`packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:102`。

### Current TurnStarted / TurnCompleted / turn_terminal Handling

- reducer 对 `turn_started` 和 `turn_completed` 静默，不生成 display entry。见 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:215`。
- feed classification 也把 turn start/completed 视为 neutral。见 `packages/app-web/src/features/session/model/useSessionFeed.ts:141`。
- `segmentByTurn` 只从 `rawEvents` 中的 `turn_completed` 读取 turn status/duration，写入 `turnMeta`。见 `packages/app-web/src/features/session/model/useSessionFeed.ts:360`、`packages/app-web/src/features/session/model/useSessionFeed.ts:367`、`packages/app-web/src/features/session/model/useSessionFeed.ts:374`。
- `segmentByTurn` 不读取 `turn_started` 的 `startedAt`，也不读取 `Platform(SessionMetaUpdate key="turn_terminal")` 的 `terminal_type/message`；如果后端不发 `turn_completed`，turn section 默认 `active` 且没有 duration。见 `packages/app-web/src/features/session/model/useSessionFeed.ts:389`、`packages/app-web/src/features/session/model/useSessionFeed.ts:399`。
- 页面层 `extractTurnLifecycleEventType` 会识别 `turn_started` / `turn_completed`，也会从 `turn_terminal.value.terminal_type` 识别 `turn_completed|turn_failed|turn_interrupted`。见 `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:52`、`packages/app-web/src/features/session/ui/SessionChatViewModel.ts:56`、`packages/app-web/src/features/session/ui/SessionChatViewModel.ts:67`。
- 页面层收到 `turn_started` 会清除 optimistic running；收到 `turn_completed|turn_failed|turn_interrupted` 会停止 optimistic running 并触发 `onTurnEnd`。见 `packages/app-web/src/features/session/ui/SessionChatView.tsx:351`、`packages/app-web/src/features/session/ui/SessionChatView.tsx:358`、`packages/app-web/src/features/session/ui/SessionChatView.tsx:362`。
- `computeProjectionRefreshKey` 会把非 started 的 turn lifecycle 作为 projection refresh 触发点，因此 `turn_terminal` 可以刷新 projection。见 `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:155`、`packages/app-web/src/features/session/ui/SessionChatViewModel.ts:167`。
- 后端 `build_turn_started_envelope` 构造 Codex `TurnStartedNotification`，`Turn.started_at` 是秒级 timestamp，`duration_ms` 为空。见 `crates/agentdash-application/src/session/hub_support.rs:39`、`crates/agentdash-application/src/session/hub_support.rs:54`。
- 后端 `build_turn_terminal_envelope` 统一使用 `PlatformEvent::SessionMetaUpdate { key: "turn_terminal", value: { terminal_type, message } }`。见 `crates/agentdash-application/src/session/hub_support.rs:78`、`crates/agentdash-application/src/session/hub_support.rs:85`、`crates/agentdash-application/src/session/hub_support.rs:90`。
- 后端 `parse_turn_terminal_event_from_envelope` 支持 `turn_lost`，但前端 `extractTurnLifecycleEventType` 目前不识别 `turn_lost`。见 `crates/agentdash-application/src/session/hub_support.rs:119`、`crates/agentdash-application/src/session/hub_support.rs:123`、`packages/app-web/src/features/session/ui/SessionChatViewModel.ts:46`。

### Current Connected / Loading / Receiving UI

- `useSessionStream` 的 lifecycle `connected` 会设置 `isConnected=true`、`isLoading=false`、`error=null`。见 `packages/app-web/src/features/session/model/useSessionStream.ts:226`。
- lifecycle `connecting|reconnecting` 会设置 `isConnected=false`、`isLoading=true`。见 `packages/app-web/src/features/session/model/useSessionStream.ts:234`。
- `isReceiving` 只表示最近 600ms 收到 event，用于 streaming cursor，而不是“已连接但等待首字”。见 `packages/app-web/src/features/session/model/useSessionStream.ts:50`、`packages/app-web/src/features/session/model/useSessionStream.ts:97`。
- 页面状态条文案只有“待创建 / 已连接 / 连接中… / 未连接”，颜色对应 muted/success/warning/destructive。见 `packages/app-web/src/features/session/ui/SessionChatView.tsx:607`、`packages/app-web/src/features/session/ui/SessionChatView.tsx:610`。
- running chip 在 action running 时显示，已连接时文案是“接收中”，未连接时是“执行中”。见 `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:235`、`packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:238`。
- stream 空态在 `isLoading && displayItems.length === 0` 时显示 spinner 与“正在连接…”。见 `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:281`、`packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:285`。
- turn section 对 completed segment 显示“已处理 {duration}”，duration 只来自 `turn_completed.turn.durationMs`。见 `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:356`、`packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:394`、`packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:425`。
- `SessionList` 另有断线 sticky 提示“连接已断开，正在尝试重新连接...”，但 `SessionChatView` 没有同样的 inline reconnect banner。见 `packages/app-web/src/features/session/ui/SessionList.tsx:71`。

### Codex-Style Retry/Reconnecting Reference

- Codex app-server 对 `EventMsg::StreamError` 的注释说明：stream error 是 retries 的 intermediate error state，不需要更新 turn summary store；随后发送 `ErrorNotification { will_retry: true }`。见 `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:928`、`references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:929`、`references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:937`。
- Codex `responses_retry.rs` 在可重试 stream error 时生成 `Reconnecting... {retry_count}/{max_retries}` 并 `notify_stream_error`。见 `references/codex/codex-rs/core/src/responses_retry.rs:48`、`references/codex/codex-rs/core/src/responses_retry.rs:67`、`references/codex/codex-rs/core/src/responses_retry.rs:69`。
- Codex TUI tests 明确期望 StreamError 不生成普通 history cell。见 `references/codex/codex-rs/tui/src/chatwidget/tests/status_and_layout.rs:1873`、`references/codex/codex-rs/tui/src/chatwidget/tests/status_and_layout.rs:1880`。
- pi-mono retry event 测试把 retry 表达为 `auto_retry_start` / `auto_retry_end`，并覆盖 attempt、success、max retries、非 retryable、retry sleep cancel 和 retry 后继续 tool loop。见 `references/pi-mono/packages/coding-agent/test/suite/agent-session-retry-events.test.ts:24`、`references/pi-mono/packages/coding-agent/test/suite/agent-session-retry-events.test.ts:38`、`references/pi-mono/packages/coding-agent/test/suite/agent-session-retry-events.test.ts:49`、`references/pi-mono/packages/coding-agent/test/suite/agent-session-retry-events.test.ts:92`、`references/pi-mono/packages/coding-agent/test/suite/agent-session-retry-events.test.ts:144`。

### Recommended Frontend Behavior

#### Reconnect / Retry

- 区分 transport reconnect 和 provider retry：
  - transport reconnect 继续由 `streamTransport.ts` 管理，只影响连接状态条和错误横幅。
  - provider retry 应来自 event stream 的业务事实，不能复用 `isLoading/isConnected`，否则“浏览器到后端断线”和“后端到模型 provider 重试”会混在一起。
- Codex 风格建议：`ErrorNotification.willRetry=true` 作为临时运行状态展示，不进入普通红色 error feed history。最小实现可在 `SessionEntry` 的 error 分支里改成 retry strip/card，并在聚合层把 willRetry error 视为 neutral 或 soft boundary，避免污染 turn 内容和工具 burst。
- 更稳妥的最小实现：新增 `ProviderRetryStatus` 派生 helper，从 `rawEvents` 扫描最近的 retry/error/turn_terminal 事件，作为 `SessionChatStatusBar` 里的 running chip / 状态 chip 展示；`willRetry=true` error entry 本身不渲染或渲染为中性 “正在重连…” strip。
- 文案建议：
  - 正在重连 provider：`正在重连模型服务`
  - 带 attempt：`正在重连模型服务 2/5`
  - 带 delay：`将在 8s 后重试`
  - 使用 `warning/info` 语义 token，而不是 destructive；只有最终 `willRetry=false` 或 `turn_failed` 才用 destructive。
- 后端如果继续只提供 Codex 文本 `Reconnecting... 1/5`，前端可以短期直接显示 message；但为了本项目 contract 正确性，建议后端提供结构化 payload，前端不解析英文 message。

#### Elapsed Time

- 当前 completed duration 只依赖 `Turn.durationMs`。如果需要 active elapsed，需要从后端提供可稳定计算的 `started_at_ms` 或使用 `turn_started.turn.startedAt`，但当前 `startedAt` 是秒级，前端生成类型为 `number | null`，命名不是 `startedAtMs`。
- 最小前端改动：在 `segmentByTurn` 的 `turnMeta` 中同时处理 `turn_started`，记录 `startedAtMs`；active segment 可由 `TurnSection` 内部定时计算 `Date.now() - startedAtMs`，completed/failed/interrupted 优先使用后端 `duration_ms`。
- 后端更推荐在 `turn_terminal` 里直接给 `started_at_ms`、`completed_at_ms`、`duration_ms`，这样即使后端不发 Codex `turn_completed`，前端仍能显示“已处理 12s / 失败于 12s”。这也能避免秒/毫秒歧义。

#### 已连接等待首字 / 思考状态

- 当前 `isConnected=true` 只说明浏览器到后端 NDJSON 已连上；`isReceiving` 只说明最近 600ms 收过 event；没有“provider 已接受请求但尚未出首字”的显式状态。
- 可从现有事件推导一个最低限状态：
  - 最近 lifecycle 是 `turn_started`，尚未看到本 turn 的 `agent_message_delta` / `reasoning_*` / tool item / terminal，则显示“已连接，等待首字”或“正在思考”。
  - 若收到 `reasoning_text_delta` / `reasoning_summary_delta`，显示现有 thinking entry；状态条可显示“思考中”。
  - 若收到 tool item inProgress，显示工具执行中；沿用 `ToolCallCardShell` active tool UI。
- 该推导需要从 `rawEvents` 扫描当前 active turn 的 first visible output，适合放在 `useSessionFeed` 或 `SessionChatViewModel` 里做派生，不写入 store。
- 后端如能提供 `Platform(SessionMetaUpdate key="provider_attempt")` 或类似事件，包含 `phase: "accepted" | "waiting_first_token" | "streaming" | "retry_scheduled" | "retrying"`，前端显示会更稳定，避免把 hook/context/tool 事件误判为“已出首字”。

### If Backend Discards Last Failed Turn

当前前端有一个重要假设：事件流是 append-only，`reduceStreamState` 只跳过旧 seq 并追加新事件，不支持“某个 seq 之后的事件被后端撤销”。如果后端物理删除或回滚最后失败轮次，但前端当前 session 已经看过那些事件，前端不会自动从 `rawEvents` 删除它们。

因此，后端丢弃最后失败轮次时，前端需要以下任一同步语义：

1. **最小推荐：发送结构化 rewind / projection reset 事件**
   - 后端持久化一个新的 `Platform(SessionMetaUpdate key="session_rewound" | "turn_discarded")`。
   - payload 包含 `discarded_turn_id`、`rewind_after_seq`、`new_last_event_seq`、`reason`、`replacement_attempt_id?`。
   - 前端 reducer 看到该事件后，把 `rawEvents` 过滤到 `event_seq <= rewind_after_seq`，重新从剩余 rawEvents reduce 出 `entries/tokenUsage/lastAppliedSeq`，然后保留该 rewind 事件本身作为 neutral/system note 或仅作为 projection refresh 触发。
   - `streamTransport.sinceId` 也需要同步到 `new_last_event_seq` 或重建 transport；否则 transport 仍携带旧 sinceId，可能错过回滚后的新事件。

2. **更简单但重一点：让前端强制 full rehydrate**
   - 后端发 `session_rebuilt` / `projection_invalidated`，payload 给 `snapshot_seq/new_last_event_seq`。
   - 前端在 `useSessionStream` 暴露 `resetFromHistory()` 或 bump `connectKey` 并清空 state，从 `after_seq=0` 重新 `fetchSessionEvents`。
   - 适合预研阶段先落地，代码风险小；代价是丢失本地已折叠状态、scroll 稳定性差一些。

3. **不推荐只改后端历史 API**
   - 如果只让 `fetchSessionEvents` 后续返回新历史，但不通知已打开页面，当前内存 rawEvents 会继续展示已丢弃 turn，直到用户刷新页面。

从现有代码看，`reduceStreamState` 是纯增量 append reducer；若要支持回滚，建议新增一个显式 reducer action，而不是把 rewind 塞进普通 `incomingEvents` 里隐式处理。原因是普通 event_seq 单调递增和 history rewrite 是不同语义，混在同一循环中会让 `lastAppliedSeq`、transport `sinceId` 与 rawEvents 裁剪顺序变复杂。

### Frontend Minimal Change Plan

1. 在 `SessionChatViewModel.ts` 增加 retry/turn activity 派生函数：
   - `extractProviderRetryEvent(event)`：识别 `error.willRetry` 和后续结构化 retry platform event。
   - `computeActiveTurnActivity(rawEvents)`：输出 `{ turnId, phase, startedAtMs?, elapsedMs?, retry?, hasFirstToken, terminal? }`。
   - 先使用现有 `turn_started`、`agent_message_delta`、`reasoning_*`、`item_started/completed`、`error.willRetry`、`turn_terminal` 推导；后端字段到位后切换为结构化优先。
2. 修改 error 渲染策略：
   - `SessionEntry.tsx` 中 `event.type === "error" && event.payload.willRetry` 渲染中性/信息 strip，例如“正在重连模型服务”；不要使用 destructive。
   - `useSessionFeed.classifyEntry` 中 willRetry error 改成 neutral 或 soft，不作为 hard error boundary；最终 error 仍 hard boundary。
3. 修改 turn 分段：
   - `segmentByTurn` 读取 `turn_started` 记录 started time。
   - `segmentByTurn` 读取 `turn_terminal` 记录 failed/interrupted/completed 状态和 message/duration。
   - `TurnSection` 对 active segment 显示小型 status strip：`思考中` / `等待首字` / `重连中` / `已处理 12s`。
4. 修改状态条：
   - `SessionChatStatusBar` 接收 `activity` 或 `providerRetryStatus`。
   - action running chip 文案从单纯 `isConnected ? "接收中" : "执行中"` 改为优先显示 retry/first-token/thinking phase。
5. 支持后端回滚：
   - 短期：收到 `session_rebuilt/session_rewound` 后清空并 full rehydrate。
   - 中期：在 reducer 中实现 `rewind_after_seq` 裁剪 + replay，保持 scroll 与已有 UI 状态更稳定。

### Backend Event Fields Needed

为避免前端解析英文 message 或猜测状态，建议后端提供以下结构化字段：

#### Provider Retry / Reconnect Event

可作为 `BackboneEvent::Error(ErrorNotification { will_retry: true })` 的扩展 companion，也可作为 `Platform(SessionMetaUpdate key="provider_retry")`：

```json
{
  "phase": "retry_scheduled",
  "turn_id": "turn-1",
  "attempt": 2,
  "max_attempts": 5,
  "retry_after_ms": 8000,
  "elapsed_ms": 12345,
  "provider_id": "openai",
  "model_id": "gpt-...",
  "reason_code": "stream_disconnected",
  "message": "正在重连模型服务",
  "will_retry": true
}
```

字段语义：

- `phase`: `waiting_first_token | retry_scheduled | reconnecting | retrying | resumed | exhausted | fatal`
- `turn_id`: 必须和 NDJSON envelope `turn_id` / Backbone trace 一致。
- `attempt/max_attempts`: 用于 `2/5`。
- `retry_after_ms`: 用于倒计时或“将在 Xs 后重试”。
- `elapsed_ms`: 后端口径的运行耗时；前端只负责显示。
- `provider_id/model_id`: 可选 debug chip，不必主文案展示。
- `reason_code/message`: code 给机器，message 给用户。

#### Turn Terminal Event

当前 `turn_terminal` 只有 `terminal_type` 和 `message`。建议扩展为：

```json
{
  "terminal_type": "turn_failed",
  "turn_id": "turn-1",
  "message": "provider stream disconnected",
  "started_at_ms": 1710000000000,
  "completed_at_ms": 1710000012345,
  "duration_ms": 12345,
  "stable": false,
  "discarded": true,
  "retryable": true,
  "attempt_id": "attempt-2"
}
```

字段语义：

- `duration_ms` 解决前端在没有 Codex `turn_completed` 时无法显示耗时的问题。
- `stable/discarded` 表达是否应保留本 turn 的 feed 内容。
- `retryable` 区分最终失败和可恢复失败。
- `attempt_id` 用于多次 retry 时避免旧尝试与新尝试的条目混淆。

#### Rewind / Rebuild Event

如果后端丢弃最后失败轮次，必须发一个前端可执行的同步事件：

```json
{
  "discarded_turn_id": "turn-1",
  "rewind_after_seq": 120,
  "new_last_event_seq": 121,
  "reason": "provider_retry",
  "replacement_turn_id": "turn-2",
  "replacement_attempt_id": "attempt-3"
}
```

前端依赖：

- `rewind_after_seq`：裁剪 rawEvents 的稳定边界。
- `new_last_event_seq`：重置 stream `sinceId` 或 full rehydrate 后的游标。
- `discarded_turn_id`：用于 UI debug 和测试。
- `replacement_turn_id/attempt_id`：retry 后新 turn/attempt 的关联展示。

## Related Specs

- `.trellis/spec/frontend/architecture.md`: 前端不创建第二套业务事实源；运行态以后端 projection/event stream 为准。
- `.trellis/spec/frontend/state-management.md`: RuntimeSession 终态统一通过 `Platform(SessionMetaUpdate key="turn_terminal")`；AgentRun workspace 收到终态后刷新权威 snapshot。
- `.trellis/spec/frontend/hook-guidelines.md`: `useSessionStream` / `useSessionFeed` 的 NDJSON、rawEvents、平台事件可见性和聚合边界契约。
- `.trellis/spec/frontend/design-language.md`: 状态 UI 应使用 `warning/info/success/destructive` 等语义 token，避免字面色。
- `.trellis/spec/backend/session/streaming-protocol.md`: Session NDJSON envelope、续传游标与前端消费规则。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: NDJSON stream envelope 和 BackboneEvent 属于 cross-layer contract，应由 Rust contract 生成到前端。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本研究按用户显式提供的 `.trellis/tasks/06-23-agent-provider-retry-reconnect` 路径写入，未修改 task 状态。
- 没有发现前端业务代码消费 `ErrorNotification.willRetry`；当前只有生成类型带字段。
- 没有发现前端支持 event rewind / truncate / replacement 的 reducer 语义；当前是 append-only rawEvents。
- 当前 `turn_terminal` 前端只用于 lifecycle side effect 和 projection refresh，不用于 feed 分段、系统卡渲染或 duration 展示。
- 当前后端 `turn_terminal` payload 不含 attempt、delay、duration、stable/discarded、rewind cursor 等 retry/rebuild 字段；前端无法可靠展示 Codex 风格 retry 或同步删除已见失败轮次。
- 未运行前端测试；本次按 research agent 约束只做调研并写 research 文件。
