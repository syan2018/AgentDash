# Checkpoint Wave 5B Pass 2

Date: 2026-06-25

## Scope

Round 5B pass 2 completed target-crate source repair after the crates-first physical split. The target implementation crates now compile independently and static forbidden-edge gates are clean. Remaining work moves to `agentdash-application` composition/facade wiring and API/local/MCP consumer integration.

## Target Crate Status

| Crate | Status |
| --- | --- |
| `agentdash-application-ports` | Green |
| `agentdash-application-runtime-gateway` | Green |
| `agentdash-application-vfs` | Green |
| `agentdash-application-runtime-session` | Green |
| `agentdash-application-agentrun` | Green |
| `agentdash-application-lifecycle` | Green |

## Completed Repairs

- Lifecycle no longer imports application workflow compiler / graph resolver. It now consumes a pure `WorkflowGraphPlanningPort`.
- Application owns `ApplicationWorkflowGraphPlanner`, which adapts existing workflow graph resolver/compiler to that port.
- RuntimeSession test-only forbidden-edge imports were removed; old hub direct-implementation tests that depended on AgentRun/VFS concrete helpers were deleted.
- AgentRun no longer compiles frame-construction application composition modules as part of its extracted crate.
- AgentRun uses local runtime-session boundary DTOs/traits and a `ProjectAgentLifecycleLaunchPort` instead of direct RuntimeSession/Lifecycle implementation imports.
- AgentRun localizes pure permission/canvas projection where needed and leaves app-owner composition facts outside the crate.

## Validation

Passed:

```powershell
cargo fmt --check
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-vfs --message-format short
cargo check -p agentdash-application-runtime-session --message-format short
cargo check -p agentdash-application-agentrun --message-format short
cargo check -p agentdash-application-lifecycle --message-format short
rg -n "agentdash_application::|agentdash_application_(agentrun|lifecycle|runtime_session|runtime_gateway|vfs)" crates/agentdash-application-ports -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|lifecycle|runtime_session|runtime_gateway)" crates/agentdash-application-vfs -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|lifecycle|runtime_gateway)|crate::(agent_run|lifecycle)" crates/agentdash-application-runtime-session/src -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(lifecycle|runtime_session)|crate::(session|lifecycle)::" crates/agentdash-application-agentrun/src -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|runtime_session)|crate::(agent_run|session)::" crates/agentdash-application-lifecycle/src -g '*.rs'
```

The target crate checks emit dead-code warnings from surfaces that will be consumed by application composition in the next wave. They are not current blockers.

## Remaining Owners

| Owner | Required repair |
| --- | --- |
| `application-composition-repair` | Wire `ApplicationWorkflowGraphPlanner`, `ProjectAgentLifecycleLaunchPort`, and RuntimeSession boundary adapters into application composition/facade services. |
| `application-frame-construction-owner` | Decide whether old `agent_run/frame/construction/**` application-composition-heavy files move back under `agentdash-application` or split into explicit ports. |
| `api-local-mcp-repair` | Update AppState/bootstrap/routes/local code to use new extracted crate APIs and composition adapters. |
| `workspace-check-owner` | Run `cargo check -p agentdash-application`, then API/local/MCP checks, then workspace check once composition is wired. |

## Next Dispatch

Round 5B pass 3 should focus on `agentdash-application` composition and API/local integration. It should not modify target implementation crates unless a compile error proves the target crate API is incomplete.
