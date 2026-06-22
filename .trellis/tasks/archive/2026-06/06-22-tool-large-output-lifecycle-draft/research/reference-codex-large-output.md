# Research: reference-codex-large-output

- Query: references/codex large terminal/tool output handling: exec output truncation, tool result handling, transcript/session persistence, rollout/resume, artifact/local storage.
- Scope: internal
- Date: 2026-06-22

## Findings

### Files Found

- `references/codex/codex-rs/core/src/exec.rs` - shell exec capture policy, retained output byte cap, live output delta cap, stdout/stderr aggregation.
- `references/codex/codex-rs/core/src/unified_exec/mod.rs` - unified exec output constants and default model-facing max output tokens.
- `references/codex/codex-rs/core/src/unified_exec/head_tail_buffer.rs` - head/tail retained transcript buffer for unified exec.
- `references/codex/codex-rs/core/src/unified_exec/async_watcher.rs` - unified exec live output delta emission and final `ExecCommandEnd` aggregation.
- `references/codex/codex-rs/core/src/tools/mod.rs` - model-facing exec output formatting and truncation.
- `references/codex/codex-rs/core/src/tools/context.rs` - `ExecCommandToolOutput` and `McpToolOutput` conversion into model response items.
- `references/codex/codex-rs/core/src/tools/events.rs` - tool lifecycle events, `ExecCommandEndEvent` payload construction, persisted `aggregated_output` source.
- `references/codex/codex-rs/core/src/context_manager/history.rs` - conversation history recording and re-truncation of function/custom tool outputs.
- `references/codex/codex-rs/core/src/mcp_tool_call.rs` - MCP result sanitization, model output path, event/rollout truncation path.
- `references/codex/codex-rs/core/src/mcp_tool_call_tests.rs` - tests proving large MCP event results collapse into bounded preview and drop structured/meta payloads.
- `references/codex/codex-rs/app-server-protocol/src/protocol/event_mapping.rs` - app-server notification mapping for exec deltas/end events.
- `references/codex/codex-rs/app-server-protocol/src/protocol/item_builders.rs` - UI `ThreadItem::CommandExecution` reconstruction from `ExecCommandEndEvent`.
- `references/codex/codex-rs/app-server-protocol/src/protocol/thread_history.rs` - persisted rollout event replay into app-server turns/history.
- `references/codex/codex-rs/rollout/src/recorder.rs` - JSONL rollout writer/reader and resume loading.
- `references/codex/codex-rs/rollout/src/policy.rs` - persistence allowlist for rollout items/events.
- `references/codex/codex-rs/rollout/src/compression.rs` - cold rollout `.jsonl.zst` compression/materialization.
- `references/codex/codex-rs/rollout/src/metadata.rs` - SQLite metadata extraction/backfill from rollout JSONL.
- `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs` - resume/fork reconstruction from rollout items and compaction replacement history.
- `references/codex/codex-rs/thread-store/README.md` - documented boundary: JSONL is canonical history, SQLite is queryable metadata.
- `.trellis/spec/backend/session/context-compaction-projection.md` - AgentDash current direction for durable facts, projection segments, lifecycle recall files.
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/implement.md` - current AgentDash execution plan to map Codex learnings onto phases.

### Terminal / Exec Output Limits

Codex has two exec paths with related but not identical boundaries.

Non-PTY shell exec uses `ExecCapturePolicy::ShellTool` to cap retained stdout/stderr/aggregated bytes at `EXEC_OUTPUT_MAX_BYTES`, which is `codex_utils_pty::DEFAULT_OUTPUT_BYTES_CAP` (`1024 * 1024`, 1 MiB). The cap is applied while reading process output by `read_output(..., max_bytes)` and `append_capped(...)`, which continues draining to EOF after the retained buffer stops growing. This avoids child process back-pressure while bounding memory and stored output (`references/codex/codex-rs/core/src/exec.rs:65`, `references/codex/codex-rs/core/src/exec.rs:69`, `references/codex/codex-rs/core/src/exec.rs:128`, `references/codex/codex-rs/core/src/exec.rs:279`, `references/codex/codex-rs/core/src/exec.rs:1487`, `references/codex/codex-rs/core/src/exec.rs:1529`).

The live shell output stream emits at most `MAX_EXEC_OUTPUT_DELTAS_PER_CALL = 10_000` delta events. Each read chunk is 8192 bytes, so the live event stream is bounded by event count even though the process is fully drained (`references/codex/codex-rs/core/src/exec.rs:61`, `references/codex/codex-rs/core/src/exec.rs:71`, `references/codex/codex-rs/core/src/exec.rs:1507`).

For stdout/stderr aggregation under contention, `aggregate_output()` prefers a bounded combined output: if the total exceeds the byte cap, it reserves roughly one third for stdout and two thirds for stderr, then rebalances unused stderr budget back to stdout. This produces a single retained `aggregated_output` string, not a full-output artifact reference (`references/codex/codex-rs/core/src/exec.rs:890`).

Unified exec uses its own constants: `DEFAULT_MAX_OUTPUT_TOKENS = 10_000`, `UNIFIED_EXEC_OUTPUT_MAX_BYTES = 1024 * 1024`, and `UNIFIED_EXEC_OUTPUT_MAX_TOKENS = UNIFIED_EXEC_OUTPUT_MAX_BYTES / 4` (`references/codex/codex-rs/core/src/unified_exec/mod.rs:70`). Its live delta chunks are capped at `UNIFIED_EXEC_OUTPUT_DELTA_MAX_BYTES = 8192`, and delta emission still stops after the shared `MAX_EXEC_OUTPUT_DELTAS_PER_CALL` (`references/codex/codex-rs/core/src/unified_exec/async_watcher.rs:29`, `references/codex/codex-rs/core/src/unified_exec/async_watcher.rs:175`).

Unified exec retains a 1 MiB head/tail transcript in `HeadTailBuffer`. The buffer splits capacity 50/50 between stable prefix and suffix, drops the middle, and `to_bytes()` concatenates retained head and tail without an explicit omitted-bytes marker in the returned bytes (`references/codex/codex-rs/core/src/unified_exec/head_tail_buffer.rs:4`, `references/codex/codex-rs/core/src/unified_exec/head_tail_buffer.rs:31`, `references/codex/codex-rs/core/src/unified_exec/head_tail_buffer.rs:104`). `async_watcher::resolve_aggregated_output()` uses this retained byte snapshot for final `aggregated_output` (`references/codex/codex-rs/core/src/unified_exec/async_watcher.rs:191`, `references/codex/codex-rs/core/src/unified_exec/async_watcher.rs:320`).

### Text Sent To Model After Truncation

Plain shell tool model output is produced by `format_exec_output_for_model()`. It builds:

```text
Exit code: <code>
Wall time: <seconds> seconds
Total output lines: <n>   # only if truncation changed line count
Output:
<truncated output>
```

The body uses `truncate_text(content, truncation_policy)`, not the raw retained output (`references/codex/codex-rs/core/src/tools/mod.rs:60`).

`format_exec_output_str()` is the flattened form used for hooks/events that need a single string. It calls `formatted_truncate_text()`, which prepends `Total output lines: <n>` only if `content.len() > policy.byte_budget()` and then returns a middle-truncated result (`references/codex/codex-rs/core/src/tools/mod.rs:89`, `references/codex/codex-rs/utils/output-truncation/src/lib.rs:12`).

Unified `exec_command` returns an `ExecCommandToolOutput`. Its model response item calls `response_text()`, which includes optional `Chunk ID`, `Wall time`, exit/running/session/original-token metadata, then `Output:` and `truncated_output(model_output_max_tokens())`. The model token cap is `resolve_max_tokens(self.max_output_tokens).min(self.truncation_policy.token_budget())` (`references/codex/codex-rs/core/src/tools/context.rs:307`, `references/codex/codex-rs/core/src/tools/context.rs:331`, `references/codex/codex-rs/core/src/tools/context.rs:399`, `references/codex/codex-rs/core/src/tools/context.rs:409`).

The actual truncation helper behavior is:

- `formatted_truncate_text()` adds `Total output lines: <count>` before the truncated body when content exceeds the policy byte budget.
- `truncate_text()` delegates to byte middle truncation or token-budget middle truncation.
- content-item tool outputs keep images/encrypted content but apply remaining budget across text items; omitted text items become a marker of the form `[omitted N text items ...]`.

Relevant code: `references/codex/codex-rs/utils/output-truncation/src/lib.rs:12`, `references/codex/codex-rs/utils/output-truncation/src/lib.rs:22`, `references/codex/codex-rs/utils/output-truncation/src/lib.rs:79`.

### Tool Result Handling Beyond Terminal

`McpToolOutput.response_payload()` converts MCP `CallToolResult` into a function-call output payload, inserts:

```text
Wall time: <seconds> seconds
Output:
```

then applies `truncate_function_output_payload(&payload, self.truncation_policy * 1.2)`. This is explicitly the model context-injection form; code-mode consumers still get the raw `CallToolResult` (`references/codex/codex-rs/core/src/tools/context.rs:65`, `references/codex/codex-rs/core/src/tools/context.rs:88`, `references/codex/codex-rs/core/src/tools/context.rs:110`).

MCP has a separate event/rollout truncation path. `handle_mcp_tool_call()` returns the raw/sanitized `result` to the model-side `HandledMcpToolCall`, but passes `truncate_mcp_tool_result_for_event(&result)` into `notify_mcp_tool_call_completed()` for UI/rollout lifecycle (`references/codex/codex-rs/core/src/mcp_tool_call.rs:389`, `references/codex/codex-rs/core/src/mcp_tool_call.rs:411`).

`truncate_mcp_tool_result_for_event()` serializes the full `CallToolResult`; if it exceeds `MCP_TOOL_CALL_EVENT_RESULT_MAX_BYTES` (also 1 MiB), it collapses the event copy into one text content preview of the serialized result, drops `structured_content`, drops `meta`, and preserves `is_error`. Errors are truncated as strings (`references/codex/codex-rs/core/src/mcp_tool_call.rs:107`, `references/codex/codex-rs/core/src/mcp_tool_call.rs:808`). Tests assert the serialized event copy remains under roughly `2 * cap + 1024`, `structured_content == None`, `meta == None`, and the preview text contains `truncated` (`references/codex/codex-rs/core/src/mcp_tool_call_tests.rs:978`).

Image support is a separate sanitization boundary: when the model does not support image input, `sanitize_mcp_tool_result_for_model()` replaces image content with text `<image content omitted because you do not support image input>` before returning the model-side result (`references/codex/codex-rs/core/src/mcp_tool_call.rs:777`, `references/codex/codex-rs/core/src/mcp_tool_call_tests.rs:900`).

### Transcript / Session Persistence

Codex persists canonical session rollouts as JSONL. `RolloutRecorder` writes `RolloutItem` records and can load them back with `load_rollout_items()`, parsing one JSON line at a time (`references/codex/codex-rs/rollout/src/recorder.rs:66`, `references/codex/codex-rs/rollout/src/recorder.rs:790`, `references/codex/codex-rs/rollout/src/recorder.rs:846`).

The rollout persistence allowlist includes `ResponseItem::FunctionCallOutput`, `ResponseItem::CustomToolCallOutput`, `ResponseItem::LocalShellCall`, `ResponseItem::McpToolCallEnd`, `EventMsg::ContextCompacted`, `EventMsg::ThreadRolledBack`, and selected turn/message events. It explicitly does not persist high-volume incremental events such as `ExecCommandOutputDelta`, `ExecCommandBegin`, `TerminalInteraction`, `RawResponseItem`, `AgentMessageContentDelta`, reasoning deltas, hook lifecycle deltas, etc. (`references/codex/codex-rs/rollout/src/policy.rs:5`, `references/codex/codex-rs/rollout/src/policy.rs:75`).

This means live UI can see deltas, but JSONL history does not keep every delta. For exec completion, JSONL may keep the final completion event if it is one of the persisted compatibility items, and app-server history can reconstruct a `ThreadItem::CommandExecution` from the event's `aggregated_output`.

Cold local rollout files are compressed in the background to `.jsonl.zst` once old enough. Reads transparently open plain or compressed rollout files; appends materialize compressed files back to plain `.jsonl` first (`references/codex/codex-rs/rollout/src/compression.rs:24`, `references/codex/codex-rs/rollout/src/compression.rs:43`, `references/codex/codex-rs/rollout/src/compression.rs:66`).

SQLite is not the transcript source. `thread-store/README.md` states `LocalThreadStore` persists history through `codex-rollout` JSONL files and queryable metadata through SQLite. `RolloutRecorder` writes canonical history, while metadata writes are separate (`references/codex/codex-rs/thread-store/README.md:7`, `references/codex/codex-rs/thread-store/README.md:22`). `rollout/src/metadata.rs` backfills SQLite metadata by reading rollout JSONL and applying rollout items to a metadata builder, so SQLite is an index/cache for listing/filtering, not where large transcript payloads should live (`references/codex/codex-rs/rollout/src/metadata.rs:94`, `references/codex/codex-rs/rollout/src/metadata.rs:133`).

### Resume / Rollout Reconstruction

Resume uses rollout JSONL, not a separate full-output artifact. `RolloutRecorderParams::Resume` opens/materializes the existing rollout path for append (`references/codex/codex-rs/rollout/src/recorder.rs:93`, `references/codex/codex-rs/rollout/src/recorder.rs:726`).

`Session::reconstruct_history_from_rollout()` rebuilds model history from persisted `RolloutItem`s. It scans newest-to-oldest to find replacement-history checkpoints from compaction, previous turn settings, reference context, rollback markers, and the surviving suffix. Then it replays the suffix forward into a fresh `ContextManager` (`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:93`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:118`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:261`).

When replaying `ResponseItem`s into `ContextManager`, it calls `history.record_items(..., turn_context.truncation_policy)`. `ContextManager::record_items()` calls `process_item()`, and `process_item()` re-truncates `FunctionCallOutput` and `CustomToolCallOutput` using `truncate_function_output_payload(policy * 1.2)` (`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:269`, `references/codex/codex-rs/core/src/context_manager/history.rs:90`, `references/codex/codex-rs/core/src/context_manager/history.rs:338`).

Compaction stores replacement history inside `RolloutItem::Compacted`; reconstruction can use the newest surviving `replacement_history` as a complete model-history base, then replay only newer rollout suffix (`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:63`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:134`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:263`).

Thread rollback is represented by `EventMsg::ThreadRolledBack` and applied during rollout truncation/reconstruction by user-turn boundaries (`references/codex/codex-rs/core/src/thread_rollout_truncation.rs:32`, `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:308`).

### UI / App-Server Text

The app-server notification stream maps `ExecCommandOutputDelta` chunk bytes directly to `CommandExecutionOutputDeltaNotification.delta` by `String::from_utf8_lossy(...)`. The chunk size is bounded upstream for unified exec, and legacy shell emits at most the configured number of deltas (`references/codex/codex-rs/app-server-protocol/src/protocol/event_mapping.rs:433`).

For final command cards, `build_command_execution_end_item()` puts `payload.aggregated_output.clone()` into `ThreadItem::CommandExecution.aggregated_output` if non-empty. There is no additional truncation in the app-server builder; it trusts the core event payload already to be bounded (`references/codex/codex-rs/app-server-protocol/src/protocol/item_builders.rs:108`).

`build_turns_from_rollout_items()` rebuilds UI turns from persisted rollout items. Tests show an `ExecCommandEndEvent` with `aggregated_output: "hello world\n"` becomes a `ThreadItem::CommandExecution { aggregated_output: Some("hello world\n"), ... }` (`references/codex/codex-rs/app-server-protocol/src/protocol/thread_history.rs:74`, `references/codex/codex-rs/app-server-protocol/src/protocol/thread_history.rs:2069`).

### External References / Versions

No external references were used. This research is based only on the local `references/codex` snapshot plus the project Trellis indexes/specs required by the research workflow.

### Related Specs

- `.trellis/spec/index.md` says task research belongs under `.trellis/tasks/`, while long-term structural facts belong in `.trellis/spec/`.
- `.trellis/spec/backend/session/context-compaction-projection.md` already defines an AgentDash direction where `session_events` are durable facts, model context is a projection, and future `session_projection_segments` may include `tool_result_digest` and `artifact_reference`.
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/implement.md` currently proposes `LargeOutputPolicy`, `LargeOutputRef`, typed `AgentToolResult.details.large_output`, cloud cache, terminal owner storage, lifecycle VFS refs, and bounded DB/session events.

