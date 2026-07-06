# Agent 生命周期事实源架构审查结论

## 审查范围

本轮审查覆盖后端 Lifecycle / RuntimeSession / AgentRun / Mailbox / Gate / WaitActivity / WaitObligation，跨层 Backbone / AgentRun journal / generated contracts，以及前端 AgentRun workspace、session stream、companion/waiting/terminal 投影。审查过程使用三个只读 subagent 分别调研 backend、frontend、cross-layer，并由主线程复核关键文件。

## 结论先行

项目当前不是缺少事实源，而是事实源和投影边界过多，且若干特解服务被命名成通用机制，导致后续扩展时容易继续复制生命周期逻辑。

当前相对稳固的事实源应归位为：

| 层级 | 权威事实 | 主要职责 |
| --- | --- | --- |
| Lifecycle control plane | `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`OrchestrationInstance`、`RuntimeNodeState` | 业务生命周期、Agent 身份、frame surface、workflow progress |
| Runtime trace | `RuntimeSession` event store、`BackboneEnvelope` | connector trace、turn/tool/event、terminal evidence、debug replay |
| Delivery binding | `AgentRunDeliveryBinding` + `RuntimeSessionExecutionAnchor` | AgentRun 当前 delivery、running/terminal/lost、runtime trace 反查控制面坐标 |
| Durable continuation | `AgentRunMailboxMessage`、mailbox state、command receipt | user/system/hook/companion wake、steer、resume、queued work |
| Wait fact | `LifecycleGate` / 后续 typed wait record | open/resolved wait、review/human/companion/workflow gate、waiting projection |
| Observation tool | `WaitActivityService` | 面向 agent 的 wait 工具观察 exec/gate/mailbox，不拥有 durable 状态 |
| Frontend projection | `AgentRunWorkspaceView`、AgentRun journal、Lifecycle views | 展示和刷新，不产生业务生命周期事实 |

`wait_obligation` 的方向基本正确：它不应是独立事实源，而应是 `LifecycleGate` 上的 producer terminal convergence policy。现有实现也基本按这个方向运行：producer terminal 来自 `AgentRunDeliveryBinding`，gate open/resolved 来自 `LifecycleGate`，parent wake 进入 `AgentRunMailbox`。问题在于它当前以独立 module 命名，并且 delivery intent 仍强 companion-specific，容易在未来成为第二套等待状态机。

## 关键证据

- `crates/agentdash-domain/src/workflow/wait_obligation.rs` 定义 `WaitProducerRef::AgentRunDelivery` 与 `WaitObligationDeclaration`，并通过 `write_into_payload` 写回 `LifecycleGate.payload_json`。
- `crates/agentdash-application-workflow/src/gate/wait_obligation.rs` 通过 `LifecycleGateRepository::list_by_wait_producer` 查 gate，再调用 gate resolver 完成 open gate 收敛。
- `crates/agentdash-api/src/agent_run_terminal_control.rs` 将 RuntimeSession terminal 先收敛为 `AgentRunDeliveryTerminalEvent`，再映射为 `WaitProducerTerminalEvent`。
- `crates/agentdash-application-agentrun/src/agent_run/delivery_state.rs` 用 `RuntimeSessionExecutionAnchor` 反查 run/agent，并只允许 current binding 对应 runtime session 写 terminal。
- `crates/agentdash-application-runtime-session/src/session/turn_processor.rs` 在 terminal event 持久化后、broadcast 前执行 control-plane terminal callback。
- `crates/agentdash-application/src/wait_activity/` 只观察 exec/gate/mailbox，未 claim/drain mailbox，也未写 lifecycle state。
- `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts` 将 companion/mailbox/platform event 转为 workspace refresh plan，前端没有直接把事件写成业务状态。
- `packages/app-web/src/features/agent-run-workspace/model/conversationCommandState.ts` 将 `mailbox.waiting_items` 从后端 snapshot 传给 chat shell。

## 主要设计问题

### P1: `wait_obligation` 的通用命名和 companion-specific 实现不匹配

`wait_obligation` 当前没有自己的表或 aggregate，实际是 gate payload 中的声明式策略。但 `companion_agent_run_delivery`、`expected_result.kind = "companion_result"`、`CompanionChildResultToParent` 等命名和 intent 让它仍像 companion 结束补丁。长期扩展 human、exec、workflow、subagent wait 时，如果继续在这个 service 内堆分支，会变成新的特解中枢。

建议将概念收束为 `LifecycleGateProducerTerminalPolicy` 或 `GateTerminalConvergence`：它只负责把 producer terminal evidence 应用到 gate。delivery intent 应变成通用 `MailboxWakeIntent` / `GateWakeIntent`，companion 只是其中一种 source identity。

### P1: terminal callback 是多个控制面副作用的串行扇出点

RuntimeSession terminal 是正确的输入信号，但现在 callback 串联 AgentRun terminal convergence、wait obligation convergence、Lifecycle terminal adapter，并通过 composite callback/outbox 做失败重试。这个结构能工作，但失败隔离粒度粗，容易出现 AgentRun binding 已 terminal、gate 未 resolved、workflow node 未推进的短暂或长期分叉。

建议长期改成持久 control-plane event/outbox 链：`RuntimeTerminalObserved -> AgentRunDeliveryConverged -> WaitProducerTerminalObserved -> LifecycleNodeTerminalObserved -> MailboxWakeRequested`。每一步有独立幂等 key、checkpoint 和 retry 状态。

### P2: 稳定信道存在，但 invalidation/event 类型仍偏场景化

Backbone、AgentRun journal、Mailbox、Lifecycle snapshot 已有清晰位置。问题是前端 refresh 仍识别多个 companion-specific event type，Backbone 中 mailbox 只是 `MailboxStateChanged { reason }`，companion 还通过 `session_meta_update.key` 这类自由 key 表达。

建议引入更通用的投影失效事件，例如 `workspace_projection_changed`、`waiting_item_changed`、`mailbox_state_changed`、`delivery_binding_changed`。前端继续只做 refresh，不从 reason 或 companion key 推导业务状态。

### P2: `LifecycleGate.payload_json` 承载太多 typed contract 压力

当前 gate payload 同时承载 companion metadata、wait obligation declaration、resolved result payload、waiting projection preview。作为预研阶段可以接受，但如果它继续承担 wait policy schema、producer identity、wake target 和结果状态，查询和约束都会弱化。

建议保留 `LifecycleGate` 作为 wait fact owner，但将 wait declaration 提升为强类型 gate 子结构或独立 wait declaration 表；状态仍从属于 gate，不另建生命周期事实源。

### P2: terminal registry 和 exec waiting projection 是 live cache，不应升级为 durable fact

`AgentRunTerminalRegistry` 明确是内存 registry，适合 exec terminal live observation。`WaitActivity` 读取它可以成立，但 API route 层拼接 exec waiting item 会让 waiting projection 出现另一条来源。

建议将 exec/activity waiting rows 也归入统一 activity projection，让 workspace waiting items 和 wait tool 消费同一 `LifecycleGateWaitingProjection` / typed activity projection。

### P2: 前端有多组运行状态字段和本地映射

前端同时消费 `shell.delivery_status`、`control_plane.status`、`conversation.execution.status`、stream turn segment status、terminal store state。多数只是投影，但 local status 白名单和 fallback 可能把新状态错误显示成 idle 或 ready。

建议用户可见 command availability、cancel、waiting、running helper 只从 `AgentRunWorkspaceView.conversation.execution + commands + mailbox.waiting_items + shell` 读取；stream 的 `isReceiving`、turn segment、provider waiting 只作为局部渲染和 trace UI。

## 收束模型

长期模型应保持两条主线：

1. control-plane facts：`LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`RuntimeSessionExecutionAnchor`、`AgentRunDeliveryBinding`、`LifecycleGate`、`AgentRunMailbox`。
2. observable stream：`RuntimeSession` + `BackboneEnvelope`，再通过 AgentRun journal 投影到产品 UI。

两条主线之间只允许通过明确 evidence / projection bridge 连接：

- RuntimeSession terminal event 是 evidence。
- `AgentRunDeliveryBinding` 是用户可见 running/terminal fact。
- `WaitProducerRef` 是 producer 的业务位置，不是 runtime session id。
- `LifecycleGate` 是 waiting fact owner。
- `AgentRunMailbox` 是 wake/result delivery owner。
- Frontend 只消费 snapshot 和 typed event，不维护第二套业务状态。

## 推荐后续拆分

1. 重命名并收束 `wait_obligation`：把 companion-specific intent 下沉为 adapter，把 service 边界改成 gate producer terminal convergence。
2. 设计 typed wait declaration：保留 gate owner，但将 producer、expected result、wake target、terminal policy 从 JSON 约定提升为可校验结构。
3. 建立通用 projection invalidation event：减少 companion/event key 分支，前端统一 refresh workspace/mailbox/waiting projection。
4. 将 exec terminal waiting projection 从 route-level 拼接收束到 activity/wait projection。
5. 梳理 terminal callback outbox：拆成可观测、可重试的 control-plane convergence steps。
