# Command / Mailbox / Delivery 研究结论

## 基本真理

1. 用户输入指令、队列状态、执行尝试是三类事实，不能互相代替。

- 用户输入指令是一次 actor intent：谁、在什么 AgentRun scope、提交了哪些 canonical `UserInputBlock`，以及这次 HTTP/client command 的幂等 key 和 request digest。它回答“用户/系统想让 Agent 看到什么”。代码上 `AgentRunComposerSubmitRequest.input` 直接使用 canonical Codex user input（`crates/agentdash-contracts/src/agent/run_mailbox.rs:212`），应用入口先 digest 再 claim receipt（`crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:177`, `:206`）。
- 队列状态是 durable delivery envelope：message id、source identity、payload retention、delivery/barrier/drain policy、priority/order、status、expected active turn、accepted refs。它回答“这份输入应在什么边界被消费”。代码上 `AgentRunMailboxMessage` 承载这些字段（`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:353`），wire view 也只暴露 mailbox row 所需字段（`crates/agentdash-contracts/src/agent/run_mailbox.rs:110`）。
- 执行尝试是 scheduler 对某个 envelope 的一次 claim/delivery attempt：claim token、lease、attempt count、实际 runtime/turn target、accepted/failure/unknown result。它回答“这次消费尝试是否已经跨过外部副作用边界”。当前最小实现把 attempt fact 折叠在 message 的 claim/attempt 字段里（`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:426`, `:457`, `:462`），scheduler 在消费前 claim，在完成时带 claim token 更新终态（`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:210`, `:236`, `:455`, `:622`）。

2. CommandReceipt 不是队列，也不是 runtime 状态。它是“单次产品命令”的幂等收据。

- Receipt 的唯一性来自 `scope_kind + scope_key + client_command_id`，并用 digest 区分 duplicate/conflict（`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:28`）。
- Receipt 记录 command kind、request digest、status、accepted refs、result JSON、error（`crates/agentdash-domain/src/workflow/command_receipt.rs:98`），repository 只提供 claim / accept / result / fail / get（`crates/agentdash-domain/src/workflow/command_receipt.rs:144`）。
- 因此 receipt 可以关联 mailbox message，但不能成为 mailbox message 的 owner；也不能替代 mailbox status。

3. RuntimeSession 是 delivery / trace substrate，不是产品命令状态机。

- 规范已明确 RuntimeSession 承载 event stream、turn、tool、resume、debug、projection 和 trace lineage，不拥有 business ownership（`.trellis/spec/backend/session/architecture.md:31`）。
- `RuntimeCommandRecord` 只表达 pending runtime context delivery（`crates/agentdash-spi/src/session_persistence.rs:390`, `:414`, `:420`），`TerminalEffectRecord` 只表达 terminal 后的 outbox effect（`crates/agentdash-spi/src/session_persistence.rs:506`）。
- session 表结构也分开：`session_runtime_commands` 与 `session_terminal_effects` 是 runtime store（`crates/agentdash-infrastructure/migrations/0001_init.sql:629`, `:643`），不是 AgentRun command receipt。

4. Tool approval 是 active runtime callback，不是 mailbox delivery。

- Agent loop 发出 `ToolExecutionPendingApproval`，等待 `await_tool_approval`，再发 `ToolExecutionApprovalResolved`（`crates/agentdash-agent/src/agent_loop/tool_call.rs:881`, `:899`, `:913`, `:927`）。
- PiAgent mapper 把它投影为 `approval_requested` / `approval_resolved` runtime stream event（`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1383`, `:1414`）。
- AgentRun scoped endpoint 只是先解析当前 delivery runtime，再复用 session approval endpoint（`crates/agentdash-api/src/routes/lifecycle_agents.rs:1238`, `:1244`, `:1245`, `:1253`, `:1260`, `:1261`）。它不应写 mailbox，也不应复用 AgentRun command receipt，除非未来产品要求“审批决策可恢复/可审计”为独立事实。

## 局部最优设计

### 1. 事实模型

推荐把控制面收敛成四层：