###重点问题回答

**它如何限制 terminal/tool 输出？**

- Shell exec: 1 MiB retained byte cap (`DEFAULT_OUTPUT_BYTES_CAP`) for stdout/stderr/aggregated output; keeps draining to EOF.
- Legacy shell live deltas: max 10,000 `ExecCommandOutputDelta` events.
- Unified exec: 1 MiB head/tail retained transcript; per live delta max 8192 bytes; max 10,000 deltas.
- Model text: exec and MCP tool outputs go through `TruncationPolicy` before becoming model-visible response items.
- MCP event/UI copy: if serialized `CallToolResult` exceeds 1 MiB, collapse to one text preview and drop `structured_content`/`meta`.

**截断后给模型和 UI 的文本是什么？**

- Exec model text includes exit/wall-time metadata and `Output:`; optionally `Total output lines: <n>`, then a middle-truncated body.
- Unified `exec_command` model text includes `Chunk ID`, wall time, exit/running status, optional original token count, then `Output:` and token-budget-truncated output.
- MCP model text includes `Wall time: ... seconds\nOutput:` plus function-output-payload truncation.
- UI command final card receives `CommandExecution.aggregated_output` from `ExecCommandEndEvent`. That value is already retained/capped upstream, but the app-server builder does not add a truncation marker.
- UI MCP large result receives a text preview of serialized `CallToolResult`; tests assert the preview contains `truncated` and structured/meta are absent.

