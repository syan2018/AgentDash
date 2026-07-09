# Research: Codex Context Compaction

- Query: Research `references/codex` for context compaction, summarization, context shrink, history compaction, and resume-after-compact behavior relevant to AgentDashboard manual context compaction.
- Scope: mixed, using checked-in Codex reference source, tests, and docs only. No web lookup.
- Date: 2026-07-07

## Scope

Inspected directories:

- `references/codex/codex-rs/core/src`
- `references/codex/codex-rs/core/tests/suite`
- `references/codex/codex-rs/app-server/src`
- `references/codex/codex-rs/app-server/tests/suite/v2`
- `references/codex/codex-rs/app-server-protocol/src`
- `references/codex/codex-rs/protocol/src`
- `references/codex/codex-rs/prompts`
- `references/codex/codex-rs/tui/src`
- `references/codex/codex-rs/analytics/src`
- `references/codex/codex-rs/codex-api/src/endpoint`
- `references/codex/codex-rs/app-server/README.md`

Keywords used:

- `compact`, `compaction`, `ContextCompacted`, `ContextCompaction`
- `summarize`, `summary`, `SUMMARIZATION_PROMPT`, `SUMMARY_PREFIX`
- `auto_compact`, `model_auto_compact_token_limit`, `AutoCompactTokenLimitScope`
- `resume`, `replacement_history`, `RolloutItem::Compacted`
- `thread/compact/start`, `Op::Compact`, `TaskKind::Compact`
- `PreTurn`, `MidTurn`, `StandaloneTurn`, `UserRequested`, `ContextLimit`

Out of scope by request:

- Business code outside `references/codex`
- Non-compaction resume/fork details except where they prove resume-after-compact semantics
- General token UI, rate limit UI, unrelated "compact" wording in layout/tests/dependencies

## Files Found

- `references/codex/codex-rs/core/src/session/turn.rs` - automatic pre-turn and mid-turn compaction trigger logic.
- `references/codex/codex-rs/core/src/tasks/compact.rs` - manual `/compact` task dispatcher.
- `references/codex/codex-rs/core/src/compact.rs` - local model-summarization compaction implementation and replacement history construction.
- `references/codex/codex-rs/core/src/compact_remote.rs` - legacy remote `/responses/compact` implementation and compacted history filtering.
- `references/codex/codex-rs/core/src/compact_remote_v2.rs` - remote v2 compaction using `ResponseItem::CompactionTrigger` and compact output item.
- `references/codex/codex-rs/core/src/compact_token_budget.rs` - token-budget mode that resets to a new context window without model/server summarization.
- `references/codex/codex-rs/core/src/session/mod.rs` - compaction history replacement persistence, active-turn steering behavior, and compact task non-steerability.
- `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs` - resume/fork reconstruction from persisted compaction checkpoints.
- `references/codex/codex-rs/core/src/session/context_window.rs` - auto-compaction threshold calculation.
- `references/codex/codex-rs/core/src/state/auto_compact_window.rs` - context window id/number advancement and new-context-window request flag.
- `references/codex/codex-rs/core/src/state/session.rs` - state methods for advancing/restoring auto-compact windows.
- `references/codex/codex-rs/core/src/client.rs` - unary `/responses/compact` client payload construction.
- `references/codex/codex-rs/codex-api/src/endpoint/compact.rs` - compact endpoint client path and response parsing.
- `references/codex/codex-rs/core/src/responses_metadata.rs` - compaction metadata attached to model requests.
- `references/codex/codex-rs/analytics/src/facts.rs` - enum values for trigger, reason, phase, implementation, strategy, and status.
- `references/codex/codex-rs/core/src/hook_runtime.rs` - pre/post compact hooks and manual/auto trigger labels.
- `references/codex/codex-rs/protocol/src/protocol.rs` - public `Op::Compact`, `CompactedItem`, and replacement history shape.
- `references/codex/codex-rs/protocol/src/config_types.rs` - auto compact token limit scope enum.
- `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs` - `thread/compact/start` request processor.
- `references/codex/codex-rs/app-server/src/message_processor.rs` - JSON-RPC request dispatch to the thread processor.
- `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs` - deprecated legacy `thread/compacted` handling note.
- `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs` - method and notification names for `thread/compact/start` and legacy compacted notification.
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs` - `ThreadCompactStartParams` and empty response type.
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs` - canonical `ThreadItem::ContextCompaction`.
- `references/codex/codex-rs/app-server-protocol/src/protocol/thread_history.rs` - conversion from legacy compaction event to `ContextCompaction` item in reconstructed history.
- `references/codex/codex-rs/app-server/README.md` - v2 API documentation for manual compaction and streamed progress.
- `references/codex/codex-rs/prompts/templates/compact/prompt.md` - local summarization prompt.
- `references/codex/codex-rs/prompts/templates/compact/summary_prefix.md` - model-visible continuation prefix after local compaction.
- `references/codex/codex-rs/tui/src/slash_command.rs` - slash command description for `/compact`.
- `references/codex/codex-rs/tui/src/chatwidget/slash_dispatch.rs` - TUI command dispatch for `/compact`.
- `references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs` - tests for queued `/compact` while another turn is active.
- `references/codex/codex-rs/tui/src/chatwidget/tests/plan_mode.rs` - tests for user input submitted while a compact turn is running.
- `references/codex/codex-rs/app-server/tests/suite/v2/compaction.rs` - app-server compaction lifecycle tests.
- `references/codex/codex-rs/core/tests/suite/compact.rs` - core local/remote/manual/auto compaction behavior tests.
- `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs` - resume/fork preservation tests after compaction.

