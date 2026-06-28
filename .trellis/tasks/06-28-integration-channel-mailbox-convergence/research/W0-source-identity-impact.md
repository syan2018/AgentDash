# Research: W0 mailbox source identity impact

- Design context read: yes
- Query: Mailbox source identity impact surface for `.trellis/tasks/06-28-integration-channel-mailbox-convergence/work-items/W0-source-schema-baseline.md`
- Scope: internal
- Date: 2026-06-28

## Findings

### Design Boundaries

- Mailbox is a per-AgentRun durable inbox and scheduler. It is not a global channel broker.
- Source identity is an open attribution / correlation / projection model. It should not continue as a closed enum that grows a variant for each new source.
- Scheduler must not branch on source identity. Delivery remains driven by `origin`, `delivery`, `barrier`, `drain_mode`, priority and runtime state.
- `RoutineExecution` and `LifecycleGate` keep business facts. `AgentRunMailboxMessage` owns the delivery fact.

### Files Found

#### Domain

- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs` - mailbox domain model, repository trait, source identity helper constructors.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs` - mailbox application service, message creation, policy selection, scheduler.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs` - hook delivery adapter into mailbox steering envelopes.
- `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs` - ProjectAgent initial mailbox message path and tests.

#### Infrastructure

- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql` - current mailbox table and closed `source` check constraint.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs` - PostgreSQL mailbox message serialization, claiming, recovery tests.
- `crates/agentdash-infrastructure/migrations/0001_init.sql` through `0031_runner_registration_tokens.sql` - existing migration sequence.

#### API

- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - composer submit creates mailbox user messages.
- `crates/agentdash-api/src/routes/canvases.rs` - Canvas submit creates mailbox user messages.
- `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs` - domain-to-contract mailbox DTO mapper.

#### Contracts

- `crates/agentdash-contracts/src/agent/run_mailbox.rs` - Rust wire DTOs for mailbox.
- `crates/agentdash-contracts/src/generate_ts.rs` - TypeScript generation registration for mailbox DTOs.
- `packages/app-web/src/generated/agent-run-mailbox-contracts.ts` - generated frontend mailbox DTOs; currently stale relative to Rust contract.
- `packages/app-web/src/generated/workflow-contracts.ts` - imports `MailboxMessageView` for `ConversationMailboxSnapshotView`.

#### Frontend

- `packages/app-web/src/services/agentRunMailbox.ts` - frontend mailbox command service.
- `packages/app-web/src/services/canvas.ts` - Canvas agent submit consumes `AgentRunMessageCommandResponse`.
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx` - mailbox row rendering.
- `packages/app-web/src/features/agent-run-workspace/ui/mailboxContent.ts` - mailbox visibility filter.
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.test.tsx` - mailbox view fixture and UI assertions.

#### Specs / Task Docs

- `.trellis/spec/backend/session/agentrun-mailbox.md` - mailbox contract still describes `source: MailboxMessageSource`.
- `.trellis/spec/backend/session/session-startup-pipeline.md` - source adapter boundary for Routine / Companion launch facts.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - generated DTO ownership and Canvas mailbox source precedent.
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/prd.md` - source enum drift and W0 requirement.
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/design.md` - target open source identity shape and scheduler boundary.
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/subagent-dispatch.md` - explicit sub-agent boundaries and source identity gate.
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/work-items/W0-source-schema-baseline.md` - W0 deliverables and validation.

### Domain

- Current workspace already has an initial domain-side `MailboxSourceIdentity` in `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:48`. Fields are `namespace`, `kind`, `source_ref`, `correlation_ref`, `actor`, `route`, `display_label_key`, `metadata` at `:49-56`.
- Domain helper constructors exist for current built-ins: `composer()` at `:108`, `draft_start()` at `:112`, hook constructors at `:116-125`, `companion_parent_resume()` at `:128`, `workflow_orchestrator()` at `:132`, `routine_trigger()` at `:136`, `local_relay_prompt()` at `:140`, and `canvas_action()` at `:144`.
- `AgentRunMailboxMessage.source` is now `MailboxSourceIdentity` at `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:359`.
- `NewAgentRunMailboxMessage.source` is now `MailboxSourceIdentity` at `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:395`.
- Application command structs already carry `MailboxSourceIdentity`: `AgentRunMailboxUserMessageCommand.source` at `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs:119` and `AgentRunMailboxUserMessageTargetCommand.source` at `:132`.
- User message creation passes the identity into the durable envelope at `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs:361-367`.
- Hook auto-resume uses `MailboxSourceIdentity::hook_auto_resume()` at `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs:489`.
- Hook steering accepts a generic `MailboxSourceIdentity` at `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs:520-527`; dedup uses `source.dedup_fragment()` at `:569-572`.
- Hook runtime adapter constructs identities for after-turn and before-stop at `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:438-450` and `:495-500`.
- ProjectAgent initial command now uses `MailboxSourceIdentity::draft_start()` at `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:72` and test helper at `:1927`.

### Infrastructure

- Migration `0013_agent_run_mailbox.sql` still stores only `agent_run_mailbox_messages.source text NOT NULL` at `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:65`.
- The same migration still adds closed `agent_run_mailbox_messages_source_check` at `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:134-149`. It includes `routine_executor` but omits `canvas_action`, matching the drift called out by the PRD.
- Current migration list ends at `crates/agentdash-infrastructure/migrations/0031_runner_registration_tokens.sql`; W0 should add the next migration as `0032_*.sql`.
- Recommended migration scope: add source identity columns for namespace/kind/source_ref/correlation_ref/actor/route/display_label_key/metadata_json or equivalent JSON shape, backfill existing `source` values, remove or bypass the closed `source` check constraint, and keep `source_dedup_key` independent.
- Repository still selects `origin,source,...` through `MAILBOX_COLS` at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:100-101`.
- Repository insert still writes `(origin, source, delivery, ...)` at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:117-123`.
- Repository bind currently calls `message.source.as_str()` at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:130`; this is stale against the current `MailboxSourceIdentity` struct unless an `as_str()` compatibility method is added, which would conflict with the W0 direction.
- Row deserialization still has a single `source: String` field at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:645`.
- Row conversion still calls `MailboxMessageSource::try_from(row.source.as_str())?` at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:686`; this is stale because domain no longer defines that enum in the observed workspace.
- Repository tests still construct `MailboxMessageSource::Composer` at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:816`.

### API

- Composer submit already constructs `MailboxSourceIdentity::composer()` at `crates/agentdash-api/src/routes/lifecycle_agents.rs:466-477`.
- Canvas submit already constructs `MailboxSourceIdentity::canvas_action()` at `crates/agentdash-api/src/routes/canvases.rs:849-860`.
- API mailbox DTO mapper now maps domain `MailboxSourceIdentity` into contract `MailboxSourceIdentity` at `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:219-231`.
- `MailboxMessageView.source` is assigned from `mailbox_source_view(message.source)` at `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:111-115`.

### Contracts

- Rust contract now defines `MailboxSourceIdentity` at `crates/agentdash-contracts/src/agent/run_mailbox.rs:36-55`.
- Rust contract `MailboxMessageView.source` now uses `MailboxSourceIdentity` at `crates/agentdash-contracts/src/agent/run_mailbox.rs:109-113`.
- TS generator imports and exports `MailboxSourceIdentity` at `crates/agentdash-contracts/src/generate_ts.rs:8-14` and `:276-295`.
- Generated TypeScript is stale: `packages/app-web/src/generated/agent-run-mailbox-contracts.ts:53` still exports `MailboxMessageSource` as a closed string union, and `:57` still types `MailboxMessageView.source` as that union.
- Contract naming caveat: design/W0 says `metadata_json`; current Rust domain and contract field is `metadata` (`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:56`, `crates/agentdash-contracts/src/agent/run_mailbox.rs:54`). Decide whether the wire/DB field should be `metadata` or `metadata_json` before migration and generated TS are finalized.

### Frontend

- Frontend mailbox services consume generated command/response types only; `packages/app-web/src/services/agentRunMailbox.ts:2-8` imports `AgentRunMessageCommandResponse` and related DTOs from generated mailbox contracts.
- Workspace mailbox rendering consumes `MailboxMessageView` at `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:6-10`.
- Current mailbox row UI does not render source labels. It classifies rows by `delivery.kind` at `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:51-57`, displays origin text for steer messages at `:210-213`, and displays preview/status/actions at `:226-257`.
- `mailboxHasContent` filters only by `delivery.kind` and `origin` at `packages/app-web/src/features/agent-run-workspace/ui/mailboxContent.ts:13-20`.
- Test fixture still sets `source: "composer"` at `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.test.tsx:14-18`; after generated contract update this must become an object identity.
- W8 will need a frontend projection decision for `display_label_key` / `namespace` / `kind`; W0 should at least make the generated type ergonomic and avoid hand-written source unions.

### Tests

- Repository tests under `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs` cover source dedup, claim, recovery and status behavior, but still use the old source enum in helpers at `:803-821`.
- ProjectAgent start tests still assert the source identity in captured commands/messages; observed workspace imports `MailboxSourceIdentity` at `crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:876-880`, uses identity helper at `:1927`, and should update any equality assertions to compare identity shape rather than old enum.
- Frontend mailbox row test fixture must update from a closed string source to a source identity object at `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.test.tsx:14-31`.
- Required validation after W0: `cargo test -p agentdash-domain agent_run_mailbox`, `cargo test -p agentdash-infrastructure agent_run_mailbox`, and `pnpm run contracts:check` per W0. Add or adjust serialization / repository / contract drift tests around source identity.

### Specs

- `.trellis/spec/backend/session/agentrun-mailbox.md:53` still documents `pub source: MailboxMessageSource`; it needs update after implementation lands.
- `.trellis/spec/backend/session/agentrun-mailbox.md:105` still names `MailboxMessageSource::DraftStart`.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md:413` and `:440` still mention `MailboxMessageSource::CanvasAction`.
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/design.md:145` is the key scheduler boundary: source identity is for attribution/dedup/correlation/projection, not delivery semantics.

### Scheduler Evidence

- Scheduler entry `schedule_for_target` branches only on `AgentRunMailboxScheduleTrigger` and runtime state at `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs:1015-1122`.
- Claiming uses `AgentRunMailboxClaimRequest { barriers, drain_mode, limit, ... }` at `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs:1205-1217`; no source identity is passed to the claim request.
- Repository `claim_next` filters by owner/runtime/status/barrier/drain_mode at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:216-248`; no source filter appears in scheduling claims.
- Consuming branches on `message.delivery`, not source, around `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs:1375-1412`.
- The only scheduler-adjacent source use found is dedup construction for hook delivery at `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs:569-572`, which is attribution/idempotency, not scheduling.

