# Product canonical presentation activation

## Purpose

本文固定 canonical Complete Agent / Managed Runtime presentation 合同通过后，Product、
API、Lifecycle VFS 与前端的唯一激活路径。Product 直接消费 Managed Runtime committed
snapshot/change，不再把 canonical item 降级为 `BackboneEvent`、重建
`AgentDashThreadItem`，也不以 generic JSON 或 tool name 猜 presentation family。

Workspace、Terminal、Canvas 与 Product mailbox 保持各自独立 Product feed。它们可以在
页面组合展示，但不能成为 Agent history、Runtime command authority 或彼此的恢复事实源。

## Final data flow

```text
Complete Agent source snapshot/change
  -> Managed Runtime validates and commits
       facts + source projection/change
       normalized projection/change + outbox
  -> Product projection gateway
       snapshot baseline + ordered tail
       gap -> snapshot reload -> resumed tail
  -> Product history UI
       ManagedRuntimeItem.presentation.body
       exact terminal outcome/evidence
       typed interactions
       thread name + source evidence
       command availability
```

`TerminalControl` item 只记录 Agent history 中的一次 terminal action；live process、PTY
output、resize/terminate availability 继续由 Terminal Product feed 拥有。

## Product / Protocol ownership

### Rust and API

Owner files include:

