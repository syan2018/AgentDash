# Research: terminal-output-path

- Query: terminal / shell / relay 输出路径与接入大返回防御
- Scope: internal
- Date: 2026-06-22

## Findings

### Files Found

- `.trellis/workflow.md` - Trellis Phase 1 research persistence and planning rules.
- `.trellis/spec/backend/index.md` - backend spec index; routes session, workflow, vfs related contracts.
- `.trellis/spec/backend/session/pi-agent-streaming.md` - PiAgent `AgentEvent` to `BackboneEvent` stream mapper contract.
- `.trellis/spec/backend/session/streaming-protocol.md` - persisted session events over NDJSON protocol.
- `.trellis/spec/cross-layer/backbone-protocol.md` - `BackboneEnvelope` persistence / stream / frontend contract.
- `.trellis/spec/backend/vfs/vfs-access.md` - lifecycle_vfs and runtime tool mount contracts, including `session/terminal`.
- `.trellis/spec/backend/vfs/vfs-materialization.md` - local materialization scope and owner-storage boundaries.
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - lifecycle runtime node / terminal evidence relationship.
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` - frontend lifecycle run / runtime trace indexing contract.
- `crates/agentdash-local/src/shell_session_manager.rs` - local process spawn, retained shell output buffer, relay output events.
- `crates/agentdash-relay/src/protocol/tool.rs` - relay shell payload/response/truncation wire structs.
- `crates/agentdash-relay/src/protocol.rs` - relay message variants for shell exec/read and terminal events.
- `crates/agentdash-relay/src/shell_output_registry.rs` - cloud-side route table for shell streaming output.
- `crates/agentdash-api/src/mount_providers/relay_fs.rs` - cloud VFS `exec` provider sending shell commands to local backend.
- `crates/agentdash-api/src/relay/ws_handler.rs` - relay event ingestion into shell registry and session eventing.
- `crates/agentdash-api/src/relay/registry.rs` - relay session sink routing for terminal/lost events.
- `crates/agentdash-application/src/vfs/tools/fs/shell.rs` - PiAgent-facing `shell_exec` tool result assembly and streaming callback wiring.
- `crates/agentdash-application/src/session/terminal_cache.rs` - cloud in-memory terminal state cache.
- `crates/agentdash-application/src/session/eventing.rs` - session event persistence and broadcast entrypoint.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` - `session_events.notification_json` insert path.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` - session event row deserialization from `notification_json`.
- `crates/agentdash-application/src/session/context_projector.rs` - model-context projection from persisted session events.
- `crates/agentdash-application/src/session/continuation.rs` - rebuild transcript and tool results from Backbone events.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` - PiAgent event to Backbone tool/command mapping.
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs` - tests asserting tool update/end payload mapping behavior.
- `crates/agentdash-application/src/lifecycle/surface/journey/mod.rs` - lifecycle session evidence projection, including `session/terminal`.
- `crates/agentdash-application/src/lifecycle/surface/journey/session_items.rs` - session item grouping and raw event exposure.
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs` - `lifecycle_vfs` read/list implementation for session and node runtime scopes.
- `packages/app-web/src/features/session/model/useSessionStream.ts` - frontend session stream enqueue path.
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts` - terminal platform event dispatch to terminal store.
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts` - terminal platform events excluded from chat feed state.
- `packages/app-web/src/features/session/model/useTerminalStore.ts` - frontend terminal state and output buffer store.
- `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx` - shell command output card and promote-to-terminal behavior.
- `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx` - xterm rendering from terminal output buffer.

### Data Flow

#### A. `shell_exec` tool result path

```text
PiAgent tool call
  -> ShellExecTool.execute
  -> VfsService.exec
  -> RelayFsMountProvider.exec
  -> RelayMessage::CommandToolShellExec
  -> local ShellSessionManager.start_shell
  -> RetainedOutputBuffer snapshot
  -> RelayMessage::ResponseToolShellExec
  -> ExecResult
  -> ShellExecTool shell_exec_result_text + details
  -> AgentToolResult
  -> AgentEvent::ToolExecutionEnd / MessageEnd(tool result)
  -> stream_mapper ItemCompleted(CommandExecution aggregated_output)
  -> SessionEvent notification_json
  -> continuation/model context as ToolResult content
```

