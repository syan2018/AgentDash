# Research: AgentRun mailbox / companion / parallel wait capability

- Query: AgentDashboard 自有 exec / companion / AgentRun mailbox / lifecycle / hook 体系中，支持 Agent 并行、等待挂起、事件唤醒、结果回传的现状和缺口。
- Scope: internal
- Date: 2026-07-02

## Findings

### 能力图

当前体系已经形成一条以 AgentRun mailbox 为中心的自有执行链路：

1. 用户 / composer / canvas / draft / local relay 输入进入 `AgentRunMailboxService`，被写成 `AgentRunMailboxMessage`，再由 scheduler 按 `ImmediateIfIdle`、`AgentLoopTurnBoundary`、`AgentRunTurnBoundary` 或 `ManualResume` 消费，最终 launch、steer 或 queue 到 runtime。
2. Hook runtime 通过 `AgentRunMailboxRuntimeDelegate` 接入 mailbox。`after_turn` 将 hook steering 写为 `AgentLoopTurnBoundary`，`before_stop` 将 before-stop/follow-up 写为 `AgentRunTurnBoundary`，terminal completed fallback 会再次 schedule turn boundary。
3. Companion / subagent 通过 runtime tool `companion_request`、`companion_respond` 和 `LifecycleGate` 建立 parent-child/human 等待点；结果、parent request、parent response、human response 都通过 companion mailbox delivery 回写到目标 AgentRun mailbox。
4. Command receipt 是用户/API 命令的幂等投影和结果回放层；receipt 可以 attach mailbox message，accepted 后保存 outcome/result JSON。
5. Frontend 通过 AgentRun workspace control plane 读取 mailbox projection，监听 `mailbox_state_changed`、hook/companion system events 后刷新 workspace，并在 session chat/status bar 中展示 mailbox 行、companion 事件卡和相关操作。

能力图的关键生产文件：

- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs`: mailbox domain 模型，定义 `MailboxMessageOrigin`、`MailboxSourceIdentity`、`MailboxDelivery`、`ConsumptionBarrier`、`MailboxDrainMode`、`MailboxMessageStatus`、`AgentRunMailboxMessage` 和 repository trait。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs`: 用户、hook auto-resume、hook steering intake 写入 mailbox。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs`: mailbox 调度、claim、launch、steer、delegate drain、resume launch source 和 terminal fallback 消费逻辑。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs`: runtime boundary delegate，把 hook turn boundary 和 mailbox schedule/drain 连接起来。
- `crates/agentdash-api/src/agent_run_mailbox.rs`: runtime terminal callback，completed 后 schedule `AgentRunTurnBoundary`，failed/interrupted 后 pause mailbox。
- `crates/agentdash-application/src/companion/tools.rs`: companion request/respond runtime tools，subagent dispatch、parent/human/platform 请求，以及 companion mailbox delivery。
- `crates/agentdash-application/src/companion/dispatch.rs`: child agent dispatch 和 wait gate 创建。
- `crates/agentdash-application/src/companion/gate_control.rs`: human response、child result to parent、parent request/response gate 控制和 mailbox 回写。
- `crates/agentdash-application-workflow/src/gate/resolver.rs`: durable lifecycle gate resolve/open 逻辑。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts`: frontend AgentRun workspace control plane 与 mailbox action 绑定。
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx`: mailbox row 展示和 companion source label。
- `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx`: companion dispatch/result/human request 等 system event 展示。

### 已具备能力

#### AgentRun mailbox

`AgentRunMailboxMessage` 已经具备 durable envelope 所需的大部分字段：runtime/session/run/agent 归属、origin/source identity、delivery、barrier、drain mode、status、priority/order、dedup、queued/consuming/expected/accepted turn refs、claim token/lease、command receipt、payload、executor config、launch planning、attempt/error/timestamps。`AgentRunMailboxRepository` 提供 create、idempotent create、list、claim、expired consuming recovery、status update、policy update、pause/resume state、backend preference、move 等操作。

代码模式：

