# Research: shared_library crate boundary review

- Query: review AgentDash backend shared_library module crate split boundary and executable migration plan; treat skill as an external fixed boundary and only analyze direct shared_library relations with skill_asset, extension_package, workflow builtin seed, first-party integrations, and API routes.
- Scope: internal
- Date: 2026-06-25

## Findings

### Conclusion

Split `crates/agentdash-application/src/shared_library/**` into a new application crate named `agentdash-application-shared-library`.

The split is feasible, but the crate must not be a blind directory move. The current module contains:

- Shared Library asset query and seeding orchestration.
- Install orchestration from `LibraryAsset` to Project assets.
- Publish orchestration from Project assets back to `LibraryAsset`.
- External marketplace import/refresh normalization.

The new crate should own Shared Library application use cases, not HTTP DTO mapping, not concrete PostgreSQL repositories, not extension package archive storage, not skill module logic, and not workflow builtin template storage.

Recommended dependency direction:

```text
agentdash-api
  -> agentdash-application-shared-library
  -> agentdash-domain
  -> repository traits

agentdash-application-shared-library
  -> agentdash-spi              // MarketplaceFetchedAsset / MarketplaceAssetListing only
  -> agentdash-application-vfs  // VFS mount path/container helpers only
  -> agentdash-domain           // entities, payloads, repository traits

agentdash-application
  -> agentdash-application-shared-library
```

Do not make `agentdash-application-shared-library` depend on `agentdash-application`, `agentdash-application-skill`, or a workflow application crate.

### Files Found

- `crates/agentdash-application/src/shared_library/mod.rs` - current public facade that re-exports seed/install/publish/external marketplace/service APIs.
- `crates/agentdash-application/src/shared_library/service.rs` - read/list service and builtin/integration embedded seed execution.
- `crates/agentdash-application/src/shared_library/seed.rs` - builtin asset seed registry; currently directly imports workflow builtin templates.
- `crates/agentdash-application/src/shared_library/install.rs` - LibraryAsset to Project asset install orchestration across agents, MCP presets, workflows, skill assets, VFS mounts, extensions.
- `crates/agentdash-application/src/shared_library/publish.rs` - Project asset to LibraryAsset publish orchestration, including extension package artifact copy.
- `crates/agentdash-application/src/shared_library/external_marketplace.rs` - external provider fetched/listing normalization into `remote_imported` LibraryAsset.
- `crates/agentdash-api/src/routes/shared_library.rs` - Shared Library HTTP routes and DTO mapping.
- `crates/agentdash-api/src/routes/marketplace.rs` - external marketplace routes, provider lookup, import/refresh route orchestration.
- `crates/agentdash-api/src/bootstrap/repositories.rs` - startup seed wiring for builtin and integration embedded LibraryAssets.
- `crates/agentdash-api/src/integrations.rs` - Host Integration collection of `library_asset_seeds` and marketplace providers.
- `crates/agentdash-first-party-integrations/src/lib.rs` - first-party integration seed/provider example.
- `crates/agentdash-application/src/repository_set.rs` - current all-repository aggregate; new crate should not depend on this type.
- `crates/agentdash-application/src/extension_package.rs` - extension package archive validation/storage/install use cases; should remain outside shared_library crate.
- `crates/agentdash-domain/src/shared_library/**` - domain entity, payload typing, repository trait, digest helper.
- `crates/agentdash-domain/src/extension_package.rs` - artifact owner/ref/entity/repository used by extension template publish/install.
- `crates/agentdash-domain/src/workflow/repository.rs` - workflow install transaction port used by shared_library install.
- `crates/agentdash-application/src/workflow/definition.rs` - current workflow builtin template DTO and JSON includes; should become a provider input, not a dependency.

### Current Responsibilities And Boundaries

`service.rs` has two separate responsibilities:

- Read/list wrapper around `LibraryAssetRepository` (`SharedLibraryService::get/list`).
- Seed application service for builtin and integration embedded assets.

Evidence:

