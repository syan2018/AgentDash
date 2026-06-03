# 实施计划

## Dispatch Scope

This task is ready for implementation as a database/business semantics convergence package. It should be split into ordered sub-slices if dispatched to multiple agents. Do not redo migration reset or reopen archived database baseline audit.

## PR Merge Priority

Closeout blockers for the lifecycle control-plane PR:

1. Phase 1 Session Runtime Head. Completed in the current closeout pass.
   - `sessions` must not keep business ownership, executor behavior or UI layout as durable Session facts if the PR claims Session is only runtime trace / delivery substrate.
2. Phase 2 Lifecycle Run Ledger. Partially completed in the current closeout pass.
   - `lifecycle_runs.active_node_keys` is removed by the active projection task.
   - `lifecycle_runs.execution_log` remains as audit-owner cleanup. It can move later if final review confirms it is read-model/audit only and not runtime control fact source.

Can move later when not blocking this PR:

3. Phase 3 UI / Settings.
   - Defer if `views` / `user_preferences` are legacy settings surfaces and do not feed lifecycle runtime decisions.
4. Phase 4 Business Redundancy.
   - Defer `stories.task_count` / `project_agents.is_default_for_task` if they remain clearly projection or product config and do not affect runtime control-plane facts.
5. Phase 5 Backend / Typed Cleanup.
   - Defer backend local identity/share/claim naming, LLM credential naming, permission JSONB scope indexing and canvas binding cleanup unless a touched contract becomes inconsistent.

## Phase 0: Baseline Audit Refresh

- [ ] Re-read `06-03-database-semantic-baseline-audit/report.md`.
- [ ] Confirm current occurrences for:
  - `sessions.executor_config_json`
  - `sessions.tab_layout_json`
  - `sessions.project_id`
  - `lifecycle_runs.active_node_keys`
  - `lifecycle_runs.execution_log`
  - `views`
  - `user_preferences`
  - `stories.task_count`
  - `project_agents.is_default_for_task`
  - backend local/share/claim fields
- [ ] For each field, record target category: business fact, runtime fact, projection/cache, audit/outbox, config, or UI state.

## Phase 1: Session Runtime Head

- [x] Remove or relocate `executor_config_json` to AgentFrame execution profile / connector launch input.
- [x] Move `tab_layout_json` to scoped UI/settings state.
- [x] Remove `sessions.project_id` as business ownership fact; expose project through anchor/read model when needed.
- [x] Update session repository, SPI persistence DTOs, API DTOs and frontend `SessionShellDto` usage.

## Phase 2: Lifecycle Run Ledger

- [ ] Coordinate with `06-02-lifecycle-run-active-projection-structure` before changing `active_node_keys`.
- [x] Remove `active_node_keys` from schema/repository after structured active refs are derived from graph instance state.
- [ ] Move `execution_log` to an explicit audit/event owner or remove if replaced by existing transition/event tables.
- [ ] Update workflow repository tests and lifecycle specs.

## Phase 3: UI / Settings

- [ ] Decide whether `views` is deleted or migrated to scoped saved views with user/project owner.
- [ ] Migrate `user_preferences` into settings contracts / repository or delete if unused.
- [ ] Update frontend settings consumers and generated settings contracts.

## Phase 4: Business Redundancy

- [ ] Remove or clearly project `stories.task_count`; update story repository, contracts and frontend selectors.
- [ ] Move `project_agents.is_default_for_task` into explicit dispatch/project policy or remove if no longer used.
- [ ] Review `projects.visibility/is_template` and keep only if they are active product facts.

## Phase 5: Backend / Typed Cleanup

- [ ] Rename/split backend local identity, share scope and runtime claim fields.
- [ ] Align LLM credential naming with provider/credential owner.
- [ ] Replace permission JSONB scope queries with typed scope columns/indexes where needed.
- [ ] Align canvas binding source contract with VFS/project surface source.

## Validation Commands

- [ ] `cargo check -p agentdash-infrastructure`
- [ ] `cargo check -p agentdash-api`
- [ ] `cargo test -p agentdash-infrastructure`
- [ ] `cargo test -p agentdash-application workflow`
- [ ] `pnpm run contracts:check`
- [ ] `pnpm --filter app-web run typecheck`
- [ ] Focused frontend tests for settings/story/project-agent surfaces touched by the slice.

## Review Gate

- [ ] `rg "executor_config_json|tab_layout_json|active_node_keys|execution_log|task_count|is_default_for_task" crates packages .trellis/spec` leaves only accepted target usages.
- [ ] `rg "CREATE TABLE views|CREATE TABLE user_preferences" crates/agentdash-infrastructure/migrations/0001_init.sql` matches the chosen target model.
- [ ] Specs explain why each retained projection exists and how it is rebuilt.

## Risk Points

- Session shell fields are used by frontend navigation; remove them only after anchor/read model replacement is in place.
- Active projection schema changes must not race the sibling active projection implementation.
- Business redundancy cleanup touches generated contracts; update frontend in the same slice.
