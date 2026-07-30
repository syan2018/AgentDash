# Physical Recovery Manifest

Reference: `D:\ABCTools_Dev\AgentDash-main-reference` at `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`.

执行策略是先恢复有价值的完整实现文件，再在当前文件上删除旧依赖、适配新 seam，最后才接入 production module/bootstrap。不得一边凭记忆重写、一边零散复制。

## A. First-pass Physical Restore

以下文件先复制回当前对应路径，暂不接 production module：

### Domain

- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs`

### AgentRun application module

- `crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs`
- `mailbox/commands.rs`
- `mailbox/controls.rs`
- `mailbox/delivery.rs`
- `mailbox/payload.rs`
- `mailbox/policy.rs`
- `mailbox/receipts.rs`
- `mailbox/scheduler.rs`
- `mailbox/target.rs`
- `mailbox/tests.rs`

`commands/controls/payload/policy/receipts` 以保留实现为主；`mod/delivery/scheduler/target/tests` 先完整取回，再在原文件上按 Complete Agent seam重写。

### Persistence

- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs`

保留 SQL state machine、排序、claim、pause/resume、settlement和recovery实现结构；删除 RuntimeSession foreign key/backend selection，并增加 owner dispatcher lease。

### Contracts And API

- `crates/agentdash-contracts/src/agent/run_mailbox.rs`
- `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs`

先恢复 Rust source of truth，再按当前 Product view/update结构适配。生成 TypeScript不得从旧 worktree复制。

### Frontend

- `packages/app-web/src/services/agentRunMailbox.ts`
- `packages/app-web/src/services/agentRunMailbox.test.ts`
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx`
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.test.tsx`
- `packages/app-web/src/features/agent-run-workspace/ui/mailboxContent.ts`

恢复 UI/interaction行为和测试主体；类型导入与 Session state接线随后适配当前 generated contract。

### Wait activity

- `crates/agentdash-application/src/wait_activity/sources/mailbox.rs`

恢复 Mailbox activity判断后适配当前 wait source registry。

## B. Reference-only Files

这些文件只读对照，禁止原样复制进当前 production：

- `mailbox_runtime_adapter.rs`：依赖 RuntimeSession、SessionCore和旧 delegate composition。
- `message_delivery.rs`：依赖旧 LaunchCommand/RuntimeSession launch。
- `control_effects.rs`：terminal control/effect结构有参考价值，但 owner/outbox接线属于旧 RuntimeSession体系。
- 旧 migrations `0013/0032/0035/0043/0047/0053`：只提取字段和约束语义；当前实现新增一份正式 migration。
- `agent_run_command_receipt_repository.rs`：只提取 intake/control receipt语义；不恢复旧 receipt schema。
- `packages/app-web/src/generated/agent-run-mailbox-contracts.ts`：必须由当前 Rust contract重新生成。

## C. Later Wiring Files

以下现有文件包含大量当前架构逻辑，只做定向 patch，不从旧 worktree整体覆盖：

- AgentRun `mod.rs`、Product command facade、Product input delivery、launch/conversation/workspace projection。
- API `routes.rs`、bootstrap repositories/session。
- infrastructure `lib.rs`、Postgres `mod.rs`、migration readiness。
- Channel service、Companion tools、Routine、Workflow、Canvas producer。
- SessionChatView、SessionEntry、SessionStatusBar、ComposerSendButton、workspace command state。
- Complete Agent contracts、Host callback、Native/Codex/Remote adapters。
- Dash source repository mutation paths。

## D. Restore Gate

物理恢复阶段完成时必须满足：

- 上述 A 类文件全部存在且可与 reference做逐文件 diff。
- 没有 production module/bootstrap引用这些恢复文件。
- 没有复制旧 migration、generated TS或 RuntimeSession adapter。
- 每个恢复文件标明保留、删除和待适配依赖。

通过该 gate后才开始 domain/schema适配和 production wiring。
