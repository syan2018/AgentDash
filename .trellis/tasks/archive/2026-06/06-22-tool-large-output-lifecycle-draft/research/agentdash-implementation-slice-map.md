# Research: AgentDash implementation slice map

- Query: 将 PiAgent 工具大返回裁切与 lifecycle ref 方案拆成可派发给 subagent 执行的工作包
- Scope: internal
- Date: 2026-06-22

## Findings

### Files found

- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/prd.md` - 需求与验收标准，要求 producer、SessionEvent、ContextProjector、NDJSON、frontend 都只消费 bounded preview。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/design.md` - 设计方向，定义 `LargeOutputGuard`、cloud cache、terminal owner storage、lifecycle ref surface。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/implement.md` - 当前阶段式实施计划，需要进一步拆成 subagent ownership。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/research/piagent-tool-result-path.md` - PiAgent final/update result 主路径与 guard 插入点研究。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/research/session-event-projection-risk.md` - SessionEvent、ContextProjector、NDJSON、frontend rawEvents 风险研究。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/research/lifecycle-vfs-ref-surface.md` - lifecycle_vfs、fs_read、防御和 ref surface 研究。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/research/terminal-output-path.md` - shell_exec、relay live output、interactive terminal 路径研究。
- `crates/agentdash-agent/src/agent_loop/tool_call.rs` - PiAgent tool execution/finalize/update/end/message 共同出口。
- `crates/agentdash-agent-types/src/runtime/tool.rs` - `AgentToolResult` / `ToolUpdateCallback` tool result contract。
- `crates/agentdash-agent-types/src/model/message.rs` - `AgentMessage::ToolResult` provider/model context contract。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs` - `PlatformEvent::TerminalOutput` wire variant。
- `crates/agentdash-agent-protocol/src/backbone/thread_item.rs` - Codex `CommandExecution` / `DynamicToolCall` builder helpers。
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` - `AgentEvent` 到 Backbone item/update 的映射。
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs` - PiAgent stream mapper/update/end 现有回归测试。
- `crates/agentdash-application/src/vfs/tools/fs/shell.rs` - `shell_exec` tool final result 与 live update forwarding。
- `crates/agentdash-local/src/shell_session_manager.rs` - 本机 shell/terminal output retained buffer 与 relay output event 发送。
- `crates/agentdash-api/src/relay/ws_handler.rs` - relay shell/terminal output ingestion 到 cloud eventing。
- `crates/agentdash-application/src/session/eventing.rs` - `SessionEventingService` append/broadcast/projection head 入口。
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` - Postgres `session_events.notification_json` 写入与读取。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - `session_events.notification_json text NOT NULL` schema。
- `crates/agentdash-application/src/session/context_projector.rs` - model context projection 从 persisted events 构建 transcript。
- `crates/agentdash-application/src/session/continuation.rs` - continuation/repository rehydrate 从 Backbone item 还原 tool result。
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs` - lifecycle provider read/list/search route。
- `crates/agentdash-application/src/lifecycle/surface/journey/mod.rs` - session projection、events.json、terminal 聚合。
- `crates/agentdash-application/src/lifecycle/surface/journey/session_items.rs` - session items/tools projection 与 raw_events 暴露。
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts` - frontend `rawEvents` append 与 entries 派生。
- `packages/app-web/src/features/session/model/useTerminalStore.ts` - interactive terminal output buffer store。
- `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx` - shell/command output UI rendering。

### Code patterns

- PiAgent final result 现在在 `finalize_executed_tool_call` 后直接进入 `emit_tool_call_outcome`，再同时写 `ToolExecutionEnd` 与 `AgentMessage::ToolResult`；这是 canonical guard 的最小共同出口（`crates/agentdash-agent/src/agent_loop/tool_call.rs:100`, `:111`, `:510`, `:615`, `:623`, `:646`）。
- approval rejected 分支目前直接调用 `emit_tool_result_message`，不会经过 `ToolExecutionEnd`；guard helper 要覆盖该路径（`crates/agentdash-agent/src/agent_loop/tool_call.rs:116`, `:204`, `:641`）。
- update path 在 `build_on_update` 中把 `AgentToolResult` 序列化进 `ToolExecutionUpdate.partial_result`，当前没有 size policy（`crates/agentdash-agent/src/agent_loop/tool_call.rs:466`, `:473`, `:477`）。
- `AgentToolResult.details` 是现有可承载 `large_output` typed schema 的字段（`crates/agentdash-agent-types/src/runtime/tool.rs:26`, `:30`; `crates/agentdash-agent-types/src/model/message.rs:194`, `:199`, `:207`）。
- Stream mapper 对 shell update 复制 `partial_result_text` 到 `CommandOutputDelta.delta`，对普通 update/end 复制 content items，shell final 从文本里反推 `exit_code` 与 `aggregated_output`（`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:928`, `:932`, `:943`, `:1040`, `:1057`, `:1071`, `:1127`）。
- `SessionEventingService` 普通路径把完整 envelope 交给 store append，然后广播；这是 persistence-side safety guard 的入口（`crates/agentdash-application/src/session/eventing.rs:133`, `:164`, `:167`, `:171`）。
- Postgres 把完整 `BackboneEnvelope` 序列化为 `notification_json`，schema 没有长度限制（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:319`, `:359`; `crates/agentdash-infrastructure/migrations/0001_init.sql:575`, `:584`）。
- Context/continuation 会从 completed `CommandExecution.aggregated_output`、dynamic content items、MCP raw output 重建 `ContentPart::Text`，所以 persisted event 必须已经 bounded（`crates/agentdash-application/src/session/continuation.rs:478`, `:511`, `:647`, `:675`, `:707`, `:750`）。
- lifecycle_vfs 当前把 `session/*` 直接委派给 journey projection，`session/tools` 是 flat file list，`search_text` 会递归 read 每个非目录 entry（`crates/agentdash-application/src/vfs/provider_lifecycle.rs:68`, `:73`, `:155`, `:810`, `:842`, `:875`）。
- `session_items.rs` projection 会把 raw persisted events 放进 item JSON，若 producer/persistence 未 bounded，lifecycle item read 会重新展开大 payload（`crates/agentdash-application/src/lifecycle/surface/journey/session_items.rs:38`, `:360`, `:393`, `:413`）。
- 本机 retained buffer 只裁 retained snapshot；单个超大 chunk 仍以原 chunk 发送 live relay event（`crates/agentdash-local/src/shell_session_manager.rs:84`, `:90`, `:97`, `:536`, `:546`）。
- cloud relay 对 interactive terminal 直接把 `payload.data` 注入 `PlatformEvent::TerminalOutput` 并 persist（`crates/agentdash-api/src/relay/ws_handler.rs:444`, `:458`, `:460`, `:471`）。
- 前端 `rawEvents` 和 terminal `outputBuffers` 都是 append retained state；后端 bounded 后 `rawEvents` 可继续作为事实源，terminal buffer 仍需要容量策略（`packages/app-web/src/features/session/model/sessionStreamReducer.ts:13`, `:282`; `packages/app-web/src/features/session/model/useTerminalStore.ts:7`, `:58`, `:62`）。

