# Research: implementation-scope

- Query: 为 CB04-C Session context usage projection 迁移确认代码级落点、调用点、DTO mapping 归属、写入文件集合与 focused validation。
- Scope: internal
- Date: 2026-06-21

## Findings

## Source Material

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-21-cb04-session-context-usage-projection/prd.md` | 要求 contracts 保留 response DTO，SPI `ContextFrame` 分析迁移到 application session projection，API/stream boundary 负责 DTO mapping。 |
| `.trellis/tasks/06-21-cb04-session-context-usage-projection/design.md` | 明确 application owns context usage analysis；contract owns response DTO shape only。 |
| `.trellis/tasks/06-21-cb04-session-context-usage-projection/implement.md` | 当前计划：移动 helper、替换 application 调用点、保留 response DTO 构造在 API/stream boundary。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md` | Owner rule：application read model owns backend-internal aggregation；API adapter owns read model -> contract DTO mapping；contract DTO owns wire shape。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/cb03-owner-map.md` | CB04-C 候选：`contracts::runtime::session::context_usage_items_from_context_frame` 与 `application::session::eventing` 应迁移为 application projection service。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | `agentdash-contracts` 是 HTTP/NDJSON/browser-facing DTO owner；API layer owns mapping into contract DTOs when route needs application/domain model internally。 |
| `.trellis/spec/backend/session/architecture.md` | `ExecutionContext` 是 connector-facing projection，不是 application 事实源；RuntimeSession owns trace/event/projection substrate。 |
| `.trellis/spec/backend/session/context-compaction-projection.md` | projection view 返回 `context_usage` 分析数据；分类估算来自 `AgentContextEnvelope` 与统一 token estimation helper，provider usage 仍是总量/窗口压力权威。 |
| `.trellis/spec/cross-layer/backbone-protocol.md` | `BackboneEnvelope` 是持久化、NDJSON 推送和前端消费统一事件 envelope；`TokenUsageUpdated` normalized payload 与 projection usage analysis 是不同口径。 |

## Current Helper Definition, I/O, And Call Sites

Current DTO definitions live in `crates/agentdash-contracts/src/runtime/session.rs`:

- `SessionContextUsageCategoryResponse` fields: `kind`, `label`, `token_estimate`, `source`, `deferred` at `crates/agentdash-contracts/src/runtime/session.rs:229`.
- `SessionContextUsageItemResponse` fields: `kind`, `label`, `name`, `token_estimate`, `source`, `deferred`, optional `source_event_seq`, optional `turn_id` at `crates/agentdash-contracts/src/runtime/session.rs:240`.
- `SessionContextUsageAnalysisResponse` fields: `categories`, `items`, `messages`, `top_tools`, `top_attachments` at `crates/agentdash-contracts/src/runtime/session.rs:291`.
- `SessionProjectionViewResponse` embeds `context_usage: SessionContextUsageAnalysisResponse` at `crates/agentdash-contracts/src/runtime/session.rs:301` and `crates/agentdash-contracts/src/runtime/session.rs:317`.

Current projection response assembly:

- `SessionProjectionViewResponse::from_envelope_and_context_items(envelope, context_items)` accepts `AgentContextEnvelope` plus `Vec<SessionContextUsageItemResponse>`, maps envelope messages to response segments, adds non-deferred context item token estimates into top-level `token_estimate`, then calls `context_usage_analysis` at `crates/agentdash-contracts/src/runtime/session.rs:512` and `crates/agentdash-contracts/src/runtime/session.rs:524`.
- `context_usage_analysis(segments, context_items)` produces response categories, message breakdown, top tools, and top attachments at `crates/agentdash-contracts/src/runtime/session.rs:672`.
- Message/tool/attachment aggregation uses response segment DTOs and token estimates at `crates/agentdash-contracts/src/runtime/session.rs:1422`, `crates/agentdash-contracts/src/runtime/session.rs:1433`, `crates/agentdash-contracts/src/runtime/session.rs:1458`, and `crates/agentdash-contracts/src/runtime/session.rs:1495`.

Current context frame helper:

- `context_usage_items_from_context_frame(frame, source_event_seq, turn_id)` is public in contracts at `crates/agentdash-contracts/src/runtime/session.rs:806`.
- Inputs:
  - `&ContextFrame` from `agentdash_spi::hooks`.
  - `Option<u64>` source event sequence.
  - `Option<String>` turn id.
- Output:
  - `Vec<SessionContextUsageItemResponse>`.
- It iterates `frame.sections` and delegates to `context_usage_items_from_section` at `crates/agentdash-contracts/src/runtime/session.rs:811` and `crates/agentdash-contracts/src/runtime/session.rs:820`.

Current section classification behavior:

- `Identity`, `ContinuationContext`, `SystemNotice`, `PendingAction`, `AutoResume` become `system_developer` items at `crates/agentdash-contracts/src/runtime/session.rs:827`, `crates/agentdash-contracts/src/runtime/session.rs:893`, `crates/agentdash-contracts/src/runtime/session.rs:913`, `crates/agentdash-contracts/src/runtime/session.rs:927`, and `crates/agentdash-contracts/src/runtime/session.rs:980`.
- `AssignmentContext` splits fragments by explicit `context_usage_kind` into `system_developer` and `agents` items at `crates/agentdash-contracts/src/runtime/session.rs:841`.
- `UserPreferences` and `ProjectGuidelines` become `memory` items at `crates/agentdash-contracts/src/runtime/session.rs:990` and `crates/agentdash-contracts/src/runtime/session.rs:1000`.
- Capability deltas become `capability_state` items at `crates/agentdash-contracts/src/runtime/session.rs:1017`, `crates/agentdash-contracts/src/runtime/session.rs:1031`, `crates/agentdash-contracts/src/runtime/session.rs:1050`, and `crates/agentdash-contracts/src/runtime/session.rs:1064`.
- `ToolSchemaDelta` maps `RuntimeToolSchemaEntry.context_usage_kind` to `system_tools` or `mcp_tools` at `crates/agentdash-contracts/src/runtime/session.rs:1096` and `crates/agentdash-contracts/src/runtime/session.rs:1271`.
- `SkillDelta` maps `RuntimeSkillEntry.context_usage_kind` to `skills`, preserving `disable_model_invocation` as `deferred`, at `crates/agentdash-contracts/src/runtime/session.rs:1100` and `crates/agentdash-contracts/src/runtime/session.rs:1301`.
- `CompanionAgentRosterDelta` maps effective agents to `agents` at `crates/agentdash-contracts/src/runtime/session.rs:1110` and `crates/agentdash-contracts/src/runtime/session.rs:1333`.
- `CompactionSummary` becomes `compaction_summary` and includes compaction metadata text at `crates/agentdash-contracts/src/runtime/session.rs:1116` and `crates/agentdash-contracts/src/runtime/session.rs:1217`.
- Text token estimate is local `chars / 4` rounded up at `crates/agentdash-contracts/src/runtime/session.rs:1410`.

Current application call site:

- `crates/agentdash-application/src/session/eventing.rs` imports `SessionContextUsageItemResponse` and `context_usage_items_from_context_frame` from contracts at `crates/agentdash-application/src/session/eventing.rs:8`.
- `SessionEventingService::build_context_usage_items(session_id, head_event_seq)` lists all session events, filters `event_seq <= head_event_seq`, scans latest unique `PlatformEvent::SessionMetaUpdate { key: "context_frame" }`, deserializes each value as `ContextFrame`, then extends items using the contracts helper at `crates/agentdash-application/src/session/eventing.rs:327` and `crates/agentdash-application/src/session/eventing.rs:359`.
- `emit_context_frame` persists runtime context frames as platform `SessionMetaUpdate(key = "context_frame")` at `crates/agentdash-application/src/session/eventing.rs:262` and `crates/agentdash-application/src/session/eventing.rs:275`.

Current API/stream call sites:

- `GET /sessions/{id}/context/projection` calls `build_agent_context_envelope`, then `build_context_usage_items`, then `SessionProjectionViewResponse::from_envelope_and_context_items` at `crates/agentdash-api/src/routes/sessions.rs:505`, `crates/agentdash-api/src/routes/sessions.rs:516`, and `crates/agentdash-api/src/routes/sessions.rs:523`.
- Session event list/stream mapping is still plain persisted event -> contract event DTO: `map_session_event(event) -> event.into()` and `stream_event_payload(event) -> SessionNdjsonEnvelope::event(event)` at `crates/agentdash-api/src/routes/sessions.rs:282` and `crates/agentdash-api/src/routes/sessions.rs:288`.
- NDJSON stream replays/backfills `PersistedSessionEvent` and serializes `SessionNdjsonEnvelope`; it does not assemble context usage at `crates/agentdash-api/src/routes/sessions.rs:735`, `crates/agentdash-api/src/routes/sessions.rs:769`, and `crates/agentdash-api/src/routes/sessions.rs:791`.
- Contract stream envelope DTOs are `SessionEventResponse` and `SessionNdjsonEnvelope` at `crates/agentdash-contracts/src/runtime/session.rs:25` and `crates/agentdash-contracts/src/runtime/session.rs:76`.

SPI facts the helper currently interprets:

- `context_usage_kind` constants are defined in SPI, with producer/consumer note: runtime context producers write marker values and projection consumers read them at `crates/agentdash-spi/src/hooks/mod.rs:28`.
- `ContextFrame` shape and `sections: Vec<ContextFrameSection>` are SPI facts at `crates/agentdash-spi/src/hooks/mod.rs:253`.
- `ContextFrameSection` variants cover identity, assignment, continuation, capability deltas, tools, skills, companion agents, pending actions, compaction summary, preferences and guidelines at `crates/agentdash-spi/src/hooks/mod.rs:272`.
- Tool/fragment/injection/skill/agent entries carry optional `context_usage_kind` markers at `crates/agentdash-spi/src/hooks/mod.rs:433`, `crates/agentdash-spi/src/hooks/mod.rs:449`, `crates/agentdash-spi/src/hooks/mod.rs:459`, `crates/agentdash-spi/src/hooks/mod.rs:470`, and `crates/agentdash-spi/src/hooks/mod.rs:494`.

## Recommended Application Read Model / Helper Shape

Recommended owner shape:

- Add an application-local module, preferably `crates/agentdash-application/src/session/context_usage_projection.rs` or `crates/agentdash-application/src/session/context_usage.rs`.
- Keep it internal to `session` unless API needs direct type names through public service methods; expose only application read facts, not contract DTOs.
- `eventing.rs` should stop importing `agentdash_contracts::session::{SessionContextUsageItemResponse, context_usage_items_from_context_frame}` and instead call application-local helpers.

Suggested read facts:

```rust
pub struct SessionContextUsageReadModel {
    pub categories: Vec<SessionContextUsageCategory>,
    pub items: Vec<SessionContextUsageItem>,
    pub messages: SessionMessageContextBreakdown,
    pub top_tools: Vec<SessionToolContextContribution>,
    pub top_attachments: Vec<SessionAttachmentContextContribution>,
}

