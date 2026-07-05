# Research: WI-02 RuntimeSession trace store decomposition current state

- Query: WI-02 executable fact inventory for RuntimeSession / SessionPersistence / runtime trace store decomposition
- Scope: internal
- Date: 2026-07-04

## Findings

### Files found

| Path | Description |
| --- | --- |
| `crates/agentdash-spi/src/session_persistence.rs` | Defines seven narrow session store traits plus the remaining `SessionPersistence` mega trait. |
| `crates/agentdash-application-runtime-session/src/session/persistence.rs` | Defines `SessionStoreSet`, which still bundles all seven stores and can be built from one all-traits implementation. |
| `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` | One `PostgresSessionRepository` implements all runtime trace store traits and writes `sessions` / `session_*` tables. |
| `crates/agentdash-application-runtime-session/src/session/core.rs` | Runtime trace meta service; still accepts full `SessionStoreSet` but mostly uses `meta`. |
| `crates/agentdash-application-runtime-session/src/session/eventing.rs` | Event append/read-model service; uses `meta`, `events`, and projection stores. |
| `crates/agentdash-application-runtime-session/src/session/context_projector.rs` | Model-context projector; uses projection/event/compaction stores only. |
| `crates/agentdash-application-runtime-session/src/session/branching.rs` | Runtime trace fork / rollback / lineage service; uses meta, events, projection, compaction, lineage. |
| `crates/agentdash-application-runtime-session/src/session/launch/orchestrator.rs` | Launch path consumes trace meta and runtime delivery command outbox. |
| `crates/agentdash-application-runtime-session/src/session/launch/commit.rs` | Accepted launch commit writes meta and marks runtime delivery commands applied/failed. |
| `crates/agentdash-application-runtime-session/src/session/terminal_effects.rs` | Already has a narrow `TerminalEffectDeps` dependency over `SessionTerminalEffectStore`. |
| `crates/agentdash-api/src/bootstrap/repositories.rs` | Production cloud bootstrap builds one `PostgresSessionRepository` and then `SessionStoreSet::from_shared_store`. |
| `crates/agentdash-local/src/runtime.rs` | Local embedded runtime also builds `SessionStoreSet::from_shared_store`. |
| `crates/agentdash-api/src/routes/sessions.rs` | Raw `/sessions/*` read routes, currently annotated as internal diagnostics for projection/meta/lineage/audit. |
| `crates/agentdash-api/src/routes/lifecycle_agents.rs` | AgentRun-scoped runtime endpoints resolve current delivery then reuse session trace helper functions. |
| `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts` | Frontend AgentRun workspace projection state still stores `runtime_session_id`. |
| `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` | Workspace runtime data still maps delivery runtime id into `sessionId` / `runtimeSessionId`. |
| `crates/agentdash-infrastructure/migrations/0001_init.sql` | Baseline schema creates `sessions`, `session_events`, `session_projection_*`, `session_compactions`, `session_runtime_commands`, `session_terminal_effects`, `session_lineage`. |
| `crates/agentdash-infrastructure/migrations/0040_session_events_envelope_only.sql` | Removes flattened event columns from `session_events`; final event log is envelope-only. |
| `crates/agentdash-infrastructure/migrations/0043_agent_run_mailbox_runtime_ref_nullable.sql` | Makes mailbox runtime session refs nullable and `ON DELETE SET NULL`. |
| `crates/agentdash-infrastructure/migrations/0044_agent_run_delivery_bindings.sql` | Introduces AgentRun-owned delivery binding and changes anchor/session FK to `ON DELETE RESTRICT`. |

### Related specs

- `.trellis/spec/backend/repository-pattern.md`: Session runtime persistence is not through `RepositorySet`; `SessionPersistence` and child records live in SPI.
- `.trellis/spec/backend/database-guidelines.md`: schema changes belong in migrations; ordinary tasks add new migrations rather than editing committed history.
- `.trellis/spec/backend/session/architecture.md`: current `Session` is semantically `RuntimeSession`, owning turn/tool/event/resume/debug/projection/trace lineage, not product ownership or permission.
- `.trellis/spec/backend/session/runtime-execution-state.md`: defines store boundaries for `SessionMetaStore`, `SessionEventStore`, terminal effects, runtime commands, projection, lineage; also states runtime session id is a trace ref.
- `.trellis/spec/backend/session/context-compaction-projection.md`: explains why compaction/head/segments remain independent projection stores.
- `.trellis/spec/backend/session/session-lineage-projection.md`: runtime lineage is diagnostic trace provenance; product fork is AgentRun.
- `.trellis/spec/backend/session/agentrun-mailbox.md`: AgentRun mailbox is durable queue; runtime session id is nullable delivery evidence.
- `.trellis/spec/backend/runtime-gateway.md`: RuntimeGateway session actions must resolve current surface through AgentRun control-plane facts, not SessionHub cache.

