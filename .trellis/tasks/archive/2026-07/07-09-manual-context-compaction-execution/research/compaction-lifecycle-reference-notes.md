# Compaction Lifecycle Reference Notes

## Codex Reference

Relevant files:

- `references/codex/codex-rs/protocol/src/protocol.rs`
- `references/codex/codex-rs/protocol/src/items.rs`
- `references/codex/codex-rs/core/src/session/handlers.rs`
- `references/codex/codex-rs/core/src/compact.rs`
- `references/codex/codex-rs/core/src/session/mod.rs`
- `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs`
- `references/codex/codex-rs/core/src/state/auto_compact_window.rs`

Findings:

- Manual compact is a dedicated operation (`Op::Compact`) handled by `compact(sess, sub_id)`, which creates a default turn context and spawns a `CompactTask`. It is not encoded as an ordinary user prompt.
- Compaction has a visible item lifecycle via `TurnItem::ContextCompaction(ContextCompactionItem::new())`, with start and completed events around the summary task.
- Successful compaction replaces the in-memory model history and persists a `RolloutItem::Compacted(CompactedItem { replacement_history: Some(new_history), window_number, first_window_id, previous_window_id, window_id, ... })`.
- Resume reconstruction prefers `replacement_history` from the latest compaction and treats legacy compactions without replacement history as degraded. This is the strongest reference point for AgentDash: the compaction result must be a durable replacement baseline, not just a marker.
- Codex maintains auto-compaction window IDs and token prefill accounting. AgentDash does not need to copy the window chain for this task because `runtime_session_compactions`, `runtime_session_projection_segments`, and `runtime_session_projection_heads` already provide the durable checkpoint identity.

Useful design lesson:

Compaction should be a managed maintenance turn with its own item lifecycle and durable replacement baseline. The durable replacement baseline is the success boundary.

## Claude Code Reference

Relevant files:

- `references/claude-code/src/commands/compact/index.ts`
- `references/claude-code/src/services/compact/compact.ts`
- `references/claude-code/src/services/compact/autoCompact.ts`
- `references/claude-code/src/query.ts`
- `references/claude-code/src/utils/messages.ts`
- `references/claude-code/src/Tool.ts`
- `references/claude-code/src/entrypoints/sdk/coreSchemas.ts`
- `references/claude-code/src/cli/transports/ccrClient.ts`

Findings:

- `/compact` is a local command with dedicated implementation, not a normal chat message.
- The compaction flow exposes progress events: `compact_start`, `compact_end`, and hook progress for `pre_compact` / `post_compact`.
- Hook schemas distinguish `PreCompact` with trigger/custom instructions and `PostCompact` with the produced summary.
- The model-facing boundary is a `compact_boundary` system message with `compact_metadata` including trigger, token counts, and preserved segment linkage.
- `getMessagesAfterCompactBoundary` makes post-compact model input explicit by slicing from the latest boundary.
- Auto compaction tracks consecutive failures as a circuit breaker and skips compaction for compact/session-memory query sources to avoid recursion/deadlock.
- Worker internal events can be tagged `is_compaction` and are used for resume, separating user-visible UI events from worker-internal recovery facts.

Useful design lesson:

The compact boundary is both a user-visible lifecycle marker and a resume/projection marker. AgentDash should keep this duality through `ContextCompactionStarted/ContextCompacted/ContextCompactionFailed` plus projection checkpoint commit, without adding Claude Code's many specialized compact variants.

## AgentDash Implications

- Keep the existing AgentDash-owned projection store as the durable baseline mechanism. Do not add a new window chain or replacement-history table.
- Treat compact-only as a `ExecutionTurnMode::ContextCompaction` maintenance turn with terminal semantics:
  - `completed`: projection checkpoint committed and request completed.
  - `noop`: model context was materialized and eligible rules genuinely found nothing to compact.
  - `failed`: restore, boundary refs, summary generation, projection commit, or cancellation failed.
- `no_eligible_messages` must not be the catch-all for broken restore or missing refs.
- A compact-only failure is the failure of that maintenance turn. In a normal provider turn, automatic compaction failure can remain a failed lifecycle item while the main turn continues if the existing runtime contract wants that behavior.
