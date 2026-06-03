# Scoped Lifecycle Artifacts Implement Plan

## Dispatch Scope

This task is ready for implementation. It should edit only scoped lifecycle artifact paths and their direct consumers. Do not reopen archived anchor, envelope, or frontend runtime-query tasks.

## Checklist

- [ ] Add typed `ActivityPortArtifactRef` and path/repository helpers.
- [ ] Replace `JourneyService::write_port_output` / `list_port_outputs` with scoped variants.
- [ ] Update lifecycle VFS read/write paths to resolve graph instance + current activity attempt.
- [ ] Replace `activity_outputs_from_port_map` and `load_port_output_map` usage in completion/orchestrator paths.
- [ ] Update hook runtime input for `port_output_gate` to use scoped outputs.
- [ ] Implement artifact binding from scoped output to scoped input, with explicit latest/history policy.
- [ ] Update migration / baseline schema if storage moves out of inline files.
- [ ] Regenerate contracts if public output/input artifact types change.
- [ ] Add focused tests for same port key across graph instances and attempts.

## Validation Commands

- [ ] `cargo test -p agentdash-application workflow::orchestrator`
- [ ] `cargo test -p agentdash-application workflow::lifecycle`
- [ ] `cargo test -p agentdash-application hooks`
- [ ] `cargo test -p agentdash-application vfs::provider_lifecycle`
- [ ] `pnpm run contracts:check` when contracts change.

## Review Gate

- [ ] `rg "list_port_outputs|write_port_output|load_port_output_map|activity_outputs_from_port_map" crates/agentdash-application crates/agentdash-domain crates/agentdash-infrastructure` shows no runtime fact-source usage.
- [ ] Any remaining run-level artifact aggregation is clearly read-model only.

## Risk Points

- Partial migration can split completion, hook gate and VFS into different fact sources.
- Alias policy for retry attempts must be explicit; implicit latest lookup will hide attempt history.