### Current trait and store shape

`SessionPersistence` is already mostly decomposed at the trait-method level: the SPI file defines narrow traits for meta, events, terminal effects, runtime commands, compactions, projections, and lineage (`crates/agentdash-spi/src/session_persistence.rs:792`, `crates/agentdash-spi/src/session_persistence.rs:801`, `crates/agentdash-spi/src/session_persistence.rs:831`, `crates/agentdash-spi/src/session_persistence.rs:856`, `crates/agentdash-spi/src/session_persistence.rs:881`, `crates/agentdash-spi/src/session_persistence.rs:895`, `crates/agentdash-spi/src/session_persistence.rs:919`).

The old broad concept still exists as a blanket trait over all seven stores: `SessionPersistence` is documented as an all-store port and lets consumers depend on `dyn SessionPersistence` to reach every child store (`crates/agentdash-spi/src/session_persistence.rs:949`, `crates/agentdash-spi/src/session_persistence.rs:953`). It is re-exported from SPI (`crates/agentdash-spi/src/lib.rs:165` from search output). Current production search did not find a direct `dyn SessionPersistence` consumer, so deleting the trait/export is a low-risk removal of an obsolete combination point.

`SessionStoreSet` still packages all seven stores into one struct (`crates/agentdash-application-runtime-session/src/session/persistence.rs:15`) and `from_shared_store` requires one object to implement every store trait before cloning it into all fields (`crates/agentdash-application-runtime-session/src/session/persistence.rs:26`). Cloud bootstrap constructs one `PostgresSessionRepository` and immediately wraps it with `SessionStoreSet::from_shared_store` (`crates/agentdash-api/src/bootstrap/repositories.rs:67`, `crates/agentdash-api/src/bootstrap/repositories.rs:68`); local embedded runtime does the same (`crates/agentdash-local/src/runtime.rs:582`, `crates/agentdash-local/src/runtime.rs:584`).

The Postgres adapter is a valid shared physical implementation, but it still reinforces the all-store injection pattern because one struct implements every store trait: meta (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:158`), event log (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:318`), terminal effects (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:541`), runtime commands (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:679`), compactions (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:881`), projections (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:935`), and lineage (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:1143`).

### Runtime trace store internal facts

These capabilities are runtime trace store internal facts:

- Trace meta CRUD and trace-head cache: `SessionMetaStore` exposes create/get/list/save/delete (`crates/agentdash-spi/src/session_persistence.rs:792`). Postgres writes `sessions` for title/source/head/status/turn/executor fields (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:163`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:188`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:216`). `save_session_meta` is still broad enough to rewrite the full meta row (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:216`).
- Event log: `SessionEventStore::append_event` is the durable append entry (`crates/agentdash-spi/src/session_persistence.rs:801`). Postgres increments `sessions.last_event_seq`, inserts `session_events`, and updates trace-head cache in one transaction (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:330`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:358`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:374`).
- Terminal effect outbox: terminal effect insert/status/list methods are separate from terminal event facts (`crates/agentdash-spi/src/session_persistence.rs:831`). The dispatcher depends only on `SessionTerminalEffectStore` (`crates/agentdash-application-runtime-session/src/session/terminal_effects.rs:63`) and writes/claims/marks outbox records (`crates/agentdash-application-runtime-session/src/session/terminal_effects.rs:221`, `crates/agentdash-application-runtime-session/src/session/terminal_effects.rs:271`, `crates/agentdash-application-runtime-session/src/session/terminal_effects.rs:387`).
- Runtime delivery command outbox: `SessionRuntimeCommandStore` names `upsert_runtime_delivery_command`, requested list, applied/failed transitions (`crates/agentdash-spi/src/session_persistence.rs:856`). Launch reads requested commands (`crates/agentdash-application-runtime-session/src/session/launch/orchestrator.rs:48`, `crates/agentdash-application-runtime-session/src/session/launch/orchestrator.rs:83`) and accepted commit marks them applied/failed (`crates/agentdash-application-runtime-session/src/session/launch/commit.rs:80`, `crates/agentdash-application-runtime-session/src/session/launch/commit.rs:225`).
- Projection and compaction: `ContextProjector` reads projection head, events, and compactions only (`crates/agentdash-application-runtime-session/src/session/context_projector.rs:29`, `crates/agentdash-application-runtime-session/src/session/context_projector.rs:55`, `crates/agentdash-application-runtime-session/src/session/context_projector.rs:120`). Eventing commits compaction projection through projections after reading events (`crates/agentdash-application-runtime-session/src/session/eventing.rs:577`, `crates/agentdash-application-runtime-session/src/session/eventing.rs:580`).
- Runtime lineage: `SessionBranchingService::fork_session` creates a child trace, writes `SessionLineageRecord`, and commits initial fork projection (`crates/agentdash-application-runtime-session/src/session/branching.rs:65`, `crates/agentdash-application-runtime-session/src/session/branching.rs:97`, `crates/agentdash-application-runtime-session/src/session/branching.rs:124`). `rollback_model_projection` appends a rollback event and moves projection head (`crates/agentdash-application-runtime-session/src/session/branching.rs:164`, `crates/agentdash-application-runtime-session/src/session/branching.rs:221`, `crates/agentdash-application-runtime-session/src/session/branching.rs:247`).

