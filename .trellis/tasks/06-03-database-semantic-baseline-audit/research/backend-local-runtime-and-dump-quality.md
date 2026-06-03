# Research: Backend/local runtime schema + dump-quality audit

- Query: Evaluate `backends`, `runtime_health`, `backend_execution_leases`, `project_backend_access`, `backend_workspace_inventory`, `views`, `user_preferences`, `state_changes`, and global index/constraint/default/dump quality in the current PostgreSQL init baseline.
- Scope: mixed
- Date: 2026-06-03

## Findings

### Files Found

- `.trellis/tasks/06-03-database-semantic-baseline-audit/prd.md` - Requires semantic audit of current baseline, separating business/runtime/audit/projection/residue and identifying removal/rename/migration candidates.
- `.trellis/tasks/06-03-database-semantic-baseline-audit/design.md` - Defines the audit model and explicitly treats runtime facts, projections, outbox/audit, seed/config, and historical residue as separate categories.
- `.trellis/tasks/06-03-database-semantic-baseline-audit/implement.md` - Names this backend/local runtime slice and requires comparing `0001_init.sql` with repository SQL and specs.
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - Current squashed baseline; target tables are defined at lines 141, 170, 190, 558, 701, 950, 1006, and 1034.
- `crates/agentdash-domain/src/backend/entity.rs` - Domain structs for `BackendConfig`, `ViewConfig`, `UserPreferences`, `RuntimeHealth`, `BackendExecutionLease`, `ProjectBackendAccess`, and `BackendWorkspaceInventory`.
- `crates/agentdash-domain/src/backend/repository.rs` - Backend repository port currently still owns backend config plus `views` and `user_preferences`, while separate ports own runtime health, leases, access, and inventory.
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs` - SQL mapping for `backends`, `views`, and `user_preferences`; also contains local backend merge logic that rewrites `views.backend_ids`.
- `crates/agentdash-infrastructure/src/persistence/postgres/runtime_health_repository.rs` - SQL mapping for runtime health lifecycle.
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs` - SQL mapping for execution lease claim/activate/release/fail/lost and active counts.
- `crates/agentdash-infrastructure/src/persistence/postgres/project_backend_access_repository.rs` - SQL mapping for project backend authorization and backend workspace inventory.
- `crates/agentdash-infrastructure/src/persistence/postgres/state_change_repository.rs` and `state_change_store.rs` - State change repository and low-level append/read helpers.
- `crates/agentdash-api/src/routes/backends.rs` - Backend, runtime health, and runtime summary API routes.
- `crates/agentdash-api/src/routes/backend_access.rs` - Project backend access, inventory refresh/register, workspace candidate and sync routes.
- `crates/agentdash-api/src/stream.rs` - Project NDJSON stream backed by project-scoped `state_changes.id` cursor.
- `crates/agentdash-application/src/workspace/resolution.rs` and `workspace/backend_sync.rs` - Workspace binding resolution and inventory-to-binding sync.
- `crates/agentdash-application/src/backend/management.rs` - Local runtime ensure/claim semantics and stable local backend identity generation.
- `crates/agentdash-infrastructure/src/persistence/postgres/settings_repository.rs` - Current scoped `settings` repository, relevant because `user_preferences` overlaps with it.
- `.trellis/spec/backend/database-guidelines.md` - Database baseline and repository rules.
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - Desktop/local runtime contract for leases, runtime health, registry, and local identity.
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - Project backend access, workspace detect, inventory, and workspace binding contract.
- `.trellis/spec/backend/session/streaming-protocol.md` - `state_changes` stream cursor contract.
- `.trellis/spec/backend/session/runtime-execution-state.md` - Runtime execution state contract, especially active backend lease semantics.
- `.trellis/spec/cross-layer/shared-library-contract.md` - Notes `agent.pi.user_preferences` as user-scope setting, relevant to the old `user_preferences` table.

### Related Specs

