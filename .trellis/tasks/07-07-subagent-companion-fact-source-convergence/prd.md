# 收束 SubAgent 与 Companion 失败事实源

## Goal

从第一性收束 SubAgent / Companion / wait / mailbox / AgentRun list 的事实源和投影链路。修复后，子 Agent 失败、缺少 `companion_respond`、gate terminal fallback、parent mailbox wake、wait tool 返回和前端 AgentRun 列表刷新必须围绕同一组权威事实工作：`LifecycleGate` 持有等待/结果状态，mailbox 只表达后续投递 envelope，wait 只观察活动状态，前端列表只消费后端 AgentRun list projection 与明确失效事件。

## Background

用户在一次 SubAgent 被 provider model 400 fatal error 阻断后观察到三个问题：

- 前端 AgentRun 列表不自动刷新 SubAgent，需要手动强制刷新。
- 主 Agent 的 `wait` 成功等到活动终态，但拿到的内容只有 `[subagent:failed] Producer reached terminal before the expected result was written.`，缺少实际可诊断的 provider error。
- mailbox 中又出现 `Companion child result is available... status: failed... summary: Producer reached terminal...` 这类消息；它与 companion request 等待返回和 wait 结果重复，而且以用户输入/continuation 形态进入会话，而不是清晰的系统投影。

## Confirmed Evidence

- `ControlPlaneProjection::AgentRunList` 已在协议中定义，见 `crates/agentdash-agent-protocol/src/backbone/platform.rs:74`，但仓库内没有生产代码发出 `ControlPlaneProjection::AgentRunList` 事件。
- mailbox runtime adapter 只发 `ControlPlaneProjection::Mailbox`，见 `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:221` 和 `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:237`。
- 前端 AgentRun list store 只订阅 Project `StateChanged`，见 `packages/app-web/src/features/agent/agent-run-list-state-store.ts:283` 和 `packages/app-web/src/features/agent/agent-run-list-state-store.ts:324`。
- workspace 页面能把 `control_plane_projection_changed` 转成列表刷新计划，见 `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:123`，但这只在当前 workspace stream 已打开时生效。
- provider terminal fallback 会把缺少 expected result 的 gate resolve 成 `summary = "Producer reached terminal before the expected result was written."`，见 `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:211` 和 `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:224`。
- fallback outcome 继续产生 `GateDeliveryIntent::MailboxWake`，见 `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:145` 和 `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:167`。
- application adapter 会立即执行 mailbox wake delivery，见 `crates/agentdash-application/src/gate_wait_policy.rs:91`。
- mailbox wake 被渲染为 `build_parent_result_mailbox_input_text`，见 `crates/agentdash-application/src/gate_wait_policy.rs:211` 和 `crates/agentdash-application/src/companion/gate_control.rs:1136`。
- companion mailbox delivery 以 `origin: MailboxMessageOrigin::Companion` 写入 mailbox，但内容是 `text_user_input_blocks(input.input_text)`，见 `crates/agentdash-application/src/companion/tools.rs:519`、`crates/agentdash-application/src/companion/tools.rs:523` 和 `crates/agentdash-application/src/companion/tools.rs:527`。
- wait 工具从同一个 resolved gate 投影 `WaitActivityItem`，并在 tool details 中返回 `items`，见 `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:10` 和 `crates/agentdash-application/src/wait_activity/tool.rs:52`。
- 当前 Codex 原生 subagent completion notification 可以以 `<subagent_notification>...</subagent_notification>` 的用户消息形态进入主会话 transcript。这与 companion mailbox wake 的问题同构：系统/子 Agent 通知被主 Agent 当成 human input，而不是 system/subagent-origin delivery projection。

## Research Consolidation

本任务已收到三条并行 research 结果，后续实现必须以这些结论为准。

### AgentRun list invalidation

- AgentRun list 刷新应来自 project-scoped projection invalidation，不应依赖某个 AgentRun workspace stream 已经打开。
- 同一个机制不应只为 AgentRun list 特化成长期专用通知链路。更干净的模型是通用 Project projection notification，`AgentRunList` 只是其中一个 projection discriminant。
- 必须覆盖 same-run SubAgent lineage、cross-run fork/new-root、delivery running/terminal、workspace title，以及 `LifecycleRun.last_activity_at` 影响 root row 排序/最近活动的变化。
- 当前列表实现已覆盖 same-run lineage、delivery running/terminal、title，并由独立 check 补了 cross-run fork/new-root；仍缺 run-level activity producer。

### Runtime diagnostic propagation

