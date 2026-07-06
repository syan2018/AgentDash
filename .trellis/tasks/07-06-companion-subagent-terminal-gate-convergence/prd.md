# 修复 SubAgent terminal gate 收束

## Goal

当 companion/SubAgent 在启动或执行阶段进入 terminal failed/interrupted，或者 runtime completed 但没有产生 `companion_respond` 时，系统必须把已经创建的 durable wait obligation 收束为 failed/cancelled/protocol_failed 结果。父 Agent、`wait` 工具、workspace projection 和启动 reconcile 都应看到同一个事实：等待的 producer 已经终止，期待的结果不可能再正常出现，而不是继续停留在 pending gate。

## User Value

- 父 Agent 不会因为子 Agent 启动失败而长期拿到 `[subagent:pending]`。
- 用户能在运行视图里看到子 Agent 的真实失败原因，例如账号不支持某个模型，而不是只看到未完成等待项。
- 系统重启后能自动修复历史残留的 zombie companion gate，不需要人工 SQL 介入。
- 后续 provider、网络、权限、connector crash 等运行失败都能走同一条收束链路。

## Confirmed Facts

### Runtime Evidence

- API 与 embedded Postgres 正常运行，当前迁移已到 `52 drop session title columns`。
- 事故运行属于同一个 `LifecycleRun`：`0d9feb94-2d26-4be2-9cbb-296430d05d05`。
- parent agent `986873d8-3e6b-4376-b5f9-fdade68c4ca4` / parent session `644ad3ff-5dbe-45f1-adc2-7c234d6d9168` 最终 `completed`。
- child agent `0b112743-adf7-4434-8bcf-67830db6e3e0` / child session `bf4147ce-6591-4240-b194-9f38fc54d334` 最终 `failed`。
- child 失败原因是 runtime 返回 `400 Bad Request`：`gpt-5.3-codex` 不支持当前 ChatGPT account。
- child `runtime_sessions.last_delivery_status = failed`，`agent_run_delivery_bindings.status = terminal` 且 `terminal_state = failed`，child mailbox 已暂停为 `turn_failed`。
- 残留等待项是 `gate_id = 9ba12cfd-c348-49d7-af3e-9eafb85d4c6a`，`gate_kind = companion_wait_follow_up`，`status = open`，`correlation_id = dispatch-1a19f37303844b63a5b8f5ec271b7e56`。
- parent 的 `companion_request(wait=true)` 等待 300 秒后 timeout；后续 `wait(activity_refs=[gate])` 继续返回 `[subagent:pending]`。

### Code Evidence

- `crates/agentdash-application/src/companion/dispatch.rs:65` 在 `wait=true` 时打开 interaction gate，`crates/agentdash-application/src/companion/dispatch.rs:230` 将 follow-up gate kind 标记为 `companion_wait_follow_up`。
- `crates/agentdash-application/src/companion/gate_control.rs:518` 和 `crates/agentdash-application-workflow/src/gate/resolver.rs:366` 是正常 child result 收束路径，前提是 child 主动调用 `companion_respond`。
- `crates/agentdash-application-workflow/src/gate/resolver.rs:470` 只接受 `completed / blocked / needs_follow_up`，没有表达 runtime failure 的状态。
- `crates/agentdash-api/src/agent_run_terminal_control.rs:124` 的 terminal callback 当前只同步 delivery state，并在 `failed/interrupted` 时暂停 child mailbox；它没有解析 companion lineage，也没有 resolve child-owned gate。
- `crates/agentdash-application-agentrun/src/agent_run/delivery_state.rs:23` 负责把 runtime terminal 同步到 `AgentRunDeliveryBinding`。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/controls.rs:437` 负责 terminal 后暂停 mailbox。
- `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:41` 将 open gate 映射为 `pending`，`crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:42` 将所有 resolved gate 映射为 `completed`。
- `crates/agentdash-application/src/wait_activity/service.rs:327` 的 gate wait 只观察 `LifecycleGate` 是否 resolved。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:196` 通过 `list_open_for_agent` 投影 workspace waiting items。
- `crates/agentdash-application/src/reconcile/boot.rs:67` 的 boot reconcile 当前只有 session recovery、task view projection 和 infrastructure placeholder，没有 companion gate terminal reconcile。

## First Principles

系统里有五类事实必须最终一致：