- Database guideline: complex value objects are stored as JSON text, native timestamp columns are preferred for `chrono::DateTime<Utc>`, repository startup must only check migrated schema readiness, and current squashed `0001_init.sql` should express the correct schema rather than old migration history (`.trellis/spec/backend/database-guidelines.md:20`, `:21`, `:22`, `:40`, `:42`, `:44`).
- Init migration should contain schema, constraints, indexes, and necessary extensions only; settings/backend registration/runtime health/session facts are written by seed/API/runtime repositories rather than schema baseline data (`.trellis/spec/backend/database-guidelines.md:46`).
- Local runtime contract states that `backend_execution_leases` records relay turn execution occupancy; `runtime_health` expresses connection health only; workspace inventory/binding express directory facts only; busy/idle is projected from active leases (`.trellis/spec/cross-layer/desktop-local-runtime.md:48`, `:52`).
- Project backend workspace routing states that Project backend access, workspace detect, inventory registration, and workspace binding are separate facts; inventory/binding do not express execution idle state (`.trellis/spec/cross-layer/project-backend-workspace-routing.md:37`, `:40`, `:41`, `:42`, `:44`, `:45`, `:47`).
- Streaming spec states that `Connected.last_event_id` and `StateChanged` are based on project-scoped `state_changes.id`, while `BackendRuntimeChanged` does not advance that cursor (`.trellis/spec/backend/session/streaming-protocol.md:26`, `:27`, `:28`).
- Runtime execution state spec states that active `backend_execution_leases` rows are backend placement/runtime summary state and do not replace session active-turn state (`.trellis/spec/backend/session/runtime-execution-state.md:22`).
- Shared library contract says `agent.pi.user_preferences` belongs to user scope, not system scope (`.trellis/spec/cross-layer/shared-library-contract.md:277`).

### Code Patterns

- `BackendRepository` still mixes backend config with `list_views`, `save_view`, `get_preferences`, and `save_preferences` (`crates/agentdash-domain/src/backend/repository.rs:23`).
- Backend repository readiness still requires `backends`, `views`, and `user_preferences` (`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:21`).
- `views` is actively read/written by `PostgresBackendRepository::list_views` / `save_view` (`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:332`, `:343`).
- `user_preferences` is actively read/written as one global row keyed by `'prefs'` (`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:365`, `:379`).
- Duplicate local backend merge rewrites `views.backend_ids`, so `views` cannot be dropped without removing or replacing that coupling (`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:552`).
- `BackendConfig` contains local identity and share fields including `profile_id`, `device_id`, `machine_id`, `machine_label`, `legacy_machine_ids`, `visibility`, `share_scope_kind`, `share_scope_id`, `capability_slot`, `device`, and `last_claimed_at` (`crates/agentdash-domain/src/backend/entity.rs:10`, `:31`, `:43`).
- `ensure_local_backend` finds existing local backends by machine/scope/capability and legacy identity candidates, then updates identity fields and `last_claimed_at` (`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:150`, `:178`, `:189`, `:234`, `:258`).
- Local runtime management generates stable backend IDs from machine id, share scope, share scope id, and capability slot (`crates/agentdash-application/src/backend/management.rs:132`, `:323`).
- Runtime health writes online/offline lifecycle facts and is exposed in backend APIs (`crates/agentdash-infrastructure/src/persistence/postgres/runtime_health_repository.rs:25`, `:77`, `:101`, `:118`, `:176`; `crates/agentdash-api/src/routes/backends.rs:163`, `:314`).
- Runtime summary joins backend config, registry online snapshot, runtime health, and active leases (`crates/agentdash-api/src/routes/backends.rs:218`, `:233`, `:238`, `:241`, `:273`, `:278`).
- Lease repository owns claim, list-active, and count-active behavior (`crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:149`).
- Project backend access routes create/update/revoke authorization rows and ensure active access before project-scoped runtime use (`crates/agentdash-api/src/routes/backend_access.rs:157`, `:499`).
- Inventory refresh/register writes `BackendWorkspaceInventory` from `workspace.detect` and then project workspace candidates/sync consume active access plus inventory (`crates/agentdash-api/src/routes/backend_access.rs:314`, `:335`, `:385`, `:395`, `:415`, `:433`; `crates/agentdash-application/src/workspace/backend_sync.rs:36`, `:49`, `:139`).
- Workspace resolution filters bindings by allowed backend ids and online status, but uses workspace bindings as execution candidates rather than inventory rows directly (`crates/agentdash-application/src/workspace/resolution.rs:55`).
- `state_changes` is appended/read as project-scoped event log and drives NDJSON replay/polling (`crates/agentdash-infrastructure/src/persistence/postgres/state_change_store.rs:6`, `:15`, `:74`, `:82`, `:105`; `crates/agentdash-api/src/stream.rs:48`, `:66`, `:102`).
- Domain explicitly describes `StateChangeRepository` as independent state-change event storage, avoiding mixing event storage into business aggregate repositories (`crates/agentdash-domain/src/story/state_change_repository.rs:8`).
- Task view projector depends on `StateChangeRepository`, so state changes still feed current projections (`crates/agentdash-application/src/task/view_projector.rs:100`, `:102`).

