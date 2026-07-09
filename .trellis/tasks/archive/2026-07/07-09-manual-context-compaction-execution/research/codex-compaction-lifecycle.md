# Research: Codex compaction lifecycle

- Query: Read-only research of `references/codex` for context compaction / compact / summarization / conversation truncation / agent lifecycle, then map the useful state-machine boundaries to AgentDash manual compact-only repair design.
- Scope: mixed, covering local AgentDash task/spec documents and local `references/codex` source. No network docs were consulted.
- Date: 2026-07-09

## Findings

### Read order and task context

- `python ./.trellis/scripts/task.py current --source` reported no active task. The user explicitly provided `.trellis/tasks/07-09-manual-context-compaction-execution`, so this research uses that task directory.
- Read task context JSONL entries first:
  - `.trellis/tasks/07-09-manual-context-compaction-execution/implement.jsonl`: task context includes PRD, design, implementation checklist, and backend session specs.
  - `.trellis/tasks/07-09-manual-context-compaction-execution/check.jsonl`: check context repeats the implementation scope and spec references.
- Then read task docs in the requested order:
  - `.trellis/tasks/07-09-manual-context-compaction-execution/prd.md`: manual compact-only must use durable model context from `ContextProjector`; invalid restore or `MessageRef` boundaries are failures, not `no_eligible_messages`; request, receipt, and lifecycle must agree (`prd.md:5`, `prd.md:15`, `prd.md:18`, `prd.md:31`).
  - `.trellis/tasks/07-09-manual-context-compaction-execution/design.md`: current failure is compact-only observing `Noop` after only system delivery; `should_execute_compaction -> bool` folds structural invalidity into false; target state machine separates completed, noop, and failed (`design.md:5`, `design.md:9`, `design.md:11`, `design.md:29`, `design.md:98`).
  - `.trellis/tasks/07-09-manual-context-compaction-execution/implement.md`: implementation plan asks for structured compaction eligibility, failed preflight finalization, compact-only restore path, command receipt turn id retention, and projection commit tests (`implement.md:9`, `implement.md:16`, `implement.md:20`, `implement.md:26`, `implement.md:31`).

### Related AgentDash specs

- `.trellis/spec/backend/session/context-compaction-projection.md`: successful compact writes `session_compactions`, `session_projection_segments`, and `session_projection_heads`; lifecycle is `ContextCompactionStarted -> ItemStarted`, `ContextCompacted -> SessionMetaUpdate -> ItemCompleted`, and `ContextCompactionFailed -> SessionMetaUpdate -> Error` (`context-compaction-projection.md:15`, `context-compaction-projection.md:16`, `context-compaction-projection.md:17`, `context-compaction-projection.md:30`, `context-compaction-projection.md:33`, `context-compaction-projection.md:35`, `context-compaction-projection.md:37`, `context-compaction-projection.md:39`).
- The same spec defines `context_compaction_noop` as a diagnostic-only event that does not create a committed compaction/projection checkpoint, because it has no summary, source range, or replacement projection (`context-compaction-projection.md:44`).
- The structural boundary fields are explicit: `summary`, `compacted_until_ref`, and `first_kept_ref` (`context-compaction-projection.md:50`, `context-compaction-projection.md:51`, `context-compaction-projection.md:52`).
- `ContextProjector` is the durable model-input source, not the UI timeline array (`context-compaction-projection.md:59`, `context-compaction-projection.md:61`, `context-compaction-projection.md:64`, `context-compaction-projection.md:71`).
- `.trellis/spec/backend/session/architecture.md` states that runtime map, active turn, and connector live session are separate concerns (`architecture.md:34`), and records the local design decision: context compaction uses a Codex-aligned lifecycle plus an AgentDash-owned projection store because compaction is both an observable lifecycle and a model-context checkpoint (`architecture.md:61`).

### Files found in `references/codex`

