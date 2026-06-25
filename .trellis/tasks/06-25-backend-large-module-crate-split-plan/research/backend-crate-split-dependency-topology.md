# Research: backend-crate-split-dependency-topology

- Query: Review AgentDash backend crate split dependency topology and execution order for workflow/hooks/shared_library; explicitly exclude skill module split.
- Scope: internal
- Date: 2026-06-25

## Findings

### Executive Conclusion

Current Cargo topology supports further splitting, but not by moving directories wholesale in one pass.

Recommended order:

1. `workflow` first.
2. `hooks` second.
3. `shared_library` last.

Reason:

- `workflow` is the dependency stabilizer. `shared_library` already imports workflow builtin/template types, and `hooks` consumes lifecycle active workflow projection. Stabilizing workflow/lifecycle ownership first prevents follow-up churn.
- `hooks` is independently splittable after workflow/lifecycle projection imports settle. Its normal runtime dependency should be lifecycle + domain + SPI, not the `agentdash-application` facade.
- `shared_library` must be last because it depends on workflow templates, VFS constants, a broad facade `RepositorySet`, marketplace SPI, integration seed types, and Project asset installation/publish transactions. It also contains project skill asset install/publish paths, so it should wait until the parallel skill work has landed or at least expose no dependency on the old facade.

Do not attempt to make `agentdash-application-workflow` depend on `agentdash-application-lifecycle` while also moving lifecycle orchestration reducer/runtime into workflow. That is the main cycle risk:

```text
agentdash-application-lifecycle -> agentdash-application-workflow
agentdash-application-workflow -> agentdash-application-lifecycle
```

The clean split is:

```text
agentdash-application-ports
  <- agentdash-application-workflow    # pure workflow compiler/script/catalog/reducer contracts
  <- agentdash-application-lifecycle   # lifecycle dispatch, executor launcher, materialization
  <- agentdash-application-hooks       # hook provider consuming lifecycle projection
  <- agentdash-application-shared-library # marketplace/install/publish consuming workflow/vfs/domain
  <- agentdash-application             # temporary facade + composition adapters
  <- agentdash-api / agentdash-mcp     # interface consumers
```

### Current Cargo Graph

Direct `cargo tree --depth 1` evidence:

- `agentdash-application` depends on `agentdash-application-agentrun`, `agentdash-application-lifecycle`, `agentdash-application-ports`, `agentdash-application-runtime-session`, `agentdash-application-vfs`, `agentdash-application-skill`, domain/SPI/contracts/workspace-module.
- `agentdash-application-lifecycle` depends on ports, VFS, domain, SPI, workspace-module, and skill, but not on `agentdash-application`.
- `agentdash-application-agentrun` depends on ports, VFS, domain, SPI, contracts, workspace-module, and skill, but not on `agentdash-application`.
- `agentdash-api` already depends on both facade and split crates: `agentdash-application`, `agentdash-application-agentrun`, `agentdash-application-lifecycle`, ports/runtime/vfs, infrastructure, executor, MCP.
- `agentdash-mcp` still depends on `agentdash-application`, domain, and SPI.
- `agentdash-infrastructure` depends only on domain, SPI, agent-protocol, and technical libraries; it does not depend on application.
- `agentdash-application-ports` is currently below application crates: agent-protocol, agent-types, domain, relay, SPI.

This means the existing graph can host new leaf application crates if those crates do not depend on the facade.

### Files Found