### Table And Field Classification

#### `backends`

DDL: `id`, `name`, `endpoint`, `auth_token`, `enabled`, `backend_type`, `created_at`, `owner_user_id`, `profile_id`, `device_id`, `device`, `last_claimed_at`, `machine_id`, `machine_label`, `legacy_machine_ids`, `visibility`, `share_scope_kind`, `share_scope_id`, `capability_slot` (`crates/agentdash-infrastructure/migrations/0001_init.sql:190`).

- Business fact: `id`, `name`, `endpoint`, `enabled`, `backend_type`, `owner_user_id`, `created_at` as backend registration/config fact.
- Local runtime fact: `profile_id`, `device_id`, `device`, `last_claimed_at`, `machine_id`, `machine_label`, `legacy_machine_ids`, `visibility`, `share_scope_kind`, `share_scope_id`, `capability_slot`.
- Lease/outbox/audit: none directly.
- Projection/cache: none directly, although `last_claimed_at` is a runtime lifecycle timestamp stored on the config row.
- Historical residue: `device_id` and `legacy_machine_ids` are explicitly legacy merge inputs in the domain comments and merge algorithm. They are still actively used to collapse old local backend identities, so they are not immediately removable without changing the merge model.

Assessment:

- Keep table. It is still the durable backend registration root used by backend API, relay auth token lookup, local runtime ensure, and authorization.
- High-priority cleanup: split or at least rename local identity/share fields so `backends` does not mix "backend config" with local machine scope and legacy merge state. The current table is overloaded but not dead.
- `auth_token` should get a uniqueness strategy. Repository currently allows duplicate token rows and detects duplicate token binding at lookup time in tests; that is too late for a relay bearer credential. Because tokens can be null, use a partial unique index on non-null `auth_token` if current semantics allow one token per backend.
- `backend_type`, `visibility`, and `share_scope_kind` should have CHECK constraints matching domain enums. The repository parses unknown values into `DomainError`, but baseline should reject invalid enum text.
- `created_at DEFAULT CURRENT_TIMESTAMP` is acceptable if inserts rely on database time; however most repository inserts bind timestamps elsewhere. Standardize default strategy across baseline.
- `device jsonb` and `legacy_machine_ids jsonb` are correctly typed as JSONB because repository binds/reads JSON values directly. This contrasts with other JSON text columns and should remain unless the project adopts all-JSONB for structured data.
- `device_id` is a code-change-removal candidate after the current local identity migration/merge behavior is retired. In a prelaunch baseline, prefer removing legacy migration fields once `ensure_local_backend` no longer uses them.
- `last_claimed_at` belongs to local runtime lifecycle, not static backend config. Move to a local backend identity/runtime claim table or rename to make runtime claim semantics explicit if keeping it on `backends`.

#### `runtime_health`

