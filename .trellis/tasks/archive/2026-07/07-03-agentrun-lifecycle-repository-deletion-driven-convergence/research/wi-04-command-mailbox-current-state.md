# Research: WI-04 Command Mailbox Current State

- Query: 基于当前仓库事实清点 `CommandReceipt` / `AgentRun mailbox` / runtime delivery operation 的 schema、repository、service、API/frontend 使用点，并给出 WI-04/WI-12 最小切片建议。
- Scope: internal
- Date: 2026-07-04

## Findings

1. 当前代码已经有三类事实的雏形，但物理边界没有拆清：`AgentRunCommandReceipt` 独立负责幂等回执；`agent_run_mailbox_messages` 同时承载 queue item 和 delivery attempt；`session_runtime_commands` 是 RuntimeSession frame transition outbox，不是 mailbox item 的投递尝试。
2. `agent_run_mailbox_messages` / `agent_run_mailbox_states` 仍由 `runtime_session_id text NOT NULL` 和 `sessions(id) ON DELETE CASCADE` 绑定，直接违反 D-005 的 AgentRun-owned queue 目标。
3. receipt 的 `result_json` 当前既缓存 mailbox outcome，又在 fork duplicate replay 中缓存 child refs / lineage；它已经超过“外部指令幂等 + accepted refs / result ref”的边界。
4. submit/promote/delete/resume/cancel 走 `client_command_id + command precondition/stale_guard + receipt`，move/reorder 只走位置参数并直接重排 mailbox row，没有 receipt、没有 stale guard、没有 command availability 校验。
5. delivery attempt 的事实目前散在 mailbox row：`claim_token`、`claimed_at`、`claim_expires_at`、`attempt_count`、`accepted_agent_run_turn_id`、`accepted_protocol_turn_id`、`consumed_at`、`last_error` 和 `Consuming/Dispatched/Steered/Failed` 状态。
6. 实现最小切片不应先单独改 schema：当前 domain/repository/service 签名大量要求非空 `runtime_session_id`。应先收敛 application port/command surface 的 owner 语义，再把 schema + repository mapping + scheduler claim/delivery attempt 原子推进。

## Files Found

- `crates/agentdash-domain/src/workflow/command_receipt.rs` - `AgentRunCommandReceipt` domain model、command kind、repo port。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_command_receipt_repository.rs` - PostgreSQL receipt repository；claim/idempotency/accepted/result_json 实现。
- `crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql` - receipt 初始表和 FK；runtime ref 是 nullable `ON DELETE SET NULL`。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql` - mailbox 表、state 表、receipt rename、mailbox FK/cascade/index。
- `crates/agentdash-infrastructure/migrations/0032_agent_run_mailbox_source_identity.sql` - mailbox source identity / dedup schema。
- `crates/agentdash-infrastructure/migrations/0035_agent_run_mailbox_backend_selection.sql` - mailbox launch planning / backend selection state。
- `crates/agentdash-infrastructure/migrations/0039_agent_run_command_receipt_fork_kinds.sql` - receipt command_kind check 扩展 fork/fork_submit。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs` - mailbox message/state/status/repo port。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs` - mailbox PostgreSQL repository；claim/recover/order/pause/resume/payload cleanup。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs` - composer/intake/hook message 写入 mailbox 与 receipt claim。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs` - claim mailbox、调用 runtime delivery/steer/resume、mark receipt。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs` - delete/promote/resume receipt 控制，以及 move 直接重排。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs` - mailbox receipt outcome/result_json/duplicate replay。
- `crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs` - cancel 作为 AgentRun runtime command 的 receipt/idempotency 实现。
- `crates/agentdash-spi/src/session_persistence.rs` - `SessionRuntimeCommandStore` / `RuntimeDeliveryCommand` 定义。
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` - `session_runtime_commands` repository 实现。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - `session_runtime_commands` DDL、索引、FK。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - AgentRun mailbox/runtime API 路由和 stale guard 校验。
- `crates/agentdash-contracts/src/agent/run_mailbox.rs` - AgentRun mailbox/receipt/move/frontend DTO。
- `packages/app-web/src/services/agentRunMailbox.ts` - frontend AgentRun mailbox service wrappers。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts` - frontend command precondition/client_command_id 构造与 move 例外。
- `packages/app-web/src/services/agentRunRuntime.ts` / `packages/app-web/src/services/executor.ts` - frontend tool approval 走 AgentRun-scoped runtime endpoint。