Key evidence:

- `ShellExecTool` resolves mount cwd, rewrites VFS URIs, creates optional streaming call id, then calls `VfsService.exec` with `streaming_call_id` (`crates/agentdash-application/src/vfs/tools/fs/shell.rs:180`, `:219`, `:247`).
- `RelayFsMountProvider.exec` sends `CommandToolShellExec` to local backend with `max_output_bytes: None`, then maps `ToolShellExecResponse` to `ExecResult` including `truncated` and `omitted_bytes` (`crates/agentdash-api/src/mount_providers/relay_fs.rs:591`, `:614`, `:623`, `:637`, `:646`).
- Local `start_shell` creates a session, reads a retained snapshot, splits chunks into stdout/stderr/pty, and returns `truncation` metadata (`crates/agentdash-local/src/shell_session_manager.rs:168`, `:191`, `:201`, `:202`, `:213`).
- `shell_exec_result_text` embeds command/cwd/state/session_id/next_seq/output and `output_truncated` into a single text result (`crates/agentdash-application/src/vfs/tools/fs/shell.rs:361`, `:383`, `:389`, `:394`).
- PiAgent loop emits the final tool result as both `ToolExecutionEnd` JSON and `AgentMessage::tool_result_full` with `result.content.clone()` (`crates/agentdash-agent/src/agent_loop/tool_call.rs:621`, `:646`, `:650`).
- Stream mapper converts final `shell_exec` result content into Codex `CommandExecution.aggregated_output` (`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1040`, `:1057`, `:1069`).
- Continuation rebuilds terminal tool results from completed `CommandExecution.aggregated_output` as `ContentPart::text`, so the final shell output re-enters model context unless bounded before persistence/projection (`crates/agentdash-application/src/session/continuation.rs:693`, `:707`, `:711`).

#### B. `shell_exec` streaming update path

```text
local ShellSessionManager.push_output
  -> RelayMessage::EventToolShellOutput(call_id, delta, stream)
  -> ws_handler shell_output_registry.route
  -> ShellExecTool forward task
  -> ToolUpdateCallback(AgentToolResult text delta, details.type=shell_output)
  -> AgentEvent::ToolExecutionUpdate
  -> stream_mapper CommandOutputDelta
  -> SessionEvent notification_json
  -> frontend command card / lifecycle session/terminal projection
```

Key evidence:

- `ShellSessionManager.push_output` sends each output chunk as `EventToolShellOutput` when the session has a `call_id` (`crates/agentdash-local/src/shell_session_manager.rs:512`, `:535`, `:536`, `:540`).
- `ShellOutputRegistry` is only a `call_id -> channel` router; it does not inspect or bound payload size (`crates/agentdash-relay/src/shell_output_registry.rs:14`, `:25`, `:35`).
- `ws_handler` routes `EventToolShellOutput` to the registry and otherwise drops unmatched chunks after logging (`crates/agentdash-api/src/relay/ws_handler.rs:435`, `:436`).
- `ShellExecTool` forwards each registry chunk into `ToolUpdateCallback` as text content with details `type = shell_output` (`crates/agentdash-application/src/vfs/tools/fs/shell.rs:224`, `:231`, `:233`, `:237`).
- PiAgent loop serializes update `AgentToolResult` directly into `AgentEvent::ToolExecutionUpdate.partial_result` (`crates/agentdash-agent/src/agent_loop/tool_call.rs:466`, `:471`, `:473`, `:477`).
- Stream mapper maps shell-output updates to `CommandOutputDelta.delta` without truncation (`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:908`, `:928`, `:930`, `:932`, `:937`).

#### C. Interactive terminal path