DDL: `backend_id`, `profile_id`, `name`, `status`, `version`, `capabilities jsonb`, `workspace_roots jsonb`, `device jsonb`, connection timestamps, `disconnect_reason`, `created_at`, `updated_at`, status CHECK (`crates/agentdash-infrastructure/migrations/0001_init.sql:701`).

- Business fact: none.
- Local runtime fact: `backend_id`, `profile_id`, `name`, `status`, `version`, `capabilities`, `workspace_roots`, `device`, `connected_at`, `last_seen_at`, `disconnected_at`, `disconnect_reason`, `created_at`, `updated_at`.
- Lease/outbox/audit: connection lifecycle trace, but not an append-only audit log.
- Projection/cache: runtime health is the last known health snapshot for APIs and runtime summary.
- Historical residue: `workspace_roots` constraint name `runtime_health_accessible_roots_not_null` references old "accessible_roots" wording.

Assessment:

- Keep table. Specs explicitly separate health from execution occupancy and inventory/binding directory facts.
- Keep JSONB types for `capabilities`, `workspace_roots`, and `device`; repositories use `sqlx::types::Json`.
- Rename `runtime_health_accessible_roots_not_null` to match the actual column (`runtime_health_workspace_roots_not_null`) or remove the named NOT NULL constraint noise.
- Consider whether `name`, `profile_id`, and `device` duplicate `backends` fields. They should remain if `runtime_health` is the last online handshake snapshot; otherwise move static identity to `backends` and keep runtime-only fields here. Current code writes them as handshake/runtime snapshot.
- `runtime_health_pkey` on `backend_id` plus FK to `backends(id)` is appropriate.
- `idx_runtime_health_status` and `idx_runtime_health_last_seen_at` support operational listing; keep.

#### `backend_execution_leases`

DDL: `id`, `backend_id`, `session_id`, `turn_id`, `executor_id`, optional workspace/root, selection/state/reason fields, lifecycle timestamps, CHECKs on selection/state/terminal kind (`crates/agentdash-infrastructure/migrations/0001_init.sql:141`).

- Business fact: none.
- Local runtime fact: `backend_id`, `session_id`, `turn_id`, `executor_id`, `workspace_id`, `root_ref`, `selection_mode`, active/terminal state and timestamps.
- Lease/outbox/audit: entire table is a lease/lifecycle fact. Released/lost/failed rows are historical lease trace.
- Projection/cache: active lease queries project execution busy/idle state into `/backends/runtime-summary`.
- Historical residue: none obvious in target fields.

Assessment:

- Keep table. Specs name it as the source for backend execution occupancy and runtime summary.
- `UNIQUE(session_id, turn_id)` should be reviewed. If future multi-backend/multi-executor execution per turn is allowed, uniqueness should include `backend_id` or `executor_id`; for current one backend per turn it is acceptable and enforces current placement contract.
- Add FK to `sessions(id)` only if lease lifetime must cascade with session deletion. Current DDL only FK's `backend_id`; lack of session FK keeps lease trace decoupled but permits dangling session IDs. Because session IDs are used as runtime trace identifiers and the table is lease/audit-ish, this looseness may be intentional.
- Consider FK to `workspaces(id)` for `workspace_id` only if workspace deletion should affect historical lease rows. Current lack of FK preserves lease history.
- Active indexes are well aligned: partial active-by-backend index, backend-state index, and session index match count/list patterns (`0001_init.sql:1736`, `:1743`, `:1750`).
- `terminal_kind` CHECK has no value for `lost`; this is consistent because `lost` is a lease state and `terminal_kind` is user-visible execution terminal kind. Keep.

#### `project_backend_access`

DDL: `id`, `project_id`, `backend_id`, `status`, `access_mode`, `priority`, `root_policy`, `capability_policy`, `note`, `created_by`, timestamps (`crates/agentdash-infrastructure/migrations/0001_init.sql:558`).