- `service.rs:23` defines `SharedLibraryService<'a>` over `&dyn LibraryAssetRepository`.
- `service.rs:49` seeds builtin assets by calling `builtin_library_seeds()`.
- `service.rs:109` seeds integration embedded assets from `IntegrationEmbeddedLibraryAssetSeed`.
- `service.rs:143` creates integration embedded assets with `LibraryAssetSource::IntegrationEmbedded`.
- `service.rs:156` allows idempotent update only for same `integration_embedded` `source_ref`.

`seed.rs` is a seed registry, but currently creates the workflow dependency that should be removed:

- `seed.rs:6` imports `crate::workflow::list_builtin_workflow_templates`.
- `seed.rs:33` builds all builtin seeds.
- `seed.rs:62` owns the builtin agent template seed.
- `seed.rs:91` builds workflow template seeds from workflow builtin templates.

`install.rs` is a cross-project-resource installer. It should move into the new crate, but with a narrow repository set instead of the monolithic `agentdash_application::repository_set::RepositorySet`.

Evidence:

- `install.rs:103` is the install entry point.
- `install.rs:140` installs `skill_template` by constructing domain `SkillAsset`.
- `install.rs:162` installs `extension_template` and resolves package artifacts.
- `install.rs:248` reads `ExtensionPackageArtifactRepository` for LibraryAsset-owned package artifacts.
- `install.rs:271` lists source status across Project resources.
- `install.rs:902` installs workflow template payloads.
- `install.rs:924` delegates workflow bundle persistence to `workflow_template_install_repo`.
- `install.rs:938` upserts SkillAsset through `SkillAssetRepository`.
- `install.rs:975` upserts ProjectExtensionInstallation.

`publish.rs` is a cross-project-resource publisher. It should move into the new crate, with the same narrow repository set.

Evidence:

- `publish.rs:28` defines Project asset kinds accepted by publish.
- `publish.rs:74` is the publish entry point.
- `publish.rs:81-87` dispatches to per-asset mappers.
- `publish.rs:141` copies extension package artifact after extension LibraryAsset publish.
- `publish.rs:248` validates/copies Project-owned package artifact to LibraryAsset-owned package artifact.
- `publish.rs:414` publishes SkillAsset into `skill_template` payload.
- `publish.rs:454` publishes workflow graph and referenced procedures into `workflow_template` payload.
- `publish.rs:479` collects lifecycle referenced procedures through `AgentProcedureRepository`.
- `publish.rs:508` builds the workflow template bundle DTO.

`external_marketplace.rs` is clean application logic and should move as-is except imports:

- `external_marketplace.rs:61` imports fetched provider payload into `LibraryAsset(source=remote_imported)`.
- `external_marketplace.rs:144` refreshes by comparing provider listing with local imported asset.
- `external_marketplace.rs:210` currently restricts external providers to `skill_template` and `mcp_server_template`.
- `external_marketplace.rs:262` finds imported assets by `source_ref`.
- `external_marketplace.rs:283` compares version and optional remote digest.

### Recommended New Crate Contents

Create:

```text
crates/agentdash-application-shared-library/
  Cargo.toml
  src/lib.rs
  src/service.rs
  src/seed.rs
  src/install.rs
  src/publish.rs
  src/external_marketplace.rs
  src/repository_set.rs
```

Move into this crate:

- `crates/agentdash-application/src/shared_library/service.rs`
- `crates/agentdash-application/src/shared_library/install.rs`
- `crates/agentdash-application/src/shared_library/publish.rs`
- `crates/agentdash-application/src/shared_library/external_marketplace.rs`
- most of `crates/agentdash-application/src/shared_library/seed.rs`
- tests currently embedded in those files.

Do not move:

- `crates/agentdash-api/src/routes/shared_library.rs`: HTTP route/DTO/auth belongs to API.
- `crates/agentdash-api/src/routes/marketplace.rs`: route/provider orchestration and contract DTO mapping belongs to API.
- `crates/agentdash-application/src/skill_asset/**`: direct consumer/external boundary only; do not migrate or redesign here.
- `crates/agentdash-application/src/skill/**`: out of scope entirely.
- `crates/agentdash-application/src/extension_package.rs`: archive parsing/storage/install use cases are package artifact management, not Shared Library.
- `crates/agentdash-application/src/workflow/**` and workflow builtin JSON files: workflow owns builtin template definitions.
- `crates/agentdash-domain/src/shared_library/**`: already domain layer; keep in domain.
- `crates/agentdash-infrastructure/src/persistence/postgres/shared_library_repository.rs`: concrete persistence stays infrastructure.

### Public API Surface

The new crate should export a narrow facade equivalent to the current `shared_library/mod.rs`, plus a new repository set and seed provider surface.

Export:

- `SharedLibraryService`
- `SeedBuiltinLibraryAssetsInput`
- `IntegrationEmbeddedLibraryAssetSeed`
- `BuiltinLibrarySeedProvider`
- `BuiltinLibrarySeedProviderInput` or `BuiltinLibrarySeedSet`
- `WorkflowTemplateSeed`
- `InstallLibraryAssetInput`
- `InstallLibraryAssetOptions`
- `AgentTemplateDependencyMode`
- `InstallLibraryAssetOutput`
- `ProjectAssetSourceStatus`
- `ProjectAssetSourceStatusItem`
- `install_library_asset_to_project`
- `list_project_asset_source_status`
- `ProjectAssetPublishKind`
- `PublishLibraryAssetInput`
- `PublishLibraryAssetError`
- `publish_project_asset_to_library`
- `ImportExternalMarketplaceAssetInput`
- `RefreshExternalMarketplaceAssetInput`
- `RefreshExternalMarketplaceAssetOutput`
- `ExternalMarketplaceRefreshStatus`
- `ExternalMarketplaceLibraryError`
- `UPSERT_LIBRARY_ASSET_IMPORT_MODE`
- `ensure_supported_external_asset_type`
- `external_marketplace_source_ref`
- `import_external_marketplace_asset`
- `refresh_external_marketplace_asset`
- `SharedLibraryRepositorySet`

Continue re-exporting `agentdash_domain::shared_library::seed_digest` only if existing callers need it; the real owner remains domain (`crates/agentdash-domain/src/shared_library/mod.rs:25`).

`SharedLibraryRepositorySet` should contain only the repositories actually used by install/publish/source-status:

- `shared_library_repo`
- `extension_package_artifact_repo`
- `project_extension_installation_repo`
- `mcp_preset_repo`
- `skill_asset_repo`
- `project_agent_repo`
- `project_vfs_mount_repo`
- `agent_procedure_repo`
- `workflow_template_install_repo`
- `workflow_graph_repo`
- `inline_file_repo`

The current `RepositorySet` includes many unrelated repositories (`repository_set.rs:48`). The application crate can add `to_shared_library_repository_set()` as an adapter; the new crate must not depend on `agentdash-application::RepositorySet`.

### skill_asset Boundary

Treat `skill_asset` as a Project resource consumer and source-status participant, not as a module to split or redesign.

Current direct relations:

- `install.rs:140` constructs a `SkillAsset` from `SkillTemplatePayload`.
- `install.rs:938` upserts through `SkillAssetRepository`.
- `publish.rs:414` maps a Project `SkillAsset` back to `SkillTemplatePayload`.
- `skill_asset/service.rs:385` URL import currently prepares a remote skill and then installs the resulting LibraryAsset to the project.
- `skill_asset/service.rs:404` writes/reads Shared Library during remote import.
- `skill_asset/service.rs:410` calls `install_library_asset_to_project`.

Recommended boundary:

- `agentdash-application-shared-library` may depend on domain `SkillAsset`, `SkillAssetFile`, `SkillAssetRepository`, and `SkillTemplatePayload`.
- It must not depend on `agentdash-application-skill` or `crate::skill`.
- Existing skill URL import should remain a caller of Shared Library APIs. Any import path update during crate migration is mechanical, not a skill module split.
- No migration plan should require skill module changes or wait for skill module work.