- `MailboxMessageOrigin` 覆盖 `User`、`System`、`Hook`、`Companion`、`Workflow`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:10`
- `MailboxSourceIdentity` 已有 composer、draft、hook、companion parent resume、workflow、routine、local relay、canvas action 等 helper：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:47`
- `MailboxDelivery` 支持 `LaunchOrContinueTurn`、`SteerActiveTurn`、`ResumeLaunchSource`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:178`
- `ConsumptionBarrier` 支持 `ImmediateIfIdle`、`AgentLoopTurnBoundary`、`AgentRunTurnBoundary`、`ManualResume`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:235`
- message status 包括 `Accepted`、`Queued`、`ReadyToConsume`、`Consuming`、`Dispatched`、`Steered`、`Paused`、`Blocked`、`Failed`、`Deleted`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:299`
- durable message 字段集中在 `AgentRunMailboxMessage`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:353`
- repository trait 覆盖 create/idempotent/claim/recover/status/state 操作：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:437`
- postgres `claim_next` 以 claim token/lease 把消息置为 consuming 并记录 consuming turn：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:224`
- expired consuming 且无 accepted refs 会恢复为 blocked `delivery_result_unknown`，防止未知交付结果被 promote：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:272`

#### command receipt

Command receipt 已经是 AgentRun command 的幂等控制层。用户提交等 API 命令先 claim receipt，mailbox message 创建后 attach receipt；delivery accepted 后 mark accepted 并保存 result JSON；失败时 mark terminal failed。

代码模式：

- `AgentRunCommandStatus` 和 `AgentRunCommandKind` 定义 pending/accepted/terminal_failed 与命令种类：`crates/agentdash-domain/src/workflow/command_receipt.rs:15`
- repository 支持 `claim`、`mark_accepted`、`attach_mailbox_message`、`store_result_json`、`mark_terminal_failed`：`crates/agentdash-domain/src/workflow/command_receipt.rs:138`
- postgres receipt repo 实现 claim/accepted/attach/store/fail：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_command_receipt_repository.rs:52`
- mailbox receipt helper 可从 duplicate receipt result 重建 command outcome：`crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs:135`

#### scheduler / terminal fallback / runtime adapter

Scheduler 已经覆盖 immediate launch、loop boundary steering、run boundary continuation、manual resume，以及 terminal completed fallback。runtime adapter 在 after-turn/before-stop 之间把 hook steering/follow-up 转成 durable mailbox message；terminal callback 在 completed 时 schedule turn boundary，在 failed/interrupted 时 pause mailbox。

代码模式：

- `AgentRunMailboxScheduleTrigger` 定义 `UserMessageSubmitted`、`AgentLoopTurnBoundary`、`AgentRunTurnBoundary`、`ManualResume`：`crates/agentdash-application-agentrun/src/agent_run/mailbox/commands.rs:28`
- `schedule_for_target` 按 trigger 选择 barrier/drain/limit：`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:44`
- `consume_claimed_message` 按 delivery 分派 launch、steer 或 resume launch source：`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:318`
- `execute_steering_delivery` 校验 active turn、expected turn、steering support，并 mark steered / complete receipt：`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:487`
- `consume_as_resume_launch_source` 当前只支持 hook auto-resume：`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:648`
- runtime adapter `schedule_agent_loop_turn_boundary` 会 schedule 并发出 mailbox state changed：`crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:198`
- `after_turn` 将 steering/follow_up 路由进 mailbox，并 schedule agent loop boundary：`crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:360`
- `before_stop` 先执行 inner hook，再 drain mailbox run boundary；若有消息则返回 Continue：`crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:406`
- terminal callback completed 后 schedule `AgentRunTurnBoundary`，failed/interrupted pause mailbox：`crates/agentdash-api/src/agent_run_mailbox.rs:80`

#### hook triggers / pending actions

Hook runtime 具备 before/after turn、before stop、subagent dispatch 前后、companion result 等触发点；companion result 可以生成 `HookPendingAction`，pending action 可在 `BeforeStop` 阻止 stop，并在下一 turn start 作为 context frame 注入。

代码模式：

- `HookPendingAction` 定义 pending action id/title/action_type/turn/source/status/resolution/injections：`crates/agentdash-spi/src/hooks/mod.rs:200`
- runtime event source 包括 `CompanionResult`：`crates/agentdash-spi/src/hooks/mod.rs:227`
- pending action 判断 blocking/follow-up：`crates/agentdash-spi/src/hooks/mod.rs:856`
- `before_stop` 检查 unresolved pending actions 和 gate 状态，必要时 Continue：`crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:714`
- pending action turn-start injection 被拆成 steering/follow_up：`crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:874`
- pending action enqueue/collect/resolve 在 hook runtime access 中维护：`crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:1214`
- pending action context frame 构造为 runtime context：`crates/agentdash-application-runtime-session/src/session/pending_action_context_frame.rs:39`

