# Research: agentrun-control-deep-dive

- Query: 深挖 AgentRun command/control、ConversationSnapshot、Mailbox、RuntimeSession runtime-control、direct steer 的事实源与控制面耦合，输出后续可拆任务候选。
- Scope: internal
- Date: 2026-06-21

## 结论摘要

1. AgentRun 用户输入的 launch / steer 主链路已经基本收敛到 `AgentConversationSnapshot.commands` 展示命令、`conversation.mailbox` 展示 mailbox、`AgentRunMailboxService` 写 durable envelope 并调度消费。产品侧 `composer-submit` 先校验 command precondition，再调用 `AgentRunMailboxService::accept_user_message`，后者 claim command receipt、创建 mailbox message，再由 scheduler 判定 launched / queued / steered；证据见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:428`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:448`、`crates/agentdash-application/src/agent_run/mailbox.rs:325`、`crates/agentdash-application/src/agent_run/mailbox.rs:355`、`crates/agentdash-application/src/agent_run/mailbox.rs:390`。
2. 当前主要耦合不在“有没有 mailbox”，而在“同一 command state 被多层重新投影”。`AgentConversationSnapshotResolver` 负责 UI commands，`AgentRunWorkspaceCommandPolicyService` 又重新构造 snapshot 做服务端 stale/enablement 校验，API mapper 再从 conversation 派生 top-level `control_plane`，frontend adapter 再把 conversation commands 转为 `SessionChatCommandState`。其中 frontend/API mapper 多数是薄适配；command policy 复用 UI resolver 但输入不完整，是后续最值得清理的 P1。
3. delivery runtime 选择逻辑存在更明确的 P0 耦合：application workspace query 和 API context 都用 `list_by_run(run_id) -> filter(agent_id) -> max(updated_at)` 选择 runtime session，且该选择直接影响 workspace snapshot、command route、mailbox target。这个事实源应被收敛成单一 resolver，并显式说明 frame revision、replacement session、orchestration node attempt 的选择策略；证据见 `crates/agentdash-application/src/agent_run/workspace/query.rs:314`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:825`。
4. `RuntimeSession` runtime-control 当前保持 trace/detail/backlink 姿态：后端 DTO 没有 mailbox/action list，只返回 session meta、anchor、run、agent、frame_runtime、subject_associations；前端 `useSessionStream.sendCancel` 明确抛出 “RuntimeSession trace 不提供取消入口”。这块不应再作为 P0 重复 review；证据见 `crates/agentdash-contracts/src/runtime/workflow.rs:1328`、`crates/agentdash-api/src/routes/sessions.rs:145`、`packages/app-web/src/features/session/model/useSessionStream.ts:153`。
5. direct steer 有两个不同性质：`AgentRunSteeringService` 仍存在并可直接调用 `SessionControlService::steer_session`，但当前检索只在 tests 使用，未发现 API/product route 引用；local relay `command.steer` 是本机 relay/session 控制面，不是 AgentRun workspace mailbox 控制面。后续可拆一个清理/封口任务，把 AgentRun direct steer 标成测试专用或移除 export。
6. hook/direct fallback 仍是需要约束的边界：`AgentRunMailboxRuntimeDelegate` 在 mailbox 写入返回 `NotFound` 时把 hook steering/follow-up 原样回退为 direct `AgentMessage`。这对 unanchored runtime 可能合理，但 anchored AgentRun 缺 anchor 时会绕过 durable mailbox；需要把 fallback 条件改成“明确 unanchored trace”，而不是任意 `NotFound`。

## 主链路拓扑

### Files Found

- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` - 本轮 review 总目标、范围和只读约束。
- `.trellis/tasks/06-21-module-topology-coupling-review/design.md` - Round 2 deep-dive 的耦合 taxonomy 与产物要求。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/03-session-agentrun-runtime-topology.md` - 第一轮 Session / AgentRun / mailbox 主链路基线。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/06-frontend-contracts-topology.md` - 第一轮 frontend/contracts 消费边界基线。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/02-workflow-lifecycle-task-topology.md` - Lifecycle / RuntimeSessionExecutionAnchor / SubjectExecution 边界基线。
- `.trellis/spec/backend/session/agentrun-mailbox.md` - durable mailbox envelope、scheduler、command receipt、hook convergence 合同。
- `.trellis/spec/backend/session/runtime-execution-state.md` - RuntimeSession trace/detail/backlink、AgentRun workspace command/mailbox control 合同。
- `crates/agentdash-application/src/agent_run/conversation_snapshot.rs` - `AgentConversationSnapshot` execution / commands / mailbox projection。
- `crates/agentdash-application/src/agent_run/workspace/query.rs` - AgentRun workspace 聚合查询与 latest delivery runtime 选择。
- `crates/agentdash-application/src/agent_run/workspace/command_policy.rs` - command precondition / stale guard 服务端校验。
- `crates/agentdash-application/src/agent_run/mailbox.rs` - durable mailbox intake、scheduler、launch/steer consumer。
- `crates/agentdash-application/src/agent_run/message_delivery.rs` - mailbox launch 到 session launch pipeline 的 adapter。
- `crates/agentdash-application/src/agent_run/steering.rs` - 保留的 AgentRun direct steer service。
- `crates/agentdash-application/src/session/mailbox_delegate.rs` - hook delegate 到 mailbox boundary，以及 NotFound direct fallback。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - AgentRun workspace / composer / mailbox / cancel HTTP 控制面。
- `crates/agentdash-api/src/routes/sessions.rs` - RuntimeSession runtime-control 只读 backlink API。
- `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs` - mailbox command response / mailbox row DTO mapper。
- `crates/agentdash-contracts/src/runtime/workflow.rs` - `AgentConversationSnapshot`、`AgentRunWorkspaceView`、`SessionRuntimeControlView` contracts。
- `crates/agentdash-contracts/src/agent/run_mailbox.rs` - mailbox message / command response contracts。
- `packages/app-web/src/pages/AgentRunWorkspacePage.conversationCommandState.ts` - conversation commands 到 SessionChat command state 的 adapter。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts` - 前端 AgentRun command endpoint adapter。
- `packages/app-web/src/features/session/model/useSessionStream.ts` - RuntimeSession trace stream hook，取消入口显式拒绝。