- `AgentRunCommandReceipt`：用户/系统产品命令的幂等 ledger。适用于 `message_submit`、`mailbox_promote`、`mailbox_delete`、`mailbox_resume`、`mailbox_move`、`cancel`、start/fork 等。它保存 command result refs，不保存队列调度细节。
- `AgentRunMailboxMessage`：AgentRun-scoped durable delivery envelope。它是队列 item，也是恢复投影 item。它保存 source/dedup、payload、delivery policy、barrier/drain、status、order、expected turn、accepted refs。
- `DeliveryAttempt`：概念上存在，最小实现不单独建表。只要系统保证同一 message 同时最多一个 claim，且不需要展示每次尝试历史，就用 message 上的 `claim_token / claimed_at / claim_expires_at / attempt_count / last_error` 表达当前 attempt。只有当要审计多次 attempt、分析 flaky backend、或支持并行 worker 历史时，才拆 `agent_run_mailbox_delivery_attempts`。
- `RuntimeSession` stores：只保存 runtime delivery instruction、trace event、terminal effect outbox。它不保存产品命令生命周期。

### 2. Mailbox 的边界

Mailbox 应是 AgentRun-scoped child aggregate，落在 child tables 上，而不是嵌入 AgentRun child fact，也不是脱离 AgentRun 的独立顶级聚合。

判断标准：

- 如果事实没有自己的并发不变量，只是 AgentRun snapshot 的字段，才嵌入 AgentRun child fact。
- 如果事实有自己的 append/list/claim/move/recover 状态机，且 scheduler 会独立修改它，就需要 child aggregate。
- 如果事实可以脱离 `run_id + agent_id` 被其他 owner 复用，才是独立顶级聚合。

当前 mailbox 满足第二类：它有独立 message table、state table、claim/recover/move repository API（`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:438`, `:457`, `:462`, `:520`），同时表通过 `run_id/agent_id` cascade 到 lifecycle facts（`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:59`, `:214`）。所以正确语言是：Mailbox 是 AgentRun child aggregate；表是 child table；聚合根 key 是 `run_id + agent_id`。

### 3. CommandReceipt 与 Mailbox 的关系

推荐关系是“应用层 process manager 统一编排”，持久化上只保留 receipt -> mailbox result ref 的单向主关系。

正确流程：

```text
command request
  -> claim AgentRunCommandReceipt
  -> create/update AgentRunMailboxMessage when command changes queue
  -> scheduler claim/deliver when needed
  -> write mailbox terminal/queued status
  -> complete receipt with outcome + accepted refs + mailbox_message_id
```

理由：

- Receipt 需要覆盖 `cancel` 这种没有 mailbox envelope 的命令（`crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs:64`, `:68`, `:79`, `:94`, `:109`）。
- Mailbox 需要覆盖 hook/system/companion wake 这种没有 user command receipt 的 envelope（`crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:348`, `:375`, `:410`, `:449`）。
- 双向 FK 会让两张表看起来互为 owner。当前 migration 同时存在 `mailbox.command_receipt_id` 和 `receipt.mailbox_message_id` FK（`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:82`, `:197`, `:201`, `:206`, `:209`）。局部最优是让 process manager 持有两者关系，DB 上保留 receipt 的 `mailbox_message_id` / `result_json` 即可；message 上如需 `command_receipt_id`，应只作为可空 correlation，不作为 lifecycle driver。

### 4. RuntimeSession command/outbox 定位

RuntimeSession command/outbox 只做 runtime substrate：