## Key Findings

1. Codex has both automatic and manual compaction, but manual compaction is modeled as a standalone turn/task, not as a command receipt with running/idle outcome. `Op::Compact` is the public protocol operation (`references/codex/codex-rs/protocol/src/protocol.rs:637`), the core submission loop dispatches it to `compact(&sess, sub.id.clone())` (`references/codex/codex-rs/core/src/session/handlers.rs:803`), and `CompactTask` routes it to token-budget, remote, remote-v2, or local compaction (`references/codex/codex-rs/core/src/tasks/compact.rs:35`, `references/codex/codex-rs/core/src/tasks/compact.rs:40`, `references/codex/codex-rs/core/src/tasks/compact.rs:51`, `references/codex/codex-rs/core/src/tasks/compact.rs:76`).

2. Automatic compaction has explicit pre-turn and mid-turn phases. At the start of `run_turn`, Codex calls `run_pre_sampling_compact` before recording new turn context/user input (`references/codex/codex-rs/core/src/session/turn.rs:152`, `references/codex/codex-rs/core/src/session/turn.rs:156`); pre-turn compaction fires when `token_status.token_limit_reached` (`references/codex/codex-rs/core/src/session/turn.rs:803`, `references/codex/codex-rs/core/src/session/turn.rs:807`, `references/codex/codex-rs/core/src/session/turn.rs:810`); mid-turn compaction fires after a sampling response if the model needs follow-up or pending input exists and either a new context window was requested or token limit was reached (`references/codex/codex-rs/core/src/session/turn.rs:345`, `references/codex/codex-rs/core/src/session/turn.rs:346`, `references/codex/codex-rs/core/src/session/turn.rs:347`, `references/codex/codex-rs/core/src/session/turn.rs:349`).

3. Automatic compaction is also triggered by model compatibility changes, not just token pressure. `maybe_run_previous_model_inline_compact` runs pre-sampling compaction when the previous and current compaction compatibility hashes differ (`references/codex/codex-rs/core/src/session/turn.rs:831`, `references/codex/codex-rs/core/src/session/turn.rs:843`, `references/codex/codex-rs/core/src/session/turn.rs:858`, `references/codex/codex-rs/core/src/session/turn.rs:863`) or when switching to a smaller context-window model while over the new limit (`references/codex/codex-rs/core/src/session/turn.rs:870`, `references/codex/codex-rs/core/src/session/turn.rs:876`, `references/codex/codex-rs/core/src/session/turn.rs:891`, `references/codex/codex-rs/core/src/session/turn.rs:899`, `references/codex/codex-rs/core/src/session/turn.rs:904`).