- Runtime fact：`RuntimeSession` 事件和 terminal state，表示执行器实际发生了什么。
- Delivery fact：`AgentRunDeliveryBinding`，表示 AgentRun 当前 runtime delivery 的终态。
- Wait obligation fact：`LifecycleGate` 或后续 wait record，表示某个 agent 正在等待哪个 producer 产生什么 durable 结果，以及 producer terminal 时如何收束。
- Wake/result fact：parent mailbox message，表示结果已经投递给父 agent，父 agent 可以继续处理。
- Read projection fact：`wait` / workspace waiting item 只观察 wait obligation 和 wake/result，不发明写侧事实。

这次问题的最小闭环是：

```text
child RuntimeSession terminal failed/interrupted
-> child AgentRunDeliveryBinding terminal
-> wait obligation convergence observes producer terminal
-> pending LifecycleGate resolved with failed/cancelled/protocol_failed result
-> parent mailbox receives companion-result wake
-> wait/workspace no longer projects pending
```

当前代码停在第二步，因此 `wait` 和 workspace 作为观察面会持续看到 open gate。

## Requirements

- R1. 新增 AgentRun-owned terminal convergence 能力：runtime terminal signal 进入 AgentRun seam 后，先收敛为 `AgentRunDeliveryBinding` terminal fact 和 AgentRun 坐标事件；wait obligation 收束只能消费 AgentRun run/agent/frame/delivery 或 runtime node 语义，不能在 companion/API/workspace 层直接解析 `RuntimeSessionExecutionAnchor`。
- R2. 新增 wait obligation terminal convergence 能力：open `LifecycleGate` 必须能声明等待的 producer、期望 result、producer terminal 后的收束策略和 wake delivery intent。convergence 接收 producer terminal event，内部定位受影响 gate，按策略 resolve，并确保 wake delivery。
- R3. terminal 收束必须投递 parent mailbox wake：父 Agent 应收到稳定、幂等的 `companion-result:{gate_id}` 消息，内容包含子任务终态和可读摘要。
- R4. wait obligation result status contract 必须能表达 runtime failure：至少支持 `failed`，并为 interrupted/cancelled 建立一个明确状态；completed-without-result 必须能表达为协议失败；保留 `terminal_state` 字段承载底层 runtime state。
- R5. `wait` 对 resolved gate 的状态投影必须读取 payload 中的真实 status；失败/取消的 wait result 应作为 ready item 返回，而不是 pending 或伪 completed。
- R6. workspace projection 必须与 durable gate 状态一致；producer delivery 已 terminal 且 gate 已 resolved 后，不应继续显示 open waiting item。
- R7. parent mailbox delivery 必须可幂等重放；如果 gate 已 resolved 但 parent mailbox 投递失败，重试应能根据同一个 gate/request 再次确保 delivery，而不是丢失 wake。
- R8. boot reconcile 必须扫描带 producer terminal policy 的 open wait obligations，并用同一套 wait obligation convergence 修复已经 terminal 的 child delivery/runtime；reconcile 不应按 `companion_wait_follow_up` 这类 gate kind 复制业务规则。
- R9. SubAgent launch 前应增加 provider/account effective model capability preflight；不可执行模型应在创建长期等待之前变成可见配置错误，或被立即收束为 dispatch failure。
- R10. 修复应优先使用现有 repository、gate resolver、parent mailbox delivery 和 wait activity 模块，避免把 `wait` 变成写事实的 authority。
- R11. 如选择引入新持久化结构，例如 wait obligation delivery outbox 或 typed producer policy columns，必须按当前迁移体系新增 migration；若只扩展 JSON payload 和应用逻辑，需确认无需 schema migration。
- R12. 收口 `LifecycleGate` waiting projection 的状态解释：`wait` 工具和 workspace mailbox waiting_items 应对同一个 gate 产出一致的 kind/status/preview 语义；resolved gate 优先读取 payload.status，open gate 仍表示 pending/open，exec terminal registry 仍保持独立非 gate 来源。
- R13. containment 约束：`RuntimeSessionExecutionAnchor` 只作为 AgentRun/runtime trace 绑定的不可变 launch evidence；本任务新增代码不得把 anchor repository 泄露给 wait obligation convergence、API terminal callback 业务分支或 workspace projection。既有直接依赖若被本次路径触碰，应改为调用 AgentRun runtime binding/terminal convergence interface。
- R14. 当前修复必须建立 AgentRun-owned runtime address/convergence seam 和 wait obligation convergence seam：terminal、gate resolved、parent mailbox wake 和 waiting projection 使用 AgentRun 坐标、wait producer ref 或 `delivery_trace_ref` 诊断字段传递上下文；仓内其它直接 anchor 读取点作为后续收束候选记录，但本任务不再新增同类事实源。

