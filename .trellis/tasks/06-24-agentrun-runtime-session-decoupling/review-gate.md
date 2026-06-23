# Final Review Gate

## Purpose

The final review gate proves that `session` has been demoted to RuntimeSession substrate and that AgentRun/Lifecycle own the application control plane and current runtime surface. Passing compile is not enough; the gate must prove dependency direction, consumer behavior and public facade shape.

## Required Evidence

### Boundary Evidence

- `session` public facade exposes RuntimeSession substrate use cases only.
- `AgentFrameRuntimeTarget` and live surface adoption port belong to AgentRun.
- RuntimeSession live adoption is an implementation adapter, not a business/API entry.
- AgentRun current/resource surface facades return DTOs without exposing domain `AgentFrame`.
- RuntimeGateway MCP access consumes AgentRun query port and does not import SessionHub/current frame resolver.
- RuntimeGateway-facing AgentRun surface/MCP access contracts live in `agentdash-application-ports`, not in a RuntimeGateway-to-AgentRun implementation dependency.
- Surface-changing business paths submit typed AgentRun update requests.

### Import Evidence

Run and record:

```powershell
rg -n "agentdash_application::session::construction_planner|agentdash_application::session::plan|agentdash_application::session::AgentFrameRuntimeTarget" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src
rg -n "SessionRuntimeInner|session::hub|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-api/src crates/agentdash-application/src/runtime_gateway
rg -n "AgentFrameBuilder" crates/agentdash-application/src/canvas crates/agentdash-application/src/workspace_module crates/agentdash-application/src/permission crates/agentdash-api/src
```

Expected result: no production call sites except explicitly documented internal adapters/tests.

### Behavioral Evidence

- Canvas idle `mcp.list_tools` works through RuntimeGateway and AgentRun current surface with backend anchor.
- `mcp.call_tool` uses current visible MCP tools and capability policy.
- Canvas runtime invoke rejects mismatched Canvas Project / runtime session Project.
- Canvas runtime bridge manifest rejects mismatched Canvas Project / runtime session Project.
- Extension runtime action/channel rejects path Project / runtime session Project mismatch.
- Terminal launch target derives backend/root from AgentRun current surface.
- VFS `SessionRuntime` and `AgentRun` resource surfaces share AgentRun resource surface facade.
- Permission grant surface-changing effect writes/adopts through AgentRun update facade.
- WorkspaceModule Canvas bind/update submits typed AgentRun update request.
- Session launch still accepts connector turns and streams events after AgentFrame/Lifecycle write ownership moves out.

### Compile And Test Gate

Minimum required commands:

```powershell
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application runtime_gateway::mcp_access
cargo test -p agentdash-application runtime_gateway
cargo test -p agentdash-application agent_run::runtime_surface
```

Additional targeted tests are required for touched areas:

- Permission runtime surface update/adoption.
- Canvas/Extension Project/session guards.
- VFS SessionRuntime/AgentRun resource surfaces.
- Terminal launch target.
- Session launch/commit boundary.

### Documentation Gate

- Update this task's work item docs with final status.
- Update `parent-child-coverage.md` if implementation changes the work item mapping.
- Update `.trellis/spec/backend/session/architecture.md` or an appropriate appendix if implementation establishes a durable contract not already captured.
- Update `.trellis/spec/backend/runtime-gateway.md` only if Gateway-facing contracts change.
- Do not record obsolete "do not do X" history. Document why the new boundary exists and what owns each fact.

## Review Failure Conditions

- API or RuntimeGateway current-surface path can still reach SessionHub.
- Business modules directly build AgentFrame revisions outside AgentRun-owned adapters.
- RuntimeSession public facade still exports AgentRun/Lifecycle ownership types.
- Route layer still selects RuntimeSessionExecutionAnchor/current AgentFrame for current/resource surface behavior.
- Tests prove only active-turn behavior but not idle/current surface behavior.
