# LifecycleRun Active Projection Structure Implement Plan

## Dispatch Scope

This task is ready for implementation after or alongside scoped lifecycle artifacts. It owns active Activity projection cleanup only. Do not reopen archived runtime anchor, frame envelope, frontend runtime query, graphless runtime, or session-agent channel tasks.

## Checklist

- [ ] Move `ActiveActivityRef` into public contract shape with `run_id + graph_instance_id + activity_key + attempt + status`.
- [ ] Update `LifecycleRunView` builder to derive refs from `WorkflowGraphInstance.activity_state`.
- [ ] Remove business reliance on `LifecycleRun.active_node_keys`.
- [ ] Update `advance_node` output to structured active refs.
- [ ] Remove or rename `lifecycle_runs.active_node_keys` in migration / repository if it is no longer needed.
- [ ] Remove `current_activity_key()` as business helper, or constrain it to tests/debug with clear naming.
- [ ] Regenerate generated TS and update frontend consumers.
- [ ] Update backend workflow specs to state that active projection is derived from graph instance state.
- [ ] Add multi graph instance same activity key tests.

## Validation Commands

- [ ] `cargo test -p agentdash-domain workflow`
- [ ] `cargo test -p agentdash-application workflow::lifecycle_run_view_builder`
- [ ] `cargo test -p agentdash-application workflow::tools`
- [ ] `pnpm run contracts:check`
- [ ] `pnpm --filter app-web test`

## Review Gate

- [ ] `rg "active_node_keys|current_activity_key" crates packages .trellis/spec` shows no production business fact-source usage.
- [ ] Any remaining `active_node_keys` reference is either removed, test-only, or explicitly debug/cache-only.

## Risk Points

- Removing the column before read builder derivation is complete can make active UI blank.
- Keeping both persisted strings and derived refs without a single owner will recreate the split fact source this task is meant to remove.