## Current State By Fact Layer

### 1. User Instruction / CommandReceipt

`AgentRunCommandReceipt` domain 当前是独立模型：status 只有 `Pending/Accepted/TerminalFailed`，适合作为外部指令幂等边界，而不是 queue/delivery 状态机。证据：`crates/agentdash-domain/src/workflow/command_receipt.rs:8`、`crates/agentdash-domain/src/workflow/command_receipt.rs:98`。

字段归属：

- receipt identity/idempotency：`id`、`scope_kind`、`scope_key`、`command_kind`、`client_command_id`、`request_digest`，见 `crates/agentdash-domain/src/workflow/command_receipt.rs:98`。
- receipt status：`status`、`error_message`、`created_at/updated_at`，见 `crates/agentdash-domain/src/workflow/command_receipt.rs:98`。
- outcome refs：`accepted_refs` 包含 run/agent/frame/runtime/turn refs，见 `crates/agentdash-domain/src/workflow/command_receipt.rs:87`。
- correlation/result cache：`mailbox_message_id`、`result_json`，见 `crates/agentdash-domain/src/workflow/command_receipt.rs:98`。

Repository port 方法覆盖幂等 claim、accepted refs、mailbox correlation、result_json、terminal_failed、get：`crates/agentdash-domain/src/workflow/command_receipt.rs:144-174`。PostgreSQL 实现用 `(scope_kind, scope_key, client_command_id)` 查重；digest 相同返回 duplicate，digest 不同返回 conflict：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_command_receipt_repository.rs:28-69`。新 receipt 先插入 pending：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_command_receipt_repository.rs:90-104`。accepted refs 写回 receipt 列：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_command_receipt_repository.rs:139-168`。`result_json` 通过 `store_result_json` 写入：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_command_receipt_repository.rs:193-212`。

Schema 现状：

- `0011` 初始 receipt 表含 `client_command_id`、`request_digest`、`status`、`runtime_session_id` 等列，`runtime_session_id` 是 nullable，FK 到 `sessions(id) ON DELETE SET NULL`：`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:5`、`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:12`、`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:54`。
- `0013` rename 为 `agent_run_command_receipts`，并增加 `mailbox_message_id`、`protocol_turn_id`、`result_json`：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:7`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:28`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:34`。
- command_kind check 当前由 `0013` 和 `0039` 维护，已有 message/project/mailbox/cancel/fork/fork_submit，但没有 move/reorder/tool approval：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:43`、`crates/agentdash-infrastructure/migrations/0039_agent_run_command_receipt_fork_kinds.sql:5`；domain enum 也只列出 `MailboxPromote/MailboxDelete/MailboxResume/Cancel` 等，不含 move/reorder：`crates/agentdash-domain/src/workflow/command_receipt.rs:40`。

`result_json` 当前使用点：

- mailbox receipt 将 `{ outcome, mailbox_message_id }` 写入 `result_json`，用于 duplicate replay 时在 mailbox row 缺失或 terminal 后返回稳定 outcome：`crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs:94-100`、`crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs:135-151`、`crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs:213`。
- fork duplicate replay 依赖 `result_json` 解析 parent/child refs、lineage 和可选 mailbox outcome：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:409-430`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:478-499`。这更像 product fork materialization cache，不是 receipt 最小结果 ref。

Stale guard 使用：

- backend command policy 重新解析当前 availability，并比对 command kind/id、run/agent、`runtime_session_id`、active turn、snapshot：`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:361`、`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:414`、`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:428`、`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:435`。
- snapshot model 的 stale guard 显式包含 `runtime_session_id`：`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:141-147`。
- API 将 stale guard 从 contract 转回 application model：`crates/agentdash-api/src/routes/lifecycle_agents.rs:2160-2172`。

