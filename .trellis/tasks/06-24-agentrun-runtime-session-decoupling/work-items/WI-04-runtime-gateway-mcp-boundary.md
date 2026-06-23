# WI-04 RuntimeGateway And MCP Boundary Hardening

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Move gateway-facing AgentRun surface/MCP access contracts to `agentdash-application-ports`, keep RuntimeGateway providers behind the RuntimeGateway facade, and prevent fallback to SessionHub/current frame resolver.

## Dependencies

- `WI-02`

## Scope

- Move gateway-facing AgentRun current surface query DTO/trait or reduced RuntimeGateway-facing contract into `agentdash-application-ports`.
- Ensure `CurrentSurfaceRuntimeMcpAccess` consumes only the ports crate contract and MCP discovery.
- Keep `McpListToolsProvider`, `McpCallToolProvider`, and dynamic providers behind RuntimeGateway facade modules.
- Add tests or static guards that RuntimeGateway MCP access does not import SessionHub, `AgentFrame`, `AgentFrameSurfaceExt`, or current frame resolver.

## Deliverables

- RuntimeGateway MCP access remains query-backed through `agentdash-application-ports`.
- Tests for idle list/call and capability filtering remain passing.

## Acceptance

- `cargo test -p agentdash-application runtime_gateway::mcp_access` passes.
- `cargo test -p agentdash-application runtime_gateway` passes.
- Forbidden imports do not appear in runtime_gateway production code.