#### companion gates / subagent / parallel work

Subagent/parallel work 的主要入口已经存在：agent 调用 `companion_request`，target 可以是 `sub`、`parent`、`human`、`platform`；sub target 会 dispatch child agent，并通过 child AgentRun mailbox 写入第一条 companion dispatch 消息；child 通过 `companion_respond` 完成 gate，parent mailbox 接收 companion result。parent request/response、human response 也已经走 gate + mailbox 回传链路。

代码模式：

- `CompanionRequestTarget` 支持 `Sub`、`Parent`、`Human`、`Platform`：`crates/agentdash-application/src/companion/tools.rs:70`
- request params 具备 `wait: bool`：`crates/agentdash-application/src/companion/tools.rs:79`
- companion mailbox delivery 会构造 `AgentRunMailboxService`：`crates/agentdash-application/src/companion/tools.rs:100`
- `deliver_child_result_to_parent` 将 child result 作为 origin `Companion` 写入 parent mailbox，dedup key 为 `companion_result:<gate_id>`：`crates/agentdash-application/src/companion/tools.rs:208`
- parent request、parent response、human response 也通过 companion mailbox delivery 写入目标 mailbox：`crates/agentdash-application/src/companion/tools.rs:243`
- `deliver_companion_mailbox_message` 统一调用 `AgentRunMailboxService.accept_intake_message`，origin 为 companion，保留 payload，并 schedule on submit：`crates/agentdash-application/src/companion/tools.rs:369`
- sub request 会触发 `BeforeSubagentDispatch` / `AfterSubagentDispatch`，然后 dispatch child 并写 child mailbox：`crates/agentdash-application/src/companion/tools.rs:655`
- wait=true 的 sub request 会轮询 durable lifecycle gate 等待 resolved：`crates/agentdash-application/src/companion/tools.rs:953`
- child dispatch wait=true 时打开 `companion_wait*` gate；wait=false 时 launch child：`crates/agentdash-application/src/companion/dispatch.rs:54`
- gate resolver 支持 open companion gate、respond human、open/resolve parent request、complete child result：`crates/agentdash-application-workflow/src/gate/resolver.rs:68`
- gate control `complete_child_result_to_parent` 完成 child gate 后写 parent mailbox 并发 notification：`crates/agentdash-application/src/companion/gate_control.rs:516`
- gate control `open_parent_request` / `resolve_parent_request` 完成 child-parent mailbox 往返：`crates/agentdash-application/src/companion/gate_control.rs:666`
- human gate respond API 路由为 `/companion-gates/{gate_id}/respond`，并接入 human response mailbox delivery：`crates/agentdash-api/src/routes/companion_gates.rs:26`

#### frontend 展示

Frontend 已经有 AgentRun mailbox service、workspace control plane action、system event refresh、mailbox row 和 companion event card。

代码模式：

- mailbox API service 包括 submit/delete/promote/resume/move/fetch content：`packages/app-web/src/services/agentRunMailbox.ts:15`
- workspace control plane 绑定 mailbox submit/promote/delete/resume/recall/move，并刷新 conversation：`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:223`
- system event planner 遇到 hook/companion/mailbox state changed 会刷新 workspace/hook runtime：`packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:119`
- chat view 在 composer 前渲染 `SessionStatusBar` 和 mailbox messages/actions：`packages/app-web/src/features/session/ui/SessionChatView.tsx:692`
- mailbox row 已有 companion dispatch/result/parent_request/parent_response/human_response/parent_resume source label：`packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:144`
- companion dispatch/result/human request/review request system event card 已存在：`packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:88`

### 缺口

1. 当前还没有统一的“Agent 自己挂起等待并由事件唤醒”的 mailbox primitive。
   `wait=true` 的 companion sub/human 语义主要是 runtime tool 内部轮询 `LifecycleGate`。sub wait 的 `poll_gate_until_resolved` 未看到明确超时；human wait 有 300 秒 timeout。这个模式会占用当前 tool call/turn，不等价于 durable suspend/resume。