### 1. AgentRun workspace read projection

```text
GET /agent-runs/{run_id}/agents/{agent_id}/workspace
  -> resolve_agent_run_context / permission
  -> AgentRunWorkspaceQueryService.resolve
  -> latest RuntimeSessionExecutionAnchor by run+agent
  -> SessionExecutionState + frame/VFS + mailbox repo state/messages
  -> AgentConversationSnapshotResolver.resolve
  -> API mapper injects conversation.mailbox state/messages
  -> AgentRunWorkspaceView
  -> frontend AgentRunWorkspacePage -> SessionChatView
```

证据：

- workspace query 通过 `delivery_runtime_session_for_agent_run` 获取 delivery runtime，再读 session meta 和 execution state，见 `crates/agentdash-application/src/agent_run/workspace/query.rs:70`、`crates/agentdash-application/src/agent_run/workspace/query.rs:125`。
- same query 读取 mailbox messages/state，计算 visible count，并把 paused/can_resume 输入 conversation snapshot，见 `crates/agentdash-application/src/agent_run/workspace/query.rs:151`、`crates/agentdash-application/src/agent_run/workspace/query.rs:173`、`crates/agentdash-application/src/agent_run/workspace/query.rs:203`。
- API mapper 把 application snapshot 中的 mailbox state/messages 写回 `conversation.mailbox`，再从 conversation 派生 top-level `control_plane`，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:989`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:992`。
- frontend runtime 模式直接使用 `input.conversation.commands`，见 `packages/app-web/src/pages/AgentRunWorkspacePage.conversationCommandState.ts:231`。

### 2. AgentRun composer / mailbox command write path