- `session_runtime_commands`：把 AgentFrame surface transition 投递到 active runtime。它回答“runtime 是否已应用某个 frame transition”，不是“用户命令是否完成”（`crates/agentdash-spi/src/session_persistence.rs:856`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:680`, `:793`, `:824`, `:832`）。
- `session_terminal_effects`：terminal event 已经成为事实后，异步执行 hook/callback/auto-resume 等副作用。它回答“terminal 后副作用是否完成”，不回滚 terminal event（`.trellis/spec/backend/session/architecture.md:58`, `crates/agentdash-spi/src/session_persistence.rs:831`）。
- `RuntimeSessionTurnDeliveryPort` 只应暴露 start/steer/cancel turn 这种物理投递能力（`crates/agentdash-application-ports/src/runtime_session_delivery.rs:59`, `:70`）。产品命令必须从 AgentRun command receipt + mailbox scheduler 进入。

### 5. cancel、tool approval、move/reorder 是否进入同一 command lifecycle

它们应共享同一个“command availability / stale guard / permission check”投影，但不都进入 mailbox delivery lifecycle。

- `cancel`：进入 AgentRunCommandReceipt lifecycle；不创建 mailbox envelope。它是对当前 runtime delivery 的 side-effect command，成功后 receipt accepted，失败后 terminal_failed。当前实现方向正确（`crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs:17`, `:27`, `:64`, `:79`）。
- `mailbox promote/delete/resume`：进入 receipt lifecycle，并修改 mailbox message/state。promote/delete 以 message 为对象，resume 以 mailbox state 为对象；它们可以触发 scheduler，但不是新的用户输入（`crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:28`, `:116`, `:212`, `:230`, `:237`）。
- `move/reorder`：如果后端持久化 order_key，就应进入 receipt lifecycle。当前 route/service 已经有 `move` 并直接改 mailbox order（`crates/agentdash-api/src/routes/lifecycle_agents.rs:137`, `crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:296`, `:347`），但没有 command receipt。局部最优是补 `AgentRunCommandKind::MailboxMove`，用 stale guard + receipt 保护重排冲突；不创建 delivery attempt。
- `tool approval`：不进入 AgentRunCommandReceipt / mailbox lifecycle。它属于 active runtime tool call 的 callback。AgentRun endpoint 只负责 run/agent -> current runtime 的解析。未来若需要持久可恢复审批，建 `runtime_tool_approval_requests/decisions` 或 PermissionGrant-like broker fact，不放进 mailbox。

### 6. 最小表 / repository / port 形态

最小表：

- `agent_run_command_receipts`
  - `id`
  - `scope_kind`, `scope_key`, `client_command_id`, `request_digest`
  - `command_kind`
  - `status`
  - accepted refs：`run_id`, `agent_id`, `frame_id`, `frame_revision`, `runtime_session_id`, `agent_run_turn_id`, `protocol_turn_id`
  - `mailbox_message_id` nullable
  - `result_json`, `error_message`, timestamps
  - unique `(scope_kind, scope_key, client_command_id)`
- `agent_run_mailbox_messages`
  - `id`, `run_id`, `agent_id`
  - source identity：`source_namespace`, `source_kind`, `source_ref`, `source_correlation_ref`, `source_actor`, `source_route`, `source_display_label_key`, `source_metadata`
  - `origin`, `payload_json`, `preview`, `has_images`, `retain_payload`
  - delivery policy：`delivery`, `delivery_json`, `barrier`, `drain_mode`, `priority`, `order_key`
  - state：`status`, `queued_agent_run_turn_id`, `expected_active_agent_run_turn_id`, `accepted_agent_run_turn_id`, `accepted_protocol_turn_id`
  - current attempt：`claim_token`, `claimed_at`, `claim_expires_at`, `attempt_count`, `last_error`
  - optional physical target/evidence fields. Prefer naming them as delivery refs, not aggregate identity. `runtime_session_id` should not be the conceptual owner of a mailbox message.
- `agent_run_mailbox_states`
  - keep only AgentRun queue-level facts: paused/resume/user attention, optional delivery preference.
  - If `backend_selection_preference` is treated as run workspace preference rather than queue state, move it out of mailbox later; current migration has it on mailbox state（`crates/agentdash-infrastructure/migrations/0035_agent_run_mailbox_backend_selection.sql:5`）。

最小 repository/port：

- `AgentRunCommandReceiptRepository`: `claim`, `mark_accepted`, `store_result_json`, `mark_terminal_failed`, `get`; keep `attach_mailbox_message` only as receipt-result helper（`crates/agentdash-domain/src/workflow/command_receipt.rs:144`, `:156`, `:162`）。
- `AgentRunMailboxRepository`: `create_message_idempotent`, `list_messages`, `claim_next`, `recover_expired_consuming`, `mark_message_status`, `update_message_policy`, `delete_message`, `move_message_after`, `pause_state`, `resume_state`, `get_state`（`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:444`, `:457`, `:462`, `:520`）。
- `AgentRunMailboxService`: application process manager that composes lifecycle/run/frame anchor, receipt, mailbox, session core/control/eventing/launch（`crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs:68`, `:75`, `:76`, `:77`, `:78`, `:79`, `:80`）。
- `RuntimeSessionCore/Control/Eventing/Launch` ports: physical runtime inspection, steer, event projection, launch（`crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:88`, `:131`, `:164`, `:202`）。
- `AgentRunCancelRuntimePort`: physical cancel bridge only（`crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs:27`）。
- No `DeliveryAttemptRepository` in the minimal design. Add it only when attempt history is a product/debug requirement.

## 删除清单

1. 删除“Mailbox 是 RuntimeSession 子状态”的建模倾向。

Mailbox 的 owner 是 `run_id + agent_id`；RuntimeSession 是当前 delivery target / trace evidence。`runtime_session_id` 可以作为 delivery ref，但不应成为 mailbox aggregate identity。规范也说 AgentRun workspace message intake 进入 mailbox，再映射到 runtime turn/start/steer（`.trellis/spec/backend/session/architecture.md:31`）。

2. 删除 receipt/mailbox 的双向 lifecycle 依赖。

保留 process manager 组合。DB 上优先保留 receipt -> mailbox result ref；message -> receipt 如果保留，也只做 nullable correlation。当前双向 FK 是应收敛点（`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:197`, `:206`）。

3. 删除无需求的 `DeliveryAttempt` 表。

当前 claim token + lease + attempt_count 已满足单 worker claim/recovery（`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:236`, `:455`, `:622`）。不要为了命名完整性建 attempt 表。只有多 attempt 历史需要查询时再拆。

4. 删除把 tool approval 放进 mailbox / AgentRunCommandReceipt 的方案。

tool approval 是 runtime tool call callback，事件已在 runtime stream 中表达（`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1383`, `:1414`）。需要持久审批时另建 runtime/broker approval fact。

5. 删除 route-local `send_next/enqueue/steer` 权威分支。

唯一权威流程是 command receipt -> mailbox envelope -> scheduler outcome。当前 `accept_intake_message_for_target` 已按这个方向实现（`crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:95`, `:206`, `:238`, `:285`）。

6. 删除从 runtime status 或 mailbox status 各自推导 command availability 的分叉。

后端 `ConversationCommandAvailabilityResolver` 是 command set 的单点（`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:517`, `:717`, `:729`, `:749`, `:761`, `:773`），command policy 再用 stale guard 校验（`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:361`, `:405`, `:414`, `:421`, `:428`, `:435`）。

7. 删除 move/reorder 绕过 command receipt 的例外。

只要 move/reorder 是后端持久化写操作，就应有 receipt/stale guard。当前已有 endpoint 和 service，但缺 receipt（`crates/agentdash-api/src/routes/lifecycle_agents.rs:137`, `crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:296`）。

## 迁移 / 实施顺序

1. 先冻结词汇。

- `CommandReceipt` = 产品命令幂等收据。
- `MailboxMessage` = AgentRun-scoped durable delivery envelope。
- `DeliveryAttempt` = message claim/delivery attempt；默认不是表。
- `RuntimeCommand` = RuntimeSession frame/context delivery instruction。
- `TerminalEffect` = terminal 后 outbox effect。

2. 补齐 command kind。

- 增加 `mailbox_move` 到 `AgentRunCommandKind`、DB check constraint、generated DTO 或 command policy。
- move/reorder route 接收 `client_command_id + command precondition`，按 promote/delete/resume 同样 claim receipt。

3. 收敛 receipt/mailbox 关系。

- 应用层保持 `AgentRunMailboxService` 为 process manager。
- 迁移目标：receipt 保存 `mailbox_message_id/result_json/accepted_refs`；message 上的 `command_receipt_id` 若保留，改名/文档化为 optional correlation。可后续移除 FK，避免循环 ownership。
- 更新 repository mapper：completion 时从 message 找 receipt 的路径改为由 service 持有 receipt id 或查询 receipt result ref。

4. 明确 mailbox message 的 runtime ref 语义。

- 将 message 的 runtime 字段从“identity”语义改为“delivery target/evidence”语义。若要更正 schema，可拆为 `target_runtime_session_id` / `accepted_runtime_session_id`，或把 actual runtime 放入 current claim/accepted refs。
- scheduler claim 时以 `run_id + agent_id` 解析 current delivery runtime；只有带 expected runtime/turn 的 anchored message 才因 mismatch block。

5. 保持 DeliveryAttempt 不建表。

- 强化 recovery 规则：expired consuming + no accepted refs -> `Blocked(delivery_result_unknown)`；accepted refs present -> terminal status restore。
- 如果后续观测需要历史，再新增 `agent_run_mailbox_delivery_attempts`，由 scheduler append attempt，不改变 message 作为 queue item 的事实源。

6. 把 RuntimeSession stores 清出产品命令语义。

- 检查 `session_runtime_commands` 只保存 frame/context delivery。
- 检查 `session_terminal_effects` 只保存 terminal effect outbox。
- AgentRun submit/cancel/promote/delete/resume/move 只使用 AgentRun command receipt。

7. 固化 tool approval 边界。

- AgentRun scoped approval endpoint 继续只解析 current delivery runtime。
- 若需要 durable approval，新增 runtime-scoped `tool_approval_requests/decisions`，并用 runtime stream 投影；不进 mailbox。

8. 最后做前端/generated 对齐。

- Generated contract 已包含 receipt、message response、mailbox message view、move request（`packages/app-web/src/generated/agent-run-mailbox-contracts.ts:13`, `:37`, `:45`, `:69`）。
- 前端继续消费后端 command snapshot，不本地推导 launched/queued/steered（`.trellis/spec/frontend/state-management.md:82`, `:83`, `:140`）。

## 需要验证的代码事实

1. 验证是否仍有 route-local launch/queue/steer 分支绕过 mailbox。

重点 grep `composer-submit`, `launch_command_in_task`, `steer_session`, `accept_user_message`。应只有 `accept_intake_message_for_target -> create_message_idempotent -> schedule_for_target` 是 submit 主线（`crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:95`, `:238`, `:285`）。

2. 验证 move/reorder 当前缺 command receipt。

证据显示 route/service 存在（`crates/agentdash-api/src/routes/lifecycle_agents.rs:137`, `crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:296`, `:347`），但 `AgentRunCommandKind` 没有 `MailboxMove`（`crates/agentdash-domain/src/workflow/command_receipt.rs:40`）且 DB constraint 没有 `mailbox_move`（`crates/agentdash-infrastructure/migrations/0039_agent_run_command_receipt_fork_kinds.sql:5`）。

3. 验证 receipt/mailbox 双向 FK 是否被业务代码依赖。

当前 service completion 会从 message 上的 `command_receipt_id` 完成 receipt（`crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs:119`），duplicate replay 从 receipt 的 `mailbox_message_id` 找 message（`crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs:47`, `:55`）。迁移前要替换 message -> receipt 的完成路径。

4. 验证 mailbox `runtime_session_id` 是否阻碍 AgentRun-scoped恢复。

当前 message schema 强制 `runtime_session_id`（`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:63`），scheduler claim 也按 runtime filter（`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:236`）。如果 AgentRun 切换 current delivery runtime，旧 queued message 的预期行为需要测试：应 pause/block/retarget，而不是静默丢失。

5. 验证 tool approval 不需要跨进程恢复。

Agent side pending approval 是 in-memory waiter（`crates/agentdash-agent/src/agent.rs:283`, `:292`, `:306`, `:589`）。如果进程重启后必须恢复 pending approval，则当前模型不够，需要 runtime-scoped durable approval fact；但仍不应进入 mailbox。

6. 验证 RuntimeSession command/outbox 没有承载产品命令。

检查 `RuntimeDeliveryCommandKind` 目前只有 `PendingRuntimeContext`（`crates/agentdash-spi/src/session_persistence.rs:414`），Postgres runtime command store 只 upsert/list/mark runtime delivery command（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:680`, `:793`, `:824`, `:832`）。