结论：receipt 层状态字段目前基本正确；错误在于 `result_json` 承载过多 canonical 结果，以及 `runtime_session_id` 被同时放进 command digest、stale guard、mailbox owner 和 scheduler claim，导致 trace ref / current delivery guard / durable queue owner 混用。

### 2. AgentRun Queue Item / Mailbox

Mailbox domain 当前已经有 queue 能力：message/state、status、claim、recover、order、dedup、payload cleanup、pause/resume。repo port 证据：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:438-520`。这满足 D-017 保留 child table 的资格。

但 owner 仍是混合状态：

- domain `AgentRunMailboxMessage` 和 `NewAgentRunMailboxMessage` 都要求 `runtime_session_id: String`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:353-357`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:391-394`。
- domain `AgentRunMailboxState` 也要求 `runtime_session_id: String`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:414-417`。
- schema `agent_run_mailbox_messages.runtime_session_id text NOT NULL`，并 FK 到 `sessions(id) ON DELETE CASCADE`：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:63`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:191-192`。
- schema `agent_run_mailbox_states.runtime_session_id text NOT NULL`，同样 FK 到 `sessions(id) ON DELETE CASCADE`：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:217`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:250-251`。
- mailbox message 的真正 durable owner 也有 `run_id` / `agent_id` FK，且 cascade 到 lifecycle run/agent：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:174`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:183`。当前因此同时有 AgentRun owner 和 RuntimeSession owner。

Repository/service 使用点仍沿 runtime 过滤：

- repository column set 把 `runtime_session_id` 当作核心列，并在 insert 时 bind 非空：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:100-132`。
- `claim_next` 以 run/agent 加可选 `runtime_session_id` 过滤，实际 scheduler 总是传 `Some(runtime_session_id)`：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:224-258`、`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:210-246`。
- `pause_state` / `resume_state` / `set_backend_selection_preference` upsert state 时写 `runtime_session_id`：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:415-452`、`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:464-497`、`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:524-544`。
- service command target 可以从 current delivery 解析 AgentRun target，但 public command structs 仍要求 runtime session id：`crates/agentdash-application-agentrun/src/agent_run/mailbox/commands.rs:36-40`、`crates/agentdash-application-agentrun/src/agent_run/mailbox/commands.rs:73-87`、`crates/agentdash-application-agentrun/src/agent_run/mailbox/commands.rs:163-170`。

Mailbox queue 字段归属：

- queue item identity/owner：`id`、`run_id`、`agent_id`、`origin`、`source_*`、`source_dedup_key`、`command_receipt_id`。
- queue policy/state：`delivery`、`delivery_json`、`barrier`、`drain_mode`、`priority`、`order_key`、`status` 中的 `Accepted/Queued/ReadyToConsume/Paused/Blocked/Deleted`。
- queue payload/projection：`payload_json`、`executor_config_json`、`launch_planning_input`、`preview`、`has_images`、`retain_payload`。
- mailbox aggregate state：`paused`、`pause_reason`、`pause_message`、`backend_selection_preference`。

Queue/delivery 混合字段：

- `Consuming/Dispatched/Steered/Failed` 更像 delivery attempt 状态，而不只是 queue item 状态；status enum 证据：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:301-310`。
- `claim_token`、`claimed_at`、`claim_expires_at` 是 worker lease/attempt 字段，不是 queue identity；column set 证据：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:100`。
- `accepted_agent_run_turn_id`、`accepted_protocol_turn_id` 是投递成功 refs，当前存在 message row：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:100`。
- `attempt_count`、`last_error` 当前暴露到 frontend mailbox view：`crates/agentdash-contracts/src/agent/run_mailbox.rs:138-144`，但 `attempt_count` 至少应由 delivery attempt 派生。

### 3. Runtime Delivery / Runtime Commands / Delivery Attempts