### Suggested work packages

#### WP0 - Large output policy, schema, and cache foundation

Ownership file range:

- `crates/agentdash-agent-types/src/runtime/tool.rs`
- `crates/agentdash-agent-types/src/model/message.rs`
- new shared helper module under a crate already visible to both agent-loop and application/executor, likely `agentdash-agent-types` for typed metadata plus application-owned cache traits under `agentdash-application` or `agentdash-spi`
- `crates/agentdash-agent/src/agent_loop.rs` if `AgentLoopConfig` needs an injected guard/cache delegate

Entry functions/types:

- `AgentToolResult`
- `AgentMessage::tool_result_full`
- `AgentLoopConfig`
- new `LargeOutputPolicy`, `LargeOutputRef`, `LargeOutputTruncation`, `LargeOutputStorage`, `LargeOutputGuard`

Preconditions:

- Decide the canonical wire location: MVP should use `AgentToolResult.details.large_output` while keeping `content` as model-visible preview.
- Decide lifecycle path format before cache writes start, because WP2/WP4/WP5 will emit refs.

Acceptance tests:

- Unit tests for head/tail preview byte caps, UTF-8 boundary handling, digest stability, metadata JSON shape, and no `is_error` mutation.
- Small threshold test config can force truncation without huge fixtures.