- `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs`: app-server request/notification names, including `thread/compact/start`, `item/started`, `item/completed`, and deprecated `thread/compacted`.
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs`: request/response shape for compact start and deprecated compacted notification.
- `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs`: app-server entry that turns a compact request into core `Op::Compact`.
- `references/codex/codex-rs/core/src/session/handlers.rs`: core op handler that spawns a compact task.
- `references/codex/codex-rs/core/src/tasks/compact.rs`: `CompactTask` selection between token-budget, remote v2, remote, and local compaction.
- `references/codex/codex-rs/core/src/compact.rs`: local model summarization compaction implementation.
- `references/codex/codex-rs/core/src/compact_remote.rs`: remote server summarization compaction implementation and installed checkpoint filtering.
- `references/codex/codex-rs/core/src/compact_remote_v2.rs`: Responses API v2 compaction implementation and stricter output validation.
- `references/codex/codex-rs/core/src/compact_token_budget.rs`: no-summary token-budget path that still uses the compaction lifecycle.
- `references/codex/codex-rs/core/src/tools/handlers/new_context_window.rs`: user/tool-facing request to start a new context window without summarization.
- `references/codex/codex-rs/core/src/session/mod.rs`: session history replacement, rollout persistence, resume reconstruction entry points, and context window creation.
- `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs`: resume/replay algorithm that treats the newest compaction replacement history as a checkpoint.
- `references/codex/codex-rs/core/src/state/session.rs`: in-memory session state history replacement and context-window counters.
- `references/codex/codex-rs/protocol/src/items.rs`: protocol `TurnItem::ContextCompaction` and `ContextCompactionItem`.
- `references/codex/codex-rs/protocol/src/protocol.rs`: core `EventMsg`, `ItemStartedEvent`, `ItemCompletedEvent`, `ErrorEvent`, and terminal error classification.
- `references/codex/codex-rs/app-server-protocol/src/protocol/thread_history.rs`: app-server history reducer that preserves compaction turns during replay.
- `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs`: v2 app-server handling of error, turn completed/interrupted, and deprecated compacted notifications.

### 1. Codex treats compaction as a managed turn and item lifecycle

The manual compaction API is a thread command, not an inline prompt mutation:

- `ThreadCompactStartParams` only carries `thread_id`, and `ThreadCompactStartResponse` is empty (`app-server-protocol/src/protocol/v2/thread.rs:939`, `app-server-protocol/src/protocol/v2/thread.rs:946`).
- The request name is `thread/compact/start` (`app-server-protocol/src/protocol/common.rs:577`).
- The app-server loads the thread and submits `Op::Compact` to core (`app-server/src/request_processors/thread_processor.rs:1771`, `thread_processor.rs:1778`, `thread_processor.rs:1779`, `thread_processor.rs:1782`).
- Core handles `Op::Compact` by creating a default turn context and spawning `CompactTask` (`core/src/session/handlers.rs:445`, `handlers.rs:449`).

`CompactTask` is a first-class session task:

- `core/src/tasks/compact.rs` implements `SessionTask` for `CompactTask`; `kind()` returns `TaskKind::Compact` and `span_name()` returns `session_task.compact`.
- The task chooses the implementation path by feature flags: token-budget, remote v2, remote, or local.
- `SessionTask` is documented as a task that runs until completion or cancellation and can complete with `Some(ResponseInputItem)`, `None`, or `CodexErr::TurnAborted` (`core/src/tasks/mod.rs:206`, `tasks/mod.rs:253`).
- `Session::spawn_task` aborts current tasks, then starts the compact task (`core/src/tasks/mod.rs:313`, `tasks/mod.rs:323`).
- `Session::start_task` emits turn-start lifecycle before running the task and later calls `on_task_finished` to emit `TurnComplete` or `TurnAborted` (`core/src/tasks/mod.rs:325`, `tasks/mod.rs:364`, `tasks/mod.rs:563`, `tasks/mod.rs:809`).

Manual compaction creates an independent turn:

- Local manual compaction sends `EventMsg::TurnStarted` before calling the inner compaction flow (`core/src/compact.rs:123`).
- Remote manual compaction does the same (`core/src/compact_remote.rs:68`, `compact_remote.rs:83`).
- Remote v2 manual compaction does the same (`core/src/compact_remote_v2.rs:77`, `compact_remote_v2.rs:92`).
- Token-budget manual compaction also sends a standalone `TurnStarted` and obtains its own `StepContext` (`core/src/compact_token_budget.rs:25`).

Compaction also has a separate item lifecycle inside the turn:

- Protocol exposes `TurnItem::ContextCompaction(ContextCompactionItem)` (`protocol/src/items.rs:52`, `items.rs:68`).
- `ContextCompactionItem` is just an item id, generated with a UUID; it can emit legacy `ContextCompactedEvent` for old consumers (`protocol/src/items.rs:380`, `items.rs:385`, `items.rs:391`).
- `ItemStartedEvent` and `ItemCompletedEvent` include `thread_id`, `turn_id`, and the `TurnItem`, so the compaction item can be correlated to the maintenance turn (`protocol/src/protocol.rs:1790`, `protocol.rs:1817`).
- Local compaction creates `TurnItem::ContextCompaction`, emits item started, replaces history, recomputes token usage, then emits item completed (`core/src/compact.rs:220`, `compact.rs:360`, `compact.rs:370`).
- Remote and remote v2 create a `ContextCompactionItem` before the model/server request; remote v2 also uses that item id as the trace compaction id (`core/src/compact_remote.rs:175`, `compact_remote.rs:184`; `core/src/compact_remote_v2.rs:186`, `compact_remote_v2.rs:195`).

Terminal and error boundaries are mostly turn-level:

- `compaction_status_from_result` maps `Ok` to completed, `Interrupted`/`TurnAborted` to interrupted, and other errors to failed for telemetry (`core/src/compact.rs:466`).
- `ErrorEvent::affects_turn_status()` defaults to true unless the error class says otherwise (`protocol/src/protocol.rs:1931`, `protocol.rs:1939`, `protocol.rs:1763`).
- The app-server history reducer marks the current turn failed when it receives an error that affects turn status (`app-server-protocol/src/protocol/thread_history.rs:1128`, `thread_history.rs:1134`).
- App-server turn completion checks `turn_summary.last_error`; if present, the v2 turn-completed notification status is failed, otherwise completed (`app-server/src/bespoke_event_handling.rs:1526`, `bespoke_event_handling.rs:1552`).
- Interrupted/aborted tasks emit `TurnAborted`/interrupted terminal flow rather than a failed item completion (`core/src/tasks/mod.rs:492`, `tasks/mod.rs:520`, `tasks/mod.rs:829`, `tasks/mod.rs:908`).
- There is no distinct `ContextCompactionItemFailed` protocol item in the Codex source. A compaction item may be started and never completed if the task fails; the terminal truth is the turn error or abort.

The app-server deliberately keeps compaction visible in replay:

- `ThreadHistoryBuilder::handle_context_compacted` pushes a renderable `ThreadItem::ContextCompaction` for the deprecated legacy event (`app-server-protocol/src/protocol/thread_history.rs:1101`, `thread_history.rs:1103`).
- `ThreadHistoryBuilder::handle_compacted` marks `saw_compaction = true` for persisted `RolloutItem::Compacted` (`thread_history.rs:1237`, `thread_history.rs:1242`, `thread_history.rs:1243`).
- `finish_current_turn` drops empty implicit turns only if they were not explicit and did not see compaction (`thread_history.rs:1272`, `thread_history.rs:1274`).
- User messages do not close an implicit empty turn if that turn only saw compaction, preserving compaction-only legacy turns (`thread_history.rs:458`, `thread_history.rs:464`).

### 2. Eligibility, noop, failure, message refs, and history boundaries

Codex does not implement an AgentDash-style `eligible/noop/failure` preflight classifier:

- A targeted search for `no_eligible`, `noop`, `Noop`, `eligible`, `should_execute`, `can_compact`, `not enough`, and `insufficient` in the compact implementations, compact task, compact request processor, and compact protocol types returned no matches.
- Manual compaction is attempted after the command is accepted. Failure comes from the task implementation, provider/server stream, hook interruption, context-window fit failure, or output validation, not from a separate eligibility state machine.

Local compaction handles history as a temporary summarization prompt plus an installed replacement history:

- `InitialContextInjection` has two cases: `DoNotInject` for pre-turn/manual compaction and `BeforeLastUserMessage` for mid-turn compaction (`core/src/compact.rs:57`, `compact.rs:65`, `compact.rs:66`, `compact.rs:67`).
- Local compaction clones session history, appends a synthetic compact prompt to temporary input, streams the model response, and drains stream items until completed (`core/src/compact.rs:220`, `compact.rs:262`, `compact.rs:661`).
- If the provider reports `ContextWindowExceeded`, local compaction removes the first input item and retries; if only one item remains, it emits an error/failure (`core/src/compact.rs:284`).
- The installed summary is a user message prefixed by `SUMMARY_PREFIX`; local compaction gathers live history user messages after summarization and builds a new replacement history (`core/src/compact.rs:325`, `compact.rs:326`).
- `build_compacted_history_with_limit` retains newest user messages under `COMPACT_USER_MESSAGE_MAX_TOKENS = 20_000` and can truncate the oldest retained user message to fit (`core/src/compact.rs:52`, `compact.rs:53`, `compact.rs:590`, `compact.rs:598`).
- `collect_user_messages` skips prior summary-prefixed user messages, so repeated local compactions do not keep re-summarizing the old summary as a user message (`core/src/compact.rs:499`, `compact.rs:525`).

Remote compaction filters and validates replacement history instead of using `MessageRef` coordinates:

- Remote compaction trims old tool/function/custom output payloads only as a fit-to-context preprocessing step (`core/src/compact_remote.rs:377`; `core/src/compact_remote_v2.rs:209`).
- `process_compacted_history` inserts initial context where appropriate, filters remote output with `should_keep_compacted_history_item`, and returns the installed history plus world-state baseline (`core/src/compact_remote.rs:313`, `compact_remote.rs:325`).
- `should_keep_compacted_history_item` keeps real user/hook prompts, assistant messages, compaction/context compaction items, and drops stale developer/context/tool payloads (`core/src/compact_remote.rs:348`).
- Remote v2 requires exactly one `ResponseItem::Compaction`; if the stream closes before completion or returns the wrong count, it fails (`core/src/compact_remote_v2.rs:397`, `compact_remote_v2.rs:432`).
- Remote v2 then builds retained history with `build_v2_compacted_history`, filters retained input through `should_keep_compacted_history_item`, and appends the compaction output item (`core/src/compact_remote_v2.rs:447`, `compact_remote_v2.rs:454`).

Codex history boundary identity is not `MessageRef` based:

- Codex commits a replacement transcript shape (`CompactedItem.replacement_history`) plus context-window metadata, not `compacted_until_ref` and `first_kept_ref`.
- Response item ids, turn ids, `TurnContext`, window numbers, and rollout order are the effective coordinates.
- This is a major difference from AgentDash, where `MessageRef` is the shared coordinate between runtime input, persisted transcript, and projection checkpoint.

No-summary/new-window paths are separate but still lifecycle-aligned:

- `new_context_window` reports: "A new context window will start without summarizing conversation history." It only requests a future new window; it is not itself a summary compact (`core/src/tools/handlers/new_context_window.rs:14`).
- Token-budget compaction explicitly skips model/server summarization and installs a fresh context window, but still models the operation as compaction so hooks and `ContextCompaction` turn items observe the same lifecycle (`core/src/compact_token_budget.rs:22`, `compact_token_budget.rs:23`, `compact_token_budget.rs:24`).
- Token-budget inner flow emits a `ContextCompaction` item, calls `sess.start_new_context_window`, and completes the item (`core/src/compact_token_budget.rs:76`, `compact_token_budget.rs:79`).

### 3. Codex avoids live/resume state split with a persisted replacement checkpoint

The key boundary is `replace_compacted_history`:

- `Session::replace_compacted_history` assigns response item ids where needed, overwrites the `CompactedItem` with `replacement_history: Some(items.clone())`, updates in-memory `SessionState.history`, persists `RolloutItem::Compacted(compacted_item)`, persists optional world state and turn context, and queues a `SessionStartSource::Compact` (`core/src/session/mod.rs:2978`, `session/mod.rs:3007`).
- `SessionState::replace_history` replaces the in-memory `ContextManager` and `reference_context_item`, and clears auto-compact prefill state (`core/src/state/session.rs:114`, `state/session.rs:123`).
- `Session::record_conversation_items` is the normal append path; compaction does not merely append an event, it replaces the model-visible history and persists a checkpoint item (`core/src/session/mod.rs:2778`, `session/mod.rs:2978`).

Resume reconstruction consumes the same checkpoint:

- `record_initial_history` uses `apply_rollout_reconstruction` for resumed and forked sessions (`core/src/session/mod.rs:1296`, `session/mod.rs:1320`, `session/mod.rs:1365`).
- `apply_rollout_reconstruction` calls `reconstruct_history_from_rollout`, then `state.replace_history` with the reconstructed history and reference context item (`core/src/session/mod.rs:1399`, `session/mod.rs:1414`).
- `reconstruct_history_from_rollout` scans newest-to-oldest; when it finds the newest surviving `RolloutItem::Compacted` with `replacement_history`, that replacement history becomes the base and `rollout_suffix` starts after it (`core/src/session/rollout_reconstruction.rs:113`, `rollout_reconstruction.rs:156`, `rollout_reconstruction.rs:181`, `rollout_reconstruction.rs:184`, `rollout_reconstruction.rs:185`).
- After selecting the base, reconstruction replaces history with the base replacement history and replays only suffix items (`rollout_reconstruction.rs:319`, `rollout_reconstruction.rs:320`, `rollout_reconstruction.rs:325`).
- Older compactions and prior events no longer affect rebuilt history after a newer replacement checkpoint is found. This is the concrete anti-split mechanism: live state and resumed state derive from the same persisted replacement history, not parallel summaries.

Trace and UI projection also point at the installed checkpoint:

- Remote compaction records `CompactionCheckpointTracePayload` with input history and replacement history before installing the checkpoint (`core/src/compact_remote.rs:294`, `compact_remote.rs:298`).
- Remote v2 does the same (`core/src/compact_remote_v2.rs:323`, `compact_remote_v2.rs:327`).
- App-server history replay builds turns from rollout items and keeps compaction-only turns via `saw_compaction`, so a compaction checkpoint is not lost just because it has no normal user/assistant item (`app-server-protocol/src/protocol/thread_history.rs:1237`, `thread_history.rs:1274`).

### 4. Recommendations for AgentDash

Ideas worth adopting:

- Treat manual compact-only as a real maintenance turn with its own turn id, request id, lifecycle item id, and terminal status. Codex's `Op::Compact -> CompactTask -> TurnStarted -> ContextCompaction item -> TurnComplete/TurnAborted` chain is a good lifecycle shape for AgentDash, even though AgentDash should use its own domain records.
- Preserve the maintenance `turn_id` in command receipts for completed, noop, failed, and still-running outcomes. Codex's app-server request response is too thin for AgentDash's current diagnostic needs, but the internal Codex lifecycle is strongly turn-correlated.
- Make the compaction item lifecycle observable, but make the durable checkpoint the success boundary. For AgentDash that means `session_compactions` + `session_projection_segments` + `session_projection_heads` must commit before emitting the final completed item/event, matching the spec contract.
- Keep `ContextProjector` as the only source for compact-only model input. Codex avoids split by deriving resume from `replacement_history`; AgentDash should avoid split by deriving both live compact-only input and resumed model context from projection facts.
- Implement newest-checkpoint-plus-suffix reconstruction semantics. Codex's rollout reconstruction is directly relevant conceptually: once a committed compaction checkpoint exists, older transcript events should not be reinterpreted as model-visible input for that projection head.
- Preserve compaction-only turns in timeline projection. Codex's `saw_compaction` rule is a useful reminder that a maintenance turn can be real even when it has no user/assistant text.
- Separate true noop from structural failure. AgentDash has `MessageRef` boundaries and Codex does not, so AgentDash needs `CompactionEligibility::{Eligible, NoEligibleMessages, InvalidInput}` and stable failure codes such as `compaction_message_ref_len_mismatch`, `compaction_boundary_ref_missing`, and `compaction_first_kept_ref_missing`.
- Terminalize consumed manual requests on failure or abort. Codex can rely on turn-level `Error`/`TurnAborted`; AgentDash also has `runtime_session_compaction_requests`, so consumed requests must be marked failed/interrupted with reason metadata rather than staying consumed or being mislabeled noop.
- If AgentDash later supports "new context window without summary", keep it as a separate strategy under the same lifecycle surface, similar to Codex token-budget compaction.

Ideas not to copy directly:

- Do not copy Codex's lack of eligibility/noop preflight. AgentDash's `MessageRef` model makes boundary validity a structural contract, and invalid refs must be failed diagnostics, not retry/noop.
- Do not copy `replacement_history` as a full transcript array if AgentDash already has normalized projection tables. The analogous AgentDash artifact should be the projection checkpoint: compaction record, projection segments, and projection head.
- Do not treat a legacy `ContextCompacted` marker or item completion as success by itself. AgentDash success must mean projection checkpoint committed.
- Do not copy legacy reconstruction fallback that rebuilds from compacted messages when replacement history is absent. This project is pre-release; require explicit committed projection facts and fail loudly when they are missing.
- Do not copy Codex's context-window-exceeded loop as AgentDash eligibility semantics. Dropping or truncating old input can be an internal provider-fit tactic only after durable model context has been correctly materialized; it should not hide missing `MessageRef` or projection restore failures.
- Do not rely on thin request response shape. Codex's compact start response is empty; AgentDash needs command receipt to expose request id, maintenance turn id, and immediate terminal state when available.
- Do not rely on swallowing non-abort task errors after emitting an event. AgentDash has a request lifecycle record, so failed manual compaction should be explicit at the domain/request layer as well as the event layer.

## Caveats / Not Found

- `task.py current --source` did not report the active task. This research followed the explicit task path supplied by the user.
- No `MessageRef` equivalent was found in the Codex compaction implementation. Codex uses response items, rollout order, turn context, and `replacement_history` checkpoints, so the mapping to AgentDash `compacted_until_ref` and `first_kept_ref` is conceptual rather than field-for-field.
- No `eligible/noop/no_eligible_messages` state machine was found in the Codex compact sources searched. This is an important difference, not a missing line reference.
- No external web references were used. The relevant external reference for this task is the local `references/codex` source snapshot.
- Line numbers refer to the current local `references/codex` and `.trellis` files and may drift if those references are updated.
