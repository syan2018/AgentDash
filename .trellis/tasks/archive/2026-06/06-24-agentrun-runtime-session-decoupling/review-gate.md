# Final Review Gate

Status: passed

Last verified: 2026-06-24

## Purpose

The final review gate proves that `session` has been demoted to RuntimeSession substrate and that AgentRun/Lifecycle own the application control plane and current runtime surface. Passing compile is not enough; the gate must prove dependency direction, consumer behavior and public facade shape.

## Required Evidence

### Boundary Evidence

- [x] `session` public facade exposes RuntimeSession substrate use cases only.
- [x] `AgentFrameRuntimeTarget` and live surface adoption port belong to AgentRun.
- [x] RuntimeSession live adoption is an implementation adapter, not a business/API entry.
- [x] AgentRun current/resource surface facades return DTOs without exposing domain `AgentFrame`.
- [x] RuntimeGateway MCP access consumes AgentRun query port and does not import SessionHub/current frame resolver.
- [x] RuntimeGateway-facing AgentRun surface/MCP access contracts live in `agentdash-application-ports`, not in a RuntimeGateway-to-AgentRun implementation dependency.
- [x] Surface-changing business paths submit typed AgentRun update requests.

### Import Evidence

Run and record:

```powershell
rg -n "agentdash_application::session::construction_planner|agentdash_application::session::plan|agentdash_application::session::AgentFrameRuntimeTarget" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src
rg -n "SessionRuntimeInner|session::hub|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-api/src crates/agentdash-application/src/runtime_gateway
rg -n "AgentFrameBuilder" crates/agentdash-application/src/canvas crates/agentdash-application/src/workspace_module crates/agentdash-application/src/permission crates/agentdash-api/src
```

Expected result: no production call sites except explicitly documented internal adapters/tests.

Result: passed on 2026-06-24. The first, second and frame/vfs internal-submodule searches returned no matches. The `AgentFrameBuilder` search returned only `#[cfg(test)]` fixtures in `permission::service::tests` and `workspace_module::tools::tests`.

### Behavioral Evidence

- [x] Canvas idle `mcp.list_tools` works through RuntimeGateway and AgentRun current surface with backend anchor.
- [x] `mcp.call_tool` uses current visible MCP tools and capability policy.
- [x] Canvas runtime invoke rejects mismatched Canvas Project / runtime session Project.
- [x] Canvas runtime bridge manifest rejects mismatched Canvas Project / runtime session Project.
- [x] Extension runtime action/channel rejects path Project / runtime session Project mismatch.
- [x] Terminal launch target derives backend/root from AgentRun current surface.
- [x] VFS `SessionRuntime` and `AgentRun` resource surfaces share AgentRun resource surface facade.
- [x] Permission grant surface-changing effect writes/adopts through AgentRun update facade.
- [x] WorkspaceModule Canvas bind/update submits typed AgentRun update request.
- [x] Session launch still accepts connector turns and streams events after AgentFrame/Lifecycle write ownership moves out.

### Compile And Test Gate

Minimum required commands:

```powershell
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application runtime_gateway::mcp_access
cargo test -p agentdash-application runtime_gateway
cargo test -p agentdash-application agent_run::runtime_surface
```

Result: passed on 2026-06-24. Additional touched-scope tests recorded in `work-items/WI-10-final-review-gate.md`.

Additional targeted tests are required for touched areas:

- Permission runtime surface update/adoption.
- Canvas/Extension Project/session guards.
- VFS SessionRuntime/AgentRun resource surfaces.
- Terminal launch target.
- Session launch/commit boundary.

### Documentation Gate

- [x] Update this task's work item docs with final status.
- [x] Update `parent-child-coverage.md` if implementation changes the work item mapping.
- [x] Update `.trellis/spec/backend/session/architecture.md` or an appropriate appendix if implementation establishes a durable contract not already captured.
- [x] Update `.trellis/spec/backend/runtime-gateway.md` only if Gateway-facing contracts change.
- [x] Do not record obsolete "do not do X" history. Document why the new boundary exists and what owns each fact.

Result: passed. The parent coverage mapping remains complete with no missing or partial item, so the matrix did not require remapping. Session and RuntimeGateway specs were updated for AgentRun project-agent context ownership and Project/session guard requirements.

## Review Failure Conditions

- API or RuntimeGateway current-surface path can still reach SessionHub.
- Business modules directly build AgentFrame revisions outside AgentRun-owned adapters.
- RuntimeSession public facade still exports AgentRun/Lifecycle ownership types.
- Route layer still selects RuntimeSessionExecutionAnchor/current AgentFrame for current/resource surface behavior.
- Tests prove only active-turn behavior but not idle/current surface behavior.