## Acceptance Criteria

- [ ] child runtime `failed` 后，AgentRun terminal convergence 先写入 child `AgentRunDeliveryBinding` terminal，再由 wait obligation convergence 根据 producer ref 收束相关 open gate；payload.status 为 `failed`，payload 包含 terminal message、delivery trace ref、turn id 和 `source = "producer_terminal"`。
- [ ] child runtime `interrupted` 后，相关 wait obligation 被 resolved 为取消/中断语义，parent mailbox 收到 companion result wake。
- [ ] child runtime `completed` 但未调用 `companion_respond` 时，相关 wait obligation 被 resolved 为协议失败，failure kind 能区分 `missing_companion_respond`。
- [ ] `companion_respond` 与 terminal callback 竞态时，先完成者保留 gate payload，后完成者只做幂等 no-op 或 ensure delivery，不覆盖已确定结果。
- [ ] gate 已 resolved 但 parent mailbox delivery 初次失败时，后续调用能用 `companion-result:{gate_id}` 重新确保投递。
- [ ] `wait(activity_refs=[gate])` 对 failed/cancelled companion result 返回 ready item，状态反映 payload.status。
- [ ] workspace projection 中 child delivery terminal 后不再展示同一个 gate 为 open waiting item；如展示 resolved 历史项，状态语义与 `wait` 工具一致。
- [ ] `LifecycleGate` 到 `WaitActivityItem` 与 `ConversationWaitingItemModel` 的状态解释共享同一套 helper 或同一套被测试锁定的 contract，避免 `resolved -> completed` 与 `resolved -> payload.status` 分叉。
- [ ] API terminal callback 不再自己查询 `RuntimeSessionExecutionAnchor` 来完成业务控制面逻辑；它只把 terminal notification 交给 AgentRun terminal convergence interface。
- [ ] wait obligation convergence 的外部 interface 不接受裸 `runtime_session_id` 或 gate kind 作为业务定位输入；runtime session id 只允许作为 diagnostic / delivery trace ref 出现在 payload 或日志中。
- [ ] 本任务触碰的 terminal、gate resolved、workspace waiting projection 路径通过代码审查确认：业务决策从 `AgentRunDeliveryBinding` / AgentRun event / wait producer ref / `LifecycleGate` 出发，`RuntimeSessionExecutionAnchor` 只出现在 AgentRun seam 内部校验或 trace 诊断语境。
- [ ] boot reconcile 能修复现有形态：open companion wait gate + child delivery terminal failed/interrupted/completed-without-response，并且实现入口是通用 wait obligation convergence。
- [ ] 不可执行模型配置在 SubAgent launch 前返回可见错误，或被即时收束为 dispatch failed；runtime provider 400 仍能通过 wait obligation convergence 收束 gate。
- [ ] 单元/集成测试覆盖 AgentRun terminal convergence、wait obligation convergence、wait projection、idempotent delivery、boot reconcile 和 model preflight 的关键路径。
- [ ] 如涉及 Rust/TS DTO 或 contract 生成，`pnpm run contracts:check` 通过；如涉及 migration，`pnpm run migration:guard` 通过。

## Out of Scope

- 人工 SQL 清理不作为交付结果，因为它不能修复未来 terminal failure 产生的新 zombie gate。
- 全面重写 companion 协议不属于本任务；本任务只补齐 child terminal 到 companion result 的权威收束链路。
- 前端视觉重做不属于本任务；UI 只需要能消费正确状态和错误摘要。
- 不在本任务内把 exec terminal registry waiting item 改造成 LifecycleGate；它与 companion gate 共用 UI waiting_items 只是展示聚合，不是同一个事实源。
- 不要求本任务一次性移除全仓所有 `RuntimeSessionExecutionAnchorRepository` 依赖；但本任务触碰的 terminal、companion、wait/workspace 路径必须朝 AgentRun-owned seam 收束，避免新增泄漏。

## Open Questions

当前没有阻塞规划的问题。设计阶段建议把用户可见 interrupted 状态命名为 `cancelled`，同时保留底层 `terminal_state = "interrupted"`。