- `crates/agentdash-application/Cargo.toml` - facade currently depends on application split crates and remaining large modules.
- `crates/agentdash-application/src/lib.rs` - facade exports current modules plus re-exports for `agent_run`, `lifecycle`, `vfs`, and skill boundary.
- `crates/agentdash-application/src/repository_set.rs` - broad application repository aggregation and adapters to split crate repository sets.
- `crates/agentdash-application/src/workflow/**` - workflow catalog, builtin definitions, graph resolver/planner, script preflight, compiler facade, plus re-export of lifecycle orchestration runtime.
- `crates/agentdash-application-lifecycle/src/workflow/**` - lifecycle-owned orchestration runtime reducer and executor launcher.
- `crates/agentdash-application/src/hooks/**` - application hook provider, active workflow snapshot/contribution, rule engine, preset registry.
- `crates/agentdash-application/src/shared_library/**` - shared library service, seed, install, publish, external marketplace import/refresh.
- `crates/agentdash-api/src/routes/workflows.rs` - API consumes `application::workflow`, `application::hooks`, and `application-lifecycle` together.
- `crates/agentdash-api/src/bootstrap/session.rs` - API constructs `AppExecutionHookProvider`.
- `crates/agentdash-api/src/bootstrap/repositories.rs` - API seeds Shared Library at startup and wires facade `RepositorySet`.
- `crates/agentdash-api/src/routes/shared_library.rs` and `crates/agentdash-api/src/routes/marketplace.rs` - HTTP surfaces for shared library and marketplace.
- `crates/agentdash-mcp/src/error.rs` - MCP depends on facade `ApplicationError`.
- `crates/agentdash-mcp/src/services.rs` - MCP service set is mostly domain repository traits; facade dependency is not for target modules.
- `crates/agentdash-infrastructure/Cargo.toml` and `crates/agentdash-infrastructure/src/**` - no application dependency; safe from reverse binding.

### Code Patterns

#### Facade state

- `crates/agentdash-application/src/lib.rs:1` re-exports `agentdash_application_agentrun::agent_run::*` under `application::agent_run`.
- `crates/agentdash-application/src/lib.rs:16` still declares `pub mod hooks`.
- `crates/agentdash-application/src/lib.rs:17` re-exports `agentdash_application_lifecycle::*` under `application::lifecycle`.
- `crates/agentdash-application/src/lib.rs:36` still declares `pub mod shared_library`.
- `crates/agentdash-application/src/lib.rs:43` re-exports `agentdash_application_vfs::*` under `application::vfs`.
- `crates/agentdash-application/src/lib.rs:48` still declares `pub mod workflow`.

Conclusion: `agentdash-application` is already a hybrid facade + remaining implementation crate. Keep it as facade during all splits; narrow only after API/MCP imports no longer need target modules through it.

#### RepositorySet coupling

- `crates/agentdash-application/src/repository_set.rs:48` defines a broad `RepositorySet`.
- `crates/agentdash-application/src/repository_set.rs:63` includes `shared_library_repo`.
- `crates/agentdash-application/src/repository_set.rs:72` through `:81` include workflow/lifecycle repositories.
- `crates/agentdash-application/src/repository_set.rs:97` creates `agentdash_application_agentrun::AgentRunRepositorySet`.
- `crates/agentdash-application/src/repository_set.rs:144` creates `agentdash_application_lifecycle::RepositorySet`.
- `crates/agentdash-application/src/repository_set.rs:252` uses `LifecycleDispatchFacade` to implement the `ProjectAgentLifecycleLaunchPort`.

Conclusion: moving `shared_library` or workflow executor code without first replacing `crate::repository_set::RepositorySet` creates a dependency back to the facade. New crates need local narrow repository sets or trait bundles.

#### Workflow current split

- `crates/agentdash-application/src/workflow/mod.rs:1` through `:6` declares catalog/definition/planner/resolver/orchestration/script.
- `crates/agentdash-application/src/workflow/mod.rs:11` through `:18` exports catalog, builtin definitions, graph planner, graph resolver.
- `crates/agentdash-application/src/workflow/mod.rs:19` through `:30` exports orchestration executor and compilers.
- `crates/agentdash-application/src/workflow/orchestration/mod.rs:2` through `:14` re-exports lifecycle orchestration runtime from `agentdash_application_lifecycle::workflow::orchestration`.
- `crates/agentdash-application/src/workflow/graph_planner.rs:1` consumes `agentdash_application_ports::workflow_graph_planning`.
- `crates/agentdash-application/src/workflow/graph_planner.rs:8` imports `crate::lifecycle::WorkflowApplicationError`.
- `crates/agentdash-application/src/workflow/catalog.rs:9` imports `crate::lifecycle::WorkflowApplicationError`.
- `crates/agentdash-application/src/workflow/graph_resolver.rs:5` imports `crate::lifecycle::WorkflowApplicationError`.

