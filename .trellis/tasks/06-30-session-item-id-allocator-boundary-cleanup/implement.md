# Session item id allocator 边界清理实现计划

## Checklist

- [ ] Load relevant specs from `implement.jsonl` and inspect current call sites for `ReadableIdRegistry`, `ToolResultRefContext`, `readable_ref`, `lifecycle_path`, `terminal_ref`, and stream mapper `tool_result_item_id`.
- [ ] Add focused characterization tests around current allocator behavior:
  - restored history advances turn/tool/cmd counters;
  - hot runtime keeps allocating from the same session allocator;
  - `ThreadItem.id` equals the item id embedded in `lifecycle_path`;
  - lifecycle cache key uses the same item id.
- [ ] Introduce a session item identity module with:
  - allocator state;
  - alias formatting/parsing;
  - tool/cmd/terminal allocation;
  - restored transcript observation;
  - typed watermark/restored state where useful.
- [ ] Introduce or revise the AgentLoop-facing abstraction so tool result bounding receives a `ToolResultRef` from an injected provider instead of owning readable id state.
- [ ] Move `ReadableIdRegistry` behavior out of `agent_loop.rs`; update exports and imports.
- [ ] Move Pi connector restored-state hydration out of connector glue into the identity module.
- [ ] Update PiAgent runtime state to hold the session identity allocator/provider and refresh only per-turn context values on prompt.
- [ ] Update stream mapper and connector tests to use the new module names and typed restore entry points.
- [ ] Update backend session and cross-layer specs with the final ownership model.
- [ ] Run validation commands and inspect diff for accidental unrelated churn.

## Validation Commands

```bash
cargo fmt
cargo test -p agentdash-agent readable_id_registry_reserves_history_item_ids_after_restore
cargo test -p agentdash-executor restored_state_hydrates_readable_id_registry_counters
cargo test -p agentdash-executor prompt_hydrates_readable_id_registry_from_restored_messages
cargo test -p agentdash-executor prompt_restores_repository_messages_before_new_user_prompt
cargo test -p agentdash-executor tool_result
git diff --check
python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-session-item-id-allocator-boundary-cleanup
```

Rename the focused test filters after the allocator type is renamed. Keep at least one test at the registry/allocator unit level and one test at the full Pi connector prompt level.

## Risky Files

- `crates/agentdash-agent/src/agent_loop.rs`
- `crates/agentdash-agent/src/agent_loop/tool_call.rs`
- `crates/agentdash-agent/src/agent_loop/tool_result.rs`
- `crates/agentdash-agent/src/lib.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs`
- `.trellis/spec/backend/session/pi-agent-streaming.md`
- `.trellis/spec/cross-layer/backbone-protocol.md`

## Review Gates

- Before moving code, confirm whether terminal aliases need to be restored from persisted facts in the same allocator pass.
- After moving code, inspect public exports from `agentdash-agent` to ensure session projection names are not leaking back through `lib.rs`.
- Before marking implementation complete, verify the UI-facing behavior is unchanged: existing historical tool cards remain stable after reconnect, and new cards append with fresh ids.

## Rollback Points

- If the new module compiles but integration fails, keep the module and revert only the call-site migration.
- If the provider trait causes awkward ownership or lifetime issues, keep the allocator module and inject `Arc<SessionItemIdentity>` directly before reattempting trait extraction.
- If stream mapper behavior changes unexpectedly, revert stream mapper edits first and compare emitted `BackboneEnvelope` item ids against the characterization tests.