2. “等待”当前分散在三个层面，而不是一个闭合状态机：
   - durable wait fact 在 `LifecycleGate`；
   - wake/result delivery 在 `AgentRunMailboxMessage`；
   - stop blocking / next-turn injection 在 `HookPendingAction`；
   - user/API 幂等在 `AgentRunCommandReceipt`。
   这些层面的职责是合理的，但缺少统一 wait owner、correlation、projection 和恢复策略。

3. Mailbox status 不表达 first-class waiting/suspended。
   现有 message status 可表达 queued/ready/consuming/dispatched/steered/paused/blocked/failed/deleted，但没有“当前 AgentRun 正等待 gate/exec/subagent event”的 durable row。`AgentRunMailboxState.paused` 更适合 terminal failure/interruption 或人工 pause，不适合普通 companion wait。

4. companion 回传链路存在，但“等待挂起”闭环不完整。
   wait=false 的 subagent 可以并行运行并通过 parent mailbox 回传 result；wait=true 通过轮询 gate 同步等待。缺口在于：parent agent 主动进入 durable wait、释放当前 turn、等 child/exec/human event 通过 mailbox 唤醒后继续执行，这个闭合能力尚未抽象出来。

5. `ResumeLaunchSource` 目前只支持 hook auto-resume，不适合直接复用给 companion/exec。
   scheduler 的 `consume_as_resume_launch_source` 只处理 `hook_auto_resume`。companion/exec event 更适合先作为 origin `Companion`/`System`/`Workflow` 的 intake message 进入 mailbox，用 `LaunchOrContinueTurn` 或有明确 active-turn 语义时用 `SteerActiveTurn`。

6. companion result 对 running parent 的唤醒更偏 turn-boundary continuation，不是实时 interrupt。
   companion mailbox delivery 传入 `delivery_intent: None`，实际策略会根据 runtime state 选择 queued/run-boundary 语义。它可以在 before-stop/terminal fallback 被消费，但如果 parent 正在一个长 tool/wait 中，结果不会自动打断当前执行。

7. HookPendingAction 可阻止 stop，但不是 durable wait source of truth。
   pending action 当前在 runtime hook access 中维护，用于 injection/blocking；如果要承载 companion result adoption/review 的 durable 等待，需要和 gate/mailbox message id 建立可恢复关系，或者从 durable gate/mailbox 重建。

8. exec/terminal 并行事件尚未看到通用 wait/wake adapter。
   项目已有 terminal fallback 处理 AgentRun turn boundary，但“本项目自有 exec 完成事件 -> wait owner resolved -> mailbox wake/result -> UI projection”的通用服务还缺。应补在 AgentRun mailbox/lifecycle 自有体系内，不接 Codex，也不新增 `/sessions/*` 控制面。

9. UI 已能展示 mailbox row 和 companion system event，但缺少 first-class “等待中”投影。
   当前 UI 可以显示 queued mailbox messages、paused/user attention、companion event card；但 open gate / pending wait / exec wait 是否属于 workspace mailbox projection 尚不明确。需要把 wait item 投影到 AgentRun workspace，而不是引入 RuntimeSession command/control。

### 等待状态应落在哪些字段

建议保留现有分层，补齐 correlation 和 projection，而不是把所有状态塞进 receipt：

