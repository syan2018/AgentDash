# Research: Claude Code compact lifecycle

- Query: Read-only investigation of `references/claude-code` compact / conversation summary / context management / lifecycle / status machine implementation, then compare with the current AgentDash manual context compaction repair design.
- Scope: mixed
- Date: 2026-07-09

## Findings

### Task Context Read Order

The task context entries were read before forming conclusions:

| File | Purpose found in context |
| --- | --- |
| `.trellis/tasks/07-09-manual-context-compaction-execution/implement.jsonl:1` | `prd.md` defines behavior, failure semantics, and acceptance criteria. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/implement.jsonl:2` | `design.md` defines compact-only restore, eligibility diagnostics, request finalization, and receipt semantics. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/implement.jsonl:3` | `implement.md` gives ordered implementation and validation commands. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/implement.jsonl:4` | Existing `research/compaction-lifecycle-reference-notes.md` summarizes Codex and Claude Code lifecycle lessons. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/implement.jsonl:5` | `context-compaction-projection.md` is the canonical durable projection contract. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/implement.jsonl:6` | `session/architecture.md` covers launch/eventing/delegate/runtime persistence boundaries. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/implement.jsonl:7` | `backend/architecture.md` covers backend layering and repository/domain boundaries. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/check.jsonl:1` | Check must verify every PRD acceptance criterion. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/check.jsonl:2` | Check must validate restore, eligibility, request lifecycle, and receipt semantics against design. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/check.jsonl:3` | Check must keep compaction as a managed lifecycle boundary without importing reference complexity. |
| `.trellis/tasks/07-09-manual-context-compaction-execution/check.jsonl:4` | Check must ensure successful compaction commits records, segments, and active projection head. |

Task documents then set the local target semantics:

- `prd.md:5` says manual compaction must use the durable model context and must expose restore or `MessageRef` boundary failures as explicit diagnostics.
- `prd.md:15-20` requires compact-only input from `ContextProjector`, true `no_eligible_messages` only after complete restore, structural problems as failed, request/receipt/session lifecycle consistency, and consumed cancel/abort requests reaching terminal state.
- `prd.md:31-37` acceptance requires projection records/heads on success, request `completed` with boundary refs, structural failures as `failed`, true noop only for complete-but-not-compactable context, compact-only restore from history, and cancel/abort terminal diagnostics.
- `design.md:7-13` identifies the current collapse points: short polling maps `Noop` to `NoEligibleMessages` with `turn_id = null`, compact-only depends on connector-provided `AgentContext`, and `should_execute_compaction` folds ref failures into `false`.
- `design.md:21-26` defines target status semantics: `completed` means projection checkpoint committed, `noop` means complete context with no compactable prefix, and `failed` covers restore, ref boundary, summary provider, projection commit, cancel/abort.
- `design.md:29-60` defines compact-only as a managed maintenance lifecycle with `requested -> maintenance_turn_launched -> context_materialized -> eligibility_checked -> compact_item_started -> summary_generated -> projection_committed -> request_completed`, plus explicit noop and failed paths.
- `design.md:96-122` proposes structured eligibility instead of a bool: `Eligible`, `NoEligibleMessages`, and `InvalidInput`.
- `design.md:124-145` requires consumed manual requests to finalize on failure/cancel and command receipts to retain maintenance `turn_id`.
- `implement.md:9-33` orders the work as eligibility classification, preflight lifecycle, compact-only restore path, receipt/request state, and projection commit success verification.

### Files Found

Claude Code reference files:

| Path | Description |
| --- | --- |
| `references/claude-code/src/commands/compact/index.ts` | Defines `/compact` as a local command, not a normal chat prompt. |
| `references/claude-code/src/commands/compact/compact.ts` | Manual compact command implementation and error mapping. |
| `references/claude-code/src/services/compact/compact.ts` | Core summary generation, compact boundary construction, post-compact result assembly, cleanup, and failure handling. |
| `references/claude-code/src/services/compact/autoCompact.ts` | Automatic compaction threshold, failure circuit breaker, and success result return. |
| `references/claude-code/src/services/compact/sessionMemoryCompact.ts` | Optional session-memory compaction variant used before legacy compaction when available. |
| `references/claude-code/src/services/compact/postCompactCleanup.ts` | Shared cache/state invalidation after manual or auto compaction. |
| `references/claude-code/src/utils/processUserInput/processSlashCommand.tsx` | Converts local command compact result into post-compact messages and suppresses normal model query. |
| `references/claude-code/src/query.ts` | Auto compaction integration before the main model call; replaces `messagesForQuery` on success. |
| `references/claude-code/src/QueryEngine.ts` | Headless/SDK path persistence, compact boundary replay, and transcript writeback. |
| `references/claude-code/src/utils/messages.ts` | Compact boundary message helpers and active-context slicing via latest boundary. |
| `references/claude-code/src/utils/sessionStorage.ts` | Transcript persistence, compact boundary parent-chain cut, preserved-segment relink and resume behavior. |
| `references/claude-code/src/utils/sessionStoragePortable.ts` | Fast compact-boundary scan for portable transcript loading. |
| `references/claude-code/src/screens/REPL.tsx` | UI progress state for compact and active-context warning for selected pre-compact messages. |
| `references/claude-code/src/components/CompactSummary.tsx` | Transcript/display rendering for compact summaries. |
| `references/claude-code/src/types/command.ts` | `LocalCommandResult` discriminated union includes `type: "compact"`. |

AgentDash files used for comparison:

| Path | Description |
| --- | --- |
| `crates/agentdash-agent/src/compaction/mod.rs` | Current summary-prefix compaction and bool eligibility gate. |
| `crates/agentdash-agent/src/agent_loop/streaming.rs` | Current `run_compaction_preflight` emits noop/failed and mutates request context after success. |
| `crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs` | Manual command receipt, compact-only request creation, launch, and short-poll outcome mapping. |
| `crates/agentdash-application-runtime-session/src/session/manual_compaction_delegate.rs` | Runtime delegate that consumes manual requests and writes noop/failed request state. |
| `crates/agentdash-application-runtime-session/src/session/launch/planner.rs` | Launch planner wraps the manual compaction delegate and can rehydrate executor state. |
| `crates/agentdash-application-runtime-session/src/session/launch/plan.rs` | Launch source selects `ExecutionTurnMode::ContextCompaction`. |
| `crates/agentdash-application-runtime-session/src/session/eventing.rs` | Builds projected transcript via `ContextProjector` and commits compaction projection records/heads. |
| `crates/agentdash-infrastructure/migrations/0059_manual_context_compaction_requests.sql` | Current manual compaction request status table. |

### Related Specs

- `.trellis/spec/backend/session/context-compaction-projection.md:7-23` defines durable compaction shape: `session_events` remain factual history; success writes `session_compactions`, `session_projection_segments`, and `session_projection_heads` in one commit unit.
- `.trellis/spec/backend/session/context-compaction-projection.md:25-45` defines runtime compact lifecycle and states that noop is diagnostic only and does not create projection records.
- `.trellis/spec/backend/session/context-compaction-projection.md:46-57` defines explicit boundary fields and says failed compaction does not replace the active projection head.
- `.trellis/spec/backend/session/context-compaction-projection.md:59-72` defines `ContextProjector` as the model input source of truth, reading projection head/compaction/segments/suffix events.
- `.trellis/spec/backend/session/architecture.md:7-35` separates launch pipeline, execution context, runtime map, active turn, and connector live session.
- `.trellis/spec/backend/session/architecture.md:61` records the local decision that context compaction is Codex-aligned lifecycle plus AgentDash-owned projection store.
- `.trellis/spec/backend/session/architecture.md:69` says command availability and policy must come from the same source, not from display state.
- `.trellis/spec/backend/session/runtime-execution-state.md:44-49` distinguishes connector live session, runtime entry, active turn, and backend leases.
- `.trellis/spec/backend/session/runtime-execution-state.md:131-160` requires terminal event persistence and AgentRun control state to converge before browser-visible terminal observations.
- `.trellis/spec/backend/session/runtime-execution-state.md:206-211` defines user command receipt as AgentRun command projection, separate from RuntimeSession trace head.
- `.trellis/spec/backend/session/session-startup-pipeline.md:21-33` defines the launch stages and accepted/terminal responsibilities.
- `.trellis/spec/backend/architecture.md:7-16` requires clean dependency direction and explicit command/unit-of-work boundaries.

### Code Patterns In Claude Code

#### Manual compact is a local command with a dedicated result type

- `references/claude-code/src/commands/compact/index.ts:4-13` declares `/compact` as `type: "local"`, describes it as clearing conversation history while keeping a summary in context, disables it through `DISABLE_COMPACT`, and supports non-interactive use.
- `references/claude-code/src/types/command.ts:16-24` defines `LocalCommandResult` with a dedicated `{ type: "compact"; compactionResult; displayText? }` variant.
- `references/claude-code/src/commands/compact/compact.ts:40-50` starts manual compact by taking `context.messages`, projecting them through `getMessagesAfterCompactBoundary`, and throwing `No messages to compact` when there is no active context.
- `references/claude-code/src/commands/compact/compact.ts:55-82` first tries session-memory compaction when there are no custom instructions; success returns `type: "compact"` with a `CompactionResult`.
- `references/claude-code/src/commands/compact/compact.ts:96-124` falls back to legacy summary compaction: microcompact first, call `compactConversation`, clear stale summary tracking, run cleanup, and return `type: "compact"`.
- `references/claude-code/src/commands/compact/compact.ts:125-134` maps cancellation, not-enough messages, incomplete response, and other errors into distinct user-facing errors.

Interpretation: manual compact is not represented as an ordinary user prompt followed by a normal assistant answer. It is a local command action that may run a summarizer request internally, then returns a structural replacement message set.

#### Summary generation runs as a compaction-scoped model action

- `references/claude-code/src/services/compact/compact.ts:387-395` defines `compactConversation(...)` as the core summary operation and returns a `CompactionResult`.
- `references/claude-code/src/services/compact/compact.ts:397-399` treats empty input as `ERROR_MESSAGE_NOT_ENOUGH_MESSAGES`.
- `references/claude-code/src/services/compact/compact.ts:406-430` emits hook and compact progress, sets SDK status to `compacting`, executes PreCompact hooks, and starts compact progress.
- `references/claude-code/src/services/compact/compact.ts:440-491` builds the summary request, calls `streamCompactSummary`, and retries prompt-too-long by dropping oldest API-round groups.
- `references/claude-code/src/services/compact/compact.ts:493-515` treats no summary text and API-error summary text as compaction failures.
- `references/claude-code/src/services/compact/compact.ts:1136-1154` starts `streamCompactSummary` and documents the forked-agent path for prompt cache sharing.
- `references/claude-code/src/services/compact/compact.ts:1188-1200` runs `runForkedAgent` with `querySource: "compact"`, `forkLabel: "compact"`, `maxTurns: 1`, `skipCacheWrite: true`, and the same abort controller.
- `references/claude-code/src/services/compact/compact.ts:1205-1210` explicitly guards against treating API error/abort synthetic assistant messages as valid summaries.
- `references/claude-code/src/services/compact/compact.ts:1292-1327` fallback streams a compact summary with system prompt `You are a helpful AI assistant tasked with summarizing conversations.`, disabled thinking, tools constrained for summarization, and `signal: context.abortController.signal`.
- `references/claude-code/src/services/compact/compact.ts:1330-1395` tracks streaming response length, returns the assistant response if present, retries if configured, and throws `ERROR_MESSAGE_INCOMPLETE_RESPONSE` when no response arrives.

Interpretation: Claude Code has a real compaction-scoped model action. In the forked path it is literally an internal agent query with `querySource: "compact"` and one turn. But the user-visible command result is still structural replacement, not a normal assistant turn.

#### The compact result is a single source of truth for future model input

- `references/claude-code/src/services/compact/compact.ts:299-310` defines `CompactionResult` as `boundaryMarker`, `summaryMessages`, `attachments`, `hookResults`, optional `messagesToKeep`, optional display text, and token usage.
- `references/claude-code/src/services/compact/compact.ts:326-338` defines `buildPostCompactMessages(result)` as a single ordering rule: boundary, summary messages, kept messages, attachments, hook results.
- `references/claude-code/src/services/compact/compact.ts:596-624` creates the `compact_boundary` system marker and a user `isCompactSummary` / `isVisibleInTranscriptOnly` summary message.
- `references/claude-code/src/utils/messages.ts:4530-4555` constructs a `SystemCompactBoundaryMessage` with subtype `compact_boundary`, trigger, pre-token count, optional summarized message count, and `logicalParentUuid` to the last pre-compact message.
- `references/claude-code/src/utils/messages.ts:4608-4656` identifies the latest compact boundary and returns messages from that boundary onward for active model context.
- `references/claude-code/src/utils/processUserInput/processSlashCommand.tsx:678-704` handles `result.type === "compact"` by appending slash command/caveat messages into `messagesToKeep`, resetting microcompact state, returning `messages: buildPostCompactMessages(...)`, and setting `shouldQuery: false`.
- `references/claude-code/src/QueryEngine.ts:430-463` pushes messages from user input into mutable state and persists them before the model loop so resume can recover accepted input.
- `references/claude-code/src/QueryEngine.ts:556-616` handles `shouldQuery === false` by yielding local command output, compact summaries, and compact boundary SDK messages, then persisting the transcript again.

Interpretation: the post-compact message array is the shared durable and in-memory context update. The UI command state does not independently claim that compaction happened; it observes the same boundary/summary messages that later model input and resume use.

#### Auto compact is pre-query context replacement, not a separate user command

- `references/claude-code/src/services/compact/autoCompact.ts:241-252` returns `{ wasCompacted, compactionResult?, consecutiveFailures? }`.
- `references/claude-code/src/services/compact/autoCompact.ts:253-277` exits with `{ wasCompacted: false }` when compact is disabled, circuit breaker is open, or threshold does not require compaction.
- `references/claude-code/src/services/compact/autoCompact.ts:287-309` tries session-memory compaction first and returns a `CompactionResult` on success.
- `references/claude-code/src/services/compact/autoCompact.ts:312-333` calls `compactConversation(..., isAutoCompact = true)` and resets failure count on success.
- `references/claude-code/src/services/compact/autoCompact.ts:334-350` logs non-abort failures, increments consecutive failure count, and returns `{ wasCompacted: false, consecutiveFailures }`.
- `references/claude-code/src/query.ts:453-467` invokes `deps.autocompact(...)` before the main query.
- `references/claude-code/src/query.ts:470-535` yields the `postCompactMessages` and then sets `messagesForQuery = postCompactMessages`, so the current provider query proceeds with compacted context.

Interpretation: auto compact is a pre-provider maintenance step inside the current query flow. On success it writes the same structural message replacement as manual compact. On failure it records a circuit-breaker count rather than pretending a successful no-op happened.

#### UI progress is transient; durable context is boundary plus summary

- `references/claude-code/src/screens/REPL.tsx:2497-2511` maps compact progress to spinner text and clears it on `compact_end`.
- `references/claude-code/src/services/compact/compact.ts:749-762` uses `finally` to reset stream mode, response length, compact progress, and SDK status.
- `references/claude-code/src/components/CompactSummary.tsx:14-69` renders compact summary metadata and transcript detail from the compact summary message itself.

Interpretation: status indicators are best-effort UI state. The durable compact state is the persisted boundary/summary in the message stream.

#### Resume and history restoration rely on compact boundaries in transcript storage

- `references/claude-code/src/utils/sessionStorage.ts:993-1070` writes transcript messages; when a message is a compact boundary, it sets `parentUuid: null` and `logicalParentUuid` to the previous parent. This cuts the physical chain while preserving logical lineage.
- `references/claude-code/src/QueryEngine.ts:685-715` flushes preserved tail messages before writing a compact boundary so resume can relink preserved segments.
- `references/claude-code/src/utils/sessionStorage.ts:1823-1905` relinks preserved segments after compaction; if the tail-to-head walk fails, it logs `tengu_relink_walk_broken` and returns without pruning, so resume loads full pre-compact history rather than a broken shortened chain.
- `references/claude-code/src/screens/REPL.tsx:4918-4929` warns when the user selects a message that is visible in scrollback but no longer in the active compacted context.

Interpretation: Claude Code avoids UI/history/context split by putting the compaction boundary into the transcript and teaching load/resume paths how to interpret it. Its failure mode for malformed preserved segments is intentionally conservative: restore too much context rather than silently restore a broken compacted suffix.

#### Post-compact cleanup invalidates stale model-context caches

- `references/claude-code/src/services/compact/postCompactCleanup.ts:12-30` defines cleanup as shared for manual and auto compaction and explains query-source-specific behavior.
- `references/claude-code/src/services/compact/postCompactCleanup.ts:31-77` resets microcompact state, context collapse, user context/memory caches, system prompt sections, approvals, speculative checks, beta tracing, and session message cache.
- `references/claude-code/src/services/compact/postCompactCleanup.ts:17-20` intentionally keeps invoked skill content across compactions because later attachments may need it.

Interpretation: after a structural context replacement, all caches that derive from old active context must be invalidated or deliberately preserved. AgentDash should apply this idea to projection/context caches and command availability projections, using its own stores.

### Answers To The Four Questions

#### 1. How does Claude Code handle manual/automatic compact lifecycle? Is it maintained as an independent agent action/turn?

Manual compact:

- It starts as `/compact`, a local command (`commands/compact/index.ts:4-13`), with a dedicated compact result variant (`types/command.ts:16-24`).
- The command reads the active context after the latest compact boundary (`commands/compact/compact.ts:40-47`), runs session-memory or legacy summary compaction (`commands/compact/compact.ts:55-124`), and returns `type: "compact"` rather than a normal assistant text result.
- The summary itself can be generated by a forked internal agent query (`services/compact/compact.ts:1188-1200`) with `querySource: "compact"`, `forkLabel: "compact"`, `maxTurns: 1`, no tool use, and the same abort controller.
- The outer command then converts the result into `[compact_boundary, compact_summary, kept_messages, attachments, hook_results]` and sets `shouldQuery: false` (`processSlashCommand.tsx:678-704`).

Automatic compact:

- It runs before the main provider query (`query.ts:453-467`) when threshold logic says compaction is needed (`autoCompact.ts:267-277`).
- On success, it yields the same post-compact messages and replaces `messagesForQuery` before continuing the current main query (`query.ts:528-535`).
- On failure, it increments a consecutive failure count and can trip a circuit breaker (`autoCompact.ts:257-265`, `autoCompact.ts:334-350`).

Conclusion:

Claude Code does maintain compaction as a distinct action with its own progress/status and summary-generation query. For manual compact, it is not a normal assistant turn; it is a local command that may spawn an internal one-turn/forked summarizer action and then structurally replaces the conversation context. For auto compact, it is a pre-query maintenance step inside the current user turn.

AgentDash implication:

AgentDash should treat compact-only as `ExecutionTurnMode::ContextCompaction` maintenance, which it already starts to do in `launch/plan.rs:325-330`. The important boundary is that this maintenance turn's success is a projection checkpoint commit, not the presence of a system delivery prompt or a user-visible command receipt alone.

#### 2. How does Claude Code distinguish no need to compact, insufficient history restoration, summary generation failure, and user cancellation?

No need / not enough active context:

- Manual `/compact` throws `No messages to compact` when active post-boundary messages are empty (`commands/compact/compact.ts:44-50`).
- Core compaction throws `ERROR_MESSAGE_NOT_ENOUGH_MESSAGES` on empty messages (`services/compact/compact.ts:397-399`).
- Auto compact returns `{ wasCompacted: false }` if disabled, below threshold, or circuit breaker prevents attempts (`autoCompact.ts:253-277`).

Insufficient or broken history restoration:

- Claude Code's normal protection is structural: active context is derived from compact boundaries (`utils/messages.ts:4618-4656`), and persisted transcript parent chains are cut at `compact_boundary` (`sessionStorage.ts:1025-1041`).
- When preserved-segment relinking cannot prove tail-to-head continuity, `applyPreservedSegmentRelinks` logs diagnostics and returns without pruning, so resume loads full pre-compact history (`sessionStorage.ts:1888-1902`).
- When UI scrollback contains messages no longer in active context, REPL shows an explicit warning instead of silently summarizing them (`REPL.tsx:4918-4929`).

Summary generation failure:

- Prompt-too-long after retries throws `ERROR_MESSAGE_PROMPT_TOO_LONG` (`services/compact/compact.ts:462-478`).
- No summary text logs `tengu_compact_failed` with `reason: "no_summary"` and throws (`services/compact/compact.ts:493-506`).
- API-error-looking summary text logs `reason: "api_error"` and throws (`services/compact/compact.ts:507-515`).
- Streaming fallback with no response logs `reason: "no_streaming_response"` and throws `ERROR_MESSAGE_INCOMPLETE_RESPONSE` (`services/compact/compact.ts:1375-1388`).
- Auto compact converts these into `{ wasCompacted: false, consecutiveFailures }` (`autoCompact.ts:334-350`).

User cancellation:

- The compact command checks `abortController.signal.aborted` and maps it to `Compaction canceled.` (`commands/compact/compact.ts:125-128`).
- The forked summarizer receives the same abort controller (`services/compact/compact.ts:1196-1200`).
- The streaming fallback passes `signal: context.abortController.signal` (`services/compact/compact.ts:1302-1308`).
- The forked summarizer guards `assistantMsg.isApiErrorMessage` so an abort-produced synthetic assistant message cannot be committed as a summary (`services/compact/compact.ts:1205-1210`).
- Cleanup of progress/status runs in `finally` (`services/compact/compact.ts:749-762`).

Conclusion:

Claude Code distinguishes these states through layered control flow rather than one global durable request status enum. But the semantic distinction is strong: skip/not-needed, restore degradation, summary failure, and cancellation are not all collapsed into "no eligible messages".

AgentDash implication:

AgentDash should not map structural restore/ref failures to `no_eligible_messages`. The task design's structured eligibility split (`design.md:96-122`) matches the Claude Code lesson. Useful stable reason codes for AgentDash would be:

| Condition | Suggested AgentDash result |
| --- | --- |
| Complete restored context but no legal compactable prefix | `noop`, reason `no_eligible_messages` |
| Restored context unexpectedly empty for compact-only | `failed`, reason `compaction_empty_restored_context` |
| `message_refs.len() != messages.len()` | `failed`, reason `compaction_message_ref_len_mismatch` |
| cut boundary `compacted_until_ref` missing | `failed`, reason `compaction_compacted_until_ref_missing` |
| first-kept ref missing when required | `failed`, reason `compaction_first_kept_ref_missing` |
| summarizer/provider produced no valid summary | `failed`, reason `compaction_summary_generation_failed` |
| user cancel/abort after request consumed | `failed`, reason `cancelled` or `compaction_cancelled` |
| projection commit failed | `failed`, reason `projection_commit_failed` |

#### 3. How are compact results written back into later model input, and how does Claude Code avoid UI/command status splitting from actual context?

Claude Code uses the post-compact message array as the writeback unit:

- `CompactionResult` contains the compact boundary and summary messages (`services/compact/compact.ts:299-310`).
- `buildPostCompactMessages` is the single ordering function for boundary, summary, kept messages, attachments, and hooks (`services/compact/compact.ts:326-338`).
- Manual compact returns those messages from slash command processing and disables a follow-up model query (`processSlashCommand.tsx:700-704`).
- Headless/SDK mode yields compact summary and compact boundary messages, then persists the transcript (`QueryEngine.ts:556-616`).
- Auto compact yields the same messages and assigns `messagesForQuery = postCompactMessages` before continuing provider execution (`query.ts:528-535`).
- Active context slicing is based on latest `compact_boundary` (`utils/messages.ts:4618-4656`).
- Transcript persistence cuts the physical chain at the compact boundary while retaining logical parent linkage (`sessionStorage.ts:1025-1041`).

This design keeps three surfaces aligned:

| Surface | Source in Claude Code |
| --- | --- |
| Later model input | `getMessagesAfterCompactBoundary` plus post-compact message array |
| UI/transcript display | compact boundary and `isCompactSummary` messages |
| Resume/recovery | compact boundary in transcript storage and parent-chain relink rules |

AgentDash implication:

AgentDash already has the right equivalent primitive: `ContextProjector`. `eventing.rs:479-487` builds projected transcript via `build_agent_context_envelope`, and `eventing.rs:493-498` delegates model-context construction to `ContextProjector::build_model_context`. The successful compact commit path writes a `SessionCompactionRecord`, summary segment, and projection head (`eventing.rs:795-890`) and then marks the manual request completed after projection commit (`eventing.rs:909-920`, `eventing.rs:925-989`).

The repair should make command receipt, request row, lifecycle event, and projection head converge on this single source of truth:

- The command receipt should keep the maintenance `turn_id` after launch, even if short polling sees an immediate terminal request state.
- The manual request should reach `completed`, `noop`, or `failed` with stable metadata tied to the same request id and lifecycle item id.
- Later model input should be built only from `ContextProjector`/projection heads, not from UI timeline shape or the existence of a system delivery item.
- Success should be acknowledged only after the projection checkpoint is committed.

#### 4. Which designs should AgentDash absorb, and which would be over-engineering?

Designs AgentDash should absorb:

1. Compact is a maintenance lifecycle, not an ordinary prompt side effect.
   - Claude Code has a command-level compact result and a compaction-scoped summary query.
   - AgentDash should keep `LaunchSource::ContextCompaction` and `ExecutionTurnMode::ContextCompaction` (`launch/plan.rs:325-330`) as the explicit maintenance boundary.

2. The durable context replacement is the success boundary.
   - Claude Code's durable replacement is boundary plus summary messages.
   - AgentDash's durable replacement should be projection commit: `runtime_session_compactions`, `runtime_session_projection_segments`, and `runtime_session_projection_heads` (`eventing.rs:795-890`), matching the spec (`context-compaction-projection.md:11-20`).

3. Model input and UI/command state need one shared fact source.
   - Claude Code shares post-compact messages across model input, transcript, and UI.
   - AgentDash should share projection commit/request/lifecycle ids across command receipt, request status, and context projection. Current short-poll behavior loses this when `NoEligibleMessages` stores `turn_id: None` (`context_compaction_command.rs:470-481`).

4. Noop must stay narrow.
   - Claude Code's skip/not-needed path is distinct from failed summary generation and abort.
   - AgentDash should replace `should_execute_compaction(...) -> bool` (`compaction/mod.rs:145-160`) with structured eligibility so ref integrity failures are `failed`, not `noop`.

5. Cancellation must not become a fake summary or dangling request.
   - Claude Code passes the same abort controller to compact summary generation and prevents abort text from becoming summary (`compact.ts:1196-1210`).
   - AgentDash should call `after_compaction_failed` for manual cancel/abort after request consumption. Current preflight skips the delegate failure call for `AgentError::Cancelled` and returns the error (`streaming.rs:847-875`), which can leave consumed manual requests without the intended terminal request state.

6. Post-compact cache/projection invalidation should be tied to structural replacement.
   - Claude Code runs cleanup after compact (`postCompactCleanup.ts:31-77`).
   - AgentDash should invalidate/rebuild context projection and command availability from the committed projection/request state, not from transient UI or launch status.

Over-engineering for AgentDash:

1. Claude Code's JSONL transcript chain mechanics.
   - `parentUuid: null`, `logicalParentUuid`, preserved-segment relinking, byte scanning, and pruning behavior (`sessionStorage.ts:993-1070`, `sessionStorage.ts:1823-1905`) are file-log-specific. AgentDash already has database-backed projection records and `ContextProjector`.

2. Prompt-cache-sharing fork complexity.
   - `runForkedAgent` with cache-sharing gates and fallback streaming (`compact.ts:1136-1248`) is optimized for Claude Code process/provider economics. AgentDash should keep summary generation through its existing provider bridge and projection contract unless a measured need appears.

3. Multiple compact variants.
   - Claude Code has session-memory compaction, microcompact, reactive compact, snip projection, context collapse, and many feature gates. This task only needs summary-prefix structural compact, as already scoped in `design.md:147-154`.

4. Extra durable run/window tables.
   - Existing research already notes Codex window chains are unnecessary because AgentDash has projection version/head identity (`research/compaction-lifecycle-reference-notes.md:54-62`). Claude Code's compact boundary mechanism also should not become a parallel AgentDash boundary table.

5. Compatibility status proliferation.
   - The project is pre-release. The clean target state should be a small request status machine plus stable result metadata, consistent with `prd.md:21` and `design.md:21-26`.

### Current AgentDash Risk Points

These are the concrete local code positions most relevant to the repair:

- `crates/agentdash-agent/src/compaction/mod.rs:145-160` uses `should_execute_compaction(...) -> bool`, returning `false` for length mismatch and missing boundary refs. This is the current root of structural failures becoming `no_eligible_messages`.
- `crates/agentdash-agent/src/agent_loop/streaming.rs:751-779` maps `!should_execute_compaction` to `ContextCompactionNoop` with reason `no_eligible_messages`.
- `crates/agentdash-agent/src/agent_loop/streaming.rs:824-846` maps `execute_compaction` returning `Ok(None)` to the same noop reason.
- `crates/agentdash-agent/src/agent_loop/streaming.rs:847-875` emits `ContextCompactionFailed` on errors, but for cancellation it skips `after_compaction_failed` and returns `Err(error)`.
- `crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs:187-237` launches compact-only turn, waits only 750ms, and maps request `Noop` into `NoEligibleMessages`.
- `crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs:451-487` stores `LaunchedCompactionTurn` with `turn_id`, but stores `NoEligibleMessages` with `turn_id: None`.
- `crates/agentdash-application-runtime-session/src/session/manual_compaction_delegate.rs:109-140` consumes manual request and returns manual compaction params.
- `crates/agentdash-application-runtime-session/src/session/manual_compaction_delegate.rs:159-189` marks manual compaction failed when `after_compaction_failed` receives manual metadata.
- `crates/agentdash-application-runtime-session/src/session/manual_compaction_delegate.rs:191-220` marks manual compaction noop.
- `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:188-195` wraps runtime compaction delegate with `ManualContextCompactionDelegate`.
- `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:196-230` can use `RepositoryRehydrate(ExecutorState)` and build `RestoredSessionState` from projected transcript.
- `crates/agentdash-application-runtime-session/src/session/launch/plan.rs:325-330` sets `ExecutionTurnMode::ContextCompaction` for `LaunchSource::ContextCompaction`.
- `crates/agentdash-application-runtime-session/src/session/eventing.rs:479-498` builds model context through `ContextProjector`.
- `crates/agentdash-application-runtime-session/src/session/eventing.rs:795-890` constructs compaction record, summary segment, and projection head in the commit object.
- `crates/agentdash-application-runtime-session/src/session/eventing.rs:892-920` commits projection and only then marks the manual request completed.
- `crates/agentdash-application-runtime-session/src/session/eventing.rs:991-1029` marks manual request failed if projection commit fails.
- `crates/agentdash-infrastructure/migrations/0059_manual_context_compaction_requests.sql:20-54` defines current request statuses: `requested`, `consumed`, `completed`, `noop`, `failed`, and modes `next_turn`, `compact_only`.

### AgentDash Repair Recommendations

1. Replace bool preflight eligibility with structured classification.

`should_execute_compaction` should become or be backed by a diagnostic classifier:

```rust
pub enum CompactionEligibility {
    Eligible,
    NoEligibleMessages {
        message_count: usize,
        keep_last_n: u32,
    },
    InvalidInput {
        reason: CompactionEligibilityFailure,
        message_count: usize,
        ref_count: usize,
    },
}
```

This classifier should distinguish at least:

- `MessageRefLengthMismatch`
- `CompactedUntilRefMissing`
- `FirstKeptRefMissing`
- `EmptyRestoredContext` for compact-only restore if the restored envelope is unexpectedly empty

2. Make compact-only preflight terminal semantics explicit.

`run_compaction_preflight` should return a maintenance outcome for compact-only execution:

```rust
pub enum CompactionPreflightOutcome {
    NotRequested,
    Noop { reason: String },
    Completed,
    Failed { reason: String },
}
```

For normal provider turns, automatic compaction failure can remain a failed lifecycle diagnostic while the main turn continues, except cancellation. For compact-only turns, failed compaction is the failed purpose of the turn.

3. Restore compact-only input from projection before eligibility.

The compact-only launch path should select `RepositoryRehydrate(ExecutorState)` whenever there is no live runtime state but the session has durable history, matching `design.md:64-72`. The restored messages and refs must be non-empty and aligned before calling the compaction classifier.

4. Finalize consumed manual requests on every terminal path.

Manual request consumption occurs in `manual_compaction_delegate.rs:127-139`. After that point:

- Success writes `completed` only after projection commit.
- True no-eligible writes `noop`.
- Invalid input, restore failure, summary failure, projection failure, and cancellation write `failed`.

The current cancellation skip in `streaming.rs:859-875` should be adjusted for manual compact metadata so consumed requests do not stay unresolved.

5. Preserve maintenance `turn_id` in command receipts.

`launch_compact_only_turn` should return the launched `turn_id` even if short polling observes immediate `Noop`, `Completed`, or `Failed`. Command results for `NoEligibleMessages` and `Failed` should still carry `turn_id` and `request_id`, matching `design.md:134-145`.

6. Treat projection commit as the only successful context replacement.

`eventing.rs:795-920` already has the right shape. The command/status repair should align around that boundary:

- `context_compacted` event without projection commit is not success.
- request `completed` without projection refs is not success.
- command receipt acceptance without request terminal state is only launch acceptance.

7. Keep the durable source of truth in AgentDash projection stores.

Claude Code's compact boundary is a useful semantic reference, but AgentDash should continue using:

- `runtime_session_compactions`
- `runtime_session_projection_segments`
- `runtime_session_projection_heads`
- `ContextProjector`

This avoids a second boundary mechanism and keeps resume/fork/rollback aligned with existing specs.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task and no source. I used the user-provided active task path `.trellis/tasks/07-09-manual-context-compaction-execution` and wrote only under that task's `research/` directory.
- This research used the local `references/claude-code` source snapshot. I did not verify an external Claude Code version, release tag, or public documentation.
- `references/claude-code/src/services/compact/reactiveCompact.ts` was referenced by imports but not present in the inspected local tree, so reactive compact details are limited to call sites and error mapping in `commands/compact/compact.ts` and `query.ts`.
- Claude Code is a local CLI with append-only JSONL transcript mechanics, while AgentDash has database-backed session events and projection stores. Recommendations compare lifecycle semantics, not one-to-one storage design.
- Existing `research/compaction-lifecycle-reference-notes.md` already covers Codex and a shorter Claude Code summary. This file expands the Claude Code-specific lifecycle/status/writeback analysis requested here.
- No code, spec, migration, or non-research task files were modified.