```text
POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit
  -> ensure_composer_submit_allowed(command stale guard)
  -> AgentRunMailboxService.accept_user_message
  -> claim AgentRunCommandReceipt
  -> create AgentRunMailboxMessage durable envelope
  -> schedule(UserMessageSubmitted)
  -> consume_as_launch OR consume_as_steering
```

证据：

- composer route 先 `ensure_composer_submit_allowed`，再调用 `AgentRunMailboxService::accept_user_message(... schedule_on_submit: true ...)`，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:428`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:448`。
- `accept_user_message_for_target` claim command receipt，duplicate 时 replay，见 `crates/agentdash-application/src/agent_run/mailbox.rs:325`、`crates/agentdash-application/src/agent_run/mailbox.rs:334`。
- 同一方法创建 durable mailbox message，并 attach command receipt，见 `crates/agentdash-application/src/agent_run/mailbox.rs:355`、`crates/agentdash-application/src/agent_run/mailbox.rs:385`。
- scheduler 对 `UserMessageSubmitted` 只在 runtime 可 launch 时消费 ImmediateIfIdle，否则保留 queued，见 `crates/agentdash-application/src/agent_run/mailbox.rs:1038`。
- user message policy 在 running + steer intent + supports steering 时选择 `SteerActiveTurn`，running/cancelling 普通输入进入 `AgentRunTurnBoundary`，idle/completed/failed/interrupted 输入进入 `ImmediateIfIdle`，见 `crates/agentdash-application/src/agent_run/mailbox.rs:2133`。

### 3. Mailbox consumption into Session launch / steer

```text
claimed mailbox envelope
  -> consume_claimed_message
  -> LaunchOrContinueTurn:
       AgentRunTurnBoundary + still running => consume_as_steering
       otherwise => consume_as_launch
  -> SteerActiveTurn => consume_as_steering
  -> accepted refs + receipt result + payload cleanup
```

证据：

- mailbox scheduler 按 trigger 分别 claim AgentLoopTurnBoundary / AgentRunTurnBoundary / ManualResume，见 `crates/agentdash-application/src/agent_run/mailbox.rs:1056`、`crates/agentdash-application/src/agent_run/mailbox.rs:1069`、`crates/agentdash-application/src/agent_run/mailbox.rs:1082`。
- `consume_claimed_message` 在 `AgentRunTurnBoundary` 且 runtime 仍 running 时把 `LaunchOrContinueTurn` 转为 steering，否则 launch，见 `crates/agentdash-application/src/agent_run/mailbox.rs:1351`、`crates/agentdash-application/src/agent_run/mailbox.rs:1379`。
- launch adapter 用 `LaunchCommand::lifecycle_agent_user_message_input` 回到 session launch pipeline，见 `crates/agentdash-application/src/agent_run/message_delivery.rs:52`。
- steer consumer 调 `SessionControlService::steer_session(SessionTurnSteerCommand { ... })`，见 `crates/agentdash-application/src/agent_run/mailbox.rs:1503`、`crates/agentdash-application/src/agent_run/mailbox.rs:1549`。
- command response mapper 把 result outcome、mailbox message、accepted refs、runtime command state 投影给前端，见 `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:13`。

### 4. RuntimeSession runtime-control backlink

```text
GET /sessions/{runtime_session_id}/runtime-control
  -> SessionMeta
  -> RuntimeSessionExecutionAnchor.find_by_session
  -> LifecycleRun + Agent + launch frame + subject associations
  -> SessionRuntimeControlView
```

证据：

- unbound trace 返回 `UnboundTrace`，没有 AgentRun action list，见 `crates/agentdash-api/src/routes/sessions.rs:156`。
- anchored trace 返回 anchor、run、agent、frame_runtime、subject_associations 和 control status，见 `crates/agentdash-api/src/routes/sessions.rs:217`、`crates/agentdash-api/src/routes/sessions.rs:244`、`crates/agentdash-api/src/routes/sessions.rs:270`。
- contract `SessionRuntimeControlView` 字段仅包含 runtime ref、session meta、control plane、anchor/run/agent/frame/subject associations，见 `crates/agentdash-contracts/src/runtime/workflow.rs:1328`。
- frontend trace stream hook 的 fallback cancel 直接抛错，见 `packages/app-web/src/features/session/model/useSessionStream.ts:153`。