### extension_package Boundary

Shared Library should only handle LibraryAsset-owned package artifact lookup/copy during extension template install/publish. Archive validation, storage object read/write, and local package import remain in extension_package use cases.

Evidence:

- `install.rs:248` finds the LibraryAsset-owned package artifact matching `ExtensionTemplatePayload`.
- `publish.rs:248` copies a Project-owned package artifact to a LibraryAsset-owned artifact after publishing `extension_template`.
- `extension_package.rs:83` validates package archives.
- `extension_package.rs:142` stores package artifacts.
- `extension_package.rs:186` installs a Project-owned package artifact into `ProjectExtensionInstallation`.
- `domain/extension_package.rs:21` defines `ExtensionPackageArtifactOwner::library_asset`.
- `domain/extension_package.rs:151` validates artifact/template identity with `matches_extension_template`.

Recommended boundary:

- New shared_library crate depends on domain `ExtensionPackageArtifactRepository`, owner/ref/entity, and `ProjectExtensionInstallationRepository`.
- Do not move package archive parsing, storage ref computation, webview asset read, or package install functions.
- Keep `library_asset_response_for_api` enrichment in API route, because DTO shape and artifact summary exposure are route concerns (`routes/shared_library.rs:248`).

### Workflow Builtin Seed Boundary

Do not make the shared_library crate depend on workflow application code.

Current coupling:

- `seed.rs:6` imports `crate::workflow::list_builtin_workflow_templates`.
- `seed.rs:91` uses workflow builtin templates to create `workflow_template` seeds.
- `workflow/definition.rs:81` owns the actual `include_str!` list for builtin workflow JSON.

Recommended design:

- New crate owns Shared Library seed mechanics and version/digest invariants.
- Workflow owns builtin template definitions and supplies them through a narrow seed provider/DTO.

Suggested DTO:

```rust
pub struct WorkflowTemplateSeed {
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub template: serde_json::Value,
}
```

Suggested provider:

```rust
pub trait BuiltinLibrarySeedProvider {
    fn builtin_library_seeds(&self) -> Result<Vec<BuiltinSeed>, DomainError>;
}
```

The application crate or API bootstrap can compose a provider that combines:

- shared_library-owned builtin agent/MCP seeds;
- workflow-provided `WorkflowTemplateSeed` values from current `list_builtin_workflow_templates()`.

This keeps the direction:

```text
workflow builtin definitions -> seed provider DTO -> shared_library seed service
```

and avoids:

```text
shared_library -> workflow application module
```

Install/publish also currently use `BuiltinWorkflowTemplateBundle` from `crate::workflow` (`install.rs:26`, `publish.rs:24`). This should be replaced with a local/shared DTO in the new crate or a domain-level workflow template DTO. The DTO only needs the current JSON shape and conversion to domain `AgentProcedure` / `WorkflowGraph`; it does not need workflow application services.

### API Route Boundary

Routes should stay in `agentdash-api`, but imports should switch to the new crate.

Evidence:

- `routes/shared_library.rs:36` mounts `/shared-library/assets`.
- `routes/shared_library.rs:51` mounts project install.
- `routes/shared_library.rs:56` mounts project publish.
- `routes/shared_library.rs:61` mounts source-status.
- `routes/marketplace.rs:48` mounts marketplace sources.
- `routes/marketplace.rs:52` mounts external asset listing.
- `routes/marketplace.rs:56` mounts external import.
- `routes/marketplace.rs:60` mounts external refresh.

Recommended route change:

- Replace `use agentdash_application::shared_library::{...}` with `use agentdash_application_shared_library::{...}`.
- Convert `state.repos` to the new `SharedLibraryRepositorySet` before calling install/publish/source-status.
- Keep auth, `CurrentUser`, `ProjectPermission`, DTO parse/response, and provider lookup in API.

### First-Party Integration Boundary

First-party integrations already expose the right external boundary:

- `AgentDashIntegration::library_asset_seeds()` declares embedded Shared Library assets (`integration.rs:156`).
- `AgentDashIntegration::marketplace_source_providers()` declares provider ports (`integration.rs:122`).
- `ConnectorCatalogIntegration::library_asset_seeds()` returns an `ExtensionTemplate` seed (`first-party-integrations/src/lib.rs:64`).
- `ConnectorCatalogIntegration::marketplace_source_providers()` returns the dev marketplace provider (`first-party-integrations/src/lib.rs:106`).
- The fixture provider supports `McpServerTemplate` only (`first-party-integrations/src/lib.rs:215`) and returns `MarketplaceFetchedAsset::McpServerTemplate` (`first-party-integrations/src/lib.rs:286`).

Recommended boundary:

- Do not move first-party integration code.
- `agentdash-api/src/integrations.rs` continues to collect integration seeds/providers.
- `agentdash-api/src/bootstrap/repositories.rs` passes collected seeds into `SharedLibraryService::seed_integration_embedded_assets`.
- If the seed provider API changes, bootstrap is the only place that should assemble builtin workflow seeds.

### Database And Migration Impact

No schema migration is required for the crate split itself.

Evidence:

- `0001_init.sql:233` creates `library_assets`.
- `0001_init.sql:1066` has the identity unique index on `(asset_type, scope, owner_id, key)`.
- Project resources already carry installed source columns, e.g. `0001_init.sql:394-395`, `0001_init.sql:430-431`.
- `0006_library_asset_source_integration_embedded.sql:3` updates source constraints to `integration_embedded`.
- `0007_library_asset_integration_source_ref.sql:3` updates source_ref prefix from `plugin:` to `integration:`.
- `0030_builtin_skill_runtime_ownership.sql:1` deprecates old builtin `skill_template` assets and moves builtin SkillAsset ownership to runtime bootstrap.
- `workflow_repository.rs:257` implements `WorkflowTemplateInstallRepository`.
- `workflow_repository.rs:262` starts a transaction for workflow template install.
- `workflow_repository.rs:410` commits the workflow template install transaction.

The migration risk is behavioral rather than schema-related:

- Accidentally breaking workflow bundle install transaction by moving logic out of `WorkflowTemplateInstallRepository`.
- Accidentally returning to direct `LibraryAsset.payload` runtime use instead of installed Project resources.
- Accidentally treating extension package artifact digest as LibraryAsset payload digest; they are separate digest domains.
- Accidentally moving API DTO mapping into the application crate.

### Executable Migration Steps

1. Add workspace crate `agentdash-application-shared-library`.
   - Dependencies: `agentdash-domain`, `agentdash-spi`, `agentdash-application-vfs`, `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`, `async-trait`, `base64`, `tokio` for tests.
   - Do not depend on `agentdash-application`, `agentdash-application-skill`, or workflow application crate.

2. Move external marketplace first.
   - Move `external_marketplace.rs` unchanged except imports.
   - Update `routes/marketplace.rs` imports to the new crate.
   - Verification: `cargo test -p agentdash-application-shared-library external_marketplace` and `cargo test -p agentdash-api marketplace`.

3. Move service seed mechanics with provider boundary.
   - Move `service.rs` and split `seed.rs` into shared-library-owned seed construction plus provider/DTO input.
   - Replace direct `crate::workflow::list_builtin_workflow_templates` with workflow seed provider assembly in API/application bootstrap.
   - Update `bootstrap/repositories.rs` to pass builtin provider and integration seeds.
   - Verification: service seed tests, first-party integration seed validation tests, API bootstrap compile.

4. Move install with `SharedLibraryRepositorySet`.
   - Create new narrow repository set in the new crate.
   - Replace `crate::vfs::PROJECT_VFS_MOUNT_CONTAINER_ID` and `crate::vfs::normalize_mount_relative_path` with `agentdash_application_vfs` imports; these are already public in `agentdash-application-vfs/src/lib.rs:40` and `:65`.
   - Replace workflow DTO dependency with local/domain workflow template DTO.
   - Add adapter from current `agentdash_application::RepositorySet` to `SharedLibraryRepositorySet`.
   - Update API shared_library route calls.
   - Verification: install tests, `cargo test -p agentdash-infrastructure workflow_template_install`, route compile.