Conclusion: workflow implementation is already conceptually below lifecycle for compiler/planner pieces, but it borrows lifecycle's error type through the facade. Extracting workflow first requires moving or re-exporting the workflow error from a non-facade crate.

#### Lifecycle workflow runtime

- `crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs:1` through `:12` imports only collections, domain workflow types, chrono, serde_json, uuid.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs:37` through `:69` exposes orchestration activation helpers.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs:266` and `:286` expose reducer entry points.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs:21` and `:22` import lifecycle error, `RepositorySet`, and `SharedPlatformConfig`.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs:12` through `:14` imports `LifecycleDispatchService`, `WorkflowAgentNodeMaterializationRequest`, and `WorkflowApplicationError`.
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/function_node_runner.rs:7` consumes `agentdash_spi::FunctionRunner`.

Conclusion: `runtime.rs` is mechanically movable into a lower workflow crate. `executor_launcher.rs`, `agent_node_launcher.rs`, `human_gate_launcher.rs`, and related launcher code are not purely workflow: they orchestrate lifecycle dispatch, runtime session creation, frame materialization, human gates, and function runners. Keep them in lifecycle first, or introduce explicit ports before moving.

#### Hooks current shape

- `crates/agentdash-application/src/hooks/mod.rs:3` imports `crate::lifecycle::ActiveWorkflowProjection`.
- `crates/agentdash-application/src/hooks/provider.rs:20` implements SPI `ExecutionHookProvider`.
- `crates/agentdash-application/src/hooks/provider.rs:32` documents `AppExecutionHookProvider` as a facade over owner resolver, active workflow snapshot builder, and hook script engine.
- `crates/agentdash-application/src/hooks/provider.rs:41` defines `AppExecutionHookProviderRepos`, already a narrow repo input struct.
- `crates/agentdash-application/src/hooks/provider.rs:58` constructs the provider from repos and a script evaluator factory.
- `crates/agentdash-application/src/hooks/provider.rs:103`, `:218`, and `:224` still reference `crate::lifecycle`.
- `crates/agentdash-application/src/hooks/active_workflow_snapshot.rs:10` through `:14` imports lifecycle execution log and active workflow projection helpers.
- `crates/agentdash-application/src/hooks/owner_resolver.rs:10` and `:11` import `crate::ApplicationError` and `crate::lifecycle::build_subject_run_context`.
- `crates/agentdash-application/src/hooks/script_engine.rs:3` consumes hook scripting types from SPI.
- `crates/agentdash-application/src/hooks/script_engine.rs:325` and `crates/agentdash-application/src/hooks/rules.rs:200` use infrastructure Rhai evaluator only in tests.

Conclusion: hooks can become `agentdash-application-hooks` without depending on the facade. Required design work is replacing `crate::ApplicationError` and `crate::lifecycle` imports with direct lifecycle/workflow crate imports and a local hook application error mapping. The Rhai adapter remains a factory supplied by API/infrastructure via SPI, so normal dependencies do not need infrastructure.

#### Shared Library current shape

- `crates/agentdash-application/src/shared_library/mod.rs:1` through `:5` declares external marketplace, install, publish, seed, service.
- `crates/agentdash-application/src/shared_library/mod.rs:7` re-exports domain `seed_digest`.
- `crates/agentdash-application/src/shared_library/seed.rs:6` imports `crate::workflow::list_builtin_workflow_templates`.
- `crates/agentdash-application/src/shared_library/install.rs:21` imports `crate::vfs::PROJECT_VFS_MOUNT_CONTAINER_ID`.
- `crates/agentdash-application/src/shared_library/install.rs:25` imports facade `RepositorySet`.
- `crates/agentdash-application/src/shared_library/install.rs:26` imports `crate::workflow::BuiltinWorkflowTemplateBundle`.
- `crates/agentdash-application/src/shared_library/install.rs:103` is the install entry point.
- `crates/agentdash-application/src/shared_library/install.rs:902` handles workflow template installation.
- `crates/agentdash-application/src/shared_library/publish.rs:21` imports facade `RepositorySet`.
- `crates/agentdash-application/src/shared_library/publish.rs:23` imports workflow template helpers/types.
- `crates/agentdash-application/src/shared_library/publish.rs:74` is the publish entry point.
- `crates/agentdash-application/src/shared_library/publish.rs:454` and `:479` handle workflow payload/lifecycle workflow collection.
- `crates/agentdash-application/src/shared_library/service.rs:23` is already repository-narrow: `SharedLibraryService<'a>` over library asset repository only.
- `crates/agentdash-application/src/shared_library/external_marketplace.rs:61` and `:144` import/refresh marketplace assets using domain/SPI plus library repository.