- Provider fatal diagnostic 已经在 connector/agent-loop 边界存在结构化来源，例如 `BridgeError::Provider` 和 `ProviderErrorClassification`。
- 诊断在 Backbone `ErrorNotification`、RuntimeSession terminal evidence、AgentRun control effects、delivery binding、boot reconciliation、`GateProducerTerminalEvent` 和 gate fallback payload 等边界被降级为 `terminal_message` 或 `additional_details` 文本。
- 本任务必须新增 bounded typed runtime terminal diagnostic，并沿 terminal/gate/wait/mailbox/system projection 链路作为结构化数据传播；禁止从日志或自由文本里反向解析 provider 诊断。

### Child evidence locator

- SubAgent result refs 必须暴露 parent-visible child evidence locator，不能把 child 自己视图下的 `lifecycle://session/...` 当成 parent 可读 URI。
- locator 应携带 child run/agent/frame、exact delivery runtime session、evidence kind 和 mount-relative path。读取时由 AgentRun journal、VFS surface resolver 或 runtime trace resolver 解析。
- `ResolvedVfsSurfaceSource::AgentRun` 会跟随当前 delivery 漂移；具体结果证据优先使用 exact `SessionRuntime { session_id }` 或校验 current delivery 仍匹配 locator runtime session。

## First Principles

- 等待是否完成是一条业务事实，权威 owner 是 `LifecycleGate`。
- 子 Agent runtime 是否 terminal 是 delivery evidence，进入 gate terminal convergence 后只能用于解析等待结果，不能成为第二条业务结果事实。
- mailbox 的价值是可恢复投递，不是结果事实的 owner；它应该携带 source identity、payload refs 和 delivery status，而不是把结构化结果降级成自由文本事实源。
- wait tool 是 watcher；它可以返回状态摘要和 refs，但不能承担结果正文传输，也不能迫使 Agent 在多个同义返回里猜哪个更权威。
- AgentRun list 是后端 projection；列表刷新需要后端 projection invalidation，不能依赖当前 workspace 页面是否打开。
- provider 400 fatal error 是诊断事实；terminal fallback summary 可以描述协议层失败，但必须保留 provider diagnostic ref / bounded detail，让主 Agent 能知道真实阻断原因。
- companion/system wake 的模型上下文语义纳入本任务范围；它必须是结构化 delivery envelope 派生出的系统/companion continuation，而不是伪装成普通 human user input 的事实源。

## Requirements

1. AgentRun list 在 SubAgent lineage、child count、child shell status、delivery terminal 或 title/activity 变化后必须自动刷新。
2. 后端必须发出或等价提供 AgentRun list projection invalidation；前端列表 store 不能只依赖 Project `StateChanged`。
3. SubAgent failed terminal fallback 必须把 provider/runtime diagnostic 以 bounded structured form 关联到 gate result 或 result refs。
4. `wait` 对 SubAgent/Companion gate 的返回必须指向权威 gate result，并给出足够诊断摘要；它不能只返回缺少 expected result 的泛化文案。
5. `companion request wait=true`、通用 `wait` tool、parent mailbox wake、workspace waiting projection 必须有清晰的职责分工，避免同一结果以多条同义消息让 Agent 决策。
6. parent mailbox wake 仍可作为唤醒/继续 parent agent 的投递机制，但它必须保留结构化 source/payload/ref 语义；UI 和模型上下文不能把它误认为普通用户输入。
7. mailbox row / conversation feed 必须能区分 human user input、companion/system wake、diagnostic/system projection。
8. terminal fallback 与 normal `companion_respond` 竞争时保持 first-writer-wins；重复 terminal replay 只确保同一 delivery，不产生重复 mailbox message 或重复模型上下文输入。
9. 文案与 DTO 命名应反映事实边界：gate result、mailbox wake、wait observation、AgentRun list invalidation，避免 companion request / wait / mailbox 三者互相包装。
10. 当前项目未上线，相关 DTO、数据库和 migration 可以直接收正模型，不引入长期兼容分支。
11. Agent-facing continuation 必须由 mailbox envelope 的 source / payload / result refs 生成，且在 Backbone / feed / model context 中保留 `companion/system` 来源身份；普通用户输入只来自 human composer 或明确 human-origin mailbox envelope。
12. blocking waiter 与 async mailbox continuation 必须通过 durable delivery state 互斥收敛。waiter 消失、超时或进程重启后，新 wait 仍能从已 resolved `LifecycleGate` 打捞结果；必要时 mailbox continuation 可从同一 gate result 补偿投递，不能丢消息。
13. SubAgent result refs 必须包含可追溯 child journal / lifecycle evidence locator，让主 Agent 能定位 child session 的 evidence。该 locator 只能表达 child AgentRun refs、child delivery runtime refs、evidence kind 与相对位置；不能把 child 自身 lifecycle mount 内部路径误当成 parent 视图下可直接读取的绝对 URI。
14. 本任务产出的 source identity、delivery envelope、result refs 和 projection discriminant 必须为后续 channel 系统迁移保持干净边界：当前不实现完整 channel 系统，但不能继续依赖自由文本消息承担跨来源消息建模。
15. durable delivery convergence marker 可以独立于 mailbox 存在，但必须保持薄模型：只表达 `gate_id + result_attempt` 的交付状态、claim/replay 所需字段和目标引用，不复制 mailbox scheduling、payload storage、conversation rendering 或 channel routing 职责。
16. 错误路径清理必须纳入验收：旧的自由文本 companion result wake、重复 mailbox/companion/wait 同义投递、child-local lifecycle URI 误暴露、mailbox 承载非 mailbox 状态、以及仍把 provider/runtime fatal 退化成 generic missing-result summary 的路径都必须移除或收束到新事实模型。
17. Codex 原生 subagent / companion / system notification 必须进入 system/subagent-origin 投影或工具等待结果，不得以 `<subagent_notification>` 等 human user message 形态注入主会话 transcript。系统投递可以被 UI 展示、被主 Agent 观察、被 future channel 系统迁移，但不能伪装成用户输入。