未发现 `DeliveryAttempt` / `RuntimeDeliveryOperation` 的专用 domain、repository 或 schema。仓库搜索只有 `RuntimeDeliveryCommand` / `RuntimeCommandRecord`：`crates/agentdash-spi/src/session_persistence.rs:396`、`crates/agentdash-spi/src/session_persistence.rs:414`。

现有 `SessionRuntimeCommandStore` 是 RuntimeSession frame transition outbox：

- status 只有 `Requested/Applied/Failed`：`crates/agentdash-spi/src/session_persistence.rs:359`。
- record 字段是 `session_id`、`frame_transition_id`、`phase_node`、`delivery`、`frame_transition`：`crates/agentdash-spi/src/session_persistence.rs:390-396`。
- delivery kind 只有 `PendingRuntimeContext`：`crates/agentdash-spi/src/session_persistence.rs:414-429`。
- store 方法是 upsert/list requested/mark applied/failed/list by status：`crates/agentdash-spi/src/session_persistence.rs:856-877`。
- schema 表 `session_runtime_commands` 以 `session_id`、`phase_node`、`status`、`payload_json`、`frame_transition_id` 为核心，并 FK 到 `agent_frame_transitions` / `sessions`：`crates/agentdash-infrastructure/migrations/0001_init.sql:629-640`、`crates/agentdash-infrastructure/migrations/0001_init.sql:1172-1176`、`crates/agentdash-infrastructure/migrations/0001_init.sql:1224-1225`。
- PostgreSQL 实现 upsert runtime delivery command 时先 fail 同 session/phase 的 requested commands，再插入 `session_runtime_commands`：`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:683-790`。

结论：`session_runtime_commands` 不应直接等同 WI-04 的 mailbox delivery attempt。它描述 frame transition/context delivery；WI-04 所需的是 queue item 投递到 current RuntimeSession 的一次尝试，当前 canonical facts 在 mailbox row 和 receipt result 中混放。

## API / Frontend Usage Points

### Submit

- API `composer-submit` 要求 `client_command_id`，解析 AgentRun context 后取 `delivery_runtime_session_id`，执行 command policy stale guard，然后调用 mailbox service `accept_user_message`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:670-707`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:753-767`。
- service digest 包含 target run/agent/frame 和 `message_stream.runtime_session_id`，然后 claim receipt `MessageSubmit`：`crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:177-214`。
- service 创建 mailbox message 时写 `runtime_session_id`，以 `command_receipt:<receipt_id>` 作为 source dedup fallback，并 attach receipt：`crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:236-274`。
- scheduler 可能立即 claim 并 deliver/steer/resume；launch 成功后 mark `Dispatched`、complete receipt、cleanup payload：`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:372-476`。
- frontend 为 submit 生成 stable in-flight `client_command_id` 并发送 command precondition：`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:274-316`。

Classification: submit = receipt + queue item + delivery attempt。当前错误是 delivery attempt 混在 mailbox row，且 runtime_session_id 同时进入 digest、message owner ref、scheduler filter。

### Promote

- API promote 要求 `AgentRunCommandOnlyRequest`，校验 command policy，然后传 `client_command_id` 和 `runtime_session_id`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:1001-1045`。
- service claim receipt kind `MailboxPromote`，更新 mailbox policy 为 steering/priority 并 schedule：`crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:91-185`。
- frontend promote 使用 `commandRequest(promoteCommand)`，即 stale_guard + new client command id：`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:379-389`。

Classification: promote = receipt + queue item policy mutation；如果 scheduler 立即消费，另有 delivery attempt。当前错误是 attempt 仍写回同一 message row。

### Delete / Recall

- API delete mailbox message 校验 command policy 后调用 service `delete_message`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:911-952`。
- service claim receipt kind `MailboxDelete`，load message、ensure owner、mark deleted、accepted receipt outcome `Deleted`：`crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:7-89`。
- frontend delete/recall 都先找 `delete_mailbox_message` command，然后发送 command request；recall 额外先取内容再 delete：`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:405-421`、`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:462-480`。

Classification: mailbox delete/recall = receipt + queue item terminal mutation；不需要 delivery attempt。

