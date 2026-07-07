# ChannelService 第一性重对齐

## 背景

2026-07-07 后续对齐推翻了此前 "Companion / SubAgent lifecycle-scoped temporary channel" 作为第一版 Channel 主模型的结论。用户明确补充：企业内部 IM 接入是 Channel 的确定需求，Project 公共 Channel 需要作为可维护资产，并作为赋予特定 Agent 的能力。

因此 Channel 不能从 `LifecycleRun.channels` 反推出来。正确主干是通用 `Channel` 领域与 `ChannelService`，Lifecycle 只是其中一种 runtime scope。

## 新出发点

```text
Channel
  = 一等通信空间 / 广播信道 / 外部入口绑定 / delivery planning owner

ChannelService
  = channel facts + participants + bindings + ingress + broadcast planning + materialization intent

LifecycleRun
  = runtime-scoped channel 的宿主环境之一

CapabilityState.channel
  = AgentFrame 当前可见/可操作 Channel 的投影

Mailbox
  = Channel delivery materialize 到 AgentRun 后的 durable scheduler

LifecycleGate
  = wait/result authority
```

## 推翻了哪些旧结论

### 1. 推翻 lifecycle-only MVP

旧结论：第一版优先 Companion / SubAgent lifecycle-scoped temporary channel。

新结论：Companion / SubAgent 可以作为第一条迁移验证链路，但不能定义 Channel 的形状。Project 公共 Channel 和企业 IM 已经是明确目标，必须从一开始在模型里出现。

原因：如果第一版只服务 `LifecycleRun`，后续 Project IM 需要重新引入 owner、binding、participant、event、delivery 和 capability assignment，等于把主干重做一遍。

### 2. 推翻 `LifecycleRun.channels` 作为最终承载

旧结论：Channel 是 `LifecycleRun.channels: Vec<LifecycleChannel>`，不新建独立 channel 表。

新结论：`LifecycleRun` 是 `ChannelOwner::LifecycleRun` 或 runtime scope，不是 Channel 的领域根。持久化可以分阶段决定，但模型必须以通用 `Channel` 为中心，支持 Project、Story、LifecycleRun、ExternalBinding、System 等 owner。

原因：企业 IM / Project Channel 必须在没有某个具体 LifecycleRun 的情况下被解析、授权、绑定和展示。

### 3. 推翻 capability 表达 participants

旧结论：参与者不用单独字段或表，由各方 `CapabilityState.channel.visible_channels` 是否持有引用表达。

新结论：participants、membership、role、broadcast policy 是 Channel 事实；`CapabilityState.channel` 是 AgentFrame 的可见/可操作投影。

原因：Capability 是运行时模型可见 surface，不能反向成为通信空间事实源。否则恢复、审计、广播 fan-out 和外部 IM delivery 都要从多个 AgentFrame projection 反推 membership。

### 4. 推翻 Message / Delivery 后置定义

旧结论：Project/Story/外部 IM 的 `ChannelMessage` / `ChannelDelivery` 等需要完整消息事实时再启用。

新结论：event log 是否首期落库可以后置，但 `ChannelMessage` / `ChannelDelivery` 的边界必须现在定义。

原因：没有 Message / Delivery 概念，就无法描述广播、fan-out、IM inbound、publish outbox、mailbox materialization 和 gate/notification delivery 的共同语言。

## ChannelService 边界

`ChannelService` 应维护：

- Channel lifecycle：create、close、status、policy。
- Participant lifecycle：join、leave、role、operations、ingress/egress policy。
- Binding lifecycle：external workspace / room / thread / user identity mapping。
- Ingress normalize：provider event -> canonical `ChannelMessage`。
- Broadcast planning：audience / role / policy -> `ChannelDelivery[]`。
- Materialization intent：delivery -> mailbox / gate / notification / publish outbox。
- Capability projection：Channel facts -> AgentFrame `CapabilityState.channel`。

它不拥有：

- Mailbox queue、claim、schedule、turn boundary。
- LifecycleGate wait/result payload。
- PermissionGrant decision lifecycle。
- Terminal stdout/stderr cursor 和完整 output。
- RuntimeSession trace/event log。

## 企业 IM 驱动的目标形态

```text
Project Channel = 企业 IM room/thread binding
  -> ChannelBinding 保存 provider workspace/room/thread identity
  -> ChannelParticipant 保存 Agent / human / external user participation
  -> ChannelPolicy 保存 mention、thread、approval、rate limit、digest 等策略
  -> ChannelCapabilityProjector 把可见 channel 和 operations 投影给特定 Agent
  -> IM adapter ingress 生成 ChannelMessage
  -> ChannelService 规划 delivery
  -> AgentRun target materialize 到 Mailbox
  -> outbound reply 进入 ChannelPublishOutbox
```

这个形态要求 Channel 是 Project-level asset / runtime domain，不能只是 LifecycleRun aggregate 的 JSON 子字段。

## 与既有任务的关系

- `07-07-channel-companion-reply-contract` 已完成 Companion reply 最小合同清理；它是未来 Channel facade 的输入基础，不是 Channel 架构最终形态。
- `06-28-integration-channel-mailbox-convergence` 已收束 Mailbox source identity 和 Companion/Routine 入站边界；ChannelService 后续应复用 mailbox materializer，而不是绕过 mailbox。
- `06-28-agent-custom-channel-draft` 已指出 IM group binding 方向；当前 realignment 将它从远期草案提升为 Channel 主干约束。

## 后续实现原则

- 先定义通用 Channel domain / service contracts，再选择 Companion/SubAgent 或 Project IM 的第一条落地链路。
- 允许分阶段落库，但不允许分阶段定义互相冲突的事实源。
- Capability projection 由 Channel facts 派生；工具执行仍走 AgentRun effective capability/admission。
- Mailbox 与 Gate 的既有 owner 边界保持不变，ChannelService 只产生 materialization intent。