4. Codex carries strong provenance vocabulary for compaction. Analytics defines `CompactionTrigger::{Manual, Auto}` (`references/codex/codex-rs/analytics/src/facts.rs:361`), `CompactionReason::{UserRequested, ContextLimit, ModelDownshift, CompHashChanged}` (`references/codex/codex-rs/analytics/src/facts.rs:368`), and `CompactionPhase::{StandaloneTurn, PreTurn, MidTurn}` (`references/codex/codex-rs/analytics/src/facts.rs:385`). The request metadata object attaches trigger/reason/implementation/phase/strategy at dispatch time (`references/codex/codex-rs/core/src/responses_metadata.rs:66`, `references/codex/codex-rs/core/src/responses_metadata.rs:73`, `references/codex/codex-rs/core/src/responses_metadata.rs:81`, `references/codex/codex-rs/core/src/responses_metadata.rs:107`, `references/codex/codex-rs/core/src/responses_metadata.rs:280`).

5. Local compaction asks a model to write a handoff summary and then installs that summary as model-visible history. The prompt says to create a handoff summary for another LLM that will resume the task (`references/codex/codex-rs/prompts/templates/compact/prompt.md:1`) and asks for progress, decisions, constraints, next steps, data, and references (`references/codex/codex-rs/prompts/templates/compact/prompt.md:3`). The installed summary is prefixed with "Another language model started..." and instructs the next model to build on prior work (`references/codex/codex-rs/prompts/templates/compact/summary_prefix.md:1`). Implementation extracts the last assistant message as `summary_suffix`, prefixes it, collects user messages, and builds replacement history (`references/codex/codex-rs/core/src/compact.rs:322`, `references/codex/codex-rs/core/src/compact.rs:325`, `references/codex/codex-rs/core/src/compact.rs:326`, `references/codex/codex-rs/core/src/compact.rs:328`).

6. Local replacement history is intentionally lossy but keeps recent real user messages plus one summary message. `collect_user_messages` drops prior summary messages (`references/codex/codex-rs/core/src/compact.rs:499`, `references/codex/codex-rs/core/src/compact.rs:504`, `references/codex/codex-rs/core/src/compact.rs:524`), `build_compacted_history_with_limit` selects messages from newest backward within a 20k token user-message budget (`references/codex/codex-rs/core/src/compact.rs:53`, `references/codex/codex-rs/core/src/compact.rs:604`, `references/codex/codex-rs/core/src/compact.rs:607`, `references/codex/codex-rs/core/src/compact.rs:627`), and appends the summary as a `role: "user"` message (`references/codex/codex-rs/core/src/compact.rs:650`, `references/codex/codex-rs/core/src/compact.rs:652`, `references/codex/codex-rs/core/src/compact.rs:653`).

7. Remote compaction paths replace local text summarization with provider/server compact outputs. Legacy remote sends a unary `/responses/compact` request through `compact_conversation_history` (`references/codex/codex-rs/core/src/client.rs:513`, `references/codex/codex-rs/core/src/client.rs:521`, `references/codex/codex-rs/core/src/client.rs:569`, `references/codex/codex-rs/codex-api/src/endpoint/compact.rs:35`, `references/codex/codex-rs/codex-api/src/endpoint/compact.rs:71`). Remote v2 appends a `ResponseItem::CompactionTrigger` request control (`references/codex/codex-rs/core/src/compact_remote_v2.rs:241`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:242`), requires exactly one `ResponseItem::Compaction` output (`references/codex/codex-rs/core/src/compact_remote_v2.rs:397`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:409`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:432`), and builds replacement history from retained prompt items plus that compaction output (`references/codex/codex-rs/core/src/compact_remote_v2.rs:447`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:451`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:463`).

8. Replacement history is the durable resume boundary. After compaction, Codex calls `replace_compacted_history`, replaces in-memory history, persists `RolloutItem::Compacted(compacted_item)`, optionally persists world-state and turn-context items, and queues a pending session start source of `Compact` (`references/codex/codex-rs/core/src/session/mod.rs:2978`, `references/codex/codex-rs/core/src/session/mod.rs:2999`, `references/codex/codex-rs/core/src/session/mod.rs:3007`, `references/codex/codex-rs/core/src/session/mod.rs:3014`, `references/codex/codex-rs/core/src/session/mod.rs:3020`). The `CompactedItem` stores `message`, optional `replacement_history`, and context window identifiers (`references/codex/codex-rs/protocol/src/protocol.rs:3188`, `references/codex/codex-rs/protocol/src/protocol.rs:3190`, `references/codex/codex-rs/protocol/src/protocol.rs:3191`, `references/codex/codex-rs/protocol/src/protocol.rs:3193`, `references/codex/codex-rs/protocol/src/protocol.rs:3202`).

9. Resume/fork reconstruction uses the newest persisted compaction checkpoint as a replacement-history base, then replays newer suffix items. Reverse replay records the newest `RolloutItem::Compacted` with `replacement_history` as `base_replacement_history` and moves `rollout_suffix` after it (`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:154`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:156`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:181`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:184`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:185`). A second replay loop replaces history with compacted replacement history if encountered (`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:341`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:342`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:345`) and clears world-state baseline on compaction (`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:393`).

10. App-server exposes manual compaction as `thread/compact/start`, but its response is intentionally empty and progress is event-driven. Protocol maps `ThreadCompactStart` to `"thread/compact/start"` (`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:577`), params only contain `thread_id` and response is `{}` (`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:939`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:946`), the processor loads the thread and submits `Op::Compact` (`references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:1771`, `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:1778`, `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:1779`, `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:1782`), and docs say the request returns immediately with `{}` while progress is emitted through `turn/*` and `item/*` notifications (`references/codex/codex-rs/app-server/README.md:674`, `references/codex/codex-rs/app-server/README.md:676`, `references/codex/codex-rs/app-server/README.md:678`, `references/codex/codex-rs/app-server/README.md:681`).