### Resume

- API resume 校验 command policy 后调用 `resume_mailbox`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:955-998`。
- service claim receipt kind `MailboxResume`，`resume_state` 后 schedule manual resume，receipt outcome 使用 scheduler 结果：`crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:187-278`。
- frontend resume 使用 mailbox projection 中的 `resume_command` 构造 command request：`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:431-451`。

Classification: resume = receipt + mailbox aggregate state transition；如果 schedule 消费消息，则另有 delivery attempt。

### Move / Reorder

- contract `AgentRunMailboxMoveRequest` 只有 `after_message_id`，没有 `client_command_id` 或 command precondition：`crates/agentdash-contracts/src/agent/run_mailbox.rs:155-158`。
- API move 只 parse `after_message_id`，直接调用 service `move_message`，返回 `{ ok, order_key }`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:1048-1080`。
- service move 只做 owner/origin/delivery/status/anchor 校验，然后调用 repo `move_message_after`；没有 receipt claim、没有 stale guard：`crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:296-347`。
- repository `move_message_after` 是 run/agent 内 order_key 重排：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:553`。
- frontend move 只发 `{ after_message_id }`，catch 后刷新，无 command id：`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:500-513`。

Classification: move/reorder = user instruction receipt + queue item ordering mutation；不应产生 delivery attempt。当前错误组合是直接 repository mutation，绕过 D-006 的 receipt/stale guard 层。

### Cancel

- cancel command struct 包含 run/agent/frame/runtime_session_id/client_command_id/reason：`crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs:17-23`。
- service digest 包含 `runtime_session_id`，claim receipt kind `Cancel`；duplicate 直接 replay，不二次调用 runtime：`crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs:56-75`。
- service 调用 `cancel_runtime_session`，成功后 mark accepted 并存 `result_json { cancelled, reason }`：`crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs:77-115`。
- API cancel 先校验 command policy，再构造 cancel command：`crates/agentdash-api/src/routes/lifecycle_agents.rs:1108-1162`。
- frontend cancel 使用 header command 的 `commandRequest(cancelCommand)`：`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:354-369`。

Classification: cancel = receipt + runtime control operation；不是 mailbox queue item。若以后要审计控制投递，可归到 runtime delivery/control attempt，而不是 mailbox message。

### Tool Approval

- decisions 明确 Q-007：tool approval 继续是 runtime connector approval，产品路径只允许 AgentRun-scoped endpoint：`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md:287`、`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/inventory.md:34`。
- API route 是 AgentRun-scoped runtime approval/reject：`crates/agentdash-api/src/routes/lifecycle_agents.rs:179-184`。
- handler 从 AgentRun context 解析 current delivery runtime session，然后调用 `session_control.approve_tool_call/reject_tool_call`；没有 receipt/mailbox：`crates/agentdash-api/src/routes/lifecycle_agents.rs:1301-1372`。
- frontend tool card 调用 AgentRun runtime service wrapper：`packages/app-web/src/features/session/ui/ToolCallCardShell.tsx:97-110`、`packages/app-web/src/services/agentRunRuntime.ts:69-87`。

Classification: tool approval = runtime connector approval / current delivery control operation；当前不归入 command receipt 或 mailbox queue item。若要有幂等回执，应作为单独 runtime control receipt，而不是复用 mailbox。

## Mismatches And Overloaded Fields

1. Mailbox owner 错误组合：message/state 同时有 `run_id + agent_id` 和 non-null `runtime_session_id`，并且 runtime FK `ON DELETE CASCADE`。这会让删除 RuntimeSession 删除 durable user intent，违反 D-005。证据：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:63`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:191-192`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:217`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:250-251`。
2. Mailbox status 混合 queue 和 attempt：`Accepted/Queued/ReadyToConsume/Paused/Blocked/Deleted` 是 queue state，`Consuming/Dispatched/Steered/Failed` 是 delivery attempt state 或 delivery projection。证据：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:301-310`。
3. Delivery lease/attempt 混在 queue row：`claim_token/claimed_at/claim_expires_at/attempt_count/accepted_*_turn_id/consumed_at/last_error` 当前都在 `MAILBOX_COLS`：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:100`。
4. Scheduler claim 以 runtime session 过滤 queue，导致 queue visibility 被 current runtime 绑定：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:243`、`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:210-246`。
5. `CommandReceipt.result_json` 既做 mailbox outcome replay，又做 fork child/lineage replay，已经从 receipt result ref 变成 product materialization cache：`crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs:94-100`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:478-499`。
6. Command availability/stale guard 没覆盖 move/reorder：command kind model 只有 submit/promote/delete/resume/cancel：`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:123-128`；move API/contract/frontend 均没有 command request：`crates/agentdash-contracts/src/agent/run_mailbox.rs:155-158`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:1048-1080`、`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:500-513`。
7. `runtime_session_id` 同时扮演四种角色：trace identity、current delivery precondition、mailbox physical owner、delivery target/attempt ref。D-006 只允许它作为 accepted/delivery/trace ref，不能作为 mailbox owner。