**原文是否存储？**

- For shell/unified exec, full original command output is generally not stored by these paths. Non-PTY shell keeps at most the retained byte cap; unified exec keeps at most 1 MiB head/tail and discards middle bytes.
- `ExecCommandToolOutput.raw_output` holds raw bytes before model truncation in memory for that tool result, but downstream model text is `response_text()` and persisted history is bounded by capture/truncation policies.
- MCP model return path has raw/sanitized `CallToolResult` before `McpToolOutput.response_payload()` truncation; event/rollout copy is separately bounded. There is no artifact/ref storage for the original large MCP payload in the files inspected.

**resume 是否依赖原文？**

- Resume depends on rollout JSONL `RolloutItem`s plus compaction `replacement_history`, not on external original-output artifacts.
- Reconstructed model history is re-fed through `ContextManager::record_items()` and truncates function/custom tool outputs again. Therefore resume works from the bounded persisted transcript/projection, not from full original terminal/tool output.

**数据库/JSONL 如何避免膨胀？**

- JSONL avoids high-volume event inflation by not persisting deltas such as `ExecCommandOutputDelta`, `ExecCommandBegin`, terminal interactions, raw response items, and content deltas.
- JSONL still stores canonical completion/history items, but those are upstream-bounded by 1 MiB capture/event policies or function-output truncation.
- Cold JSONL files are compressed to `.jsonl.zst`.
- SQLite stores queryable metadata extracted/backfilled from JSONL, not full transcript rows; listing can fall back to filesystem JSONL when DB is unavailable/stale.

