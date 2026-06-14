# 实现计划

## Start Gate

当前任务处于 `planning`。进入实现前需要：

- 用户认可 PRD/design/implement 的修复范围。
- 执行 `python ./.trellis/scripts/task.py start 06-13-agentrun-mailbox-acceptance-hardening`。
- 实现前加载 `trellis-before-dev` 并读取相关 backend/frontend/cross-layer spec。

## Implementation Slices

### 1. Contract And API Command Idempotency

Goal: 修复 mailbox control command 的 P0 幂等边界。

Files:

- `crates/agentdash-contracts/src/workflow.rs`
- `crates/agentdash-contracts/src/generate_ts.rs` if generation list changes
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `packages/app-web/src/generated/workflow-contracts.ts`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- `packages/app-web/src/services/lifecycle.ts`
- `packages/app-web/src/services/lifecycle.test.ts`

Steps:

- 增加控制命令 request DTO，包含 `command` 和 `client_command_id`。
- delete/promote/resume/cancel route 使用 request-level `client_command_id` claim receipt，不再使用 `body.command.command_id`。
- 前端控制命令点击时生成新的 UUID，并传入 request。
- 增加测试：同类操作不同 message 使用不同 client id，不再 digest conflict；duplicate same id replay。
- 视实现成本决定 cancel 是否返回 command receipt response；若不改 response，至少让 cancel request 有独立 client id 并更新 contract/spec 说明。

Validation:

```powershell
pnpm run contracts:check
cargo check -p agentdash-api
pnpm --filter app-web typecheck
pnpm --filter app-web test -- lifecycle AgentRunWorkspacePage
```

### 2. Claim Recovery Hardening

Goal: 避免 expired `Consuming` 自动重复 launch/steer。

Files:

- `crates/agentdash-domain/src/workflow/mailbox.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs`
- `crates/agentdash-infrastructure/migrations/*` if schema extension is needed
- `crates/agentdash-application/src/workflow/agent_run_mailbox.rs`
- `crates/agentdash-application/src/test_support/workflow_repositories.rs`

Steps:

- 检查现有 repository trait 是否能表达 `delivery_result_unknown` recovery。
- 修改 `recover_expired_consuming`：不要无条件 `Queued`；有 accepted refs/result 时恢复终态，无证据时转 `Blocked` 或 `Failed`。
- 如需 schema 字段，新增 migration 并同步 repository row mapping。
- 确保 `claim_token` completion 行为仍严格。
- 增加 repository/application 测试覆盖 expired consuming recovery 和不重复 delivery。

Validation:

```powershell
cargo test -p agentdash-infrastructure agent_run_mailbox
cargo test -p agentdash-application mailbox
```

### 3. Hook Boundary And HookAutoResume Replay

Goal: 修复 follow_up 边界与 HookAutoResume failure replay。

Files:

- `crates/agentdash-application/src/session/mailbox_delegate.rs`
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs`
- `crates/agentdash-application/src/session/terminal_effects.rs`
- `crates/agentdash-agent/src/agent_loop.rs` only if delegate event timing requires change
- `crates/agentdash-application/src/session/hub/tests.rs`
- `crates/agentdash-application/src/session/terminal_effects.rs` tests

Steps:

- AfterTurn 只把普通 `steering` 作为 `HookAfterTurn + AgentLoopTurnBoundary`。
- `follow_up` 改为 stop-boundary continuation：写入 `HookBeforeStop + AgentRunTurnBoundary + ContinueOnStop`，或延迟到 BeforeStop decision 中统一归一。
- dedup key 加入 `runtime_session_id`、turn/event facts 和 index。
- 修改 HookAutoResume route result，让 mailbox enqueue failure 传回 terminal effect executor 并保留 retry。
- 增加 hook tests：AfterTurn steering、follow_up stop-boundary、dedup 不误折叠、HookAutoResume failure not succeeded。

Validation:

```powershell
cargo test -p agentdash-application hook_auto_resume
cargo test -p agentdash-application mailbox
```

### 4. Frontend Mailbox Projection

Goal: 让 UI 真实展示后端 mailbox 状态并移除无行为控件。

Files:

- `packages/app-web/src/features/session/ui/composer/MailboxMessageRow.tsx`
- `packages/app-web/src/features/session/ui/composer/MailboxMessageRow.test.tsx`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`

Steps:

- row 显示 `status`、`barrier/delivery`、`last_error`。
- pause banner 使用根 mailbox state message；conversation mailbox 继续提供 command availability。
- 删除编辑菜单项和拖拽手柄。
- 增加 status/barrier/delivery 渲染测试。

Validation:

```powershell
pnpm --filter app-web typecheck
pnpm --filter app-web test -- MailboxMessageRow AgentRunWorkspacePage
```

### 5. Final Verification

Run cut-line and quality checks:

```powershell
rg -n "PendingQueueService|pending_queue\\.(enqueue|dequeue_front|requeue_front|take|pause|resume|delete|list)" crates packages
rg -n "classify_composer_submit_kind|ConversationCommandKind::SendNext|ConversationCommandKind::Enqueue|ConversationCommandKind::Steer" crates/agentdash-api crates/agentdash-application packages/app-web/src
rg -n "accepted_receipt\\(" crates/agentdash-api crates/agentdash-application
rg -n "pending-messages|PendingMessageView|PendingQueueStateView|PendingMessageRow|resume_pending_queue|promote_pending" crates packages
cargo check -p agentdash-api
cargo test -p agentdash-application mailbox
cargo test -p agentdash-application hook_auto_resume
cargo test -p agentdash-infrastructure agent_run_mailbox
pnpm run contracts:check
pnpm --filter app-web typecheck
pnpm --filter app-web test -- lifecycle MailboxMessageRow AgentRunWorkspacePage
```

## Risk Points

- Contract DTO rename will require generated TS sync and service payload updates in the same commit.
- Claim recovery cannot fully infer external side effects unless the scheduler records enough pre-delivery evidence; default should favor visible blocked state over automatic duplicate side effect.
- Hook `follow_up` timing must preserve existing non-AgentRun runtime behavior; only AgentRun mailbox delegate path should change delivery routing.
- HookAutoResume failure propagation must not break unanchored fallback.
- Frontend tests should assert visible states without coupling to broad page layout.

## Review Gate Before Start

Recommended decision: start implementation with all four slices in this task. The scope is narrow enough to finish together, and splitting would leave mailbox in a partially hardened state.