### Suggested Execution Order

1. Finalize the source identity shape and naming (`metadata` vs `metadata_json`; separate columns vs JSON object) in domain and contract.
2. Add migration `0032_*.sql`: add/backfill open source identity fields, drop the closed `agent_run_mailbox_messages_source_check`, preserve `source_dedup_key`.
3. Update PostgreSQL repository constants, insert binds, row struct, row-to-domain conversion, and repository tests to use source identity fields.
4. Update API mapper and command call sites if any stale references remain; composer/Canvas/ProjectAgent/hook paths are already mostly on identity in the observed workspace.
5. Regenerate/check TypeScript contracts so `packages/app-web/src/generated/agent-run-mailbox-contracts.ts` exports `MailboxSourceIdentity` and `MailboxMessageView.source` as that object.
6. Update frontend fixtures and UI source label projection preparation. Do not invent a frontend closed source union.
7. Update specs after code behavior is stable: mailbox spec and frontend/backend contract should describe `MailboxSourceIdentity` and Canvas submit using identity, not enum variants.
8. Run W0 validations: domain mailbox tests, infrastructure mailbox tests, `pnpm run contracts:check`, and a focused frontend type/test pass for mailbox UI fixtures.

## External References

- None. This was an internal codebase/spec impact scan.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this sub-agent environment. The report uses the explicit task path provided in the prompt.
- The workspace changed while this research ran. The observed final state has partial W0 implementation already in domain/application/API/contract, while infrastructure and generated TS still show old closed-source assumptions. Line references reflect the latest reads during this research turn.
- No scheduler branch on source identity was found.
- No frontend mailbox source-label rendering was found; current mailbox UI mainly displays preview/status/actions and minimal origin text.
- No migration after `0031_runner_registration_tokens.sql` was found; W0 should use migration number `0032`.
