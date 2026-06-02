# Scoped Lifecycle Artifacts Implement Plan

## Checklist

- [ ] Introduce `ActivityPortArtifactRef` and path helpers.
- [ ] Replace journey `write_port_output/read_port_output/list_port_outputs` with scoped variants.
- [ ] Update lifecycle VFS write/read paths to use mount graph instance and current attempt.
- [ ] Update `load_port_output_map` callers to scoped APIs.
- [ ] Update completion policy and `complete_lifecycle_node` output loading.
- [ ] Update hook runtime `port_output_gate`.
- [ ] Update artifact binding / downstream input materialization.
- [ ] Add migration for inline lifecycle artifact paths.
- [ ] Update read model aggregation.

## Validation Commands

- [ ] `cargo test -p agentdash-application workflow::lifecycle`
- [ ] `cargo test -p agentdash-application workflow::orchestrator`
- [ ] `cargo test -p agentdash-application hooks`
- [ ] `cargo test -p agentdash-application vfs::provider_lifecycle`

## Risk Points

- Output path changes touch completion, hooks, VFS, and read model together; partial migration can create split facts.
- Alias policy must be explicit when multiple attempts have the same port.
