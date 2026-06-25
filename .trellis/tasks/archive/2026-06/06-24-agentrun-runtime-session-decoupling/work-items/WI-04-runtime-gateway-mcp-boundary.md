# WI-04 RuntimeGateway And MCP Boundary Hardening

Status: done

Assigned Worker: Codex

## Tracking

- Files changed:
  - `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs`
  - `crates/agentdash-application-ports/src/lib.rs`
  - `crates/agentdash-application/src/runtime_gateway/mcp_access.rs`
  - `crates/agentdash-application/src/agent_run/runtime_surface.rs`
- Tests run:
  - `cargo check -p agentdash-application-ports` passed.
  - `cargo test -p agentdash-application runtime_gateway::mcp_access` passed in final integration.
  - `cargo test -p agentdash-application runtime_gateway` passed in final integration.
  - `rg -n "SessionHub|AgentFrame|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref|crate::agent_run::|agentdash_application::agent_run::" crates/agentdash-application/src/runtime_gateway` returned no matches.
- Blockers:
  - 无。
- Handoff summary:
  - RuntimeGateway MCP access now consumes the `agentdash-application-ports` Gateway MCP surface query contract plus MCP discovery only.
  - AgentRun current runtime surface remains the implementation source and maps its closed surface DTO into the ports crate Gateway MCP DTO.
  - `runtime_gateway::mcp_access` includes a production-code static guard for SessionHub, AgentFrame, AgentFrameSurfaceExt, and current frame resolver references.

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