## D-005/D-006/D-007/D-017 Classification

- D-005: Mailbox 是 AgentRun-owned durable queue。当前 schema 与 domain 仍把 runtime session 设为 non-null owner/cascade；应把 owner 收敛到 `run_id + agent_id`，runtime session 只保留 nullable delivery/correlation/attempt ref。决策证据：`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md:82-90`；当前违例证据：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:63`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:191-192`。
- D-006: 三层事实应是 `CommandReceipt -> Mailbox queue item -> RuntimeDeliveryOperation/DeliveryAttempt`。当前 submit/promote/delete/resume/cancel 已有 receipt；move/reorder 缺 receipt；delivery attempt 缺独立事实。决策证据：`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md:94-111`。
- D-007: start/fork admission 要在原子边界形成初始 mailbox envelope / outer receipt。当前 fork receipt 使用 `result_json` 缓存 child refs/lineage；fork-submit 还会写 child mailbox。下一轮实现应避免继续把 admission materialization 作为 receipt result_json 的 canonical source。决策证据：`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md:114`；当前代码证据：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:409-430`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:478-499`。
- D-017: mailbox child table 有 claim/recover/order/dedup/payload cleanup/pause-resume 扫描需求，保留物理表有正当性；但 delivery attempt 应基于锁/claim/retry/审计需求独立建模，不能因为历史上字段在 mailbox row 就保留为同一事实。决策证据：`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md:232`、`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/inventory.md:49-50`。

## Migration Ledger Candidates

WI-12 至少需要登记这些 mailbox/receipt/runtime command 相关 schema change：

1. `agent_run_mailbox_messages.runtime_session_id`
   - 当前：`text NOT NULL` + `sessions(id) ON DELETE CASCADE` + runtime status index。
   - 候选：改为 nullable delivery ref、改名为 `delivery_runtime_session_id`，或迁入 delivery attempt 表；FK 不得 cascade 删除 mailbox durable intent，若保留 ref 则应为 `ON DELETE SET NULL` 或等价 nullable trace ref。
   - 证据：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:63`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:160-161`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:191-192`。

2. `agent_run_mailbox_states.runtime_session_id`
   - 当前：`text NOT NULL` + runtime FK cascade。
   - 候选：删除或改为 nullable trace/ref；mailbox state owner 应是 `(run_id, agent_id)`。
   - 证据：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:214-217`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:249-251`。

3. Delivery attempt table/fields
   - 当前不存在 `DeliveryAttempt` / `RuntimeDeliveryOperation` 专用 schema。
   - 候选字段：attempt id、message_id、run_id、agent_id、delivery_runtime_session_id nullable、attempt_no、status、claim_token/lease、accepted turn refs、last_error、created/updated/terminal timestamps。
   - 需要迁出或降级的 message fields：`claim_token`、`claimed_at`、`claim_expires_at`、`attempt_count`、`accepted_agent_run_turn_id`、`accepted_protocol_turn_id`、`consumed_at`、attempt-level `last_error`。
   - 证据：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:100`。