## 耦合矩阵

| Coupling | From | To | Relationship | Evidence | Risk |
| --- | --- | --- | --- | --- | --- |
| delivery runtime selection duplicated | `AgentRunWorkspaceQueryService` | API route context / mailbox command target | 两处都按 run+agent anchors 的 `updated_at` 选择 latest runtime，影响 read projection 与 write command target | `crates/agentdash-application/src/agent_run/workspace/query.rs:314`; `crates/agentdash-api/src/routes/lifecycle_agents.rs:825` | P0 |
| command state resolver reused by policy | `AgentConversationSnapshotResolver` | `AgentRunWorkspaceCommandPolicyService` | policy 重新构造 snapshot 校验 command enabled/stale guard，但输入不含 subject/resource，model config 硬编码 resolved | `crates/agentdash-application/src/agent_run/workspace/command_policy.rs:134`; `crates/agentdash-application/src/agent_run/workspace/command_policy.rs:168`; `crates/agentdash-application/src/agent_run/workspace/command_policy.rs:474` | P1 |
| top-level control plane derived from conversation | API mapper | contracts/frontend | `AgentRunWorkspaceView.control_plane` 从 `conversation.execution.status` 派生，是 legacy/top-level status duplication，但不含 command action list | `crates/agentdash-api/src/routes/lifecycle_agents.rs:1051`; `crates/agentdash-contracts/src/runtime/workflow.rs:1098` | P2 |
| frontend command adapter | `AgentConversationSnapshot.commands` | `SessionChatCommandState` / command hook | runtime 模式直接传 generated commands，command hook 只打包 stale guard + client_command_id；属于必要 UI adapter | `packages/app-web/src/pages/AgentRunWorkspacePage.conversationCommandState.ts:231`; `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:96` | P2 |
| mailbox row flags duplicated in mapper | domain mailbox status | `MailboxMessageView.can_promote/can_delete/can_reorder/can_recall` | API mapper根据 message status/delivery/last_error 派生 UI row action flag；可接受但应保持纯 projection，不得驱动 scheduler truth | `crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:88` | P2 |
| direct AgentRun steer retained | `AgentRunSteeringService` | `SessionControlService::steer_session` | 绕过 mailbox envelope 的 direct service 存在并导出；当前产品代码未发现调用，tests 仍使用 | `crates/agentdash-application/src/agent_run/steering.rs:61`; `crates/agentdash-application/src/agent_run/mod.rs:35`; `crates/agentdash-application/src/session/hub/tests.rs:937` | P1 |
| hook NotFound direct fallback | `AgentRunMailboxRuntimeDelegate` | direct `TurnControlDecision.steering/follow_up` | mailbox accept hook message NotFound 时回退 direct messages；如果 anchored AgentRun 只是 anchor 缺失，会绕过 durable mailbox | `crates/agentdash-application/src/session/mailbox_delegate.rs:190`; `crates/agentdash-application/src/session/mailbox_delegate.rs:219`; `crates/agentdash-application/src/session/mailbox_delegate.rs:337` | P0 |
| RuntimeSession runtime-control backlink | `/sessions/{id}/runtime-control` | Lifecycle/AgentRun refs | 当前只读 backlink，不复制 mailbox/actions；属于已收敛边界 | `crates/agentdash-api/src/routes/sessions.rs:145`; `crates/agentdash-contracts/src/runtime/workflow.rs:1328` | Low |
| local relay direct steer | local runtime relay | `SessionControlService::steer_session` | 本机 `command.steer` 是 session/relay control，不是 AgentRun workspace mailbox；需在后续 review 中与 AgentRun direct steer区分 | `crates/agentdash-local/src/handlers/prompt.rs:303`; `crates/agentdash-local/src/handlers/prompt.rs:306` | Low |