- Business fact: Project-to-backend authorization and configuration (`project_id`, `backend_id`, `status`, `access_mode`, `priority`, policies, note, created_by).
- Local runtime fact: none directly, but references local/remote backend authorization.
- Lease/outbox/audit: none; `created_by` is a light audit field, not an append-only audit.
- Projection/cache: none.
- Historical residue: no obvious dead column, but `access_mode` has only one domain enum value (`use_inventory`) today.

Assessment:

- Keep table. It is the Project/backend authorization boundary and is referenced by workspace routes, extension runtime archive access, and workspace binding sync.
- Add FK to `backends(id)`. Current DDL only has FK to `projects(id)` (`0001_init.sql:2373`) and an index on `backend_id` (`:1988`), so stale backend access rows can exist unless repository deletion cascades indirectly. Because this is an authorization relation, a direct FK is appropriate unless preserving authorization history after backend removal is desired.
- Add CHECK constraints for `status` (`active`, `paused`, `revoked`) and `access_mode` (`use_inventory`) to match domain parse functions.
- `root_policy` and `capability_policy` are stored as TEXT JSON while repositories serialize/parse JSON strings. This follows current database guideline for complex values as TEXT, but defaults should be reviewed: `root_policy DEFAULT '{"kind":"backend_inventory"}'` encodes a current policy decision in DDL. Since repository creation already sets it, baseline can either keep as table invariant or remove default to force explicit use-case writes.
- `UNIQUE(project_id, backend_id)` is correct for current one access relationship per project/backend. It supports revoke/reactivate in place.
- `idx_project_backend_access_status` alone may be low value compared with `(project_id, status)` because all key reads are project scoped. Replace with or add composite index if query plans matter.

#### `backend_workspace_inventory`

DDL: `id`, `backend_id`, `root_ref`, `identity_kind`, JSON text payloads/facts, `status`, `source`, `last_seen_at`, `last_error`, timestamps (`crates/agentdash-infrastructure/migrations/0001_init.sql:170`).

- Business fact: none directly; it is not a Project workspace.
- Local runtime fact: backend-known workspace root and detected local facts.
- Lease/outbox/audit: none.
- Projection/cache: snapshot/cache of detect/register results used to create/sync workspace bindings.
- Historical residue: no obvious dead column.

Assessment:

- Keep table. Cross-layer spec explicitly says detect/register success produces directory facts and inventory/binding are separate from execution state.
- Add FK to `backends(id)`. Current DDL has unique `(backend_id, root_ref)` and backend/status indexes but no FK. Inventory rows should not outlive their backend unless deliberately used as historical detection log.
- Add CHECK constraints for `identity_kind`, `status`, and `source` matching domain enums. Repositories currently reject unknown values late.
- `identity_payload` and `detected_facts` use TEXT JSON rather than JSONB. This follows database guideline for complex value objects, but differs from `runtime_health` JSONB. Keep only if the project wants "domain value object serialized as text" for inventories; otherwise standardize structured JSON facts to JSONB.
- `UNIQUE(backend_id, root_ref)` is correct for snapshot semantics. If multiple identities can exist under the same root later, identity kind/payload digest would need to join the key; current code upserts by backend/root.
- `source DEFAULT 'manual_refresh'` is a behavior default. Since all creation paths pass source explicitly (`manual_refresh`, `capability_expansion_ack`, etc.), remove the default to avoid hiding missing source writes.

#### `views`

DDL: `id`, `name`, `backend_ids` TEXT JSON default `[]`, `filters` TEXT JSON default `{}`, `sort_by`, `created_at` (`crates/agentdash-infrastructure/migrations/0001_init.sql:1034`).

- Business fact: weak/legacy user UI configuration.
- Local runtime fact: `backend_ids` references backend selection in a UI view.
- Lease/outbox/audit: none.
- Projection/cache: user-defined view/filter projection, but not a current runtime projection.
- Historical residue: likely historical dashboard view model. Current frontend coordinator store has a `views` field but no clear active API route surfaced in current route scan; backend repository still supports it.

Assessment:

- Do not drop directly: repository port, PostgreSQL implementation, tests, and duplicate-local-backend merge still depend on it.
- High-priority code-change-removal or migration candidate. `views` does not belong inside `BackendRepository`; if still product-valid, it should move to a scoped user/project settings or dedicated saved-view repository with user/project ownership. If not product-valid, remove repository methods, tests, merge rewrite, and table.
- Add ownership/scope if retained. Current table has no `user_id`, `project_id`, or scope columns, so all users would share one global saved-view namespace. That conflicts with current auth/settings direction.
- `backend_ids` should not be TEXT JSON if backend references matter. If retained, use a join table or JSONB with explicit update semantics. Current duplicate merge rewrites JSON text manually.
- `created_at` lacks `updated_at`, so saved view updates lose modification time. Add `updated_at` if retained.
- Add FK strategy only after deciding representation. A join table `view_backends(view_id, backend_id)` could FK to `backends`; JSON array cannot.

#### `user_preferences`

DDL: `key`, `value`, primary key on `key` (`crates/agentdash-infrastructure/migrations/0001_init.sql:1006`).

- Business fact: legacy global user preference blob.
- Local runtime fact: none.
- Lease/outbox/audit: none.
- Projection/cache: none.
- Historical residue: strong. Current system has scoped `settings(scope_kind, scope_id, key, value, updated_at)` and frontend settings API. `agent.pi.user_preferences` is handled through settings paths, not this table.

Assessment:

- Highest-confidence code-change-removal candidate. It should not remain as a global key-value table in a multi-user/scoped settings architecture.
- Do not drop directly without code changes: `BackendRepository` still exposes `get_preferences`/`save_preferences`; `PostgresBackendRepository::initialize` requires the table; tests and mocks implement these port methods.
- Replace old `UserPreferences` backend-port methods with `SettingsRepository` or remove if unused by API. Existing PI user preferences use `settings` (`packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:23`, `:67`; `crates/agentdash-executor/src/connectors/pi_agent/factory.rs:26`, `:85`, `:91`).
- If a generic preference table is still needed, it should be scoped (`user_id` or `scope_kind/scope_id`) and should not duplicate `settings`.

#### `state_changes`

DDL: `id bigint`, sequence, `project_id DEFAULT ''`, `entity_id`, `kind`, `payload DEFAULT '{}'`, optional `backend_id`, `created_at` (`crates/agentdash-infrastructure/migrations/0001_init.sql:950`, `:965`, `:977`, `:1109`).

- Business fact: no; it is not the authoritative Story/Task table.
- Local runtime fact: optional `backend_id` can attribute changes to backend, but this is not local runtime health/lease state.
- Lease/outbox/audit: project-scoped event/outbox log for state changes and streaming cursors.
- Projection/cache: source feed for project NDJSON stream and task view projection.
- Historical residue: `project_id DEFAULT ''` and `payload DEFAULT '{}'` are legacy/backfill-style defaults for fields that code always binds explicitly.

Assessment:

- Keep table. It still carries current state-change stream and view projector inputs.
- Rename conceptually in future if broader than "state changes": `project_state_events` or `project_change_events` would better express event-log/outbox semantics. Not required for this baseline slice if report scope is cleanup first.
- Remove `project_id DEFAULT ''`; it creates invalid project-scoped events and contradicts current append code, which binds project ID.
- Remove `payload DEFAULT '{}'` if every append binds payload. Empty JSON default can hide missing payload bugs. If retained, use `jsonb DEFAULT '{}'::jsonb` only after changing repository field type.
- Add index on `(project_id, id)` because stream reads are `WHERE project_id = $1 AND id > $2 ORDER BY id ASC LIMIT $3`; current target-table index scan did not show a state_changes project/id index. This is a high-priority performance/contract fix.
- Consider CHECK on `kind` only if the set is stable. Current `ChangeKind` is enumerated in repository parser, but event logs often need additive kinds; prelaunch can still add CHECK if migrations are easy.
- Consider FK to `projects(id)`. Because stream is project-scoped and default empty project ID is invalid, FK would improve integrity. If state events must outlive projects for audit, keep no FK and remove invalid default instead.
- `backend_id` should remain optional and likely no FK if events may refer to deleted/offline/legacy backends. It is attribution, not authorization.

