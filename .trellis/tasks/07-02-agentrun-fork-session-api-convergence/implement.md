# Implementation Plan

## Delivery Strategy

Use this single Trellis task as the tracking container. Implementation is split into independently verifiable work items under this task, not into separate Trellis tasks.

Recommended push order:

1. Planning commit: commit only this Trellis task directory so the agreed architecture, permission model and delivery slices are reviewable before code changes.
2. Work item A, Project participation permissions: migrate `viewer` semantics to `member`, reframe permissions as `Use / Configure / ManageSharing`, and audit current route checks. This unblocks AgentRun participation without touching fork internals.
3. Work item B, AgentRun ownership facts: add `created_by_user_id` / ownership projections and make workspace/composer know whether the current user controls the Run.
4. Work item C, Session surface safety: add missing permission checks to retained internal Session routes and introduce AgentRun scoped runtime endpoints. This reduces leakage before frontend migration.
5. Work item D, AgentRun fork backend: add cross-run fork lineage, `AgentRunForkMaterializationPort`, `AgentRunForkService`, command receipt idempotency and mailbox delivery.
6. Work item E, frontend fork actions: migrate product callers away from direct Session APIs, handle fork redirect outcomes, and add per-turn copy/fork toolbar.
7. Final spec/check pass: update `.trellis/spec/`, run cross-layer searches and targeted backend/frontend tests.

Track work item status in this file. Each work item must keep explicit dependencies, scope, acceptance and validation notes here; do not create separate Trellis tasks for this scope.

## Work Item Tracker

| Item | Status | Summary |
| --- | --- | --- |
| A | completed | Project participation permissions |
| B | completed | AgentRun ownership facts |
| C | pending | Session surface safety and AgentRun runtime endpoints |
| D | pending | AgentRun fork backend |
| E | pending | Frontend fork and Session API convergence |
| F | pending | Spec and final integration |

### A. Project Participation Permissions

Purpose: make project membership mean product participation, while preserving a separate configuration boundary.

Dependencies: planning commit only.

Scope:

- Rename/reframe domain permission vocabulary to `Use / Configure / ManageSharing`.
- Migrate `ProjectRole::Viewer` product semantics to `ProjectRole::Member` and update parsing/serialization/contracts.
- Audit every current `ProjectPermission::Edit` usage.
- Move AgentRun participation endpoints to `Use`: AgentRun list/workspace read, ProjectAgent run start, composer submit for own Run, fork/fork-submit, mailbox resume when it only continues the user's own Run.
- Keep Project asset mutation endpoints on `Configure`: ProjectAgent CRUD, Project config, VFS mounts/surfaces writes, backend access, workflows, MCP presets, skill assets, extension installation/configuration.
- Keep sharing/member management on `ManageSharing`.

Out of scope:

- Do not add AgentRun fork service in this task.
- Do not add AgentRun ownership fields beyond whatever tests need as fixtures.

Acceptance:

- Existing member-level users can start or fork their own AgentRun without Project configuration rights.
- Users without Project membership cannot read or use AgentRun surfaces.
- Asset configuration endpoints reject `member` and allow `editor` / `owner`.
- Generated contracts and frontend role labels no longer expose `viewer` as a long-term product role.

Validation:

```powershell
cargo test -p agentdash-domain project
cargo test -p agentdash-api project permission agent_run
pnpm --filter app-web test -- project permission
```

### B. AgentRun Ownership Facts

Purpose: make Run control ownership explicit so composer behavior can decide between original Run continuation and fork continuation.

Dependencies: work item A permission vocabulary is either complete or its compatibility shims are in place.

Scope:

- Add migration fields for `lifecycle_runs.created_by_user_id` and `lifecycle_agents.created_by_user_id`.
- Update domain entities, repository traits, Postgres repositories and memory test repositories.
- Populate ownership on ProjectAgent start, lifecycle materialization, test launch helpers and any existing launch path.
- Expose ownership/control in `AgentRunWorkspaceView` and command state models.
- Add control predicate: current user controls Run when they own the AgentRun or hold a future explicit control grant.
- Add `AgentRunCommandKind::AgentRunFork` and `AgentRunCommandKind::AgentRunForkSubmit`.

Out of scope:

- Do not change composer submit routing to fork yet.
- Do not add cross-run lineage.

Acceptance:

- Workspace payload tells frontend whether current user controls the AgentRun.
- Current owner can still submit to their Run.
- Non-owner editor/member can view the Run but is represented as non-control.
- Backfilled rows have deterministic ownership such as existing actor context or `system`.

Validation:

```powershell
cargo test -p agentdash-domain -p agentdash-infrastructure ownership lifecycle
cargo test -p agentdash-application-agentrun workspace ownership
pnpm --filter app-web test -- AgentRunWorkspace ownership
```

### C. Session Surface Safety And AgentRun Runtime Endpoints

Purpose: make Session an internal runtime trace surface before product fork starts depending on it.

Dependencies: work item A permission vocabulary; work item B ownership is useful but not required for read-only runtime endpoints.

Scope:

- Add permission checks to retained `/sessions/{id}/fork`, `/sessions/{id}/lineage`, `/sessions/{id}/projection/rollback` while they still exist.
- Add AgentRun scoped runtime endpoints for events, stream, context projection, context audit, runtime control and tool approvals.
- Route AgentRun scoped endpoints through current delivery `RuntimeSessionExecutionAnchor`.
- Keep internal diagnostic Session routes named and documented as diagnostic/internal.
- Add missing RuntimeSession branching tests for message-ref boundary, unfinished turn, assistant tool-call boundary, incomplete tool-result groups, compaction fork-point validation and cleanup.

Out of scope:

- Do not migrate all frontend callers in this work item unless needed for endpoint tests.
- Do not remove Session service functions yet.

Acceptance:

- Product-capable runtime reads can be done with AgentRun refs only.
- Raw Session fork/lineage/rollback cannot bypass Project permission.
- RuntimeSession boundary tests fail on unstable fork points.

Validation:

```powershell
cargo test -p agentdash-application-runtime-session branching
cargo test -p agentdash-api sessions agent_run_runtime
rg -n "ensure_session_permission" crates/agentdash-api/src/routes/sessions.rs
```

### D. AgentRun Fork Backend

Purpose: implement product-grade AgentRun fork orchestration.

Dependencies: work item A, work item B, and the RuntimeSession primitive tests from work item C.

Scope:

- Add `agent_run_lineages` domain entity, repository trait, Postgres migration/repository, memory repository and contract projection.
- Add `AgentRunForkMaterializationPort` with one-transaction child control-plane materialization.
- Add `AgentRunForkService` with explicit fork and fork-submit commands.
- Claim outer command receipt before creating child RuntimeSession.
- Use `SessionBranchingService::fork_session` as the internal projection primitive.
- Adopt child RuntimeSession into child LifecycleRun/LifecycleAgent/AgentFrame/RuntimeSessionExecutionAnchor.
- Write cross-run fork lineage with parent/child runtime ids, fork point, forked_by_user_id and metadata.
- For fork-submit, create child mailbox envelope and schedule delivery through `AgentRunMailboxService`.
- Implement duplicate replay, pending duplicate conflict and terminal failure replay.
- Ensure parent AgentRun mailbox and parent RuntimeSession events are unchanged by fork-submit.

Out of scope:

- Do not build per-turn frontend toolbar here.
- Do not remove Session frontend services here.

Acceptance:

- Explicit fork creates navigable child AgentRun without initial input.
- Fork-submit creates child AgentRun and delivers the submitted input to child mailbox.
- Duplicate client command replays same child refs and does not create a second child RuntimeSession.
- Parent Run remains unchanged.
- Cross-run lineage appears in parent and child workspace projections.

