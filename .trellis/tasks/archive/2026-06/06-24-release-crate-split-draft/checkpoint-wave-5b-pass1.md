# Checkpoint Wave 5B Pass 1

Date: 2026-06-25

## Scope

Round 5B pass 1 repaired the first layer of source/path blockers after the crates-first physical split. This checkpoint keeps the branch moving with red target crates where failures now identify the next required port/composition boundary.

## Results

| Crate | Status | Notes |
| --- | --- | --- |
| `agentdash-application-vfs` | Green | Old `crate::runtime`, `crate::skill_asset`, `crate::runtime_tools` imports are removed. Generic VFS no longer depends on application owner modules. |
| `agentdash-application-runtime-session` | Green | Production paths no longer need old `crate::runtime`, `crate::vfs`, `crate::context`, `crate::hooks`, `crate::backend_execution_placement` roots. |
| `agentdash-application-lifecycle` | Red with narrow blocker | Old `crate::session`, `crate::agent_run`, `crate::vfs`, `crate::runtime_tools`, `crate::repository_set`, `crate::platform_config` gates are clean. Remaining blocker is workflow compiler / graph resolver ownership in `dispatch_service.rs`. |
| `agentdash-application-agentrun` | Red with broad composition blockers | Direct Lifecycle/RuntimeSession crate imports and repository_set/Lifecycle read-model dependencies are removed. Remaining errors are session-facing DTO/service imports, capability/context helpers, and frame construction dependencies on application composition modules. |

## Validation

Passed:

```powershell
cargo fmt --check
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-vfs --message-format short
cargo check -p agentdash-application-runtime-session --message-format short
```

Expected red:

```powershell
cargo check -p agentdash-application-lifecycle --message-format short
cargo check -p agentdash-application-agentrun --message-format short
```

## Remaining Owners

| Owner | Required repair |
| --- | --- |
| `lifecycle-workflow-compiler-port` | Move workflow compiler / graph resolver dependency out of Lifecycle implementation. Lifecycle should consume a port/closed request or composition-provided compiled plan, not own the compiler. |
| `agentrun-session-port-repair` | Replace AgentRun direct `crate::session` service/DTO imports with existing ports or new minimal neutral DTOs. |
| `agentrun-frame-composition-repair` | Move frame construction owner-bootstrap facts that depend on canvas/companion/context/mcp/project/story/workspace modules to application composition or ports. |
| `agentrun-capability-context-repair` | Split capability/context helpers needed by AgentRun into allowed ports/spi/domain helpers, or keep them in application composition. |
| `stale-test-repair` | Remove or rewrite test-only RuntimeSession forbidden-edge imports instead of reintroducing peer crate deps. |

## Next Dispatch

The next wave should not ask `agentdash-application-agentrun` to absorb application composition modules. It should either create explicit port contracts or move composition-heavy construction adapters back to `agentdash-application`.
