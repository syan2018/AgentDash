# Research: Phase 3/4 cleanup verification

- Query: 独立验收 AgentRun workspace Phase 3/4 迁移后 API 旧路径清理边界、`sessions.rs` 复用安全性，以及最终可用的 `rg` 验收命令。
- Scope: internal
- Date: 2026-06-14

## Final State

Phase 3/4 的目标边界已经落地：

- Workspace read assembly 已迁入 application 层 `AgentRunWorkspaceQueryService`。API route 只负责 path/body/current user/permission/context 装配，并把 application read model 映射到 contract DTO。
- Command availability、stale guard、replacement command、typed conflict 已迁入 application 层 `AgentRunWorkspaceCommandPolicyService`。API route 只负责提交 endpoint intent 与前端 command precondition，并把 application conflict 映射成 `ApiErrorWithCode`。
- `MailboxMessageView` / `MailboxStateView` 的 contract mapper 已从 `lifecycle_agents.rs` 拆到 `agent_run_mailbox_contracts.rs`，供 AgentRun workspace route 与 RuntimeSession detail route 共同复用。
- `lifecycle_agents.rs` 不再直接匹配 `SessionExecutionState` 或持有 workspace shell/control/action/status 投影规则。

## Verification Commands

### Rust checks

```powershell
cargo test -p agentdash-application workflow::agent_run_workspace
cargo test -p agentdash-api routes::lifecycle_agents
cargo check -p agentdash-api
```

Result:

- `agentdash-application workflow::agent_run_workspace`: 13 passed.
- `agentdash-api routes::lifecycle_agents`: 3 passed.
- `agentdash-api` check: passed.

### Old API helper cleanup

```powershell
rg -n "build_agent_run_workspace_view|ensure_agent_run_command_allowed|ensure_command_submission_matches_snapshot|ensure_composer_command_precondition_matches_agent_run|AgentRunCommandPrecondition|stale_command_conflict|replacement_command_for_state|conversation_state_code|workspace_delivery_status|execution_state_turn_id|execution_state_active_turn_id" crates/agentdash-api/src/routes/lifecycle_agents.rs
```

Result: no output.

### API must not own runtime projection branches

```powershell
rg -n "SessionExecutionState::(Idle|Running|Cancelling|Completed|Failed|Interrupted)|inspect_session_execution_state|supports_session_steering|conversation_snapshot_id|AgentConversationSnapshotResolver|ConversationModelConfigResolver" crates/agentdash-api/src/routes/lifecycle_agents.rs
```

Result: no output.

### Policy conflict terms

```powershell
rg -n "stale_command|command_unavailable|starting_claimed|connector_steer_unsupported|active_turn_mismatch|snapshot_id_mismatch|runtime_session_mismatch|frame_mismatch|agent_run_identity_mismatch|submitted_guard|replacement_command" crates/agentdash-api/src/routes/lifecycle_agents.rs
```

Result: only `replacement_command: conflict.replacement_command`, which is the allowed application conflict -> API error mapper field assignment.

### Application service/module existence

```powershell
rg -n "pub mod query|pub mod command_policy|AgentRunWorkspaceQueryService|AgentRunWorkspaceCommandPolicy|AgentRunWorkspaceCommandConflict" crates/agentdash-application/src/workflow/agent_run_workspace crates/agentdash-api/src/routes/lifecycle_agents.rs
```

Result: application query service, command policy service, conflict type, and API imports/calls are present.

### Shared mailbox mapper safety

```powershell
rg -n "mailbox_message_view|mailbox_message_visible|mailbox_state_view" crates/agentdash-api/src/routes/lifecycle_agents.rs crates/agentdash-api/src/routes/sessions.rs crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs
```

Result: mapper definitions live in `agent_run_mailbox_contracts.rs`; `lifecycle_agents.rs` and `sessions.rs` only import/use them.

## Residual Notes

- `sessions.rs` keeps its own RuntimeSession detail projection because it starts from runtime trace identity rather than AgentRun workspace identity. The shared mailbox mapping is now neutral API contract mapping, so it no longer depends on AgentRun workspace route internals.
- No Rust contract DTO shape changed in this phase, so generated TypeScript contract refresh was not required.