4. Queue indexes
   - 保留/调整：`run_id + agent_id + order_key`、source dedup/source identity。
   - 替换：`runtime_session_id,status,barrier,drain_mode` 应改到 attempt/delivery ref 或 nullable partial index。
   - 证据：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:153-164`、`crates/agentdash-infrastructure/migrations/0032_agent_run_mailbox_source_identity.sql:73`。

5. Receipt command kind/check
   - 若 move/reorder 进入 receipt，需要新增 domain enum 和 DB check 值，例如 `mailbox_move` 或 `mailbox_reorder`。
   - 证据：`crates/agentdash-domain/src/workflow/command_receipt.rs:40`、`crates/agentdash-infrastructure/migrations/0039_agent_run_command_receipt_fork_kinds.sql:5`。

6. Receipt/message FK direction
   - 当前 `message.command_receipt_id -> receipt ON DELETE SET NULL`，`receipt.mailbox_message_id -> message ON DELETE SET NULL`。D-006 要求 receipt -> mailbox result ref 是主方向，message -> receipt 只能 nullable correlation；这两个 FK 可保留，但 migration ledger 要明确不是双向事实源。
   - 证据：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:199-210`。

7. RuntimeSession table rename interactions
   - WI-12/WI-02 若把 `sessions` 重命名为 `runtime_sessions`，所有保留的 runtime trace refs/FKs 要同步；但 mailbox durable rows 不应因为 RuntimeSession 删除 cascade。
   - 证据：`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-12-database-migration-verification.md:25`、`.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-12-database-migration-verification.md:50-51`。

## Minimal Implementation Slices

1. Classification / contract slice
   - 给 move/reorder 补齐 command surface：contract 加 `AgentRunCommandOnlyRequest` 或等价 `client_command_id + command precondition`，conversation availability 增加 move/reorder command kind，backend command policy 覆盖 stale guard。
   - 影响文件：`crates/agentdash-contracts/src/agent/run_mailbox.rs`、`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs`、`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs`、`crates/agentdash-api/src/routes/lifecycle_agents.rs`、`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts`、generated contracts。
   - 理由：先把用户指令是否需要 receipt 的边界定住，避免 schema 改完后 API 仍能绕过 receipt 直接重排。

2. Application port owner slice
   - 把 mailbox service public command target 从 required `runtime_session_id` 转成 AgentRun target/current delivery resolution；runtime session 只作为 resolved delivery ref 进入 scheduler/attempt。
   - 影响文件：`mailbox/commands.rs`、`mailbox/target.rs`、`mailbox/delivery.rs`、`mailbox/controls.rs`、API route glue。
   - 理由：当前 Rust domain/repo/service 签名要求 non-null `runtime_session_id`，单独先改 DB 会立即造成 mapping 和 insert 不一致。