Rollback point:

- If integration stalls, revert to typed helpers unused by runtime; no persisted data shape changes until WP2/WP3 emit metadata.

Likely conflict files:

- `crates/agentdash-agent-types/src/runtime/tool.rs`
- `crates/agentdash-agent/src/agent_loop.rs`
- Any TS/protocol generator output if a first-class field is introduced instead of details schema.

#### WP1 - PiAgent final/update guard

Ownership file range:

- `crates/agentdash-agent/src/agent_loop/tool_call.rs`
- `crates/agentdash-agent/src/agent_loop.rs`
- focused tests in `crates/agentdash-agent/src/**` or a new agent-loop test module

Entry functions/types:

- `build_on_update`
- `finalize_executed_tool_call`
- `emit_tool_call_outcome`
- `emit_tool_result_message`
- `ToolCallPreparation::Immediate`
- `ApprovalResolution::Rejected`

Preconditions:

- WP0 metadata/policy API available.
- For storage-backed refs, provide a guard delegate that can store bytes or return metadata without application dependency cycles.

Acceptance tests:

- Large dynamic tool final result: `ToolExecutionEnd.result`, `MessageEnd(ToolResult)`, and returned `AgentMessage::ToolResult.content` contain preview/ref metadata and do not contain sentinel.
- Large `ToolExecutionUpdate.partial_result` is bounded per update policy.
- approval rejected / missing tool / invalid args paths pass through the same guard helper without breaking `is_error`, call id, tool name, and details.
- Provider bridge smoke assertion: next provider request sees only preview content.

Rollback point:

- A single guarded emit helper should allow rollback by routing final/update result through identity guard while keeping structural changes.

Likely conflict files:

- `crates/agentdash-agent/src/agent_loop/tool_call.rs`
- `crates/agentdash-agent/src/agent_loop.rs`
- Any concurrent change to tool approval, runtime delegate, or `after_tool_call` behavior.

#### WP2 - PiAgent stream mapper and protocol contract alignment

Ownership file range:

- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs`
- `crates/agentdash-agent-protocol/src/backbone/thread_item.rs`
- `crates/agentdash-agent-protocol/src/backbone/platform.rs` only if terminal output wire shape changes
- generated `packages/app-web/src/generated/backbone-protocol.ts` if protocol crate changes

Entry functions/types:

- `convert_event_to_envelopes_with_runtime_context`
- `make_shell_exec_item`
- `decode_tool_result_to_content_items`
- `AgentDashNativeThreadItem::ShellExec`
- `PlatformEvent::TerminalOutput`

Preconditions:

- WP1 emits bounded `AgentToolResult` consistently.
- WP0 defines how `large_output` metadata is read from details.

Acceptance tests:

- Update existing `tool_execution_updates_preserve_full_tool_result_payload` into bounded/ref semantics.
- DynamicToolCall/MCP/native tool final item content_items contain preview and not sentinel.
- `shell_exec` final `aggregated_output` is preview; `exit_code`, status, cwd, args, item id, and entry_index are preserved.
- Shell output update maps to bounded `CommandOutputDelta` or marker with ref metadata.
- If `PlatformEvent::TerminalOutput` changes, generated TS check passes.

Rollback point:

- If protocol changes are too broad, keep Backbone item shape stable and rely on details/preview strings only; defer first-class terminal output variant changes.

Likely conflict files:

- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs`
- `crates/agentdash-agent-protocol/src/backbone/platform.rs`
- `packages/app-web/src/generated/backbone-protocol.ts`

