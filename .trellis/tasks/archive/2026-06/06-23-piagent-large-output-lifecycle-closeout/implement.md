# PiAgent 大输出 lifecycle 链路收口实施计划

## Implementation Checklist

- [ ] Add stable tool result item id helper, using `{turn_id}:{tool_call_id}`.
- [ ] Extend `AgentLoopConfig` / `Agent` runtime config with current turn tool-result ref context.
- [ ] Change `bound_tool_result_for_call` and update-result bounding to pass `session_id`, stable `item_id`, and real cache writer data.
- [ ] Wire `PiAgentConnector::prompt` to refresh the ref context every turn before `agent.prompt(...)`.
- [ ] Add a shared `Arc<SessionToolResultCache>` at runtime bootstrap level.
- [ ] Pass the shared cache to PiAgent connector and lifecycle VFS provider.
- [ ] Update `MountProviderRegistryBuilder::with_builtins` and lifecycle provider construction to require or explicitly receive the shared cache.
- [ ] Change stream mapper tool item ids to the same stable helper; keep non-tool synthetic chunk ids unchanged.
- [ ] Update lifecycle VFS tests so available body reads prove the same cache instance is used.
- [ ] Update connector / stream mapper tests to assert lifecycle path id equals ThreadItem id.
- [ ] Add or adjust an integration-style sentinel test covering: tool result -> bounded event -> cache body -> lifecycle `result.txt`.
- [ ] Verify projection / continuation / repository rehydrate tests still prove sentinel does not re-enter model context.
- [ ] Update specs only if final implementation changes the documented stable id shape or bootstrap ownership.

## Validation Commands

Use focused validation first:

```powershell
cargo test -p agentdash-agent tool_result
cargo test -p agentdash-agent runtime_alignment
cargo test -p agentdash-executor pi_agent
cargo test -p agentdash-application lifecycle
cargo test -p agentdash-application projected_transcript
pnpm run frontend:check
pnpm run frontend:lint
pnpm run contracts:check
```

Before final handoff, run the broader affected Rust check if time allows:

```powershell
cargo check -p agentdash-agent -p agentdash-executor -p agentdash-application -p agentdash-local -p agentdash-api
```

## Risk Areas

- `Agent` hot reuse: current turn ref context must not retain a previous `turn_id`.
- Parallel tool execution: cache writer must be thread-safe and not rely on mutable per-call state.
- Stream mapper id change: tool start/update/end must agree on the same id or frontend upsert will split cards.
- Lifecycle VFS bootstrap: API and local runtime must not construct distinct caches for connector and provider.
- Tests that hand-build bounded lifecycle paths must derive them from the same helper or explicitly assert the contract.

## Sub-Agent Context Manifest Candidates

If implementation is delegated, provide these files to implement/check agents:

- `.trellis/tasks/06-23-piagent-large-output-lifecycle-closeout/prd.md`
- `.trellis/tasks/06-23-piagent-large-output-lifecycle-closeout/design.md`
- `.trellis/spec/backend/session/pi-agent-streaming.md`
- `.trellis/spec/backend/session/context-compaction-projection.md`
- `.trellis/spec/cross-layer/backbone-protocol.md`
- `crates/agentdash-agent/src/agent_loop.rs`
- `crates/agentdash-agent/src/agent_loop/tool_call.rs`
- `crates/agentdash-agent/src/agent_loop/tool_result.rs`
- `crates/agentdash-agent/src/agent.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-application/src/session/tool_result_cache.rs`
- `crates/agentdash-application/src/vfs/provider.rs`
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
- `crates/agentdash-api/src/bootstrap/session.rs`
- `crates/agentdash-api/src/bootstrap/vfs.rs`