11. The canonical UI/protocol lifecycle is `ContextCompaction` item start/completion, with the old `thread/compacted` notification deprecated. App-server docs expect one `contextCompaction` item started and completed (`references/codex/codex-rs/app-server/README.md:676`, `references/codex/codex-rs/app-server/README.md:678`, `references/codex/codex-rs/app-server/README.md:679`); protocol still names deprecated `ContextCompacted => "thread/compacted"` and says to use the `ContextCompaction` item type instead (`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:1665`, `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:1666`); `ThreadItem::ContextCompaction { id }` is the v2 item shape (`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:395`).

12. Codex has no evidence of a server-side "manual request scheduled for next turn" branch like AgentDashboard wants. The TUI queues `/compact` locally while a turn is active (`references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:170`, `references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:175`, `references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:188`, `references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:194`), but core `spawn_task` aborts/replaces all active tasks before starting a new task (`references/codex/codex-rs/core/src/tasks/mod.rs:314`, `references/codex/codex-rs/core/src/tasks/mod.rs:320`, `references/codex/codex-rs/core/src/tasks/mod.rs:322`). Also, compact turns reject steering (`references/codex/codex-rs/core/src/session/mod.rs:3860`, `references/codex/codex-rs/core/src/session/mod.rs:3867`, `references/codex/codex-rs/core/src/session/mod.rs:3868`), and the TUI test falls back from attempted steer to queued user message while compacting (`references/codex/codex-rs/tui/src/chatwidget/tests/plan_mode.rs:1147`, `references/codex/codex-rs/tui/src/chatwidget/tests/plan_mode.rs:1161`, `references/codex/codex-rs/tui/src/chatwidget/tests/plan_mode.rs:1169`, `references/codex/codex-rs/tui/src/chatwidget/tests/plan_mode.rs:1192`).

13. Codex does not appear to no-op manual compaction when no user history exists. The local builder always appends a summary message, using `(no summary available)` only if summary text is empty (`references/codex/codex-rs/core/src/compact.rs:644`, `references/codex/codex-rs/core/src/compact.rs:650`, `references/codex/codex-rs/core/src/compact.rs:653`), and a core test explicitly says "Manual /compact with no prior user turn currently still issues a compaction request" (`references/codex/codex-rs/core/tests/suite/compact.rs:4492`, `references/codex/codex-rs/core/tests/suite/compact.rs:4518`, `references/codex/codex-rs/core/tests/suite/compact.rs:4536`, `references/codex/codex-rs/core/tests/suite/compact.rs:4546`).