### Product-surface exposure and diagnostic/internal routes

Current raw session router is read-only in this file: `GET /sessions/{id}`, runtime-control, meta, state, events, context projection, lineage, audit, and stream (`crates/agentdash-api/src/routes/sessions.rs:83`). The context projection, lineage, meta, and audit handlers are explicitly marked internal diagnostics (`crates/agentdash-api/src/routes/sessions.rs:778`, `crates/agentdash-api/src/routes/sessions.rs:921`, `crates/agentdash-api/src/routes/sessions.rs:949`, `crates/agentdash-api/src/routes/sessions.rs:1188`). These routes call `ensure_session_permission`, which resolves the `RuntimeSessionExecutionAnchor` and checks Project permission through the owning run (`crates/agentdash-api/src/routes/sessions.rs:70`, `crates/agentdash-api/src/routes/sessions.rs:76`, `crates/agentdash-api/src/routes/sessions.rs:77`).

No current raw `POST /sessions/{id}/fork`, raw projection rollback route, or raw session delete route was found in `crates/agentdash-api/src/routes/sessions.rs`. The runtime branching service still has fork/rollback methods, but the route surface appears diagnostic/read-only at this point. This means WI-02 should not spend effort on raw route deletion; that is WI-01/WI-09 verification unless new callers appear.