### Direct Removal / Code-Change / Migration Candidates

- Directly removable with no code changes: none found in this slice. All target tables are either actively used or referenced by readiness checks/ports.
- Remove after code changes:
  - `user_preferences` table and `BackendRepository::{get_preferences, save_preferences}`. Replace any remaining call sites with scoped `SettingsRepository`.
  - `views` table if saved backend dashboard views are no longer a product surface. Remove backend port methods, repository SQL, duplicate-merge rewrite, frontend/store references, and tests. If still needed, migrate to scoped saved views outside `BackendRepository`.
  - `backends.device_id` and possibly `backends.legacy_machine_ids` after local identity merge compatibility is no longer needed in the clean prelaunch baseline.
- Wrong location / should migrate:
  - `views` belongs to user/project saved UI state or settings, not backend repository.
  - `user_preferences` belongs to scoped `settings`, not backend repository.
  - `backends.last_claimed_at` and local machine/share fields may belong to a local backend identity/claim table if the team wants `backends` to remain pure connection config.
- Keep but normalize:
  - `backends`: add enum CHECKs, token uniqueness, and decide whether local identity should split.
  - `runtime_health`: rename stale constraint; keep as runtime snapshot.
  - `backend_execution_leases`: keep; review uniqueness and optional FKs.
  - `project_backend_access`: add backend FK and enum CHECKs; review JSON defaults and status index shape.
  - `backend_workspace_inventory`: add backend FK and enum CHECKs; remove behavior default on `source` if all writers are explicit.
  - `state_changes`: add `(project_id, id)` index; remove invalid defaults; consider FK/rename.
- Looks odd but should keep:
  - `runtime_health.workspace_roots` being separate from inventory: spec says empty roots does not mean cannot browse/detect, and inventory/binding are directory facts while runtime health is connection snapshot.
  - `backend_execution_leases` not being part of `runtime_health`: spec says busy/idle is lease-derived, not health-derived.
  - `backend_workspace_inventory` separate from workspace bindings: spec says inventory registration and workspace binding maintenance are separate.
  - `state_changes` despite projection tables: it still advances stream cursors and feeds projection rebuild.

### Dump Quality Audit

