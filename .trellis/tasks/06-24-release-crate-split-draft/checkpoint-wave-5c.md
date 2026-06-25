# Round 5C Integration Checkpoint

Date: 2026-06-25

## Boundary Result

- `agentdash-application` now owns cross-owner frame construction composition through `frame_construction`.
- `agentdash-application-agentrun` keeps frame primitives, runtime surface, mailbox, workspace command/read-model and AgentRun-owned DTOs; it no longer compiles the composition-heavy frame construction service.
- RuntimeSession to AgentRun service wiring is explicit through `runtime_session_agent_run_bridge`.
- API bootstrap wires extracted crates through composition adapters:
  - `RepositorySet::to_agent_run_repository_set`
  - `RepositorySet::to_lifecycle_repository_set`
  - `LifecycleTerminalCallbackAdapter`
  - lifecycle `PlatformConfig` projection
- VFS owner-provider registration moved to `agentdash_application::vfs_owner_providers`, keeping owner-provider composition out of the `session` facade path.
- API/local/MCP no longer import moved modules through old `agentdash_application::session|agent_run|lifecycle|vfs` paths.

## Validation

Passed:

```powershell
cargo fmt --check
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-runtime-session -p agentdash-application-vfs -p agentdash-application-agentrun -p agentdash-application-lifecycle -p agentdash-application --message-format short
cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp --message-format short
cargo check --workspace --message-format short
rg -n "agentdash_application::(session|agent_run|lifecycle|vfs)::" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
```

The final `rg` gate exited with no matches.

## Remaining Warnings

Workspace check is green with warnings concentrated in extracted implementation crates:

- RuntimeSession: unused context continuation helpers and baseline capability helpers.
- AgentRun: unused presentation/lifecycle read-model helpers and runtime capability discovery helpers.
- Lifecycle: unused fields in lifecycle projection/provider structs.
- Application: two unused re-export/helper imports.

These warnings are non-blocking for Round 5C because the physical dependency contract is enforced and workspace check passes. They should be handled as follow-up cleanup after the crate split stabilizes.

## Follow-Up Candidates

- Move `PlatformConfig` and terminal callback contracts into `agentdash-application-ports` so Lifecycle and RuntimeSession consume one canonical type.
- Replace lifecycle tool provider's placeholder `SharedSessionToolServicesHandle` with a real port contract owned outside application composition.
- Review dead presentation/read-model helpers after consumers settle and delete unused surfaces that remain unreferenced.