Conclusion: `service.rs` and `external_marketplace.rs` are mechanically movable. `seed.rs` needs the workflow crate first. `install.rs` and `publish.rs` require a new narrow `SharedLibraryRepositorySet` / trait bundle and direct dependency on workflow/VFS/domain; otherwise the new crate would depend on `agentdash-application`, creating a cycle.

#### API consumption

- `crates/agentdash-api/src/routes/workflows.rs:19` imports `agentdash_application::hooks::hook_rule_preset_registry`.
- `crates/agentdash-api/src/routes/workflows.rs:20` through `:22` imports `agentdash_application::workflow::*`.
- `crates/agentdash-api/src/routes/workflows.rs:24` imports `agentdash_application_lifecycle::*`.
- `crates/agentdash-api/src/routes/workflows.rs:543` constructs `OrchestrationExecutorLauncher`.
- `crates/agentdash-api/src/routes/workflows.rs:894` still types the drain mapper as `agentdash_application::workflow::OrchestrationExecutorDrainResult`.
- `crates/agentdash-api/src/bootstrap/session.rs:5` imports `agentdash_application::hooks::AppExecutionHookProvider`.
- `crates/agentdash-api/src/bootstrap/session.rs:248` constructs `AppExecutionHookProvider`.
- `crates/agentdash-api/src/bootstrap/session.rs:261` supplies `agentdash_infrastructure::RhaiHookScriptEvaluator`.
- `crates/agentdash-api/src/routes/shared_library.rs:10` through `:16` imports shared library application functions through the facade.
- `crates/agentdash-api/src/routes/shared_library.rs:141` calls `install_library_asset_to_project`.
- `crates/agentdash-api/src/routes/shared_library.rs:215` calls `publish_project_asset_to_library`.
- `crates/agentdash-api/src/routes/marketplace.rs:9` imports external marketplace helpers through the facade.
- `crates/agentdash-api/src/bootstrap/repositories.rs:8` through `:9` imports `IntegrationEmbeddedLibraryAssetSeed` and `SharedLibraryService` through the facade.

Conclusion: API can migrate endpoint-by-endpoint to direct crates. It should continue to depend on the facade until all non-target modules have their own crates or imports.

#### MCP and infrastructure

- `crates/agentdash-mcp/src/error.rs:1` imports `agentdash_application::ApplicationError`.
- `crates/agentdash-mcp/src/services.rs:3` through `:11` mostly uses domain repository traits.
- `crates/agentdash-infrastructure/Cargo.toml` has no application crate dependency.

Conclusion: MCP should not block target module splits. Keep facade for MCP until `ApplicationError` and task services are independently available. Infrastructure is already in the correct direction and should remain application-free.

### Recommended Execution Plan

#### Phase 0 - Dependency Baseline

Goal: prove the current dependency graph and lock scope before moving code.

Actions:

- Run `cargo tree -p agentdash-application --depth 1 --edges normal`.
- Run `cargo tree -p agentdash-application-lifecycle --depth 1 --edges normal`.
- Run `cargo tree -p agentdash-application-agentrun --depth 1 --edges normal`.
- Run `cargo tree -p agentdash-api --depth 1 --edges normal`.
- Run `rg "agentdash_application::(workflow|hooks|shared_library)" crates/agentdash-api/src crates/agentdash-mcp/src`.

Stop condition:

- Baseline captured and no implementation starts until target import owners are known.

#### Phase 1 - Split Workflow Core First

Goal: create `agentdash-application-workflow` as the stable lower-level workflow crate.

Target contents first:

- Move `crates/agentdash-application/src/workflow/definition.rs`.
- Move `catalog.rs`.
- Move `graph_resolver.rs`.
- Move `graph_planner.rs`.
- Move `script/**`.
- Move `orchestration/compiler.rs`.
- Move `orchestration/script_compiler.rs`.
- Move pure `crates/agentdash-application-lifecycle/src/workflow/orchestration/runtime.rs` if lifecycle can depend on the new workflow crate.

Do first:

- Decide error ownership before file moves. `WorkflowApplicationError` cannot stay exclusively in lifecycle if workflow core imports it. Preferred: define/re-export workflow application error from the new workflow crate, then make lifecycle re-export it for existing call sites during migration.
- Keep lifecycle executor launchers in `agentdash-application-lifecycle` initially because they depend on `LifecycleDispatchService`, lifecycle `RepositorySet`, `SharedPlatformConfig`, runtime session creation, and frame materialization.
- Keep `agentdash-application/src/workflow/mod.rs` as a facade re-export after the move.

Mechanical:

- Builtin JSON/template loading.
- Builder document parsing.
- Capability summary extraction.
- Graph resolver/planner after error import is fixed.
- Script compiler and graph compiler after imports target the new crate.
- Pure orchestration reducer/activation if it stays free of lifecycle imports.

Needs design port / ownership decision:

- `WorkflowApplicationError` ownership.
- Whether orchestration reducer lives in workflow core and launchers remain lifecycle, or whether a future second step introduces launcher ports.
- API import migration timing.

Do not do:

- Do not move lifecycle `executor_launcher.rs` and `agent_node_launcher.rs` in the same first pass unless `LifecycleDispatchService` is also ported. That would create either a cycle or an oversized workflow crate.

Validation:

- `cargo check -p agentdash-application-workflow`
- `cargo check -p agentdash-application-lifecycle`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo test -p agentdash-application-workflow`
- `cargo test -p agentdash-application-lifecycle`
- `pnpm run contracts:check` if workflow API DTO/conversion signatures change.

Stop condition:

- `agentdash-application-workflow` has no normal dependency on `agentdash-application` or `agentdash-application-lifecycle`.
- `agentdash-application-lifecycle` may depend on `agentdash-application-workflow`.
- `application::workflow` remains a facade only.
- Existing workflow API behavior compiles and tests pass.

#### Phase 2 - Migrate Workflow Consumers off Facade

Goal: make API and downstream application crates consume workflow/lifecycle directly.

Actions:

- Update `crates/agentdash-api/src/routes/workflows.rs` imports:
  - workflow script/catalog/compiler types from `agentdash-application-workflow`.
  - orchestration executor launcher types from `agentdash-application-lifecycle` while launcher remains there.
  - hook preset registry from hooks crate after Phase 3, or keep facade until hooks split.
- Update type annotations like `agentdash_application::workflow::OrchestrationExecutorDrainResult` to direct crate paths.
- Leave `agentdash-application` facade exports during this phase.

Validation:

- `cargo check -p agentdash-api`
- `cargo check -p agentdash-application`
- `rg "agentdash_application::workflow" crates/agentdash-api/src crates/agentdash-mcp/src`

Stop condition:

- No API/MCP target workflow imports through `agentdash_application::workflow`.
- Facade re-export remains only for transitional/non-migrated internal consumers.

#### Phase 3 - Split Hooks

Goal: create `agentdash-application-hooks` without depending on the facade.

Target contents:

- Move `crates/agentdash-application/src/hooks/**` into the new crate.
- Keep `agentdash-application/src/hooks/mod.rs` as facade re-export temporarily.

Do first:

- Replace `crate::lifecycle::*` imports with direct `agentdash-application-lifecycle` or workflow crate imports.
- Replace `crate::ApplicationError` in `owner_resolver.rs` and script engine surfaces with local hook error mapping or direct `HookError` conversion. Do not import facade.
- Keep script execution behind `agentdash_spi::HookScriptEvaluator`; API continues to supply `agentdash_infrastructure::RhaiHookScriptEvaluator`.

Mechanical:

- Preset registry and scripts.
- Global rules.
- Script decision building.
- Provider struct and narrow `AppExecutionHookProviderRepos`.

Needs design port / ownership decision:

- Error mapping from domain/lifecycle into `HookError`.
- Final owner for `ActiveWorkflowProjection` if Phase 1 moves projection-related types.

Validation:

- `cargo check -p agentdash-application-hooks`
- `cargo test -p agentdash-application-hooks`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-application`
- `rg "agentdash_application::hooks" crates/agentdash-api/src crates/agentdash-mcp/src`

Stop condition:

- `agentdash-application-hooks` has no normal dependency on `agentdash-application`.
- API imports hook provider/preset registry directly from hooks crate.
- `agentdash-application::hooks` is only a temporary re-export.

#### Phase 4 - Split Shared Library Last

Goal: create `agentdash-application-shared-library` after workflow and hooks imports are stable.

Target contents:

- Move `service.rs` and `external_marketplace.rs` first; they are closest to mechanical.
- Move `seed.rs` after workflow builtins are available from `agentdash-application-workflow`.
- Move `install.rs` and `publish.rs` after introducing a local shared-library repository set / trait bundle.

Do first:

- Introduce `SharedLibraryRepositorySet` in the new crate with only the repos needed by install/publish/source-status. Do not use facade `RepositorySet`.
- Replace `crate::workflow::*` with `agentdash-application-workflow::*`.
- Replace `crate::vfs::PROJECT_VFS_MOUNT_CONTAINER_ID` with direct `agentdash-application-vfs` import or a lower shared constant if appropriate.
- Keep integration seed type available from the new shared-library crate and update API integration imports.
- Coordinate with parallel skill split before moving or changing Project SkillAsset install/publish behavior. This plan does not review or alter skill module boundaries.

Mechanical:

- `SharedLibraryService`.
- External marketplace import/refresh.
- Domain payload validation/digest helpers.

Needs design port / ownership decision:

- `SharedLibraryRepositorySet` fields and transaction expectations for install/publish.
- Workflow template provider dependency.
- VFS constant dependency.
- Extension package artifact storage and package ownership flow, if publish/install signatures expose storage-related behavior later.

Validation:

- `cargo check -p agentdash-application-shared-library`
- `cargo test -p agentdash-application-shared-library`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-infrastructure`
- `pnpm run contracts:check` if Shared Library DTO import/export signatures change.
- `pnpm run migration:guard` only if schema/migration files are touched; crate split alone should not require migration.

Stop condition:

- `agentdash-application-shared-library` has no normal dependency on `agentdash-application`.
- API shared-library/marketplace/integration imports come from the new crate.
- Facade only re-exports shared-library public surface.
- No changes were made to skill module split scope.

#### Phase 5 - Narrow Facade

Goal: reduce `agentdash-application` from implementation owner to transitional facade/composition crate.

Actions:

- Remove target module implementation files only after direct imports are migrated.
- Keep `RepositorySet` and composition adapters until remaining modules no longer need them.
- Move `ApplicationError` only when MCP/task/application consumers have direct alternatives.
- Do not delete facade while `agentdash-mcp/src/error.rs:1` still imports `agentdash_application::ApplicationError`.

Validation:

- `rg "agentdash_application::(workflow|hooks|shared_library)" crates`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-mcp`
- `cargo check -p agentdash-application`

Stop condition:

- No interface crate imports target modules through facade.
- Facade retains only intentionally transitional re-exports and composition adapters.

### Dependency Cycles to Avoid

1. Workflow core depending on lifecycle while lifecycle uses workflow reducer/compiler.
   - Avoid by placing compiler/reducer in workflow core and keeping launcher/materialization in lifecycle.

2. Hooks crate depending on `agentdash-application`.
   - Avoid by replacing `crate::ApplicationError` and `crate::lifecycle` imports with direct lifecycle/workflow/domain/SPI dependencies.

3. Shared Library crate depending on `agentdash-application`.
   - Avoid by replacing facade `RepositorySet` with a local narrow repository set and direct dependencies on workflow/VFS/domain/SPI.

4. `agentdash-application-ports` depending upward on new application crates.
   - Avoid by keeping ports pure and below all application split crates.

5. Infrastructure depending on application for script/runtime helpers.
   - Avoid by keeping Rhai script evaluators in infrastructure behind SPI traits. Current code already follows this for hooks and workflow script evaluator.

### Facade Recommendation

`agentdash-application` should continue as facade during all target splits.

Narrow it only when:

- API imports workflow/hooks/shared_library directly.
- MCP no longer requires facade `ApplicationError` or target module APIs.
- Remaining modules have either moved or are explicitly owned by the facade/composition layer.

The facade should not be used by new split crates. New split crates must treat `agentdash-application` as an outer compatibility surface, never as an inner dependency.

### Verification Matrix

Baseline:

```bash
cargo tree -p agentdash-application --depth 1 --edges normal
cargo tree -p agentdash-application-lifecycle --depth 1 --edges normal
cargo tree -p agentdash-application-agentrun --depth 1 --edges normal
cargo tree -p agentdash-api --depth 1 --edges normal
cargo tree -p agentdash-mcp --depth 1 --edges normal
cargo tree -p agentdash-infrastructure --depth 1 --edges normal
```

Workflow phase:

```bash
cargo check -p agentdash-application-workflow
cargo check -p agentdash-application-lifecycle
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application-workflow
cargo test -p agentdash-application-lifecycle
```

Hooks phase:

```bash
cargo check -p agentdash-application-hooks
cargo test -p agentdash-application-hooks
cargo check -p agentdash-api
cargo check -p agentdash-application
```

Shared Library phase:

```bash
cargo check -p agentdash-application-shared-library
cargo test -p agentdash-application-shared-library
cargo check -p agentdash-api
cargo check -p agentdash-application
cargo check -p agentdash-infrastructure
```

Interface and contracts:

```bash
cargo check -p agentdash-mcp
pnpm run contracts:check
pnpm run migration:guard
```

Use `pnpm run migration:guard` only when migrations/schema are touched; the crate split itself should not require database changes.

### Related Specs

- `.trellis/spec/backend/architecture.md` - clean architecture and current crate responsibilities.
- `.trellis/spec/backend/directory-structure.md` - crate layering and application-ports role.
- `.trellis/spec/backend/workflow/architecture.md` - workflow/lifecycle/orchestration ownership.
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - runtime reducer/executor contract.
- `.trellis/spec/backend/workflow/lifecycle-edge.md` - workflow graph edge rules.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - lifecycle subject association and runtime-session trace relationship.
- `.trellis/spec/backend/hooks/architecture.md` - hook provider and active workflow projection authority.
- `.trellis/spec/backend/hooks/execution-hook-runtime.md` - loop boundary and SPI `ExecutionHookProvider` contract.
- `.trellis/spec/backend/hooks/hook-script-engine.md` - Rhai hook engine split between application adapter and infrastructure evaluator.
- `.trellis/spec/backend/shared-library.md` - seed/install/publish backend invariants.
- `.trellis/spec/cross-layer/shared-library-contract.md` - Shared Library/Marketplace/Project asset contract.
- `.trellis/spec/guides/cross-layer-thinking-guide.md` - runtime hook/workflow boundary thinking guide.

### External References

- No web or third-party documentation was needed.
- Local tool evidence: `cargo tree --depth 1 --edges normal` for application, lifecycle, agentrun, api, mcp, infrastructure, and application-ports.

### Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned `Current task: (none)`, so this research used the user-provided task path `.trellis/tasks/06-25-backend-large-module-crate-split-plan`.
- Skill module split was intentionally not reviewed. Existing references to skill-related crate dependencies or Project SkillAsset install/publish appear only as dependency caveats for shared_library ordering.
- This research did not modify `design.md`, `implement.md`, spec files, Cargo manifests, or business code.
- `cargo tree` emitted package cache file-lock wait messages but completed successfully.

## Caveats / Not Found

No current Cargo cycle exists among the inspected crates. The risks are prospective cycles introduced by an incorrect split, especially new crates depending on the `agentdash-application` facade.

The most important not-found item: no existing narrow repository set for Shared Library install/publish was found. That should be designed before moving `install.rs` or `publish.rs`.