## P0/P1/P2 backlog candidates

### P0

1. **收敛 AgentRun delivery runtime target resolver**
   - 问题：workspace query 和 API route context 各自实现 `delivery_runtime_session_for_agent_run`，选择策略都是 run+agent anchors 中 `updated_at` 最大值。该选择同时决定 workspace read projection、composer/cancel/mailbox write target、resource surface frame resolution。
   - 影响范围：`crates/agentdash-application/src/agent_run/workspace/query.rs`、`crates/agentdash-api/src/routes/lifecycle_agents.rs`、`AgentRunMailboxCommandTarget` 解析。
   - 建议 task scope：新增 application-level resolver，输入 run_id/agent_id/可选 expected frame/node coordinate，返回 delivery runtime、anchor、frame ref、selection reason；API 和 workspace query 都消费同一个 resolver。
   - 验收方向：删除 API route local duplicate resolver；workspace read 和 command route 使用同一选择函数；测试覆盖 multi runtime session、frame replacement、orchestration node attempt。
   - 证据：`crates/agentdash-application/src/agent_run/workspace/query.rs:314`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:825`。

2. **收紧 hook mailbox NotFound fallback，禁止 anchored AgentRun 绕过 durable mailbox**
   - 问题：`route_hook_delivery_messages` 只要 `accept_hook_steering_messages` 返回 `NotFound` 就把 hook messages 回退为 direct steering/follow_up。这个 fallback 没有区分“真正 unanchored runtime”与“anchored AgentRun anchor/repo 异常”。
   - 影响范围：hook `after_turn`、`before_stop`、mailbox envelope dedup、AgentRunTurnBoundary continuation。
   - 建议 task scope：让 fallback 先显式判定 runtime 是否 unbound trace；anchored target 的 NotFound 应进入 diagnostic/error，不应 direct continue；必要时增加 distinct error type。
   - 验收方向：anchored runtime anchor missing 时不会注入 direct messages；unanchored runtime 仍可保留 legacy direct path；hook delivery tests 覆盖两种分支。
   - 证据：`crates/agentdash-application/src/session/mailbox_delegate.rs:190`、`crates/agentdash-application/src/session/mailbox_delegate.rs:219`、`crates/agentdash-application/src/session/hub/hook_dispatch.rs:230`。

### P1

1. **拆分 command policy 的 snapshot resolver 依赖**
   - 问题：command policy 为校验 route precondition 重新构造 `AgentConversationSnapshot`，但输入不是 workspace query 的完整输入：subject associations 空、resource surface 空、model config 强制 resolved。当前用于 command stale/enablement 校验尚可，但未来 command 规则若依赖 model/resource/subject 会形成双事实源。
   - 影响范围：`AgentConversationSnapshotResolver`、`AgentRunWorkspaceCommandPolicyService`、composer/delete/promote/resume/cancel route。
   - 建议 task scope：提取 `ConversationCommandPolicySnapshot` 或 `ConversationCommandAvailabilityResolver`，只包含 command 规则所需字段；workspace snapshot 和 route policy 共用同一 command availability core，而非 policy 构造完整 UI snapshot。
   - 验收方向：policy 不再构造完整 `AgentConversationSnapshot`；stale guard 校验保持 run/agent/runtime/frame/active_turn/snapshot_id 一致；command availability 单元测试覆盖 cancelling/running/terminal/model-required。
   - 证据：`crates/agentdash-application/src/agent_run/workspace/command_policy.rs:134`、`crates/agentdash-application/src/agent_run/workspace/command_policy.rs:168`、`crates/agentdash-application/src/agent_run/workspace/command_policy.rs:474`。

2. **移除或封装 AgentRun direct steer service**
   - 问题：`AgentRunSteeringService` 仍提供 runtime-session-id direct steer，并从 anchor 反查 run/agent/frame 后直接调用 `SessionControlService::steer_session`。当前未发现 API/product route 调用，但它作为 exported application surface 容易被后续误用。
   - 影响范围：`crates/agentdash-application/src/agent_run/steering.rs`、`agent_run/mod.rs` export、session hub tests。
   - 建议 task scope：若仅测试需要，移入 test support 或标为 non-product internal；若仍有内部必要用途，改为写 mailbox envelope 或要求调用方提供 explicit test-only gate。
   - 验收方向：产品 crate path 无可直接 import 的 AgentRun direct steer service；所有 AgentRun workspace steer 都从 mailbox scheduler 进入；保留 connector/session-level `SessionControlService` 给 relay/session control。
   - 证据：`crates/agentdash-application/src/agent_run/steering.rs:61`、`crates/agentdash-application/src/agent_run/mod.rs:35`、`crates/agentdash-application/src/session/hub/tests.rs:937`。

3. **明确 cancel 的 command receipt owner 与 mailbox 非目标边界**
   - 问题：spec 已规定 cancel 不创建 mailbox envelope，但必须 claim durable receipt。当前 route 直接 claim receipt 后调用 `session_runtime.cancel`，是正确形态；但 receipt scope string 使用 `"agent_run_mailbox"`，容易让读者误以为 cancel 属于 mailbox envelope 队列。
   - 影响范围：cancel route、command receipt semantics、frontend command hook。
   - 建议 task scope：整理命名/文档/测试，明确 cancel 是 AgentRun runtime command receipt，不是 mailbox message；不改变行为。
   - 验收方向：cancel receipt scope/mapper/docs 与 mailbox message command 区分；duplicate/conflict tests 明确 cancel 不产生 mailbox row。
   - 证据：`crates/agentdash-api/src/routes/lifecycle_agents.rs:682`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:722`、`.trellis/spec/backend/session/agentrun-mailbox.md`。