```text
frontend terminal tab spawn/input/resize
  -> API terminal routes
  -> relay terminal commands
  -> local ShellSessionManager.spawn_terminal/input/resize
  -> EventTerminalOutput(data) / EventTerminalStateChanged
  -> ws_handler
  -> SessionTerminalCache state lookup/update
  -> BackboneEvent::Platform(TerminalOutput/TerminalStateChanged)
  -> SessionEvent notification_json
  -> NDJSON stream/backlog
  -> frontend dispatchSessionPlatformEvent
  -> useTerminalStore.outputBuffers
  -> terminal-tab xterm write
```

Key evidence:

- `spawn_terminal` creates a PTY session with `terminal_id` and default output cap but no `call_id` (`crates/agentdash-local/src/shell_session_manager.rs:217`, `:224`, `:230`, `:233`, `:235`).
- Local output converts stdout to `Pty` for terminal sessions, retains it in the buffer, and sends `EventTerminalOutput { terminal_id, data }` (`crates/agentdash-local/src/shell_session_manager.rs:519`, `:525`, `:545`, `:546`, `:550`).
- `PlatformEvent::TerminalOutput` carries only `{ terminal_id, data }` and is documented as interactive terminal stream data (`crates/agentdash-agent-protocol/src/backbone/platform.rs:30`).
- `ws_handler` injects terminal output into session eventing as a full `BackboneEnvelope` with `payload.data.clone()` (`crates/agentdash-api/src/relay/ws_handler.rs:444`, `:458`, `:460`, `:462`, `:471`).
- `SessionTerminalCache` stores terminal metadata only, explicitly pure in-memory state; it does not retain output (`crates/agentdash-application/src/session/terminal_cache.rs:7`, `:13`, `:38`, `:63`, `:77`).
- Frontend intercepts terminal platform events before normal React stream state and appends raw data into terminal store (`packages/app-web/src/features/session/model/useSessionStream.ts:125`, `:128`; `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:19`, `:22`).
- `useTerminalStore.outputBuffers` is an unbounded `Map<terminal_id, string>` append store (`packages/app-web/src/features/session/model/useTerminalStore.ts:7`, `:58`, `:62`).
- xterm writes the delta between previous written length and the current output string (`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:56`, `:133`, `:141`, `:143`).

#### D. Lifecycle/session evidence path

```text
session_events notification_json
  -> LifecycleJourneyProjection.session_events
  -> lifecycle_vfs agent_run_session or node_runtime
  -> session/events.json, session/items, session/tools, session/terminal
```

Key evidence:

- `SessionEventingService.persist_notification_inner` appends the envelope to `stores.events.append_event` and broadcasts the persisted event (`crates/agentdash-application/src/session/eventing.rs:133`, `:164`, `:167`, `:171`).
- Postgres persistence serializes the entire `BackboneEnvelope` into `notification_json` (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:347`, `:357`, `:359`, `:364`, `:378`).
- Lifecycle journey loads all session events through `session_persistence.list_all_events` (`crates/agentdash-application/src/lifecycle/surface/journey/mod.rs:71`, `:75`).
- `session/events.json` returns pretty-printed full events; `session/terminal` currently concatenates only `BackboneEvent::CommandOutputDelta`, not interactive `PlatformEvent::TerminalOutput` (`crates/agentdash-application/src/lifecycle/surface/journey/mod.rs:112`, `:168`, `:172`, `:173`).
- `lifecycle_vfs` exposes `session/terminal` in both agent-run-session and node-runtime session projections (`crates/agentdash-application/src/vfs/provider_lifecycle.rs:512`, `:523`, `:549`, `:573`).

### Retained Buffer / Truncation Current State

- Local retained output is per-process, in-memory only. `ShellSession` owns `RetainedOutputBuffer`, and `SessionTerminalCache` on the cloud side only stores terminal state, not output (`crates/agentdash-local/src/shell_session_manager.rs:36`, `:43`; `crates/agentdash-application/src/session/terminal_cache.rs:7`).
- `RetainedOutputBuffer` stores a head vector and tail deque with `head_bytes`, `tail_bytes`, `omitted_bytes`, `omitted_chunks`, and `next_seq` (`crates/agentdash-local/src/shell_session_manager.rs:64`, `:66`, `:70`, `:72`).
- First chunk larger than max bytes is truncated into retained head, while the original chunk is still returned from `push` and can be streamed as full delta (`crates/agentdash-local/src/shell_session_manager.rs:84`, `:90`, `:92`, `:97`, `:117`).
- After head is filled, later chunks enter tail; old tail chunks are dropped while omitted counters advance (`crates/agentdash-local/src/shell_session_manager.rs:100`, `:106`, `:109`, `:114`).
- `chunks_after(after_seq, max_bytes)` bounds read snapshots but only over retained head/tail, not over live relay events (`crates/agentdash-local/src/shell_session_manager.rs:120`, `:127`, `:133`).
- Truncation metadata includes `truncated`, `omitted_bytes`, `omitted_chunks`, and an omitted token estimate (`crates/agentdash-relay/src/protocol/tool.rs:221`, `:224`, `:226`, `:229`; `crates/agentdash-local/src/shell_session_manager.rs:145`, `:147`, `:150`).
- There is a test only for retained-buffer truncation metadata, not for stream/event truncation (`crates/agentdash-local/src/shell_session_manager.rs:953`, `:987`, `:988`).

Conclusion: retained buffer protects final/poll snapshots, but live `EventToolShellOutput`, `CommandOutputDelta`, and `PlatformEvent::TerminalOutput` can still carry unbounded chunk data into `session_events.notification_json`, NDJSON backlog, and frontend terminal buffers.

### Terminal vs Ordinary Tool Result Differences

- Ordinary `AgentToolResult` is a single completion payload with `content`, `is_error`, and optional `details` (`crates/agentdash-agent-types/src/runtime/tool.rs:24`, `:27`, `:30`). Terminal/shell has both completion payload and continuous update streams.
- `fs_read` rejects too-large full reads over 256 KB or 5000 lines and tells the model to use `offset/limit` (`crates/agentdash-application/src/vfs/tools/fs/read.rs:26`, `:187`, `:234`, `:250`). Terminal output cannot be rejected after the process has already produced bytes; it needs preview + ref.
- `fs_grep` and `fs_glob` cap results at tool level (`crates/agentdash-application/src/vfs/tools/fs/grep.rs:166`, `:180`, `:248`; `crates/agentdash-application/src/vfs/tools/fs/glob.rs:126`, `:128`, `:143`). Shell output uses local retained buffer for snapshots, but live chunks bypass that cap.
- Shell command output has stable runtime coordinates: `call_id`, `session_id`, `terminal_id`, `next_seq`, `stream`, process state, and exit code (`crates/agentdash-relay/src/protocol/tool.rs:55`, `:83`, `:241`, `:263`, `:318`). Generic tool output normally only has `tool_call_id/tool_name/result`.
- Interactive terminal is not a model-visible tool result; it is a platform event explicitly routed to xterm and excluded from chat feed state (`crates/agentdash-agent-protocol/src/backbone/platform.rs:30`; `packages/app-web/src/features/session/model/sessionStreamReducer.ts:237`, `:240`).
- The model-context path still consumes shell command terminal results through completed `CommandExecution.aggregated_output`; interactive `PlatformEvent::TerminalOutput` is ignored by continuation, but still persisted and streamed (`crates/agentdash-application/src/session/continuation.rs:693`, `:707`; `crates/agentdash-api/src/relay/ws_handler.rs:444`, `:471`).

### Lifecycle Ref Candidate Design

#### Candidate 1: Guard at PiAgent tool boundary

Add a result guard around both:

- `build_on_update` before serializing `ToolExecutionUpdate.partial_result`.
- `emit_tool_call_outcome` before `ToolExecutionEnd` and before `AgentMessage::tool_result_full`.

The guard should produce:

- bounded model-visible `ContentPart::text` preview;
- structured `details` with `truncated`, `original_bytes`, `inline_bytes`, `policy`, `lifecycle_ref`, storage owner/cache ref, expiry/cache-miss metadata;
- for shell, preserve current details fields (`session_id`, `terminal_id`, `next_seq`, `omitted_bytes`) and add ref metadata rather than replacing them.

Why here: these two functions are the common source before Backbone persistence and before ToolResult re-enters model context (`crates/agentdash-agent/src/agent_loop/tool_call.rs:466`, `:621`, `:646`).

Risk: the agent crate currently has no application-layer VFS/lifecycle/cache dependency, so the guard likely needs an injected runtime delegate/service rather than direct storage ownership.

#### Candidate 2: Shell/terminal owner storage at local backend

For shell output produced by local backend:

- extend local retained buffer into a materialized output store keyed by `session_id` / `terminal_id` / `seq` or by `tool_call_id` + relay `call_id`;
- retain full-ish output locally with TTL/size policy;
- send cloud only bounded deltas/previews and a pointer/ref;
- expose reads through lifecycle_vfs path such as:
  - `lifecycle://.../session/terminal/{terminal_id}.log`
  - `lifecycle://.../session/tools/{item}/output.log`
  - `lifecycle://.../session/tool-results/{tool_call_id}.txt`

Why here: local shell output is owned by the process/session that produced it, and existing VFS materialization spec already treats `lifecycle_vfs` session resources as session-scoped local materialization (`.trellis/spec/backend/vfs/vfs-materialization.md`).

Risk: cloud lifecycle_vfs provider currently reads session events from cloud persistence only. A local-output provider requires a cloud-to-local read path or cache mirror for lifecycle ref resolution.

#### Candidate 3: Cloud short-term cache for generic tool results

For non-terminal PiAgent tools and remote/MCP outputs:

- store raw large result in cloud short-term cache;
- persist only preview + lifecycle ref + cache metadata into `session_events`;
- lifecycle_vfs read resolves ref to cache, applying `fs_read`-style range/limit defenses;
- expired/missing cache returns a clear synthetic text explaining that the large result cache expired.

Why here: generic `AgentToolResult` lacks terminal seq/process ownership and may originate from cloud-only tools. It should not force local backend storage.

Risk: needs a new cache service/provider contract and tests that no path rehydrates full content back into `session_events`.

#### Candidate 4: Lifecycle projection changes

Update lifecycle journey/provider surfaces so they no longer reconstruct large content from full raw events:

- `session/events.json` should expose bounded event bodies or structured refs for guarded payloads.
- `session/terminal` should read from terminal ref/provider rather than concatenating `CommandOutputDelta` from session events.
- `session/tools/*` should render preview + ref metadata, not raw full events.

Why here: current journey projection is event-backed and will reflect whatever was persisted (`crates/agentdash-application/src/lifecycle/surface/journey/mod.rs:112`, `:168`; `crates/agentdash-application/src/lifecycle/surface/journey/session_items.rs:38`, `:413`).

Risk: existing projections and tests may assume `raw_events` are available in rendered item JSON (`crates/agentdash-application/src/lifecycle/surface/journey/session_items.rs:360`, `:393`, `:413`).

### Recommended Integration Shape

1. Define a shared "large result descriptor" in the application-facing tool/session contract:
   - preview text;
   - `truncated: true`;
   - original/inline byte counts;
   - `lifecycle_ref`;
   - storage owner kind: `cloud_cache` or `local_terminal`;
   - optional `terminal_id`, `shell_session_id`, `seq_start`, `seq_end`, `expires_at`, digest.
2. Install guard before any `AgentToolResult` becomes `AgentEvent`:
   - updates: bound each partial result and avoid raw chunk persistence;
   - final: bound final result before `ToolExecutionEnd` and before `AgentMessage::tool_result_full`.
3. For `shell_exec`, treat live output and final result as one logical producer:
   - live `CommandOutputDelta` carries bounded deltas or progress notices;
   - final `CommandExecution.aggregated_output` is preview only and includes ref metadata.
4. For interactive terminal, change `PlatformEvent::TerminalOutput` persistence path:
   - persist bounded deltas or sampled markers;
   - store full output in local terminal owner storage;
   - frontend terminal rendering may continue receiving live data, but backlog/session event replay should not require full raw output in `session_events`.
5. Extend lifecycle_vfs:
   - add stable ref paths under existing `session/terminal` / `session/tools` surfaces;
   - ref reads use bounded read semantics and return cache-expired text on miss.

### High-Risk Test Points

- A shell command emits one single chunk larger than the retained cap. Current retained snapshot truncates the head, but `EventToolShellOutput` returns the original chunk from `push`; add regression asserting no full chunk reaches `ToolExecutionUpdate`, `CommandOutputDelta`, or `notification_json` (`crates/agentdash-local/src/shell_session_manager.rs:90`, `:97`, `:536`).
- A long-running `shell_exec` emits many small chunks. Verify cumulative persisted `CommandOutputDelta` data remains bounded and final `aggregated_output` is preview + ref, not full concatenation.
- A completed `shell_exec` with huge output should not re-enter model context via continuation `CommandExecution.aggregated_output` (`crates/agentdash-application/src/session/continuation.rs:707`).
- Interactive terminal output should not make `session_events.notification_json` grow linearly with raw PTY output while still rendering live in xterm.
- `session/events.json`, `session/items`, `session/tools`, and `session/terminal` should expose preview/ref consistently and avoid raw-event expansion.
- Cache miss / expired ref should be deterministic and model-readable; it should not panic lifecycle_vfs read paths.
- Relay disconnect should preserve terminal state/lost event behavior while not depending on local output storage availability (`crates/agentdash-api/src/relay/ws_handler.rs:278`, `:290`).
- Frontend `useTerminalStore.outputBuffers` currently appends unbounded strings; browser memory tests should cover large terminal streams and backlog replay (`packages/app-web/src/features/session/model/useTerminalStore.ts:58`, `:62`).
- PiAgent stream mapper tests currently assert full tool-result payload mapping for updates/end; update tests to assert bounded payload/ref behavior after guard introduction (`crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs:1215`, `:1223`, `:1229`).
- Ensure `ToolExecutionUpdate` for non-shell dynamic tools is guarded too, because mapper decodes arbitrary `AgentToolResult.content` into dynamic tool content items (`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:943`, `:1127`, `:1135`).

### Related Specs

- `.trellis/spec/backend/session/pi-agent-streaming.md` - tool update/end mapping; currently states `ToolCallResult` completion maps to `ItemCompleted`.
- `.trellis/spec/cross-layer/backbone-protocol.md` - `BackboneEnvelope` is the persisted, streamed, frontend-consumed event envelope.
- `.trellis/spec/backend/session/streaming-protocol.md` - NDJSON stream carries persisted `notification`.
- `.trellis/spec/backend/vfs/vfs-access.md` - `lifecycle_vfs` exposes `session/terminal`, `session/tools`, node artifacts, and records.
- `.trellis/spec/backend/vfs/vfs-materialization.md` - `lifecycle_vfs` is session-scoped when materialized locally; materialization key excludes tool/turn ids.
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - terminal callback is workflow runtime evidence.
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` - frontend indexes lifecycle state by run/orchestration/runtime node, while session id is trace/debug ref.

### External References

- None. This research only inspected repository code/specs and did not require external docs.

## Caveats / Not Found

- No existing generic large-result guard, `lifecycle_ref` schema, cloud cache provider, or local terminal output materialization service was found in the inspected paths.
- `terminal_cache.rs` is a state cache only; despite its name it does not cache terminal output.
- `session/terminal` in lifecycle journey currently reads only `CommandOutputDelta`; it misses interactive `PlatformEvent::TerminalOutput`.
- Local retained buffer truncation does not imply persisted event truncation, because `push_output` streams the original chunk returned by `RetainedOutputBuffer::push`.
- This research did not run tests and did not inspect every terminal API route; focus was the user-specified output path and lifecycle projection surfaces.
