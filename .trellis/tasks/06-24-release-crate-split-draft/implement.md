# Crate Split Draft Checklist

## Draft Status

- [x] Capture candidate crate graph.
- [x] Capture dependency direction.
- [x] Capture extraction waves.
- [x] Capture blocking conditions.
- [ ] Revisit after `06-24-agentrun-runtime-session-decoupling` completes facade/import cleanup.

## Future Wave Checklist

### Wave 1: Ports Expansion

- [ ] Add AgentRun current/resource surface ports.
- [ ] Add RuntimeSession delivery/adoption ports.
- [ ] Add VFS runtime projection ports.
- [ ] Add gateway-facing MCP/current surface contracts.
- [ ] Keep implementations in existing crates.

### Wave 2: RuntimeGateway / RuntimeSession

- [ ] Extract RuntimeGateway once it only consumes ports.
- [ ] Extract RuntimeSession substrate once it no longer owns AgentRun/Lifecycle facts.
- [ ] Keep API/local as composition roots.

### Wave 3: AgentRun / Lifecycle

- [ ] Extract AgentRun after RuntimeSession and Lifecycle links are port-mediated.
- [ ] Extract Lifecycle after AgentRun materialization and RuntimeSession creation use ports.
- [ ] Defer VFS physical split until generic VFS provider and AgentRun resource surface ownership are clean.

## Validation Commands For Future Work

```powershell
cargo metadata --no-deps --format-version 1
cargo check --workspace
rg -n "crate::session::|agentdash_application::session::" crates/agentdash-application/src/agent_run crates/agentdash-application/src/lifecycle crates/agentdash-api/src
```

## Non-Goals

- No code movement now.
- No Cargo workspace edits now.
- No DB migration.