3. Schema + repository atomic slice
   - 新增 WI-12 migration：mailbox state/message 去除 runtime ownership/cascade，runtime ref nullable或迁入 attempt 表；同步 domain structs、row mapping、repository SQL、indexes。
   - 影响文件不能拆开并行：`migrations/*.sql`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs`、`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs`。
   - 验证点：schema readiness、migration guard、repository integration。

4. Delivery attempt scheduler slice
   - 将 `claim_next/recover_expired_consuming/mark_message_status` 中的 lease、attempt_count、accepted turn refs、delivery terminal status 抽到 delivery attempt operation；mailbox message 保留 queue state/projection。
   - 影响文件：mailbox domain/repo/scheduler/receipt mapping/API mailbox view。
   - 理由：这是行为风险最高的 slice，必须在 schema/repo 基础稳定后做。

5. Receipt result_json cleanup slice
   - mailbox duplicate replay 只依赖 receipt result ref + mailbox/delivery attempt result；fork/admission materialization 不继续把 canonical child refs/lineage 放在 receipt result_json。
   - 影响文件：`mailbox/receipts.rs`、`fork.rs`、admission/fork service tests。
   - 理由：降低 receipt 从 result cache 变成事实源的风险；但不要和 fork/admission 工作项并行改同一文件。

## Parallelization Advice

- 串行：`agent_run_mailbox` domain model、Postgres mailbox repository、mailbox migrations 必须同一切片推进；字段 nullability/attempt 表会同时影响 compile、SQLx bind/read、tests。
- 串行：scheduler delivery attempt split 与 repository claim/recover SQL 不能并行；两者共享 `claim_next`、`mark_message_status`、`recover_expired_consuming` 的语义。
- 串行：API route + generated contract + frontend move/reorder command request 应按 contract 生成链路串行，否则前端会短暂依赖不存在的 DTO。
- 避免并行：`crates/agentdash-api/src/routes/lifecycle_agents.rs` 是 submit/delete/promote/resume/move/cancel/tool approval 的共同入口，WI-04 与 WI-09/WI-01 同时编辑会冲突。
- 可以并行：tool approval 保持 runtime connector approval 的清理可与 mailbox attempt 表设计并行，但不要同时改 `lifecycle_agents.rs`。
- 可以并行：`SessionRuntimeCommandStore` 的 runtime session table rename/WI-02 可与 WI-04 设计并行；实现时 FK 名称和 `sessions` -> `runtime_sessions` rename 需要 WI-12 汇总。
- 可以并行：receipt result_json/fork cleanup 的研究和测试用例设计可并行；实际修改 `fork.rs` / `mailbox/receipts.rs` 不应并行。

## Related Specs

- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/prd.md:86-100` - R7/R8 定义 command/mailbox/delivery 三层事实和 mailbox AgentRun ownership。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/design.md:93-106` - 设计层明确 `CommandReceipt -> Queue -> DeliveryAttempt`。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/design.md:157-169` - 当前 `AgentRunCommandReceipt`、`AgentRunMailbox`、`SessionRuntimeCommand` 可能表达三件事，需要拆分。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md:82-111` - D-005/D-006。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md:232` - D-017 physical storage rule。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/inventory.md:21` - inventory 已记录 mailbox runtime ownership/cascade 问题。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-04-command-mailbox-queue.md:20-23` - WI-04 scope 要删除 runtime ownership/cascade，并定义 delivery attempt。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-12-database-migration-verification.md:22-25` - WI-12 要登记字段迁移、FK/cascade、索引。
- `.trellis/spec/backend/repository-pattern.md:53-55` - schema 变更应通过 migrations 进入，repository 负责持久化/事务/映射。
- `.trellis/spec/backend/database-guidelines.md:37-45` - migrations 是 schema 事实源；普通任务新增 migration，不由 repository 初始化补 schema。
- `.trellis/spec/backend/session/runtime-execution-state.md:176-185` - command receipt 属于 AgentRun command projection，不属于 RuntimeSession trace metadata。
- `.trellis/spec/backend/session/runtime-execution-state.md:205-216` - composer-submit 先 claim receipt，再写 mailbox envelope。
- `.trellis/spec/backend/session/runtime-execution-state.md:460` - `SessionRuntimeCommandStore` 是 runtime delivery command request/upsert/applied/failed。
- `.trellis/spec/backend/session/agentrun-mailbox.md:51` - 现有 spec 仍示例 `runtime_session_id: String`，与本任务 D-005 目标冲突，需要后续 spec 更新。

## External References

None. 本次研究只使用仓库内 task/spec/code/migration。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回当前 task 为 none；本研究按用户显式提供的 active task 路径写入。
- 未发现专用 `DeliveryAttempt` / `RuntimeDeliveryOperation` domain、repository、migration 或 API contract；当前只有 `RuntimeDeliveryCommand` / `SessionRuntimeCommandStore`，语义不是 mailbox delivery attempt。
- `.trellis/spec/backend/session/agentrun-mailbox.md` 仍包含 `runtime_session_id: String` 的旧式 mailbox command 示例；它和 D-005/WI-04 目标不一致，本文按当前任务 PRD/design/decisions 优先分类。
- 未运行测试、未执行 git 操作、未修改代码。
