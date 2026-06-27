# Research: Codex turn/run status and retry UI protocol

- Query: references/codex 中是否有每个 turn/run 的 elapsed time、首包前状态、已连接/等待模型吐字/思考中、reconnecting/will_retry 等 UI/协议表达；重点搜索 codex-rs app-server、core、tui、protocol。
- Scope: mixed
- Date: 2026-06-23

## Findings

### Files Found

- `references/codex/codex-rs/protocol/src/protocol.rs` - core -> UI event vocabulary, including turn start/complete timing and stream retry intermediate event.
- `references/codex/codex-rs/protocol/src/error.rs` - core error classification, including retryable stream/transport errors and non-retryable fatal classes.
- `references/codex/codex-rs/core/src/session/turn.rs` - sampling loop catches retryable stream errors and delegates retry handling.
- `references/codex/codex-rs/core/src/responses_retry.rs` - shared Responses stream retry/backoff handling and `Reconnecting... n/max` notification emission.
- `references/codex/codex-rs/core/src/session/mod.rs` - `notify_stream_error` converts retryable stream failures into core `EventMsg::StreamError`.
- `references/codex/codex-rs/core/src/compact.rs` - compaction path emits the same `Reconnecting... n/max` style stream error during retries.
- `references/codex/codex-rs/codex-client/src/retry.rs` - request-level retry policy for HTTP 429, 5xx, timeout/network transport errors.
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs` - app-server public `Thread` and `Turn` DTOs with status and elapsed fields.
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs` - app-server public `TurnStatus` enum and turn lifecycle notifications.
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/notification.rs` - app-server public `ErrorNotification { will_retry }` contract.
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs` - app-server delta notifications that mark first visible model output/reasoning but do not define a separate "first token waiting" state.
- `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs` - app-server maps core events into public turn/error notifications and keeps stream retry as non-terminal.
- `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs` - `turn/start` response creates an `InProgress` turn before core start timing is known.
- `references/codex/codex-rs/tui/src/chatwidget/protocol.rs` - TUI consumes app-server notifications, routes `will_retry` errors to retry status instead of terminal error display.
- `references/codex/codex-rs/tui/src/chatwidget/turn_runtime.rs` - TUI starts local running state and "Working" status on turn start.
- `references/codex/codex-rs/tui/src/chatwidget/streaming.rs` - TUI displays stream retry/reconnect as status header/details and maps terminal-title state to `Thinking`.
- `references/codex/codex-rs/tui/src/chatwidget/status_state.rs` - TUI compact status buckets: `Working`, `WaitingForBackgroundTerminal`, `Thinking`.
- `references/codex/codex-rs/tui/src/bottom_pane/mod.rs` - bottom pane status indicator defaults to "Working" and renders elapsed seconds in snapshots.

### Code Patterns

#### Turn elapsed time

- Core protocol has terminal turn timing fields on `TurnCompleteEvent`: `completed_at`, `duration_ms`, and `time_to_first_token_ms` at `references/codex/codex-rs/protocol/src/protocol.rs:1857`, `:1863`, `:1867`, `:1871`.
- Core protocol also has `TurnStartedEvent { turn_id, trace_id, started_at }` beginning at `references/codex/codex-rs/protocol/src/protocol.rs:1875`.
- App-server public `Turn` DTO has `started_at`, `completed_at`, and `duration_ms` at `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs:163`, `:168`, `:171`. This is the directly reusable public shape for elapsed display.
- App-server public `TurnStatus` is only `Completed | Interrupted | Failed | InProgress` at `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:28`.
- `turn/start` initially returns a `Turn { status: InProgress, started_at: None, completed_at: None, duration_ms: None }` before core emits richer timing, at `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:480`, `:485`, `:486`, `:487`, `:488`.
- App-server maps completed turns to terminal notification and tests assert `TurnStatus::Completed`, `started_at`, `completed_at`, `duration_ms` at `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:3331`, `:3333`, `:3337`, `:3338`, `:3339`; failed/interrupted are similarly mapped at `:3384`/`:3386`/`:3389` and `:3434`/`:3436`/`:3446`.
- TUI also has local active-turn timing for goal usage: `TurnLifecycleState` stores `goal_status_active_turn_started_at`, starts it on `start(now)`, and clears it on finish at `references/codex/codex-rs/tui/src/chatwidget/turn_lifecycle.rs:12`, `:15`, `:29`, `:31`, `:35`, `:37`.

Interpretation: Codex has direct reference material for turn elapsed time at both protocol and app-server DTO layers. It does not expose a continuously updated per-turn elapsed event; the TUI can render live elapsed locally from its running state, while app-server terminal DTOs persist final duration.

#### Stream connected / waiting for first token