- `LifecycleGate`：作为 durable wait fact。companion/subagent/human/review 等待、未来 exec wait 都应有 gate 或同级 lifecycle wait record。gate 保存等待对象、request id、parent/child/runtime refs、status、resolution。
- `AgentRunMailboxMessage`：作为 wake/result envelope。并行事件完成时写入 mailbox，字段建议为：
  - `origin`: companion result 使用 `Companion`；exec/system event 使用 `System` 或 `Workflow`。
  - `source.namespace/kind/source_ref/correlation_ref`: namespace 如 `companion`、`exec`、`workflow`；kind 如 `result`、`parent_response`、`human_response`、`exec_result`；`source_ref` 用 gate id / exec id；`correlation_ref` 用 request id / dispatch id。
  - `source_dedup_key`: 稳定来自 gate id 或 exec event id，保证重试幂等。
  - `delivery`: 默认 `LaunchOrContinueTurn`；需要注入当前 active loop 时才用 `SteerActiveTurn`；不要把 companion/exec 伪装成 hook `ResumeLaunchSource`。
  - `barrier`: idle 可 `ImmediateIfIdle`；等待当前 turn 完成后继续用 `AgentRunTurnBoundary`；明确 active-loop steering 才用 `AgentLoopTurnBoundary`；人工恢复用 `ManualResume`。
  - `drain_mode`: result/response 默认 `One`；hook steering batch 可 `All`。
  - `status`: 等待唤醒消息未消费时为 `Queued`/`ReadyToConsume`，claim 时为 `Consuming`，成功为 `Dispatched`/`Steered`，不可恢复为 `Blocked`/`Failed`。`Paused` 只用于 mailbox state 或 terminal 异常，不用于普通 companion wait。
  - `queued_agent_run_turn_id` / `expected_active_agent_run_turn_id`: 需要绑定当前 active turn 时写入；idle/terminal launch 不应伪造 expected active turn。
  - `accepted_agent_run_turn_id` / `accepted_protocol_turn_id`: delivery accepted 后写入。
- `AgentRunCommandReceipt`：只表示用户/API command intake 的幂等与结果回放，不作为长期等待状态。内部 companion/exec result 可以有内部 client command id 参与 dedup，但 wait owner 不应是 receipt。
- `HookPendingAction`：作为 runtime injection 和 stop-blocking projection。它可以引用 gate/mailbox/correlation id，但不应单独成为 durable source of truth。
- `AgentRunMailboxState.paused`: 表示 failed/interrupted/manual pause 等需要人工 resume 的 mailbox 级状态，不表示普通“正在等待 companion/exec”。

### 设计约束

- 不得把 Codex 接入这个能力；parallel/wait/wake/result 必须在 AgentDashboard 自有 runtime、companion、lifecycle、AgentRun mailbox、hook、frontend workspace 体系内闭合。
- 不得新增任何对外 `/sessions/*` 端点。既有 AgentRun command 应继续走 `/agent-runs/{run_id}/agents/{agent_id}/...`，human gate response 可继续走 `/companion-gates/{gate_id}/respond`。
- RuntimeSession 可以作为 delivery/runtime ref，不应重新变成 frontend command owner 或外部 control plane。
- 兼容性/回退方案不是目标；当前预研项目应保持最正确的模型，并通过 migration 让数据库结构跟随正确模型。

### 建议实现切片

1. 定义 wait owner 与 projection contract。
   明确 companion/subagent/human/exec wait 的 source of truth 是 `LifecycleGate` 或同级 lifecycle wait record；明确 mailbox 只承载 wake/result envelope；明确 workspace projection 应暴露 open waits。这个切片是后续依赖，不涉及 terminal 修复。

2. 为现有 companion mailbox 回传链路补齐测试基线。
   覆盖 sub wait=false dispatch child mailbox、child `companion_respond` 回写 parent mailbox、parent request/response 往返、human API respond 回写 human_response mailbox、重复 gate/result dedup。可与终端修复并行。

3. 增加 AgentRun workspace “waiting items” 读模型。
   从 open lifecycle gates / unresolved pending actions / future exec waits 投影出等待中条目，让 UI 能显示 Agent 正等待哪个 companion/exec/human event。不要新增 `/sessions/*`；放在 AgentRun workspace/conversation projection 或 mailbox-adjacent projection。可与终端修复并行，前提是 contract 已定。

4. 建立通用 wait wake adapter。
   在 application 层提供 “event resolved -> build source identity -> write AgentRun mailbox -> schedule” 服务，复用 `AgentRunMailboxService.accept_intake_message`，支持 companion、exec、workflow event。这个切片依赖 wait owner contract，可与低层 terminal IO 修复并行，但如果终端修复也在定义 terminal event 语义，则需要先对齐。

5. 将 companion wait=true 从 tool 内轮询逐步改为 durable suspend/resume。
   目标是 parent agent 可以表达等待、释放当前 turn；child/human/result 完成后通过 wake adapter 写 mailbox 唤醒 parent。短期至少为 sub wait 加 timeout/cancel 策略，并记录 durable wait projection。依赖切片 1、3、4，不建议和终端修复强并行。

6. 对齐 HookPendingAction 的持久恢复。
   pending action 应引用 gate/mailbox/correlation id；runtime restart 后可从 durable gate/mailbox 重建，或 pending action 本身持久化。依赖 wait projection 和 companion result wake adapter。