#### WP3 - SessionEvent persistence safety guard

Ownership file range:

- `crates/agentdash-application/src/session/eventing.rs`
- `crates/agentdash-application/src/session/persistence.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/session_core.rs`
- Postgres tests under `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`

Entry functions/types:

- `SessionEventingService::persist_notification_inner`
- `SessionEventStore::append_event`
- `PostgresSessionRepository::append_event`
- `PersistedSessionEvent`

Preconditions:

- WP0 provides bounded-envelope safety helper or event scanner.
- WP1/WP2 should already produce bounded events; this layer is a last-resort protection for missed producer paths.

Acceptance tests:

- Persist a synthetic oversized `BackboneEnvelope`; `notification_json` is under policy threshold and does not contain sentinel.
- Projection indexes still extract `session_update_type`, `turn_id`, `entry_index`, and `tool_call_id`.
- Safety guard emits diagnostic metadata explaining that persistence guard intervened.
- Normal small events are byte-for-byte or semantically unchanged.

Rollback point:

- Guard can initially warn/reject oversized event in test mode, then switch to truncating once WP0 sanitizer is reliable.

Likely conflict files:

- `crates/agentdash-application/src/session/eventing.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
- Any migration that adds DB constraints for `notification_json`.

#### WP4 - Lifecycle VFS ref resolver and projection surface

Ownership file range:

- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
- `crates/agentdash-application/src/lifecycle/surface/journey/mod.rs`
- `crates/agentdash-application/src/lifecycle/surface/journey/session_items.rs`
- `crates/agentdash-application/src/vfs/tools/fs/read.rs` only for tests around ref read + full-read guard
- new application service/provider for `LargeResultCache` read/range/metadata

Entry functions/types:

- `LifecycleVfsProvider::read_agent_run_session_scope`
- `LifecycleVfsProvider::list_agent_run_session_scope`
- `LifecycleVfsProvider::read_node_runtime_scope`
- `LifecycleVfsProvider::search_text`
- `LifecycleJourneyProjection::read_session_projection`
- `session_item_projections`
- `fs_read` path through `VfsService::read_text_range`

Preconditions:

- WP0 ref metadata shape fixed.
- WP1/WP5 write refs into tool/terminal metadata.
- Decide cloud cache provider interface and terminal owner storage read interface.

Acceptance tests:

- `lifecycle://session/tool-results/{item_id}/result.txt` reads cache content through `fs_read` and respects existing full-read rejection without `limit`.
- `offset/limit` can page through ref content.
- `metadata.json` exposes preview/ref/cache status without raw content.
- cache miss/expired returns stable bounded text and metadata status.
- `session/events.json`, `session/tools`, `session/items`, and `session/terminal` expose preview/ref and do not expand raw_events into sentinel.
- lifecycle `search_text` indexes metadata/preview and does not scan full large result bodies.

Rollback point:

- Keep existing `session/tools` flat files unchanged and add only `session/tool-results/...`; if direct ref read is risky, ship metadata-only files first.

Likely conflict files:

- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
- `crates/agentdash-application/src/lifecycle/surface/journey/session_items.rs`
- Any concurrent lifecycle/session projection changes.

#### WP5 - Terminal owner storage and shell live/final output bounding

Ownership file range:

- `crates/agentdash-local/src/shell_session_manager.rs`
- `crates/agentdash-relay/src/protocol/tool.rs`
- `crates/agentdash-relay/src/protocol.rs`
- `crates/agentdash-relay/src/shell_output_registry.rs`
- `crates/agentdash-api/src/mount_providers/relay_fs.rs`
- `crates/agentdash-api/src/relay/ws_handler.rs`
- `crates/agentdash-application/src/vfs/tools/fs/shell.rs`
- `crates/agentdash-application/src/session/terminal_cache.rs`