- Core has `time_to_first_token_ms` only on `TurnCompleteEvent`, not an intermediate "waiting for first token" event, at `references/codex/codex-rs/protocol/src/protocol.rs:1868`-`:1871`.
- App-server public item delta notifications are the first observable model-output boundary: `AgentMessageDeltaNotification` at `references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:1162`, `ReasoningSummaryTextDeltaNotification` at `:1184`, and `ReasoningTextDeltaNotification` at `:1207`.
- Core event vocabulary includes `EventMsg::StreamError` for stream failures, but no dedicated `StreamConnected`, `WaitingForFirstToken`, or `FirstToken` lifecycle event in the searched protocol enum around `references/codex/codex-rs/protocol/src/protocol.rs:1160` and `:1287`-`:1289`.
- TUI app-server event stream disconnect is separate from provider/model stream state: `AppServerEvent::Disconnected` logs "app-server event stream disconnected" and requests fatal exit at `references/codex/codex-rs/tui/src/app/app_server_events.rs:53`-`:56`.

Interpretation: Codex does not provide a clean public state for "stream connected" or "waiting for first token" for model turns. The closest reference is to derive "waiting for first visible token/delta" from `TurnStarted/InProgress` until the first agent/reasoning delta, and to record `time_to_first_token_ms` only at completion.

#### Thinking / Working / running status

- TUI sets turn running on `TurnStarted` notification and calls `on_task_started()` at `references/codex/codex-rs/tui/src/chatwidget/protocol.rs:57`-`:61`.
- `on_task_started()` calls `turn_lifecycle.start(Instant::now())`, updates task running state, sets terminal-title status kind to `Working`, and sets status header to `"Working"` at `references/codex/codex-rs/tui/src/chatwidget/turn_runtime.rs:49`-`:70`.
- Status indicator default text is `"Working"` in `StatusIndicatorState::working()` at `references/codex/codex-rs/tui/src/chatwidget/status_state.rs:13`-`:17`.
- Compact title status buckets are intentionally small: `TerminalTitleStatusKind::{Working, WaitingForBackgroundTerminal, Thinking}` at `references/codex/codex-rs/tui/src/chatwidget/status_state.rs:26`-`:36`.
- Status-line setup describes `run-state` as "Compact session run-state text (Ready, Working, Thinking)" at `references/codex/codex-rs/tui/src/bottom_pane/status_line_setup.rs:83`-`:84` and `:156`.

Interpretation: "Thinking" exists in Codex as a TUI compact status bucket, not as a core/app-server turn state. "Working" is the normal running indicator after turn start. "Ready" is a status-line display concept, not a protocol value. For AgentDash, the directly reusable idea is keeping a compact UI run-state separate from the durable turn status enum.

#### Reconnecting / will_retry

- Core error enum documents `CodexErr::Stream(String, Option<Duration>)` as SSE stream disconnected after handshake but before `response.completed`; session loop treats it as transient and automatically retries the turn at `references/codex/codex-rs/protocol/src/error.rs:72`-`:79`.
- `CodexErr::is_retryable()` treats `Stream`, `Timeout`, `RequestTimeout`, unexpected status, response stream failure, connection failure, internal server error, internal agent death, and IO as retryable, while excluding usage/quota/context-window/invalid-request/fatal categories at `references/codex/codex-rs/protocol/src/error.rs:173`-`:204`.
- Request-level retry policy covers HTTP 429, 5xx, timeout/network transport, exponential backoff and retry limit at `references/codex/codex-rs/codex-client/src/retry.rs:9`-`:19`, `:23`-`:34`, `:38`-`:47`, `:49`-`:72`.
- Sampling loop checks `err.is_retryable()` and delegates to `handle_retryable_response_stream_error()` at `references/codex/codex-rs/core/src/session/turn.rs:999`-`:1011`.
- `handle_retryable_response_stream_error()` increments retry count, uses server-requested delay from `CodexErr::Stream(_, requested_delay)` when present, otherwise backoff, and emits `Reconnecting... {retry_count}/{max_retries}` via `notify_stream_error()` at `references/codex/codex-rs/core/src/responses_retry.rs:48`-`:75`.
- Release builds suppress the first websocket retry notification to reduce noise, unless debug or websocket is disabled, at `references/codex/codex-rs/core/src/responses_retry.rs:59`-`:64`.
- `notify_stream_error()` builds `EventMsg::StreamError(StreamErrorEvent { message, codex_error_info, additional_details })` at `references/codex/codex-rs/core/src/session/mod.rs:3097`-`:3112`.
- App-server `ErrorNotification` includes `will_retry`; comments say `will_retry=true` means transient, app-server will automatically retry, and it will not interrupt a turn at `references/codex/codex-rs/app-server-protocol/src/protocol/v2/notification.rs:41`-`:48`.
- App-server maps core `EventMsg::StreamError` to `ServerNotification::Error(ErrorNotification { will_retry: true, ... })` at `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:928`-`:939`. Core ordinary `EventMsg::Error` maps to `will_retry: false` at `:913`-`:922`.
- TUI treats `ServerNotification::Error { will_retry: true }` specially: it identifies retry errors at `references/codex/codex-rs/tui/src/chatwidget/protocol.rs:18`-`:24`, calls `on_stream_error()` instead of terminal error display at `:121`-`:128`, and restores the previous status header around retry notifications at `:25`-`:26`.
- `on_stream_error()` remembers the previous retry status header, ensures the status indicator, sets terminal-title status kind to `Thinking`, and displays the retry message/details at `references/codex/codex-rs/tui/src/chatwidget/streaming.rs:233`-`:241`.