AgentRun-scoped runtime endpoints are application query/command ports over a resolved current delivery runtime session. For example, AgentRun runtime control resolves run/agent, extracts current delivery runtime session, then calls the session helper (`crates/agentdash-api/src/routes/lifecycle_agents.rs:1190`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1199`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1206`). AgentRun runtime events/context/audit/stream do the same (`crates/agentdash-api/src/routes/lifecycle_agents.rs:1221`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1287`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1301`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1316`). Tool approval remains product-scoped by route but delegates to session control after resolving delivery (`crates/agentdash-api/src/routes/lifecycle_agents.rs:178`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1361`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1363`).

Frontend product workspace still leaks delivery trace identity into product state. `AgentRunWorkspaceProjectionState` stores `runtime_session_id` (`packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:10`, `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:16`), derives it from `workspace.delivery_runtime_ref` (`packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:139`, `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:140`), and `AgentRunWorkspacePage` maps it into `WorkspaceRuntimeData.sessionId` / `runtimeSessionId` (`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:511`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:513`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:514`). This is a WI-09/WI-01 product identity cleanup, not a WI-02 store decomposition prerequisite.

Contracts have mixed state: `ProjectAgentRunStartResult` no longer exposes a top-level `runtime_session_id` (`crates/agentdash-contracts/src/agent/project_agent.rs:78` through `crates/agentdash-contracts/src/agent/project_agent.rs:92`), but `AgentRunWorkspaceView` still carries `delivery_runtime_ref` and `delivery_trace_meta` (`crates/agentdash-contracts/src/runtime/workflow.rs:1374`, `crates/agentdash-contracts/src/runtime/workflow.rs:1381`, `crates/agentdash-contracts/src/runtime/workflow.rs:1384`). That can be valid diagnostic/delivery evidence if product command availability stops using it as identity.

### Application command/query ports

These are ports over runtime trace facts, not runtime trace store internals to collapse:

- `RuntimeSessionCreationPort` is already implemented by `SessionMetaStoreRuntimeSessionCreator` with only `SessionMetaStore` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:35`, `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:46`, `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:70`).
- `DeliveryRuntimeSelectionService` selects current delivery from AgentRun-owned `AgentRunDeliveryBindingRepository`, then validates the anchor (`crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:107`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:160`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:166`). Its output keeps business address and message stream trace ref separated (`crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:262`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:273`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:278`).
- `AgentRunForkService` uses `SessionBranchingService::fork_session` as a runtime projection primitive, then continues with AgentRun product materialization (`crates/agentdash-application-agentrun/src/agent_run/fork.rs:101`, `crates/agentdash-application-agentrun/src/agent_run/fork.rs:257`). It also calls `SessionCoreService::delete_session` for child runtime cleanup on failure (`crates/agentdash-application-agentrun/src/agent_run/fork.rs:581`). WI-02 constructor changes to branching/core can affect WI-08 call sites.
- `RuntimeSessionExecutionAnchorRepository` has already moved to create-once semantics: the trait says same coordinates are idempotent and different coordinates conflict (`crates/agentdash-domain/src/workflow/repository.rs:154`, `crates/agentdash-domain/src/workflow/repository.rs:157`); Postgres does `ON CONFLICT DO NOTHING`, then compares existing coordinates and errors on mismatch (`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:763`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:789`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:799`).
- AgentRun-owned delivery binding is already in schema and repository: trait at `crates/agentdash-domain/src/workflow/repository.rs:84`; migration creates `agent_run_delivery_bindings` (`crates/agentdash-infrastructure/migrations/0044_agent_run_delivery_bindings.sql:1`), backfills from old lifecycle agent columns (`crates/agentdash-infrastructure/migrations/0044_agent_run_delivery_bindings.sql:117`), then drops `lifecycle_agents.current_delivery_*` columns (`crates/agentdash-infrastructure/migrations/0044_agent_run_delivery_bindings.sql:168`).

### Physical trace tables and D-017 rationale

Runtime trace tables should remain independent tables after naming cleanup because each table has a positive storage reason:

| Table | Current final role | Keep independent because |
| --- | --- | --- |
| `sessions` | Runtime trace root / trace-head meta | Trace identity, event sequence allocation, connector continuation, recovery head. |
| `session_events` | Append-only BackboneEnvelope event log | `(session_id, event_seq)` primary key and transactionally assigned order; 0040 removed derived columns (`crates/agentdash-infrastructure/migrations/0040_session_events_envelope_only.sql:1`). |
| `session_compactions` | Compaction/checkpoint record | Projection checkpoint audit and source range (`crates/agentdash-infrastructure/migrations/0001_init.sql:547`). |
| `session_projection_heads` | Active projection cursor | `(session_id, projection_kind)` current head (`crates/agentdash-infrastructure/migrations/0001_init.sql:601`, `crates/agentdash-infrastructure/migrations/0001_init.sql:972`). |
| `session_projection_segments` | Rebuildable projection segments | Unique ordered segments per session/kind/version (`crates/agentdash-infrastructure/migrations/0001_init.sql:611`, `crates/agentdash-infrastructure/migrations/0001_init.sql:978`). |
| `session_runtime_commands` | Runtime delivery command outbox | Requested/applied/failed state and frame transition reference (`crates/agentdash-infrastructure/migrations/0001_init.sql:629`, `crates/agentdash-infrastructure/migrations/0001_init.sql:640`). |
| `session_terminal_effects` | Terminal side-effect outbox | Pending/running/succeeded/failed/dead-letter replay and recovery (`crates/agentdash-infrastructure/migrations/0001_init.sql:643`). |
| `session_lineage` | Runtime trace branch provenance | Child trace parent edge and fork projection coordinate (`crates/agentdash-infrastructure/migrations/0001_init.sql:587`). |
| `runtime_session_execution_anchors` | Runtime trace -> control-plane reverse index | Required by permission and AgentRun delivery resolution (`crates/agentdash-infrastructure/migrations/0001_init.sql:533`); 0044 changed runtime session FK to `ON DELETE RESTRICT` (`crates/agentdash-infrastructure/migrations/0044_agent_run_delivery_bindings.sql:53`). |