### P2

1. **保留但瘦身 top-level `AgentRunWorkspaceView.control_plane`**
   - 问题：top-level `control_plane` 从 conversation execution status 派生，和 `conversation.execution` 有重复表达。当前它只提供 workspace shell status，不含 command action list，可作为 legacy/list UI convenience；但不应继续扩张。
   - 影响范围：API mapper、contracts、前端 workspace tests。
   - 建议 task scope：标注/测试它只做 display status；新增断言不从 top-level control_plane 读取 command enablement。
   - 验收方向：命令按钮只消费 `conversation.commands`；`control_plane` 仅用于粗粒度 workspace status。
   - 证据：`crates/agentdash-api/src/routes/lifecycle_agents.rs:1051`、`packages/app-web/src/generated/workflow-contracts.ts:133`。

2. **把 mailbox row action flag 保持为纯 projection**
   - 问题：`MailboxMessageView.can_promote/can_delete/can_reorder/can_recall` 由 API mapper 从 status/delivery/last_error 派生。它是 UI convenience，不应被 scheduler/command policy 当作事实源。
   - 影响范围：`agent_run_mailbox_contracts.rs`、`SessionStatusBar` mailbox row UI。
   - 建议 task scope：补充 mapper 测试覆盖 `delivery_result_unknown`、terminal rows、user/system origin；前端只用这些 flags 隐藏 UI，不跳过后端 policy。
   - 验收方向：后端 command endpoint 仍以 command policy + mailbox service 校验为准；UI flag stale 时服务端可拒绝。
   - 证据：`crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:88`。

3. **区分 AgentRun direct steer 与 local relay session steer 文档/命名**
   - 问题：local relay `command.steer` 也直接调 `SessionControlService::steer_session`，但它属于 relay/session boundary，不是 AgentRun workspace durable mailbox。两者如果在 review/backlog 中混写，会误判风险。
   - 影响范围：local relay docs、session control docs、follow-up backlog 命名。
   - 建议 task scope：只在架构文档中明确 relay/session steer 是 connector transport surface；AgentRun workspace steer 必须走 mailbox。
   - 验收方向：搜索 direct steer 时能清晰区分 `agent_run/steering.rs` 和 `agentdash-local` relay handler。
   - 证据：`crates/agentdash-local/src/handlers/prompt.rs:303`、`crates/agentdash-application/src/session/control.rs:33`。