- `crates/agentdash-application-agentrun/src/agent_run/product_protocol/feed.rs`
- `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs`
- `crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs`
- `crates/agentdash-application-agentrun/src/agent_run/product_protocol/thread_name_projection.rs`
- `crates/agentdash-application-lifecycle/src/lifecycle/surface/journey/mod.rs`
- `crates/agentdash-application-lifecycle/src/lifecycle/surface/journey/session_items.rs`
- `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- Product command/API DTO generator source

Responsibilities:

1. Snapshot/change ingress invokes the complete Runtime aggregate validator.
2. Projection fold handles `thread_name_changed` and every typed `item_transitioned`.
3. Product exposes one typed Runtime command endpoint over the durable
   `AgentRunProductCommandFacade`.
4. Compaction and interaction commands carry
   `client_command_id + expected_revision`; interaction response uses `interaction_id`.
5. Item-ID tool approval and the separate legacy context-compact routes are removed after caller
   zero.
6. AppState composition receives the existing durable Runtime command and Product mailbox facades.
7. Lifecycle VFS `session_records` is rebuilt strictly from canonical Runtime history.
   The name is valid only because its content is `fold(history)`; delivery Runtime IDs、
   presentation journal readers and independent compaction archive input are removed.
8. Thread name is read only from authoritative Runtime projection/change evidence.

This owner does not modify migration、PostgreSQL adapters、root `Cargo.lock` or workspace
membership.

### Frontend

Owner files include:

- `packages/app-web/src/services/agentRunRuntime.ts`
- `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeProjection.ts`
- `packages/app-web/src/features/agent-run-runtime/model/useAgentRunRuntimeFeed.ts`
- `packages/app-web/src/features/session/model/types.ts`
- `packages/app-web/src/features/session/model/threadItemKind.ts`
- `packages/app-web/src/features/session/model/companionSubagentDispatch.ts`
- `packages/app-web/src/features/session/ui/SessionEntry.tsx`
- `packages/app-web/src/features/session/ui/toolCardRegistry.ts`
- `packages/app-web/src/features/session/ui/bodies/**`
- compaction action/card and typed interaction UI

Target shape:

- history view model carries `ManagedRuntimeItem` without a Backbone transcode;
- registry dispatches on `presentation.body.kind`;
- display status comes from item status plus exact terminal outcome/evidence;
- completed、failed、interrupted and lost remain distinct;
- interaction request/detail/resolution is rendered by family and submitted by interaction ID;
- JSON is only displayed where the canonical contract itself defines typed arguments、result or
  versioned structured content;
- Runtime revision/sequence uses generated bigint codecs end to end.

`features/session` may remain as a history UI module. Terminal store、context delivery、
capability、mailbox and other platform state move to their Product feature owner and are composed
by the page.

## W8 persistence and composition

W8 owns only:

- `migrations/0084_*.sql`;
- final Runtime/Host repository adapters;
- Product change delivery and Product persistence composition;
- AppState/bootstrap dependency injection;
- canonical generated outputs、workspace/lock and zero-consumer deletion.

Persistence contracts:

1. Item and interaction remain nested canonical JSONB under the Runtime fact owner; no independent
   item/interaction fact tables or writer are introduced.
2. Facts、source projection/change、normalized projection/change and outbox commit in one
   transaction.
3. JSONB columns constrain expected object/array shape.
4. Load and commit recursively validate body digest、presentation digest、status/terminal、
   interaction request/status/resolution、identity、reference and revision evidence.
5. Digest uses the domain canonical SHA-256 algorithm over recursively key-sorted JSON; database
   `md5(jsonb::text)` is not evidence.
6. Same-transaction failpoints prove no partial facts/projection/change/outbox commit.
7. Restart roundtrip preserves the exact committed snapshot/change/outbox and `u64::MAX`.

## Activation order

1. Canonical Service/Runtime source contract and Codex/Native/Remote projectors pass independent
   review.
2. W8 adds recursive PostgreSQL validation/roundtrip without changing Product callers.
3. Product owner activates Rust command/query/API/Lifecycle history consumers.
4. Product owner activates the frontend canonical renderer、interaction and exact terminal
   semantics.
5. W8 composes durable services, writes canonical generated artifacts and removes zero-consumer
   legacy files.
6. Architecture and behavior checkers run against one staging tip before S5 checkpoint.

No stage creates a compatibility DTO、parallel presentation store、journal fallback or temporary
Product dependency on Runtime concrete/Service API.

## Legacy deletion input

Delete only after every real consumer reaches its final owner:

- `crates/agentdash-contracts/src/runtime/session.rs`;
- session NDJSON validator/generator roots;
- `crates/agentdash-api/src/dto/session.rs`;
- unmounted legacy AgentRun workspace/runtime-trace files after Product route inventory proves
  replacement or zero consumers;
- `packages/app-web/src/generated/backbone-protocol.ts`;
- legacy tool-approval/context-compact service helpers;
- journal/session persistence and legacy crate/workspace/lock remnants.

An omitted module/router entry is not zero-consumer evidence. Workspace、Canvas、Companion and
other Product consumers of Backbone must migrate before the generated file is deleted.

## Required tracer bullets

- snapshot baseline plus full change fold equals final snapshot;
- `item_transitioned` started、all update variants and four terminal outcomes;
- duplicate、reconnect、revision regression、sequence discontinuity and gap reload;
- compaction completed、failed、interrupted、lost with distinct status/text/presentation;
- command stdout/stderr/exit、file patch/move/read/search、MCP/dynamic、collaboration/subagent、
  web/image families;
- four interaction request families and pending/resolved/cancelled/expired/lost lifecycle;
- approve/deny/free-form/structured interaction response idempotent retry;
- source-authoritative thread-name outbox delivery and Product invalidation;
- `u64::MAX` PG/API/TypeScript bigint roundtrip without `Number`;
- PostgreSQL restart exact replay and tampered digest/terminal/interaction rejection;
- transaction failpoint leaves no partial Runtime facts.

## Directed gates

```powershell
cargo test -p agentdash-application-agentrun agent_run::product_protocol -- --nocapture
cargo test -p agentdash-api --test agent_runtime_target_projection
cargo test -p agentdash-application-lifecycle lifecycle::surface::journey -- --nocapture
cargo test -p agentdash-infrastructure final_repositories_replay_exact_facts_without_advancing_revision -- --exact --nocapture
cargo test -p agentdash-infrastructure runtime_thread_name_set_clear_replays_from_canonical_facts_and_rejects_projection_drift -- --exact --nocapture
pnpm --filter app-web exec vitest run src/features/agent-run-runtime src/features/session src/services/agentRunRuntime.test.ts
pnpm --filter app-web typecheck
pnpm contracts:check
cargo check -p agentdash-api -p agentdash-application-agentrun -p agentdash-application-lifecycle --all-targets
```

```powershell
rg -n "BackboneEvent|AgentDashThreadItem|generated/backbone-protocol" packages/app-web/src/features/session packages/app-web/src/features/agent-run-runtime
rg -n "PersistedSessionEvent|AgentRunJournal|SessionEventResponse" crates/agentdash-application-lifecycle crates/agentdash-api crates/agentdash-contracts
rg -n "runtime/tool-approvals|runtime/context/compact" packages/app-web/src
```