14. Token-budget mode is a separate "context shrink" style path that skips summarization and starts a fresh context window, while preserving the same compact lifecycle and hooks. The file says token-budget compaction skips model/server summarization and installs a fresh context window (`references/codex/codex-rs/core/src/compact_token_budget.rs:20`, `references/codex/codex-rs/core/src/compact_token_budget.rs:22`, `references/codex/codex-rs/core/src/compact_token_budget.rs:23`), and implementation emits a context compaction item, calls `start_new_context_window`, then completes the item (`references/codex/codex-rs/core/src/compact_token_budget.rs:76`, `references/codex/codex-rs/core/src/compact_token_budget.rs:79`, `references/codex/codex-rs/core/src/compact_token_budget.rs:81`).

## Two Trigger Modes

Codex has the following trigger modes:

1. Automatic compaction:
   - Pre-turn, before normal sampling, when auto-compaction token limit or usable window is exhausted (`references/codex/codex-rs/core/src/session/turn.rs:797`, `references/codex/codex-rs/core/src/session/turn.rs:806`, `references/codex/codex-rs/core/src/session/turn.rs:807`, `references/codex/codex-rs/core/src/session/turn.rs:816`).
   - Pre-turn, on model compatibility hash change or model downshift (`references/codex/codex-rs/core/src/session/turn.rs:831`, `references/codex/codex-rs/core/src/session/turn.rs:858`, `references/codex/codex-rs/core/src/session/turn.rs:863`, `references/codex/codex-rs/core/src/session/turn.rs:899`, `references/codex/codex-rs/core/src/session/turn.rs:904`).
   - Mid-turn, after sampling, when follow-up is needed and token/new-context conditions require compaction (`references/codex/codex-rs/core/src/session/turn.rs:345`, `references/codex/codex-rs/core/src/session/turn.rs:346`, `references/codex/codex-rs/core/src/session/turn.rs:347`, `references/codex/codex-rs/core/src/session/turn.rs:355`).

2. Manual compaction:
   - TUI `/compact` maps to `Op::Compact` (`references/codex/codex-rs/tui/src/slash_command.rs:40`, `references/codex/codex-rs/tui/src/slash_command.rs:88`, `references/codex/codex-rs/core/src/session/handlers.rs:803`).
   - App-server `thread/compact/start` maps to `Op::Compact` and returns `{}` (`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:577`, `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:1778`, `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:1779`, `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:1782`).
   - Manual local/remote compaction uses `trigger=Manual`, `reason=UserRequested`, `phase=StandaloneTurn` (`references/codex/codex-rs/core/src/compact.rs:141`, `references/codex/codex-rs/core/src/compact.rs:142`, `references/codex/codex-rs/core/src/compact.rs:143`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:97`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:98`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:99`).

Running/idle branch status:

- Not found in inspected scope: a backend command outcome split equivalent to AgentDashboard's `scheduled_next_turn` for running-active and `launched_compaction_turn` for idle.
- The closest running behavior is TUI-side queuing of a `/compact` slash command until the active turn completes (`references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:170`, `references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:177`, `references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:188`, `references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:194`).
- Core task startup itself replaces active tasks rather than scheduling an active-turn-safe manual compaction request (`references/codex/codex-rs/core/src/tasks/mod.rs:314`, `references/codex/codex-rs/core/src/tasks/mod.rs:320`, `references/codex/codex-rs/core/src/tasks/mod.rs:322`).
- App-server docs only say manual compaction makes the thread effectively in a turn while progress streams (`references/codex/codex-rs/app-server/README.md:674`, `references/codex/codex-rs/app-server/README.md:681`); no command receipt/idempotency outcome was found for compaction.

## Resume/Continuation Semantics

Codex resume-after-compact is based on persisted replacement history, not on mutating old transcript events in place.

- Compaction installs new in-memory history and persists a `RolloutItem::Compacted` carrying `replacement_history` (`references/codex/codex-rs/core/src/session/mod.rs:2978`, `references/codex/codex-rs/core/src/session/mod.rs:2999`, `references/codex/codex-rs/core/src/session/mod.rs:3007`; `references/codex/codex-rs/protocol/src/protocol.rs:3188`, `references/codex/codex-rs/protocol/src/protocol.rs:3191`).
- Local compaction turns model output into a prefixed handoff summary for the next model (`references/codex/codex-rs/prompts/templates/compact/summary_prefix.md:1`, `references/codex/codex-rs/core/src/compact.rs:325`, `references/codex/codex-rs/core/src/compact.rs:352`).
- Replacement history carries recent real user messages plus the summary item; prior summaries are filtered out (`references/codex/codex-rs/core/src/compact.rs:499`, `references/codex/codex-rs/core/src/compact.rs:504`, `references/codex/codex-rs/core/src/compact.rs:524`, `references/codex/codex-rs/core/src/compact.rs:650`).
- Resume reconstruction finds the newest compaction checkpoint with replacement history and replays only newer suffix facts (`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:154`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:181`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:184`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:185`).
- Resume/fork tests assert that the model-visible input after resume/fork preserves the compacted prefix (`references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:168`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:183`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:187`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:220`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:231`).
- Multiple compactions are handled by reusing the latest compacted history on later resume (`references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:282`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:296`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:342`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:353`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:398`).
- App-server's current v2 UI surface exposes a `ContextCompaction` item, while legacy `thread/compacted` exists only for old clients (`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:395`, `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:1665`, `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:926`, `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:927`).