## 不重复项

- 不重复论证 mailbox 是否应该存在。spec 和第一轮研究已经确认 AgentRun mailbox 是 durable message intake、scheduler、recovery projection 的事实源；本轮只验证是否仍有绕过 durable mailbox 的路径。
- 不重复把 frontend runtime command adapter 作为高风险事实源。`buildRuntimeSessionCommandState` runtime 模式直接返回 `conversation.commands`，`useAgentRunWorkspaceCommands` 只打包 precondition/stale guard 和调用 AgentRun command endpoint；这是必要 UI/transport adapter，证据见 `packages/app-web/src/pages/AgentRunWorkspacePage.conversationCommandState.ts:231`、`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:96`。
- 不重复把 draft start 的本地 command 构造当作运行态 AgentRun command/control 分裂。`buildDraftSessionCommandState` 只覆盖 `/agent-runs/new` 的 ProjectAgent draft creation，运行态 AgentRun workspace 仍消费 backend conversation snapshot；证据见 `packages/app-web/src/pages/AgentRunWorkspacePage.conversationCommandState.ts:162`。
- 不重复把 RuntimeSession runtime-control 作为第二 command surface。当前 contract 和 frontend 均显示该入口只读 backlink；若未来出现 action 字段或 frontend command 调用，再重新升级风险。
- 不重复 review `SessionControlService::steer_session` 本身。它是底层 connector/session capability；问题只在 AgentRun workspace 是否绕过 mailbox 调用它。
- 不重复 06-14 已覆盖的 VFS/Permission/Capability/Extension 双事实源问题；本文件只记录 AgentRun command/control 直接相邻的 resource surface 和 runtime-control 边界。

## Code Patterns

- Durable mailbox before delivery: `composer-submit -> command receipt -> mailbox envelope -> scheduler outcome`，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:428`、`crates/agentdash-application/src/agent_run/mailbox.rs:325`、`crates/agentdash-application/src/agent_run/mailbox.rs:355`。
- Command snapshot as UI source: `AgentConversationSnapshot` 包含 `commands` 与 `mailbox`，见 `crates/agentdash-contracts/src/runtime/workflow.rs:1081`。
- Command precondition as stale guard: frontend 从 `ConversationCommandView` 提取 `command_id/kind/stale_guard`，后端按 run/agent/runtime/frame/turn/snapshot 校验，见 `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:96`、`crates/agentdash-application/src/agent_run/workspace/command_policy.rs:310`。
- Runtime-control read-only backlink: `SessionRuntimeControlView` 不含 mailbox/actions，见 `crates/agentdash-contracts/src/runtime/workflow.rs:1328`。
- NotFound direct fallback: hook delivery 写 mailbox 失败为 NotFound 时恢复 direct messages，见 `crates/agentdash-application/src/session/mailbox_delegate.rs:219`。

## External References

- None. 本研究只使用仓库内 task artifacts、Trellis specs 和代码。

## Related Specs

- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/streaming-protocol.md`
- `.trellis/spec/cross-layer/backbone-protocol.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/frontend/type-safety.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)` / `Source: none`；本文件按用户显式给出的 task path 写入。
- 未运行测试、未启动 `pnpm dev`、未修改业务代码、未执行 git 操作。
- 未发现 `AgentRunSteeringService` 被 API route 或 app-web 产品路径调用；当前发现的使用点是 export 和 session hub tests。
- 未全面证明多 runtime session / replacement frame 场景存在真实 bug；P0 resolver backlog 来自重复选择策略和缺少显式 coordinate contract 的架构风险。
- `RuntimeSession` resource surface 仍可通过 `resolveVfsSurface({ source_type: "session_runtime" })` 读取；这属于 WorkspacePanel/resource surface 边界，不是本轮 command/control 控制面。