5. Move publish with the same `SharedLibraryRepositorySet`.
   - Replace workflow DTO dependency with the same local/domain workflow template DTO.
   - Keep extension package archive use cases outside this crate; only copy artifact records by repository.
   - Update API shared_library route calls.
   - Verification: publish unit tests, extension package artifact repository tests, API shared_library route tests.

6. Leave a temporary facade in `agentdash-application/src/shared_library/mod.rs` only if needed to keep application-internal imports compiling during the same PR.
   - Since this project does not require compatibility layers, remove the facade before the task is considered done unless a same-PR mechanical import update makes it unnecessary.
   - Final target: `agentdash-application` depends on and re-exports the new crate only where broader application callers still import through `agentdash_application::shared_library`.

7. Update workspace and package imports.
   - Add new crate to root `Cargo.toml` workspace members and workspace dependencies.
   - Add dependency to `agentdash-api` and `agentdash-application` as needed.
   - Remove `shared_library` files from `agentdash-application/src` once imports are clean.

### Risk Files

- `crates/agentdash-application/src/shared_library/install.rs` - largest blast radius; touches Project Agent, MCP preset, SkillAsset, VFS, workflow, extension installation.
- `crates/agentdash-application/src/shared_library/publish.rs` - high risk because it maps Project resources to public templates and copies extension package artifact records.
- `crates/agentdash-application/src/shared_library/seed.rs` - direct workflow builtin dependency must be removed.
- `crates/agentdash-api/src/bootstrap/repositories.rs` - startup seed ordering and fail-fast behavior.
- `crates/agentdash-api/src/routes/shared_library.rs` - route import and repository-set adapter changes.
- `crates/agentdash-api/src/routes/marketplace.rs` - external marketplace import/refresh route imports.
- `crates/agentdash-api/src/integrations.rs` - integration seed/provider collection; should stay API host composition.
- `crates/agentdash-application/src/repository_set.rs` - add adapter only; do not make new crate depend on it.
- `crates/agentdash-application/src/workflow/definition.rs` - source of builtin templates; should provide DTOs outward.
- `crates/agentdash-application/src/skill_asset/service.rs` - URL import caller may need import-path update; no skill split or redesign.
- `crates/agentdash-application/src/extension_package.rs` - should remain separate; avoid moving archive/storage logic by mistake.
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` - workflow install transaction must remain authoritative.

### Validation Commands

Use targeted Rust checks first:

```bash
cargo check -p agentdash-application-shared-library -p agentdash-application -p agentdash-api
cargo test -p agentdash-application-shared-library shared_library
cargo test -p agentdash-api shared_library
cargo test -p agentdash-api marketplace
cargo test -p agentdash-infrastructure shared_library_repository
cargo test -p agentdash-infrastructure workflow_template_install
cargo test -p agentdash-infrastructure extension_package_artifact_repository
cargo test -p agentdash-infrastructure skill_asset_repository
```

Final smoke command when the split compiles:

```bash
pnpm dev
```

`pnpm dev` is a runtime smoke check only; the crate split should be validated primarily with package-level Rust checks and the repository transaction tests above.

## Caveats / Not Found

- No external references were needed; this review is based on repository source and Trellis specs.
- No code or business files were modified.
- `task.py current --source` returned no active task in this Codex session; the output path was taken from the user-provided active task `.trellis/tasks/06-25-backend-large-module-crate-split-plan`.
- I intentionally did not review or plan `skill` module splitting. Mentions of `skill_asset/service.rs` are limited to direct Shared Library URL import/install boundaries.
- The exact workflow seed provider shape can be implemented either in the new shared_library crate as a trait or in a small application composition adapter. The important boundary is that workflow builtin templates flow into shared_library as DTOs or `BuiltinSeed` values, not via a dependency from shared_library to workflow application code.