7. 接入 exec/terminal completion event。
   把自有 exec 完成、失败、取消等事件映射为 lifecycle wait resolution 和 AgentRun mailbox result envelope，source namespace 建议为 `exec` 或 `workflow`。不要引入 Codex，不要新增 `/sessions/*`。若终端修复只处理 UI/PTY fallback，这个切片可并行；若终端修复要改事件模型，则应串行。

8. Frontend 展示等待与结果回传。
   在 AgentRun workspace/session status bar 中展示 wait items、pending gates、exec waits；继续监听 `mailbox_state_changed`、companion events 和未来 exec events 刷新。依赖 contract/generated types，可与后端切片后半并行。

9. 端到端验证与 contract guard。
   增加检查，确保新增能力只使用 `/agent-runs/...` 和 `/companion-gates/...`，不新增 `/sessions/*`，不出现 Codex adapter。

### 风险

- 当前 wait=true 轮询 gate 会占用 active tool call/turn；sub wait 未看到明确 timeout，可能形成长时间占用。
- pending action 可阻止 stop，但不具备独立 durable wait source 语义；重启恢复风险需要用 gate/mailbox correlation 化解。
- companion result 进入 parent mailbox 后通常在 turn boundary 被消费，不是对长时间 active execution 的即时中断。
- 内部 companion result 既用 mailbox dedup，又可能经 receipt/client command id 回放；需要稳定 source_dedup_key 和 correlation_ref，避免重试产生重复结果。
- `delivery_result_unknown` 的 blocked message 不能 promote；UI/运维需要能解释和观察。
- `ResumeLaunchSource` 当前只支持 hook auto-resume；把 companion/exec 硬塞进去会扩大 scheduler 语义风险。
- `target=platform` 的 capability grant 仍依赖平台 broker；当前测试表明缺 broker 时会返回诊断，不是完整闭环。

### 建议测试

- Repository/domain：Companion origin/source identity 的 create-idempotent、claim、expired consuming recovery、dedup key 与 gate correlation。
- Scheduler：companion/exec result 对 idle target 会 launch；对 running target 在 before-stop/terminal fallback 被 drain；expected turn mismatch 会 blocked；`delivery_result_unknown` 不能 promote。
- Runtime adapter：after_turn/before_stop hook steering/follow-up 写 mailbox；terminal completed fallback 消费 run boundary；failed/interrupted pause mailbox 并保留 resume 语义。
- Companion integration：sub wait=false dispatch child mailbox；child `companion_respond` 回写 parent mailbox；parent request/response 回写 child mailbox；human gate API respond 回写 human_response mailbox。
- Hook pending action：blocking review 阻止 stop；follow-up required 进入 follow_up injection；pending action 与 gate/mailbox id 可恢复。
- Frontend：mailbox source label 覆盖 companion dispatch/result/parent/human；`mailbox_state_changed` 和 companion result event 刷新 workspace；等待中条目和结果行不会重叠或丢失。
- Contract guard：新增 API 不包含 `/sessions/*`；命令入口仍是 `/agent-runs/{run_id}/agents/{agent_id}/...` 或 `/companion-gates/{gate_id}/respond`。

### Files found

- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs`: AgentRun mailbox domain model、message fields、source identity、delivery/barrier/status/repository trait。
- `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql`: mailbox messages/state 基础表和索引。
- `crates/agentdash-infrastructure/migrations/0032_agent_run_mailbox_source_identity.sql`: mailbox source identity 字段迁移。
- `crates/agentdash-infrastructure/migrations/0035_agent_run_mailbox_backend_selection.sql`: mailbox backend selection 字段迁移。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs`: postgres mailbox repository，包括 claim、recover、status update。
- `crates/agentdash-domain/src/workflow/command_receipt.rs`: AgentRun command receipt domain 和 repository trait。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_command_receipt_repository.rs`: command receipt postgres 实现。
- `crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql`: command receipt table migration。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs`: mailbox service 入口和常量。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/commands.rs`: mailbox command/result/target/schedule trigger 类型。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/policy.rs`: user message policy 和 runtime launch 判断。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs`: mailbox intake、hook auto-resume、hook steering message 写入。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs`: mailbox schedule、claim、launch、steer、resume launch source。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/receipts.rs`: receipt claim/accepted/failure/duplicate replay helper。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/target.rs`: AgentRun mailbox command target/current delivery runtime 解析。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs`: mailbox promote/delete/resume/move/content/pause controls。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs`: mailbox runtime port 和 turn boundary delegate。
- `crates/agentdash-api/src/agent_run_mailbox.rs`: mailbox terminal callback 和 terminal fallback schedule/pause。
- `crates/agentdash-api/src/bootstrap/session.rs`: runtime mailbox port 和 terminal callback 注入。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`: AgentRun mailbox/composer/control API routes。
- `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs`: AgentRun mailbox response contract mapping。
- `crates/agentdash-application/src/companion/tools.rs`: companion request/respond tools、subagent dispatch、mailbox delivery。
- `crates/agentdash-application/src/companion/dispatch.rs`: companion child dispatch 和 wait gate creation。
- `crates/agentdash-application/src/companion/gate_control.rs`: companion gate control 和 mailbox delivery intents。
- `crates/agentdash-application-workflow/src/gate/resolver.rs`: lifecycle gate open/respond/resolve/complete logic。
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs`: durable lifecycle gate domain。
- `crates/agentdash-api/src/routes/companion_gates.rs`: human companion gate response API。
- `crates/agentdash-application/src/companion/notifications.rs`: companion notification helper，说明 human response continuation 走 mailbox。
- `crates/agentdash-application/src/companion/payload_types.rs`: companion request/response payload validation。
- `crates/agentdash-spi/src/hooks/mod.rs`: hook runtime snapshot、pending action、runtime event source。
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs`: hook delegate before_stop、pending action injection、resolve。
- `crates/agentdash-application-runtime-session/src/session/hook_messages.rs`: pending action prompt instructions。
- `crates/agentdash-application-runtime-session/src/session/pending_action_context_frame.rs`: pending action context frame。
- `packages/app-web/src/services/agentRunMailbox.ts`: frontend mailbox API service。
- `packages/app-web/src/services/agentRunMailbox.test.ts`: frontend mailbox service URL tests。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts`: workspace mailbox actions and refresh flow。
- `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts`: system event refresh planning。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`: session chat mailbox/status bar render。
- `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx`: mailbox row labels/actions。
- `packages/app-web/src/features/agent-run-workspace/ui/mailboxContent.ts`: mailbox visible content predicate。
- `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx`: companion system event card。
- `packages/app-web/src/features/session/model/platformEvent.ts`: platform mailbox event mapping。
- `packages/app-web/src/features/session/model/systemEventPolicy.ts`: system event policy includes companion events。
- `packages/app-web/src/generated/agent-run-mailbox-contracts.ts`: generated mailbox frontend contracts。
- `packages/app-web/src/generated/companion-contracts.ts`: generated companion frontend contracts。

### Related specs

- `.trellis/spec/backend/session/agentrun-mailbox.md`: AgentRun mailbox envelope、delivery/barrier/drain/status/receipt/recovery contract。
- `.trellis/spec/backend/session/runtime-execution-state.md`: RuntimeSession 只能作为 delivery ref，AgentRun Workspace identity 是 command owner。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md`: hook/runtime delegate、mailbox delivery mapping、companion/subagent hook trigger/pending action contract。
- `.trellis/spec/backend/story-task-runtime.md`: companion subagent task context、Task facts 与 execution view 边界。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: frontend command API 应走 AgentRun routes，不把 RuntimeSession 作为 command owner。

### External references

- None. 本次调研只读项目内生产代码、migration 和 Trellis spec，未使用外部文档。

## Caveats / Not Found

- 未修改任何生产代码、spec、workflow 或 API；仅写入本 research 文件。
- 未执行完整测试；本文件是代码静态调研结论。
- 未发现 generic exec wait/wake adapter；只确认 terminal completed fallback 和 companion-specific mailbox delivery 存在。
- 未发现新增 `/sessions/*` 的必要性；现有设计约束也要求继续使用 AgentRun 和 companion gate 自有控制面。
- 未发现 Codex integration 点；建议实现切片也不依赖 Codex。
- `target=platform`/capability grant 的完整平台 broker 闭环未在本次代码路径中确认，只看到缺 broker 诊断相关测试线索。