Validation:

```powershell
cargo test -p agentdash-application-agentrun fork
cargo test -p agentdash-api agent_run_fork composer_submit
cargo test -p agentdash-infrastructure agent_run_lineage
```

### E. Frontend Fork And Session API Convergence

Purpose: expose the new fork behavior to users and remove Session as a product-level interaction API.

Dependencies: work item C AgentRun runtime endpoints and work item D fork APIs.

Scope:

- Update generated contracts and AgentRun services for fork/fork-submit/runtime endpoints.
- Change composer submit handler to navigate when response outcome is `forked`.
- Migrate product components away from direct `forkSession`, `fetchSessionLineage`, `rollbackSessionProjection` and direct Session runtime reads.
- Add per-turn action toolbar to stable conversation rounds.
- Copy action writes the current round's last agent reply message to clipboard.
- Fork action sends stable `forkPointRef` to AgentRun scoped fork API and navigates to child.
- Disable fork action for streaming/incomplete boundaries and show reason in tooltip.
- Keep internal reusable runtime feed components allowed to use Session naming only as implementation detail.

Out of scope:

- Do not redesign the whole AgentRun workspace layout.
- Do not add prefix/handoff copy; only copy last agent reply.

Acceptance:

- Non-owner member/editor submitting in another user's Run lands in a new child AgentRun.
- Owner can explicitly fork their own Run from a stable round.
- Product UI has no direct call to Session fork/lineage/rollback.
- Copy toolbar action writes only the last agent reply in that round.

Validation:

```powershell
pnpm --filter app-web test -- agent-run-workspace session copy fork
pnpm --filter app-web typecheck
rg -n "forkSession|fetchSessionLineage|rollbackSessionProjection|/sessions/" packages/app-web/src
```

### F. Spec And Final Integration

Purpose: close the architecture contract and catch cross-layer drift.

Dependencies: work items A-E.

Scope:

- Update backend permission specs with `member/use` participation semantics.
- Update backend session specs so Session fork/lineage/projection are internal runtime trace capabilities.
- Update frontend architecture specs so AgentRun is the only user-visible execution workspace.
- Update cross-layer specs for AgentRun scoped runtime endpoints and fork outcomes.
- Run final targeted checks and cross-layer searches.

Acceptance:

- Specs describe why AgentRun owns user-visible control and RuntimeSession owns trace/projection.
- No spec presents `/sessions/*` as a product interaction surface.
- Review gates in this parent task all pass.

Validation:

```powershell
rg -n "viewer|ProjectPermission::Edit|POST /sessions|GET /sessions|forkSession|Session workspace" .trellis/spec docs packages/app-web/src crates
cargo test -p agentdash-domain -p agentdash-application-agentrun -p agentdash-api fork permission session
pnpm --filter app-web test -- agent-run-workspace session
pnpm --filter app-web typecheck
```

## Phase 1: Safety And Surface Audit

- Add permission checks to current session fork/lineage/rollback routes while they still exist.
- Add missing RuntimeSession branching tests for message-ref boundary, unfinished turn, assistant tool-call boundary, incomplete tool-result groups, compaction fork-point validation, and best-effort cleanup behavior.
- Audit current `ProjectPermission::Edit` route usage and classify endpoints into Project `Use` versus `Configure`; AgentRun start/fork/fork-submit/continue-own-run must be `Use`.
- Inventory all front-end direct `/sessions/*` calls and classify as product flow, internal runtime feed, or removable diagnostic.
- Update generated contract expectations after any route/DTO move.

Validation:

```powershell
rg -n "/sessions/|/session/|forkSession|rollbackSessionProjection|fetchSessionLineage" packages/app-web/src crates/agentdash-api/src
cargo test -p agentdash-api sessions
```

## Phase 2: Ownership Facts