- Dump comments and `public.` prefixes: `0001_init.sql` retains pg_dump headers/comments (`-- PostgreSQL database dump`, per-object `Name: ... Type: ... Schema: public`) and every DDL uses `public.` (`0001_init.sql:2`, `:10`, `:2466`). For a curated baseline, remove dump comments and usually remove explicit `public.` unless the project intentionally supports non-default `search_path` constraints. A hand-curated baseline is easier to review and less tied to dump ordering noise.
- `IF NOT EXISTS`: target baseline contains no `CREATE TABLE IF NOT EXISTS` or `CREATE INDEX IF NOT EXISTS` matches. For a clean initial migration, absence is acceptable because SQLx migration versioning should run it once on an empty database. For forward migrations, project guidelines call for idempotent `ADD COLUMN IF NOT EXISTS` / `ON CONFLICT DO NOTHING`; this does not require `IF NOT EXISTS` in the squashed `0001`.
- Constraint/index naming: mixed style exists. Most constraints use generated names (`backend_execution_leases_session_id_turn_id_key`, `project_backend_access_project_id_backend_id_key`), some use stale/manual names (`runtime_health_accessible_roots_not_null` on `workspace_roots`), and one FK uses `fk_session_runtime_commands_frame_transition` while others use generated `_fkey`. Normalize baseline names by intent: `pk_*`, `uq_*`, `ck_*`, `fk_*`, `ix_*` or a single existing convention.
- Defaults with backfill/legacy semantics: outside the target tables, defaults such as `created_by_kind DEFAULT 'backfill'` and zero UUID project IDs exist (`0001_init.sql:76`, `:114`, `:1057`). Within target tables, `state_changes.project_id DEFAULT ''`, `state_changes.payload DEFAULT '{}'`, `backend_workspace_inventory.source DEFAULT 'manual_refresh'`, and `project_backend_access.root_policy DEFAULT '{"kind":"backend_inventory"}'` deserve cleanup because repository/use-case code should write these facts explicitly.
- Timestamp consistency: target runtime/backend tables use `timestamp with time zone`, while session/runtime event tables elsewhere use `bigint` ms. This can be acceptable if boundary is "domain/business DB time uses timestamptz; event stream sequence/runtime log uses ms bigint", but the baseline should document that split. `state_changes` uses `timestamptz` plus bigint sequence, consistent with project stream cursor.
- JSON type consistency: target tables mix JSONB (`backends.device`, `runtime_health.capabilities/workspace_roots/device`) and TEXT JSON (`project_backend_access.root_policy/capability_policy`, `backend_workspace_inventory.identity_payload/detected_facts`, `views.backend_ids/filters`, `state_changes.payload`). This partly follows the guideline "complex value objects as TEXT", but runtime health uses JSONB because repository binds `sqlx::types::Json`. Recommendation: document the distinction or standardize. Queryable/patchable JSON should be JSONB; opaque domain payloads can remain TEXT.
- FK strictness:
  - `runtime_health.backend_id` and `backend_execution_leases.backend_id` correctly FK to `backends`.
  - `project_backend_access` lacks FK to `backends`, too loose for authorization rows.
  - `backend_workspace_inventory` lacks FK to `backends`, too loose for snapshot rows.
  - `backend_execution_leases.session_id` and `workspace_id` lack FKs; this may be intentionally loose for runtime audit/history.
  - `state_changes.project_id` lacks FK and has invalid empty-string default; tighten unless audit log must outlive projects.
  - `views.backend_ids` cannot FK while stored as JSON text; this is another reason to remove/migrate `views`.
- Index quality:
  - Runtime lease and runtime health indexes match current route/repository reads.
  - `project_backend_access_status` alone is probably less useful than `(project_id, status)`.
  - `state_changes` needs `(project_id, id)` for project-scoped stream polling.
  - `backends.auth_token` needs a partial unique index if relay token uniqueness is invariant.

### Priority Recommendations

1. Highest priority: remove or migrate `user_preferences`; scoped `settings` is the current model, while `user_preferences(key,value)` is global and still only kept alive by backend repository legacy methods.
2. Highest priority: decide `views` fate. Either remove it with repository/frontend cleanup or re-home it as scoped saved UI state. Do not leave it in `BackendRepository` with global rows and JSON-text backend references.
3. High priority: normalize `backends` local identity/share fields. Keep current local runtime identity facts, but split/rename so static backend config, local machine identity, share scope, and claim lifecycle are not semantically blurred.
4. High priority: tighten integrity for `project_backend_access`, `backend_workspace_inventory`, and `state_changes`: add missing FKs/CHECKs where compatible with intended lifecycle, add `(project_id, id)` index for `state_changes`, and remove defaults that encode missing/legacy facts.
5. Medium priority: convert dump-style `0001_init.sql` into a curated baseline by removing pg_dump comments/public prefix noise, standardizing constraint/index names, and making JSON/timestamp/default choices consistent.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task, but the user provided the exact task directory and research output path, so this research used the explicit path instead of guessing.
- No external web references were needed; findings are based on local task artifacts, specs, migration SQL, repository code, application code, API routes, and generated/frontend references.
- I did not run compile/tests because this is a read-only research slice and no code/schema changes were made.
- I did not audit all 55 baseline tables; this file covers the requested backend/local runtime partition and dump-quality patterns relevant to that partition.
- `views` direct product usage appears weak from route/service search, but repository methods/tests and merge logic are real dependencies. Treat removal as code-change work, not direct baseline deletion.
- `user_preferences` may have hidden consumers through older mocks or tests because the backend repository port still requires methods. Current settings UI and PI connector path use `settings`, so the table is semantic residue even if port cleanup is required.