Entry functions/types:

- `ShellExecTool::execute`
- `shell_exec_result_text`
- `ShellSessionManager::push_output`
- `RetainedOutputBuffer`
- `RelayMessage::EventToolShellOutput`
- `RelayMessage::EventTerminalOutput`
- `ShellOutputRegistry::route`
- `PlatformEvent::TerminalOutput`

Preconditions:

- WP0 metadata schema available.
- WP4 has or stubs terminal ref resolver path.

Acceptance tests:

- A single shell chunk larger than cap writes full content to owner storage but sends bounded live update / persisted event.
- Many small shell chunks do not make cumulative `CommandOutputDelta` or persisted history grow linearly with raw output.
- `shell_exec` final result uses preview/ref and preserves `exit_code`, `state`, `cwd`, `session_id`, `terminal_id`, `next_seq`.
- Interactive terminal still renders live data, while persisted `PlatformEvent` is bounded marker/delta with ref metadata.
- Relay disconnect/lost terminal flow still emits state changes.

Rollback point:

- Start with cloud-side bounded relay events while owner storage read is metadata-only; later enable lifecycle terminal ref reads.

Likely conflict files:

- `crates/agentdash-local/src/shell_session_manager.rs`
- `crates/agentdash-api/src/relay/ws_handler.rs`
- `crates/agentdash-application/src/vfs/tools/fs/shell.rs`
- `crates/agentdash-agent-protocol/src/backbone/platform.rs`

#### WP6 - ContextProjector, continuation, repository rehydrate safety

Ownership file range:

- `crates/agentdash-application/src/session/context_projector.rs`
- `crates/agentdash-application/src/session/continuation.rs`
- `crates/agentdash-application/src/session/launch/planner.rs`
- `crates/agentdash-application/src/session/launch/preparation.rs`
- `crates/agentdash-agent/src/compaction/mod.rs` only for compact source request assertions if needed

Entry functions/types:

- `ContextProjector::build_projected_transcript`
- `build_projected_transcript`
- `extract_tool_call_from_codex_thread_item`
- `extract_tool_call_from_agentdash_thread_item`
- `update_restored_tool_result`
- `RestoredSessionState.messages`

Preconditions:

- WP1/WP2/WP3 make persisted events bounded.
- WP0 schema parser exists to render ref metadata in continuation frames without following refs.

Acceptance tests:

- No active projection head: projected transcript contains preview/ref only.
- Active projection head + suffix: suffix tool results are preview/ref only.
- Repository rehydrate executor-state path returns `RestoredSessionState.messages` without sentinel.
- Compaction summary source containing large-result preview does not inline ref content.
- MCP/dynamic/shell/native item paths all preserve metadata and never auto-read lifecycle refs.

Rollback point:

- Projection can initially treat large_output details as opaque details and rely on bounded content; later improve human-readable ref rendering.

Likely conflict files:

- `crates/agentdash-application/src/session/continuation.rs`
- `crates/agentdash-application/src/session/context_projector.rs`
- Any concurrent branch/fork/rollback projection work.

#### WP7 - Frontend bounded rendering and terminal capacity

Ownership file range:

- `packages/app-web/src/generated/backbone-protocol.ts` if WP2 changes protocol
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.ts`
- `packages/app-web/src/features/session/model/useTerminalStore.ts`
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts`
- `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx`
- dynamic/native tool card bodies under `packages/app-web/src/features/session/ui/bodies/`
- `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx`

Entry functions/types:

- `reduceStreamState`
- `useSessionFeed`
- `useTerminalStore.appendOutput`
- `dispatchSessionPlatformEvent`
- `CommandExecutionCardBody`
- tool card registry/body renderers

Preconditions:

- WP2/WP5 define frontend-visible event/item shape.
- If "read full output" UI is included, WP4 provides lifecycle ref read route or existing `fs_read` invocation path.