Interpretation: This is the strongest directly reusable pattern. Codex expresses reconnect/retry as a non-terminal error notification with `will_retry=true` and a user-facing message like `Reconnecting... 2/5`; it does not mutate the turn to failed while retrying.

#### Waiting

- TUI has a specific "Waiting for background terminal" UI state for polling background command output, setting `TerminalTitleStatusKind::WaitingForBackgroundTerminal` and status header/details at `references/codex/codex-rs/tui/src/chatwidget/command_lifecycle.rs:89`-`:103`.
- Protocol has collab waiting events (`CollabWaitingBegin` / `CollabWaitingEnd`) around `references/codex/codex-rs/protocol/src/protocol.rs:1339`-`:1342`, but these are collaboration waits, not model/provider first-token waits.

Interpretation: Codex's "Waiting" examples are domain-specific UI states, not a provider stream state. They can inspire UI vocabulary, but not a ready-made protocol contract for waiting on a model.

### Directly Referenceable For AgentDash

- Use an explicit running/terminal turn status split like Codex `TurnStatus::{InProgress, Completed, Interrupted, Failed}` (`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:28`-`:33`).
- Use public turn timing fields `started_at`, `completed_at`, `duration_ms` and optionally final `time_to_first_token_ms` (`references/codex/codex-rs/protocol/src/protocol.rs:1857`-`:1871`; `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs:163`-`:171`).
- Use `ErrorNotification { will_retry: true }` as a non-terminal visible retry/reconnect notification (`references/codex/codex-rs/app-server-protocol/src/protocol/v2/notification.rs:41`-`:48`; `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:928`-`:939`).
- Use `Reconnecting... {attempt}/{max}` message semantics and optional provider/requested delay handling (`references/codex/codex-rs/core/src/responses_retry.rs:48`-`:75`).
- Keep compact UI state (`Working`, `Thinking`, waiting variants) separate from durable turn state (`references/codex/codex-rs/tui/src/chatwidget/status_state.rs:26`-`:36`).
- Treat retryable stream errors as intermediate state that does not become a terminal turn summary until retries are exhausted (`references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:928`-`:939`; ordinary terminal error path at `:913`-`:922`).

### Not Present As Ready-Made Codex State

- No public `stream_connected` / `provider_connected` event was found in app-server/core/protocol for model turn streams.
- No public `waiting_for_first_token` event or status enum was found. Codex records `time_to_first_token_ms` at completion and clients can infer waiting from `TurnStarted/InProgress` before the first delta.
- No durable protocol-level `thinking` state was found. `Thinking` is a TUI compact status bucket and is used when displaying retry/reconnect status, not a core turn state.
- No app-server protocol field carrying retry attempt count, max attempts, delay, or provider name was found on `ErrorNotification`; those details are currently encoded in the human-readable message/details.
- No continuous elapsed-time event was found. Final elapsed is in turn DTOs; live elapsed is a UI-local timer.

### External References

- No network documentation lookup was needed. This research uses the local `references/codex` checkout only.
- The local checkout includes generated TypeScript schema under `references/codex/codex-rs/app-server-protocol/schema/typescript/v2/`, confirming the app-server protocol types are intended as public DTOs; this file cites the Rust sources rather than generated mirrors.

### Related Specs

- `.trellis/spec/backend/session/streaming-protocol.md` - AgentDash NDJSON stream uses `connected/event/heartbeat` envelopes and requires frontends to consume backend-projected `session_update_type`; this is separate from provider/model stream state.
- `.trellis/spec/backend/session/runtime-execution-state.md` - AgentDash runtime state distinguishes active turn lifecycle and terminal cleanup; relevant when deciding whether retry/reconnect is an active turn state or an event notification.
- `.trellis/spec/backend/runtime-gateway.md` - Session turn control follows Codex app-server protocol shape for `turn/start` and `turn/steer`; Codex turn DTOs are an appropriate reference for browser-facing shape.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - browser-facing DTO/event shapes should be Rust contract generated; retry/reconnect status should be structured rather than frontend-invented text parsing.
- `.trellis/tasks/06-23-agent-provider-retry-reconnect/prd.md` - task PRD already identifies provider retry/reconnect, half-stream risks, and failure-turn recovery as planning requirements.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task, so this research used the task directory explicitly supplied in the prompt: `.trellis/tasks/06-23-agent-provider-retry-reconnect`.
- The Codex reference has both `protocol` and `app-server-protocol`; for AgentDash UI/API design, `app-server-protocol` is the closer public DTO reference, while `protocol` shows core event semantics.
- Search focused on `references/codex/codex-rs/app-server`, `core`, `tui`, `protocol`, plus `app-server-protocol` and `codex-client` when the initial hits showed those were necessary for public DTOs and retry policy.
- `will_retry` in Codex is binary. If AgentDash needs structured attempt/max/delay/provider/source, Codex does not provide a ready-made public field; add a structured event/DTO instead of parsing `Reconnecting... n/max`.
- Codex's "Thinking" and "Waiting" labels are UI concepts with multiple causes. They should not be copied as backend truth unless AgentDash defines explicit semantics.
