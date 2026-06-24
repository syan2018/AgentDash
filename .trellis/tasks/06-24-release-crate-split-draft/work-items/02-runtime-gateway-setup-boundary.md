# Work Item 02: RuntimeGateway Setup Boundary

## Objective

让 RuntimeGateway setup actions 通过 `runtime_gateway_setup` backing ports 调用 MCP probe / workspace detect / browse / discover 能力，为 `agentdash-application-runtime-gateway` 抽取清除 implementation imports。

## Owns

- `crates/agentdash-application/src/runtime_gateway/**`
- setup port adapter wiring in API/local composition root
- RuntimeGateway setup tests

## Implementation Strategy

1. Replace direct setup helper calls with injected port traits.
2. Keep RuntimeGateway provider input/output DTO unchanged for route consumers.
3. Wire existing `mcp_preset` and `workspace` helper implementations in composition root.
4. Preserve `surface_for_actor` and `invoke` semantics.
5. Use static grep to verify RuntimeGateway setup dependencies are expressed through backing ports.

## Completion Gates

```powershell
cargo test -p agentdash-application runtime_gateway::session_actions
cargo test -p agentdash-application runtime_gateway::mcp_access
cargo test -p agentdash-application runtime_gateway::extension_actions
rg -n "use crate::(mcp_preset|workspace)::" crates/agentdash-application/src/runtime_gateway -g '*.rs'
```

## Handoff

Report provider constructor changes, composition root wiring changes, and any setup action behavior still coupled to application implementation.