What is injected after compact:

- Local path injects a human-readable summary as a model-visible message prefixed by `SUMMARY_PREFIX` (`references/codex/codex-rs/prompts/templates/compact/summary_prefix.md:1`, `references/codex/codex-rs/core/src/compact.rs:325`, `references/codex/codex-rs/core/src/compact.rs:650`).
- Remote v2 path injects provider/server `ResponseItem::Compaction` output into replacement history, not a plaintext local summary message (`references/codex/codex-rs/core/src/compact_remote_v2.rs:409`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:463`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:631`).
- No special "resume after compact" system message was found beyond summary prefix/replacement history/session start source. The persistence hook queues `SessionStartSource::Compact`, but the inspected files do not show a separate system prompt dedicated only to resume-after-compact (`references/codex/codex-rs/core/src/session/mod.rs:3020`).

## Lessons For AgentDashboard

Directly adoptable:

- Use explicit compaction provenance vocabulary: `trigger=manual|auto`, `reason=user_requested|context_limit|model_downshift|comp_hash_changed`, `phase=standalone_turn|pre_turn|mid_turn`. Codex has these as first-class analytics/request metadata fields (`references/codex/codex-rs/analytics/src/facts.rs:361`, `references/codex/codex-rs/analytics/src/facts.rs:368`, `references/codex/codex-rs/analytics/src/facts.rs:385`, `references/codex/codex-rs/core/src/responses_metadata.rs:73`).
- Treat compaction as a visible lifecycle item, not an invisible state mutation. Codex emits `ContextCompaction` item start/completion and app-server docs expect clients to show progress from notifications (`references/codex/codex-rs/app-server/README.md:676`, `references/codex/codex-rs/app-server/README.md:678`, `references/codex/codex-rs/app-server/README.md:679`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:395`).
- Persist compacted replacement/projection as the durable resume boundary. Codex `replacement_history` maps cleanly to AgentDashboard `session_compactions` plus `session_projection_segments` plus `session_projection_heads` (`references/codex/codex-rs/core/src/session/mod.rs:3007`, `references/codex/codex-rs/protocol/src/protocol.rs:3191`; `.trellis/spec/backend/session/context-compaction-projection.md:11`, `.trellis/spec/backend/session/context-compaction-projection.md:15`, `.trellis/spec/backend/session/context-compaction-projection.md:17`).
- Use one shared compaction preflight/implementation for automatic and manual paths, differing by trigger/reason/phase. Codex routes manual and auto through the same local/remote/token-budget implementations (`references/codex/codex-rs/core/src/tasks/compact.rs:35`, `references/codex/codex-rs/core/src/session/turn.rs:917`, `references/codex/codex-rs/core/src/session/turn.rs:938`, `references/codex/codex-rs/core/src/session/turn.rs:980`).
- Keep summaries as handoff material, not the only truth. This aligns with AgentDashboard's spec that compact summary is model-visible handoff while original facts remain in session events/lifecycle recall surfaces (`references/codex/codex-rs/prompts/templates/compact/prompt.md:1`, `references/codex/codex-rs/prompts/templates/compact/summary_prefix.md:1`; `.trellis/spec/backend/session/context-compaction-projection.md:9`, `.trellis/spec/backend/session/context-compaction-projection.md:104`).
- Preserve normal provider-adapter/request semantics for summarization. AgentDashboard spec already says structural compact summary should go through the normal provider bridge using native message sequences (`.trellis/spec/backend/session/context-compaction-projection.md:23`), which matches Codex's reuse of standard model request paths for local compaction (`references/codex/codex-rs/core/src/compact.rs:257`, `references/codex/codex-rs/core/src/compact.rs:262`, `references/codex/codex-rs/core/src/compact.rs:668`).

Not suitable to copy directly:

- Do not copy Codex core's `spawn_task` replacement behavior for manual compaction. It aborts existing tasks before starting compact (`references/codex/codex-rs/core/src/tasks/mod.rs:314`, `references/codex/codex-rs/core/src/tasks/mod.rs:320`), while AgentDashboard requires running-active manual compaction to avoid interrupting the active turn.
- Do not copy app-server's empty response for manual compaction. AgentDashboard needs command receipt, idempotency, disabled state, and outcome such as scheduled vs launched; Codex `ThreadCompactStartResponse` is empty and params only have `thread_id` (`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:939`, `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:946`).
- Do not copy Codex's behavior of compacting even with no previous user messages. AgentDashboard should return no-op when no legal cut point exists; Codex test documents that manual compact without prior user messages still issues a request (`references/codex/codex-rs/core/tests/suite/compact.rs:4492`, `references/codex/codex-rs/core/tests/suite/compact.rs:4546`).
- Do not use Codex local history shape literally. Codex summary is encoded as a `role: "user"` message (`references/codex/codex-rs/core/src/compact.rs:650`, `references/codex/codex-rs/core/src/compact.rs:652`), while AgentDashboard's projection spec has typed provenance fields for `origin=projection`, `synthetic=true`, `projection_segment_id`, and `active_compaction_id` (`.trellis/spec/backend/session/context-compaction-projection.md:83`, `.trellis/spec/backend/session/context-compaction-projection.md:86`, `.trellis/spec/backend/session/context-compaction-projection.md:87`).

## Suggested Improvements Beyond Manual Trigger

Only suggestions tied to AgentDashboard's current compaction chain:

1. Add first-class compaction provenance fields to request records, event payloads, projection segments, and context frames:
   - `trigger`
   - `reason`
   - `phase`
   - `request_id`
   - `implementation`
   - `strategy`
   Codex's field set provides a proven vocabulary (`references/codex/codex-rs/analytics/src/facts.rs:361`, `references/codex/codex-rs/analytics/src/facts.rs:368`, `references/codex/codex-rs/analytics/src/facts.rs:385`, `references/codex/codex-rs/core/src/responses_metadata.rs:73`).

2. Persist a compaction "install/checkpoint" trace separate from the summary generation request:
   - Codex remote paths record input history and replacement history at install time (`references/codex/codex-rs/core/src/compact_remote.rs:291`, `references/codex/codex-rs/core/src/compact_remote.rs:294`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:323`, `references/codex/codex-rs/core/src/compact_remote_v2.rs:327`).
   - AgentDashboard can mirror this with a diagnostic record tying source refs, summary segment ids, projection head, and provider request ids.