The remaining table issue for WI-02 is naming: code and migrations still use `sessions` / `session_*`, not `runtime_sessions` / `runtime_session_*`. Search found no schema/code table named `runtime_sessions` or `runtime_session_events`. Because this is schema-wide, it needs a migration and should be registered with WI-12 rather than mixed into the first port-decomposition slice.

### D-003 / D-004 / D-015 / D-017 assessment

| Decision | Current assessment | WI-02 implication |
| --- | --- | --- |
| D-003 RuntimeSession is internal trace substrate | Backend store facts are already trace-shaped: event log, projection, outboxes, lineage. Raw session API is currently read-only diagnostic in `sessions.rs`; product-scoped AgentRun routes still reuse trace helpers after delivery resolution. | Keep trace stores; do not delete runtime trace/debug read paths. Name ports/tables as runtime trace, and avoid product words in store traits. |
| D-004 SessionPersistence mega trait -> narrow ports | Narrow traits exist, but the mega trait/export and `SessionStoreSet::from_shared_store` preserve all-store injection. Many runtime-session services take full `SessionStoreSet` even when they use one or two stores. | Delete `SessionPersistence`; replace broad service fields/constructors with narrow deps by service. Keep a composition/root-only helper if needed, but do not expose it as a use-case dependency. |
| D-015 permission/product API identity control-plane scoped | Raw sessions use anchor-derived permission. Product workspace still holds runtime session id as state and `WorkspaceRuntimeData.sessionId`. | WI-02 should not solve frontend identity; record as WI-09/WI-01 parallel work. Store decomposition must not introduce new product endpoints keyed only by runtime session id. |
| D-017 physical storage by lock/query/rebuild need | Trace tables each have append/order, projection checkpoint, outbox, lineage, or reverse-index reasons. Mailbox/runtime ref and delivery binding migrations already moved away from runtime ownership. | Preserve independent trace tables; the executable cleanup is destructive renaming and port naming, not table merging. |

### Executable implementation slices

#### 1. Delete `SessionPersistence` mega trait and production all-store constructor path

- Deletion target: remove the old "one trait grants all trace stores" concept.
- Allowed write range: `crates/agentdash-spi/src/session_persistence.rs`, `crates/agentdash-spi/src/lib.rs`, `crates/agentdash-application-runtime-session/src/session/persistence.rs`, production bootstraps in `crates/agentdash-api/src/bootstrap/repositories.rs` and `crates/agentdash-local/src/runtime.rs`, plus affected tests/imports.
- Shape: remove `SessionPersistence` trait/export first; introduce an explicit `SessionStoreSet::new(meta, events, terminal_effects, runtime_commands, compactions, projections, lineage)` if needed. Keep `PostgresSessionRepository` implementing all traits, but make the composition root pass each trait object explicitly instead of requiring an all-traits bound. If test helper convenience remains, name it test-only or runtime-trace helper rather than `from_shared_store`.
- Migration: no.
- Validation commands: `rg "SessionPersistence" crates`; `rg "from_shared_store" crates`; `cargo check -p agentdash-spi -p agentdash-application-runtime-session -p agentdash-api`.
- Parallel with WI-08: yes if call signatures for `SessionBranchingService` / `SessionCoreService` are not changed yet.
- Parallel with WI-04: yes; it does not touch mailbox ownership or command receipt schema.

#### 2. Narrow runtime-session service constructors by actual store usage

- Deletion target: delete full `SessionStoreSet` as a service dependency where the service only uses a subset.
- Allowed write range: `crates/agentdash-application-runtime-session/src/session/{core.rs,eventing.rs,context_projector.rs,branching.rs,runtime_control.rs,runtime_services.rs,runtime_builder.rs,hub/**,launch/**}`, plus direct constructor call sites in `crates/agentdash-api/src/bootstrap/session.rs`, `crates/agentdash-api/src/bootstrap/vfs.rs`, `crates/agentdash-local/src/runtime.rs`, tests.
- Shape: introduce small dependency structs by service: `SessionCoreStores { meta }`; `ContextProjectionStores { events, projections, compactions }`; `SessionBranchingStores { meta, events, projections, compactions, lineage }`; `LaunchRuntimeStores { meta, runtime_commands }`; keep `TerminalEffectDeps` as the model because it is already narrow (`crates/agentdash-application-runtime-session/src/session/terminal_effects.rs:63`).
- Migration: no.
- Validation commands: `rg "stores: SessionStoreSet|SessionStoreSet" crates/agentdash-application-runtime-session/src/session`; `cargo check -p agentdash-application-runtime-session -p agentdash-api`.
- Parallel with WI-08: only partially. Avoid changing `SessionBranchingService::new` or `SessionCoreService` public call sites while WI-08 is editing `agent_run/fork.rs`, or add compatible constructors for one release of the branch.
- Parallel with WI-04: yes unless WI-04 touches runtime launch/mailbox adapter constructors in the same files.