## Acceptance Criteria

- [ ] SubAgent 创建、失败、完成和 child shell activity 变化后，AgentRun list 无需手动刷新即可出现正确 child / subagent count / terminal status。
- [ ] 代码中存在明确的 AgentRun list projection invalidation 生产路径，且前端列表 store 能消费该路径。
- [ ] provider model 400 fatal failure 通过 gate result 或 diagnostic refs 保留 `kind/code/http_status/provider/model/message` 的 bounded 信息。
- [ ] `wait(activity_refs=[gate_id])` 返回的 failed item 能让主 Agent 识别真实 provider/runtime failure，而不只看到 generic missing-result summary。
- [ ] parent mailbox wake 对同一 gate/result 重放时幂等，不重复产生 mailbox rows、command receipts 或 parent model continuation。
- [ ] companion/system wake 在会话展示中不以 human user input 身份出现；模型续跑需要消费时也带有结构化来源和 bounded payload。
- [ ] companion/system wake 注入模型上下文时有稳定的来源 discriminant、gate/result refs 和 bounded diagnostic，不再只依赖 `text_user_input_blocks(input_text)` 表达语义。
- [ ] workspace waiting projection、wait tool result、mailbox message row 对同一 gate 的 status/summary/ref 一致。
- [ ] normal `companion_respond` 先到时，terminal fallback 只 ensure delivery；terminal fallback 先到时，后续 normal result 不覆盖已有 terminal-derived result。
- [ ] SubAgent result 与 wait result 包含 child journal / lifecycle evidence locator，主 Agent 可通过 refs 追溯 child 运行日志和失败诊断；具体 parent 视图下解析方式由实现阶段调研确认。
- [ ] waiter 结束与 result resolve 同时发生时，系统不会既丢失结果也重复投递；后续新 wait 直接返回 resolved gate result。
- [ ] companion/system delivery envelope 不把未来 channel 语义固化为自由文本；字段保留来源、目标、correlation、payload refs 与 delivery state，便于后续 channel 系统接管。
- [ ] 独立 delivery convergence marker 保持薄边界；mailbox row/status 不承载 `delivered_to_waiter` 这类非 mailbox 投递状态，也不复制 gate result payload。
- [ ] 错误路径被清理：同一 SubAgent failure 不再同时以 wait result、companion request result、mailbox user-like text 三条事实正文进入 Agent 决策；旧自由文本 wake 只作为有来源的 bounded projection 存在。
- [ ] provider/runtime fatal 不再落成单一 generic summary；gate result、wait result 和 mailbox projection 都能引用同一 bounded diagnostic。
- [ ] result refs 不再输出 parent 视图下不可解析的 child-local lifecycle URI；child evidence 只通过经调研确认的 locator 合同暴露。
- [ ] 相关 Rust unit/integration tests、frontend store/model tests、TypeScript generated contract check 按实际改动范围通过。
- [ ] Codex 原生 subagent completion/failure notification 不再以用户消息出现在主会话；它必须被归类为 system/subagent-origin event、wait result 或 bounded delivery projection。

## Out of Scope

- 不重做完整 companion protocol。
- 不在本任务实现完整 channel 系统；本任务只收束现有 companion/subagent/wait/mailbox 链路，为后续 channel 化迁移保留干净数据边界。
- 不引入新的独立 wait result table，除非设计阶段证明 `LifecycleGate` 无法承载诊断和 result refs。
- 不保留旧自由文本 wake 作为长期兼容路径；当前项目未上线，错误路径应直接收正，不做兼容分支。
