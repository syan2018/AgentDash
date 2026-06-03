# Lifecycle 控制面长链路收敛与 Frame 化 Implement Plan

## Remaining Dispatch Order

1. `06-02-scoped-lifecycle-artifacts`
   - Owns output port / completion / hook gate / artifact binding fact source.
   - Gate: no runtime path uses run-level `port_outputs/{port_key}` as fact source.
2. `06-02-lifecycle-run-active-projection-structure`
   - Owns `active_node_keys`退场 and structured `ActiveActivityRef` exposure.
   - Gate: no business path depends on `graph_instance_id:activity_key` strings.
3. Parent final integration
   - Verify sibling results together with archived anchor/envelope/frontend-query tasks.
   - Update specs only with target invariants and reasons.

## Parent Integration Checklist

- [ ] Confirm archived children remain valid:
  - runtime session anchor
  - frame launch envelope
  - frontend session runtime frame query
- [ ] Confirm scoped artifact child removed run-level output fact source.
- [ ] Confirm active projection child removed string active fact source.
- [ ] Run residual scans:
  - `rg "list_port_outputs|write_port_output|load_port_output_map|activity_outputs_from_port_map" crates`
  - `rg "active_node_keys|current_activity_key" crates packages .trellis/spec`
  - `rg "RuntimeLaunchRequest" crates/agentdash-application/src/workflow crates/agentdash-application/src/session`
- [ ] Run focused validation from both remaining children.
- [ ] Archive parent when both children and parent integration pass.

## Validation Commands

- [ ] `cargo test -p agentdash-application workflow::orchestrator`
- [ ] `cargo test -p agentdash-application workflow::lifecycle`
- [ ] `cargo test -p agentdash-application hooks`
- [ ] `cargo test -p agentdash-application vfs::provider_lifecycle`
- [ ] `cargo test -p agentdash-domain workflow`
- [ ] `pnpm run contracts:check`
- [ ] `pnpm --filter app-web test`

## Notes For Dispatcher

The parent should not be the implementation target unless the assignee is doing the final integration scan. Dispatch implementation to the two remaining child tasks with disjoint ownership.