pub struct SessionContextUsageItem {
    pub kind: String,
    pub label: String,
    pub name: String,
    pub token_estimate: u64,
    pub source: String,
    pub deferred: bool,
    pub source_event_seq: Option<u64>,
    pub turn_id: Option<String>,
}
```

Recommended helper split:

- `context_usage_items_from_context_frame(frame, source_event_seq, turn_id) -> Vec<SessionContextUsageItem>` moves to application unchanged semantically.
- `context_usage_analysis(segments, items) -> SessionContextUsageReadModel` should also move to application, because category assembly, deferred handling, message/tool/attachment breakdown, and top-level token estimate adjustment are read-model decisions, not DTO shape.
- If avoiding a larger first wave, keep segment DTO projection in contracts temporarily but still move `ContextFrame` analysis. The cleaner final shape is application returns a full `SessionContextProjectionReadModel` and API maps it to `SessionProjectionViewResponse`.

Recommended full read model:

```rust
pub struct SessionContextProjectionReadModel {
    pub session_id: String,
    pub projection_kind: String,
    pub projection_version: u64,
    pub head_event_seq: u64,
    pub active_compaction_id: Option<String>,
    pub token_estimate: Option<u64>,
    pub message_count: u64,
    pub segments: Vec<SessionProjectionSegmentReadModel>,
    pub context_usage: SessionContextUsageReadModel,
}
```

Why full read model is preferable:

- The current `SessionProjectionViewResponse::from_envelope_and_context_items` combines `AgentContextEnvelope` projection, segment token estimation, context item token estimates, category assembly, and DTO construction in contracts at `crates/agentdash-contracts/src/runtime/session.rs:512`.
- The task design says application owns context usage analysis and API/stream boundary maps application facts to DTOs; therefore contracts should keep response structs but should not own `from_envelope_and_context_items`, `context_usage_analysis`, or `context_usage_items_from_context_frame` as application-consumed builders.

Recommended service methods:

- Replace `build_context_usage_items(session_id, head_event_seq) -> Vec<...Response>` with either:
  - `build_context_usage_items(session_id, head_event_seq) -> Vec<SessionContextUsageItem>` as a minimal step, or
  - `build_context_projection_read_model(session_id) -> SessionContextProjectionReadModel` as the cleaner API route surface.
- Keep event scanning and dedupe in `SessionEventingService` because it reads persisted runtime events and enforces `head_event_seq` visibility at `crates/agentdash-application/src/session/eventing.rs:327`.
- Move pure section classification and aggregation into the new module so eventing remains orchestration/read access, not a large mapper.

## API / Stream DTO Mapping Ownership

API route owns mapping from application read model to `agentdash-contracts` DTOs.

Recommended location:

- Route-local mapper functions in `crates/agentdash-api/src/routes/sessions.rs` are acceptable for this narrow task.
- If mapper grows, use an API-local module such as `crates/agentdash-api/src/routes/sessions/context_projection_mapper.rs`; keep it under API, not application or contracts.

Mapping responsibilities:

- `SessionContextUsageItem -> SessionContextUsageItemResponse`.
- `SessionContextUsageCategory -> SessionContextUsageCategoryResponse`.
- Message/tool/attachment contribution read facts -> corresponding response DTOs.
- `SessionContextProjectionReadModel -> SessionProjectionViewResponse`.

Stream boundary:

- Current NDJSON session stream does not include context usage projection assembly; it maps persisted events to `SessionNdjsonEnvelope` only at `crates/agentdash-api/src/routes/sessions.rs:288` and `crates/agentdash-api/src/routes/sessions.rs:735`.
- Keep `SessionEventResponse` / `SessionNdjsonEnvelope` mapping in contracts or API as wire envelope mapping; do not route context usage through stream unless a future event explicitly carries a context projection view.
- If a future stream event pushes projection snapshots, it should use the same API-local read model -> DTO mapper as `GET /sessions/{id}/context/projection`.

## Suggested Write Set

Primary files:

- `crates/agentdash-application/src/session/context_usage_projection.rs` or `context_usage.rs`: new application read model/helper module for `ContextFrame` analysis, context item construction, category aggregation, message/tool/attachment breakdown and local text token estimate.
- `crates/agentdash-application/src/session/mod.rs`: register the new module and export only what API/service needs.
- `crates/agentdash-application/src/session/eventing.rs`: remove contract usage DTO/helper import; return application read facts or full context projection read model.
- `crates/agentdash-api/src/routes/sessions.rs`: map application read model to `SessionProjectionViewResponse` and nested response DTOs in `get_session_context_projection`.
- `crates/agentdash-contracts/src/runtime/session.rs`: keep response DTO structs and NDJSON envelope structs; remove `ContextFrame` imports, `context_usage_items_from_context_frame`, section-analysis helpers, and ideally `SessionProjectionViewResponse::from_envelope_and_context_items` if full read model mapping moves to API.

Likely test files / test modules:

- Move `projection_view_aggregates_context_frame_usage_items` out of `contracts::runtime::session` into application tests because it constructs SPI `ContextFrame` and verifies classification at `crates/agentdash-contracts/src/runtime/session.rs:1682`.
- Keep or rewrite response DTO tests in contracts only for DTO serde/TS drift if needed; tests that assert usage classification should live under application.
- Add or adjust application tests near `crates/agentdash-application/src/session/eventing.rs` tests, using the existing `test_eventing_service` fixture at `crates/agentdash-application/src/session/eventing.rs:845`.
- If API mapper is non-trivial, add focused route-mapper unit tests under `agentdash-api` rather than broad HTTP tests.

Cargo/dependency notes:

- Do not expect this task alone to remove `agentdash-contracts` from `agentdash-application/Cargo.toml`; application still imports contract DTOs in AgentRun workspace/capability/workspace module paths at `crates/agentdash-application/Cargo.toml:10`.
- Do not expect this task alone to remove `agentdash-spi` from `agentdash-contracts/Cargo.toml`; `runtime/session.rs` still uses SPI persistence/lineage records at `crates/agentdash-contracts/src/runtime/session.rs:16`, unless a broader DTO mapper split is done.
- The required win is narrower: contracts should no longer import or interpret `agentdash_spi::hooks::{ContextFrame, ContextFrameSection, Runtime*Entry}` for context usage analysis.

## Parallel Conflict Boundary

Safe to own in this task:

- `crates/agentdash-contracts/src/runtime/session.rs` context usage helpers and projection view response builder area.
- `crates/agentdash-application/src/session/eventing.rs` context frame usage item construction path.
- New application session context usage module.
- `crates/agentdash-api/src/routes/sessions.rs` mapping inside `get_session_context_projection`.

Avoid touching:

- Session terminal/lost semantics, event commit, stream supervision and runtime registry behavior.
- AgentRun mailbox, scheduler, workspace snapshot, command policy, and lifecycle side effects.
- MCP preset incoming conversion, Routine/LLM/Settings conversion cleanup, backend access command conversion, and capability catalog read model split.
- Frontend generated files by hand; run contract generation/check instead.

Potential merge conflicts:

- `agentdash-contracts/src/runtime/session.rs` is shared with any session lineage, rollback, NDJSON envelope, or projection DTO work.
- `agentdash-api/src/routes/sessions.rs` is shared with session route work; keep mapping functions narrow and close to `get_session_context_projection`.
- `agentdash-application/src/session/eventing.rs` is shared with compaction projection and stream eventing changes; only edit imports and context usage/query helper area.

## Focused Validation Commands

```powershell
cargo test -p agentdash-application session::eventing
cargo test -p agentdash-application session::context_usage
cargo test -p agentdash-contracts runtime::session
cargo check -p agentdash-application -p agentdash-api -p agentdash-contracts
pnpm run contracts:check
```

If no `session::context_usage` module/test path exists after implementation, replace that command with the exact new module path, for example:

```powershell
cargo test -p agentdash-application session::context_usage_projection
```

## Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/cross-layer/backbone-protocol.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/streaming-protocol.md`
- `.trellis/spec/backend/session/context-compaction-projection.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/cb03-owner-map.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell; this report uses the user-provided task path `.trellis/tasks/06-21-cb04-session-context-usage-projection/`.
- No external references were needed; this was an internal code-boundary research pass.
- No business code, specs, manifests, generated files, or git state were modified.
- I did not run validation commands because this research task only writes the scope report; commands above are for the implement/check pass.