3. Add tests that prove resume uses projection checkpoint before replaying suffix facts:
   - Codex tests assert post-compact prefixes survive resume/fork (`references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:168`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:187`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:231`, `references/codex/codex-rs/core/tests/suite/compact_resume_fork.rs:353`).
   - AgentDashboard should test `session_projection_heads -> active compaction -> segments -> suffix session_events` per spec (`.trellis/spec/backend/session/context-compaction-projection.md:59`, `.trellis/spec/backend/session/context-compaction-projection.md:62`, `.trellis/spec/backend/session/context-compaction-projection.md:64`).

4. Add explicit no-op diagnostics for manual compaction:
   - Codex lacks this behavior in inspected scope and compacts even without prior user messages (`references/codex/codex-rs/core/tests/suite/compact.rs:4546`).
   - AgentDashboard should make `no_eligible_messages` observable on command result and event diagnostics without writing a projection head.

5. Keep running-active manual compaction as a durable pending request, not a UI-only queue:
   - Codex TUI can queue `/compact` locally after active turn (`references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:170`, `references/codex/codex-rs/tui/src/chatwidget/tests/slash_commands.rs:194`), but AgentDashboard has multi-client/API requirements where the backend must own idempotency and recovery.

6. Add compact-turn non-steerability or mailbox policy tests:
   - Codex rejects steering compact turns (`references/codex/codex-rs/core/src/session/mod.rs:3867`, `references/codex/codex-rs/core/src/session/mod.rs:3868`; `references/codex/codex-rs/core/src/session/tests.rs:10003`, `references/codex/codex-rs/core/src/session/tests.rs:10043`).
   - AgentDashboard compact-only turns should likewise avoid ordinary user input/assistant response paths and route queued user work through mailbox/next normal turn.