7. 验证 command availability 单点仍覆盖所有用户意图。

`ConversationCommandKindModel` 包含 submit/promote/delete/resume/cancel（`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:123`），但 move/reorder 不在其中。若 move 成为 receipt command，也要进入 command availability / stale guard。

## 关键 file:line 证据

- `.trellis/spec/backend/session/agentrun-mailbox.md:5`：Mailbox 是统一 message intake、调度队列和恢复投影。
- `.trellis/spec/backend/session/agentrun-mailbox.md:164`：Mailbox runtime adapter 只作为 turn boundary delegate，不拥有工具授权/provider telemetry。
- `.trellis/spec/backend/session/agentrun-mailbox.md:170`：`thread/resume` 不隐式 drain mailbox。
- `.trellis/spec/backend/session/architecture.md:31`：AgentRun message intake 进入 mailbox，再映射到 runtime turn/start/steer。
- `.trellis/spec/backend/session/architecture.md:36`：pending runtime delivery command 只保存投递指令。
- `.trellis/spec/backend/session/architecture.md:58`：terminal effect 使用 outbox。
- `.trellis/spec/backend/session/architecture.md:66`：RuntimeSession 不拥有 Grant authorization。
- `.trellis/spec/backend/repository-pattern.md:12`：Repository 接口语义对应聚合边界，不混跨聚合事务。
- `.trellis/spec/backend/repository-pattern.md:20`：Session persistence 不通过 RepositorySet 表达。
- `.trellis/spec/backend/repository-pattern.md:34`：跨聚合一致性使用显式 Command Port。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:353`：`AgentRunMailboxMessage` domain record。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:426`：claim request。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:438`：`AgentRunMailboxRepository`。
- `crates/agentdash-domain/src/workflow/command_receipt.rs:98`：`AgentRunCommandReceipt` domain record。
- `crates/agentdash-domain/src/workflow/command_receipt.rs:144`：`AgentRunCommandReceiptRepository`。
- `crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:1`：receipt table。
- `crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:28`：receipt unique command key。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:59`：mailbox messages table。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:82`：message has `command_receipt_id`。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:201`：message -> receipt FK。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:209`：receipt -> message FK。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:214`：mailbox states table。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:177`：submit digest。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:206`：submit claims receipt。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs:238`：submit creates mailbox message.
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:236`：scheduler claims messages。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:396`：launch delivery to RuntimeSession。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:557`：steer active RuntimeSession。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:143`：unknown delivery result cannot promote。
- `crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs:64`：cancel claims receipt。
- `crates/agentdash-application-agentrun/src/agent_run/cancel_command.rs:79`：cancel calls runtime cancel。
- `crates/agentdash-spi/src/session_persistence.rs:390`：runtime command record。
- `crates/agentdash-spi/src/session_persistence.rs:506`：terminal effect record。
- `crates/agentdash-agent/src/agent_loop/tool_call.rs:881`：tool approval pending event。
- `crates/agentdash-agent/src/agent_loop/tool_call.rs:899`：waits approval callback。
- `crates/agentdash-agent/src/agent_loop/tool_call.rs:913`：approval resolved accepted event。
- `crates/agentdash-agent/src/agent_loop/tool_call.rs:927`：approval resolved rejected event。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:169`：AgentRun-scoped approve endpoint。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1244`：approval resolves current delivery runtime before delegating。
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:123`：conversation command kind set。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:361`：stale guard command validation。