- Replace Project `viewer` product semantics with Project `member`; migrate role values and generated contracts accordingly.
- Rename/reframe Project permissions from `View/Edit/ManageSharing` to `Use/Configure/ManageSharing`, with `Use` covering AgentRun participation and `Configure` covering Project asset configuration.
- Add migration for AgentRun owner/initiator fields.
- Update domain entities, repositories, Postgres mapping and in-memory test repositories.
- Populate ProjectAgent start and existing launch paths with current user or system actor.
- Expose ownership/control projection in `AgentRunWorkspaceView`.
- Add `AgentRunCommandKind::AgentRunFork` and `AgentRunCommandKind::AgentRunForkSubmit`.

Validation:

```powershell
cargo test -p agentdash-domain -p agentdash-infrastructure -p agentdash-application-agentrun
pnpm --filter app-web test -- AgentRunWorkspace
```

## Phase 3: AgentRun Fork Service

- Add `agent_run_lineages` domain entity, repository, migration, Postgres implementation, in-memory test implementation and workspace projection support.
- Add `AgentRunForkMaterializationPort` that writes child LifecycleRun, LifecycleAgent, AgentFrame, RuntimeSessionExecutionAnchor and AgentRun fork lineage in one transaction.
- Add application service that orchestrates RuntimeSession fork and AgentRun adoption.
- Treat `SessionBranchingService::fork_session` as an internal projection primitive, not the product service boundary.
- Define a single idempotency key / command receipt digest over current user, parent AgentRun refs, fork point, optional input and backend selection before creating the child RuntimeSession.
- Add command receipt handling for fork submit idempotency.
- Write AgentRun fork lineage `fork` relation with metadata.
- Create child mailbox envelope and schedule delivery.
- Support explicit fork without initial input from a stable `fork_point_ref`, including self-owned AgentRun forks.
- Add tests for parent unchanged, child created, cross-run lineage, mailbox delivery, duplicate replay, pending duplicate conflict and failure cleanup.

Validation:

```powershell
cargo test -p agentdash-application-agentrun fork
cargo test -p agentdash-api agent_run
```

## Phase 4: Product API And Frontend Flow

- Add AgentRun scoped fork endpoint and fork submit endpoint, or extend composer submit command dispatch while preserving explicit fork action.
- Return typed API outcomes: `accepted_current`, `forked`, `queued`, `failed`, with redirect child AgentRun refs when forked.
- Add AgentRun scoped runtime events / stream / projection / control / tool approval endpoints and migrate product callers away from raw `/sessions/*`.
- Update command state / generated DTO / services.
- Add per-turn action toolbar to runtime feed UI with copy and fork icon actions.
- Disable fork action for streaming / incomplete boundaries and show tooltip reason.
- Implement clipboard payload builder for the current round's last agent reply message.
- Navigate to forked AgentRun when response includes redirect target.
- Remove product UI usage of `forkSession`, `rollbackSessionProjection`, and direct Session lineage panel.

Validation:

```powershell
pnpm --filter app-web test -- agent-run-workspace session
pnpm --filter app-web typecheck
```

## Phase 5: Spec Convergence

- Update backend session specs to describe Session as internal runtime trace/projection service.
- Update frontend architecture wording so AgentRun is the only user-visible execution workspace.
- Update cross-layer contract notes for AgentRun scoped runtime trace endpoints.

Validation:

```powershell
rg -n "POST /sessions|GET /sessions|/sessions/|Session detail|Session workspace" .trellis/spec docs packages/app-web/src
```

## Review Gates

- No product component calls `/sessions/{id}/fork`, `/sessions/{id}/lineage`, or `/sessions/{id}/projection/rollback`.
- No user-facing copy presents Session as a business object.
- AgentRun fork tests prove current-user fork ownership and parent immutability.
- Frontend tests prove self-owned AgentRun can fork from a turn action, and copy action writes the expected last agent reply content from the current round.
- Session internals remain reachable only through AgentRun scoped APIs or internal services.