## Borrowable Points

- Use one canonical bounded representation before model/history/UI fan-out. Codex is strongest where `ToolOutput.to_response_item()`, `ContextManager.record_items()`, and event copy all apply shared truncation helpers.
- Keep live deltas out of durable transcript. Persist final bounded item facts, not every streaming chunk.
- Bound at multiple layers: read-time retained bytes, live event chunk/count, model-facing token policy, event/UI serialized payload policy, telemetry preview policy.
- Make SQLite an index/metadata store, not the transcript blob store.
- Compress cold JSONL/append-only logs if local transcript files remain part of the architecture.
- Re-run truncation during resume/projection reconstruction, so old or mixed histories cannot bypass newer limits.
- For structured tool results, collapse large event copies to a text preview and explicitly drop bulky structured/meta fields.

## Non-Borrowable Points

- Codex mostly discards full terminal output. AgentDash's current plan requires user-readable full output via lifecycle refs / owner storage; directly copying Codex would fail that requirement.
- Codex's final command UI `aggregated_output` can be silently head/tail or byte capped without a typed ref/status. AgentDash should expose explicit `LargeOutputTruncation` and `LargeOutputRef` metadata so UI and lifecycle reads can explain what is missing.
- Codex has separate model/event/code-mode paths for MCP; code-mode and post-tool-use can still see raw result. AgentDash should avoid hidden raw bypasses unless deliberately scoped and tested.
- Codex uses string markers (`Total output lines`, `truncated`, omitted text item marker) instead of a typed large-output schema. AgentDash already has typed contract goals, so marker-only detection should not be copied.
- Codex's unified head/tail `to_bytes()` omits middle bytes without inserting an omitted-byte marker into the returned aggregated output. AgentDash should keep preview/ref metadata explicit.