#### 3. Rename runtime command/outbox vocabulary without schema rename

- Deletion target: delete the misleading "session runtime commands == user command" naming at the Rust API level.
- Allowed write range: `crates/agentdash-spi/src/session_persistence.rs`, `crates/agentdash-application-runtime-session/src/session/runtime_commands.rs`, `session/launch/**`, `session/hub/facade.rs`, `session/runtime_transition_service.rs`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`, tests.
- Shape: rename trait/type aliases toward `RuntimeSessionDeliveryCommandOutbox` / `RuntimeDeliveryCommandRecord` semantics while leaving `session_runtime_commands` table untouched in this slice. Ensure AgentRun `AgentRunCommandReceipt` remains the only client-command idempotency fact.
- Migration: no for this slice.
- Validation commands: `rg "SessionRuntimeCommandStore|session_runtime_commands|RuntimeSessionCommandStateDto" crates/agentdash-application-runtime-session crates/agentdash-spi crates/agentdash-infrastructure`; `cargo check -p agentdash-spi -p agentdash-application-runtime-session -p agentdash-infrastructure`.
- Parallel with WI-08: yes if not changing `SessionBranchingService`.
- Parallel with WI-04: coordinate. WI-04 owns command/mailbox/delivery vocabulary and may touch `RuntimeSessionCommandStateDto` or mailbox outcome naming.

#### 4. Destructive runtime trace table rename with WI-12

- Deletion target: delete generic `sessions` / `session_*` table names for runtime trace storage.
- Allowed write range: new migration under `crates/agentdash-infrastructure/migrations/NNNN_*.sql`, SQL strings and row mappers in `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`, schema readiness/guard list in `crates/agentdash-infrastructure/src/migration.rs`, affected integration tests. Do not include frontend/API product identity cleanup here.
- Shape: rename `sessions -> runtime_sessions`, `session_events -> runtime_session_events`, `session_compactions -> runtime_session_compactions`, `session_projection_heads -> runtime_session_projection_heads`, `session_projection_segments -> runtime_session_projection_segments`, `session_runtime_commands -> runtime_session_delivery_commands` or `runtime_session_runtime_commands` after naming decision, `session_terminal_effects -> runtime_session_terminal_effects`, `session_lineage -> runtime_session_lineage`. Update FKs from anchor, mailbox runtime refs, delivery binding, and trace projection tables.
- Migration: yes, mandatory. Needs WI-12 scheduling because 0043/0044 already changed important FKs and because table rename touches readiness and embedded PostgreSQL initialization.
- Validation commands: `pnpm run migration:guard`; `cargo check -p agentdash-infrastructure -p agentdash-api -p agentdash-application-runtime-session`; runtime-session repository tests if available; embedded Postgres clean initialization.
- Parallel with WI-08: no if WI-08 is adding fork-lineage migrations or touching session lineage SQL.
- Parallel with WI-04: no if WI-04 is touching mailbox runtime FK/index migrations; otherwise code-only WI-04 can proceed but migration files must be serialized by main session.

## Recommended next implementation slice

Start with slice 1: delete `SessionPersistence` mega trait/export and replace production `SessionStoreSet::from_shared_store` construction with explicit store-field composition.

Reasons:

- It removes an old broad access concept directly tied to D-004.
- Current search shows no direct production consumer of `dyn SessionPersistence`, so the blast radius is mainly imports/bootstrap/tests.
- It requires no migration and should not conflict with WI-04 mailbox owner work or WI-08 product fork work if service constructor signatures remain stable.
- It creates a clean base for slice 2 to narrow individual service constructors without also carrying the SPI cleanup.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task; this file was written under the explicit task path supplied in the user request.
- Raw session product write routes named fork/delete/projection rollback were not found in the current `sessions.rs` router; current raw route evidence is read-only diagnostics.
- No `runtime_sessions` / `runtime_session_events` table names were found in the current migration/code search; WI-02 table naming remains future migration work.
- I did not use external web references; this was an internal code/spec research pass.
- I did not run git validation commands while writing this file because the trellis-research worker scope forbids git operations.