## External References

- `references/codex/codex-rs/app-server/README.md:165` documents `thread/compact/start`.
- `references/codex/codex-rs/app-server/README.md:674` documents manual history compaction returning `{}` immediately.
- `references/codex/codex-rs/app-server/README.md:676` documents progress through standard `turn/*` and `item/*` notifications.
- `references/codex/codex-rs/prompts/templates/compact/prompt.md:1` documents the local summarizer's intended handoff semantics.
- `references/codex/codex-rs/prompts/templates/compact/summary_prefix.md:1` documents the continuation prefix shown to the next model.
- No upstream release tag, commit hash, or web documentation was inspected in this scope.

## Related Specs

- `.trellis/spec/backend/session/context-compaction-projection.md:5` - AgentDashboard structural compact scope includes session resume, model context query, fork, and rollback baseline.
- `.trellis/spec/backend/session/context-compaction-projection.md:9` - compact does not rewrite historical session events; it submits a new model context projection.
- `.trellis/spec/backend/session/context-compaction-projection.md:15` - `session_compactions` records status, strategy, trigger, phase, source range, first kept pointer, token stats, summary, and replacement projection metadata.
- `.trellis/spec/backend/session/context-compaction-projection.md:19` - compaction event, record, segments, and projection head must commit atomically.
- `.trellis/spec/backend/session/context-compaction-projection.md:42` - checkpoint commit must complete before item completed enters the normal event stream.
- `.trellis/spec/backend/session/context-compaction-projection.md:44` - internal `context_compacted` payload must carry explicit boundary fields.
- `.trellis/spec/backend/session/context-compaction-projection.md:59` - `ContextProjector` builds model input from durable facts instead of clipping UI timeline arrays.
- `.trellis/spec/backend/session/context-compaction-projection.md:71` - user input item boundaries must survive resume, fork, rollback, and later compact.
- `.trellis/spec/backend/session/context-compaction-projection.md:104` - summary is model-visible handoff while original intent/tool facts remain factual sources.
- `.trellis/spec/backend/session/architecture.md:61` - AgentDashboard explicitly adopts Codex-aligned lifecycle plus AgentDash-owned projection store.
- `.trellis/spec/backend/session/architecture.md:68` - runtime delegate composition has a compaction facet suitable for shared manual/auto preflight.
- `.trellis/spec/backend/session/architecture.md:69` - command availability must come from `ConversationCommandAvailabilityResolver` and command policy, not UI status.
- `.trellis/spec/backend/session/runtime-execution-state.md:206` - user command receipt is an AgentRun command projection with idempotency and accepted result semantics.
- `.trellis/spec/backend/session/runtime-execution-state.md:246` - `starting_claimed` and `running_active` are distinct execution states relevant to command availability.

## Caveats / Not Found

- Not found in inspected scope: a Codex backend endpoint or core service with AgentDashboard-style command receipt, `client_command_id`, or idempotent manual compaction outcome.
- Not found in inspected scope: Codex backend running-active manual compaction that records a durable pending request for the next provider boundary. TUI queues `/compact` locally, while core `spawn_task` replaces active work if invoked directly.
- Not found in inspected scope: Codex manual compaction no-op for missing eligible cut points. Evidence points the other way: a test documents compacting with no prior user turn still issues a compaction request.
- Not found in inspected scope: an explicit `compacted_until_ref` / `first_kept_ref` boundary contract in Codex. Codex uses `replacement_history` checkpoints and context window ids; AgentDashboard's `MessageRef` boundary contract remains more precise.
- Not found in inspected scope: a special resume-only system message after compaction beyond local `SUMMARY_PREFIX`, replacement history, and pending session-start source `Compact`.
- Codex remote v2 `ResponseItem::Compaction` output is provider/server-specific and may not map to AgentDashboard's provider-agnostic summary segment without an adapter.