Acceptance tests:

- `rawEvents` and derived entries do not store sentinel when receiving bounded large-output events.
- Tool/command card displays preview, truncation status, original bytes, ref availability/expiry, and keeps exit code/status visible.
- Terminal store enforces a capacity policy for live buffers.
- Promote-to-terminal from command card uses preview or paged ref content intentionally, not raw sentinel.
- `pnpm run frontend:check` and focused frontend tests pass.

Rollback point:

- Ship read-only preview/status rendering first; defer "expand full output" UI while keeping event stream bounded.

Likely conflict files:

- `packages/app-web/src/generated/backbone-protocol.ts`
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts`
- `packages/app-web/src/features/session/model/useTerminalStore.ts`
- `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx`

#### WP8 - End-to-end verification harness and fixture policy

Ownership file range:

- focused backend tests in agent/executor/application/infrastructure/local crates
- focused frontend tests in `packages/app-web/src/features/session/**`
- package scripts only if a narrow test command alias is useful

Entry functions/types:

- Existing test modules listed above; no runtime feature ownership.

Preconditions:

- WP1 through WP7 merged enough to exercise full chain.

Acceptance tests:

- PiAgent dynamic/MCP tool returning sentinel-sized text: next model context, `ToolExecutionEnd`, `MessageEnd(ToolResult)`, Backbone `ItemCompleted`, and Postgres `notification_json` contain preview/ref only.
- `ToolExecutionUpdate`, shell live output, shell final `aggregated_output`, interactive terminal persistence, NDJSON backlog, `/sessions/{id}/events`, `rawEvents`, lifecycle refs, and ContextProjector all pass sentinel absence checks.
- cache miss/expired lifecycle ref read returns deterministic bounded text.
- Commands after implementation should be narrowed from:
  - `cargo test -p agentdash-agent tool_result`
  - `cargo test -p agentdash-executor pi_agent`
  - `cargo test -p agentdash-application lifecycle_vfs`
  - `cargo test -p agentdash-infrastructure session_events`
  - `cargo test -p agentdash-local retained_buffer`
  - `pnpm run contracts:check`
  - `pnpm run frontend:check`

Rollback point:

- Keep tests as pending/ignored only during intermediate package handoff; before task completion they must be active.

Likely conflict files:

- Broadly conflicts with all work packages through shared fixtures and generated contracts; schedule WP8 last or let it run as a check subagent after integrations land.

### Recommended dispatch order

1. Dispatch WP0 first because every later package needs the policy/schema/ref vocabulary.
2. Dispatch WP1 and WP3 after WP0. WP1 protects model/event source; WP3 protects persistence if a producer path is missed.
3. Dispatch WP5 in parallel with WP1 only after metadata shape is fixed; terminal has separate local/relay ownership.
4. Dispatch WP2 after WP1/WP5 provide emitted shapes, because stream mapper tests will change from "preserve full payload" to bounded/ref behavior.
5. Dispatch WP4 after WP0 plus at least one producer emits refs; otherwise lifecycle resolver can only implement metadata scaffolding.
6. Dispatch WP6 after WP2/WP3, because projection should consume final persisted bounded events.
7. Dispatch WP7 after protocol/item shape stabilizes.
8. Dispatch WP8 as final integration verification.

### Cross-work-package conflict map

| File | Packages likely touching it | Coordination note |
| --- | --- | --- |
| `crates/agentdash-agent/src/agent_loop/tool_call.rs` | WP1 | Single owner; other packages should not edit. |
| `crates/agentdash-agent-types/src/runtime/tool.rs` | WP0, WP1 | WP0 owns schema; WP1 only consumes. |
| `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` | WP2, WP6 tests indirectly | WP2 owns mapper changes. |
| `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs` | WP2, WP8 | WP2 updates unit expectations; WP8 adds integration/sentinel coverage. |
| `crates/agentdash-agent-protocol/src/backbone/platform.rs` | WP2, WP5, WP7 | If terminal wire shape changes, assign one protocol owner and regenerate TS once. |
| `packages/app-web/src/generated/backbone-protocol.ts` | WP2, WP7 | Generated file should be updated by protocol owner, frontend consumes. |
| `crates/agentdash-application/src/vfs/tools/fs/shell.rs` | WP5, WP1 only via guard behavior | WP5 owns shell-specific details/ref metadata. |
| `crates/agentdash-local/src/shell_session_manager.rs` | WP5 | Single owner; tests here validate local storage/live cap. |
| `crates/agentdash-api/src/relay/ws_handler.rs` | WP5, WP3 indirectly | WP5 owns terminal event bounding before eventing. |
| `crates/agentdash-application/src/session/eventing.rs` | WP3, WP6 indirectly | WP3 owns append safety guard. |
| `crates/agentdash-application/src/vfs/provider_lifecycle.rs` | WP4 | Single owner for ref routing/list/search. |
| `crates/agentdash-application/src/lifecycle/surface/journey/session_items.rs` | WP4 | Single owner for item projection raw_events behavior. |
| `crates/agentdash-application/src/session/continuation.rs` | WP6 | Single owner for rehydrate transcript semantics. |
| `packages/app-web/src/features/session/model/sessionStreamReducer.ts` | WP7 | Frontend owner only after backend event shape stabilizes. |
| `packages/app-web/src/features/session/model/useTerminalStore.ts` | WP7 | Capacity policy should align with WP5 terminal event semantics. |

### Suggested child-task granularity

Each WP above is independently reviewable if the parent task keeps an integration checklist. Good child task candidates:

- Child A: WP0 + minimal cloud cache interface.
- Child B: WP1 final/update guard in agent-loop.
- Child C: WP5 terminal owner storage and shell/terminal bounded producer.
- Child D: WP2 stream mapper/protocol/generated TS contract.
- Child E: WP3 persistence safety guard.
- Child F: WP4 lifecycle ref resolver/projection surface.
- Child G: WP6 ContextProjector/continuation/rehydrate.
- Child H: WP7 frontend rendering/capacity.
- Child I: WP8 end-to-end sentinel verification.

WP0 must complete before B/C/D/E/F/G/H. B and C are the main producers. D/E/F/G/H should not each invent their own metadata shape.

## External references

- No external references were used. This research is based on repository code, Trellis specs, and existing task research only.

## Related specs

- `.trellis/spec/backend/session/pi-agent-streaming.md` - PiAgent `AgentEvent` to Backbone mapping, item lifecycle, entry_index rules.
- `.trellis/spec/backend/session/streaming-protocol.md` - session NDJSON envelope shape and replay behavior.
- `.trellis/spec/backend/session/context-compaction-projection.md` - `session_events` as durable fact source for ContextProjector, compaction, fork/rollback.
- `.trellis/spec/cross-layer/backbone-protocol.md` - BackboneEnvelope, ThreadItem, PlatformEvent, TS generation contract.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - Rust contract to generated TS workflow and `contracts:check`.
- `.trellis/spec/backend/vfs/vfs-access.md` - lifecycle_vfs path contract, VFS path normalization, `session/tools` / `session/terminal` surfaces.
- `.trellis/spec/backend/vfs/vfs-materialization.md` - lifecycle_vfs session materialization and local owner boundaries.
- `.trellis/spec/frontend/hook-guidelines.md` - frontend NDJSON hook, raw event fact source, useSessionFeed aggregation contract.

## Caveats / Not Found

- `task.py current --source` reported no active task in this subagent session; the output path was taken from the explicit user dispatch prompt.
- No existing generic `LargeOutputGuard`, `large_output` schema, cloud result cache, terminal owner output store, or lifecycle tool-result ref resolver was found.
- Current tests include assertions that full tool update payloads are preserved; those tests are expected to change as part of WP2.
- I did not run implementation tests because this research task only reads code and writes this research artifact.