## Suggestions For AgentDash implement.md

- Phase 0 should define exact caps separately for: model preview tokens, DB/event JSON byte budget, live stream chunk byte cap, live stream total retained cap, terminal owner full-output storage cap/expiry, and telemetry preview cap.
- Phase 0 should require `LargeOutputTruncation` to carry `original_bytes`, `retained_bytes`, `omitted_bytes`, `preview_strategy` (`head_tail`, `prefix`, `middle`), and `has_full_ref`.
- Phase 2/3 should add a final append/session-event guard similar in spirit to Codex's event MCP truncation: every tool result event must be bounded even if a specific runtime forgets to guard.
- Phase 3 should explicitly test that live output delta events are not persisted as full history rows and that final `aggregated_output`/`notification_json` carries only preview/ref metadata.
- Phase 4/5 should keep full terminal output outside cloud DB/session JSON and read it via ranged owner-storage refs. This is the main improvement over Codex's discard-only approach.
- Phase 6 should render truncation from typed metadata, not by searching for text such as `truncated`.
- Verification should include a resume test that reconstructs model context from persisted DB/JSONL facts and proves it contains only bounded preview plus ref metadata, never full original output.

## Caveats / Not Found

- I did not find a Codex artifact/local-storage mechanism that preserves full original terminal output and later resolves it by ref. The inspected Codex implementation is mostly bounded-retention plus JSONL history, not full-output artifact storage.
- I did not inspect outside `references/codex` except required Trellis workflow/spec/task artifacts and README/index-style context.
- `rg` could not run in this PowerShell environment, so searches used `Get-ChildItem` and `Select-String`.
- Some app-server process/command exec processors also mention `DEFAULT_OUTPUT_BYTES_CAP`, but the core lifecycle patterns above were sufficient for the requested questions.
