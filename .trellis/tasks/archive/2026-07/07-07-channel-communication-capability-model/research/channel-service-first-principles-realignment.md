# ChannelService 第一性重对齐

## 背景

2026-07-07 后续对齐推翻了此前 "Companion / SubAgent lifecycle-scoped temporary channel" 作为第一版 Channel 主模型的结论。用户明确补充：企业内部 IM 接入是 Channel 的确定需求，Project 公共 Channel 需要作为可维护资产，并作为赋予特定 Agent 的能力。

因此 Channel 不能从 `LifecycleRun.channels` 反推出来。正确主干是通用 `Channel` 领域与 `ChannelService`，Lifecycle 只是其中一种 runtime scope。

后续第一性修正：Channel 的一等领域地位不推出独立关系表。Agent runtime channel 高频随 LifecycleRun / SubAgent 生灭，适合 owner-local document registry；新增 owner document 物理列使用 `jsonb` 并映射 typed domain document。Project 公共 Channel 是未来 Assets 系统要收束的资产，不在当前任务里固定到 `ProjectConfig` 或具体表。

## 新出发点

```text
Channel
  = 一等通信空间 / 广播信道 / 外部入口绑定 / delivery planning owner

ChannelService
  = owner-scoped lazy registry resolver + participants + bindings + ingress + broadcast planning + materialization intent

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

新结论：`LifecycleRun` 是 runtime-scoped channel 的 owner document，不是 Channel 的领域根。模型必须以通用 `Channel` 为中心，支持 Project、Story、LifecycleRun、ExternalBinding、System 等 owner；但 runtime channel 的物理事实可以落在 `LifecycleRun.channel_registry` 这类 owner-local document 中。

原因：企业 IM / Project Channel 必须在没有某个具体 LifecycleRun 的情况下被解析、授权、绑定和展示。

### 2b. 推翻独立 channel 表作为一等性的默认表达

旧结论：因为 Channel 是一等领域，所以需要 `channels` / `channel_participants` / `channel_bindings` 这类独立关系表。

新结论：一等性属于领域语言、服务边界和 capability projection，不等同于一等关系表。runtime Channel facts 默认落在 owner-local `ChannelRegistryDocument`；Project Channel 的物理承载等待 Project Assets 系统收束。

原因：运行时通信关系通常短命、强 owner 绑定、随 LifecycleRun / SubAgent 生灭。拆成独立表会引入孤立清理、额外 join、第二套状态机和过早的持久化承诺。只有事实具备跨 owner 全局扫描、多 worker 抢占、独立审计保留或独立生命周期时，才需要单独 store。

### 2c. ChannelService 必须懒加载

旧风险：把 `ChannelService` 做成启动期扫描全部 Project / LifecycleRun / Assets 的全局 channel runtime。

新结论：`ChannelService` 是 owner-scoped lazy resolver。AgentFrame projection、IM ingress、Companion facade、delivery materialization 各自携带 owner ref 触发 registry load。

原因：Channel registry 是业务事实，不是需要常驻拉起的运行时进程。全局预加载会把大量无关 LifecycleRun 和已释放 SubAgent channel 重新激活，违背临时运行时事实随 owner 生灭的边界。

### 2d. 明确 owner document 使用 JSONB

旧风险：把结构化 runtime document 存成 `TEXT` JSON，再由各 repository 通过字符串解析工具分散读写。

新结论：新增 Channel registry 这类 owner-local document 使用 PostgreSQL `jsonb`，列名仍使用业务语义名，例如 `channel_registry`，repository 映射为 typed `ChannelRegistryDocument`。

原因：Channel registry 是结构化业务事实，不是字符串协议。`jsonb` 能让数据库验证 JSON 形态，并为未来必要的 operator / expression index 留出规范路径；typed document 则保证业务层仍按领域模型演进，而不是把动态 JSON 传播到 application/service。

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
- Owner-scoped lazy load：只在具体 owner ref 被 projection、ingress、facade 或 delivery intent 访问时读取 registry。

它不拥有：

- Mailbox queue、claim、schedule、turn boundary。
- LifecycleGate wait/result payload。
- PermissionGrant decision lifecycle。
- Terminal stdout/stderr cursor 和完整 output。
- RuntimeSession trace/event log。

## 企业 IM 驱动的目标形态

```text
Project Channel = 企业 IM room/thread binding
  -> Project/Assets owner store 保存 channel asset facts
  -> ChannelBinding 保存 provider workspace/room/thread identity
  -> ChannelParticipant 保存 Agent / human / external user participation
  -> ChannelPolicy 保存 mention、thread、approval、rate limit、digest 等策略
  -> ChannelCapabilityProjector 把可见 channel 和 operations 投影给特定 Agent
  -> IM adapter ingress 生成 ChannelMessage
  -> ChannelService 规划 delivery
  -> AgentRun target materialize 到 Mailbox
  -> outbound reply 进入 ChannelPublishOutbox
```

这个形态要求 Channel 是 Project-level asset / runtime domain，不能只是 LifecycleRun aggregate 的 JSON 子字段；但当前任务不决定 Project asset 的物理表/文档形态。

## 与既有任务的关系

- `07-07-channel-companion-reply-contract` 已完成 Companion reply 最小合同清理；它是未来 Channel facade 的输入基础，不是 Channel 架构最终形态。
- `06-28-integration-channel-mailbox-convergence` 已收束 Mailbox source identity 和 Companion/Routine 入站边界；ChannelService 后续应复用 mailbox materializer，而不是绕过 mailbox。
- `06-28-agent-custom-channel-draft` 已指出 IM group binding 方向；当前 realignment 将它从远期草案提升为 Channel 主干约束。

## 后续实现原则

- 先定义通用 Channel domain / service contracts，再选择 Companion/SubAgent 或 Project IM 的第一条落地链路。
- 允许分阶段落库，但不允许分阶段定义互相冲突的事实源；runtime channel 默认走 owner-local document。
- 新增 owner-local document 物理列使用 `jsonb`，repository 使用 typed codec 映射为 domain document。
- Capability projection 由 Channel facts 派生；工具执行仍走 AgentRun effective capability/admission。
- Mailbox 与 Gate 的既有 owner 边界保持不变，ChannelService 只产生 materialization intent。
- 既有 Mailbox / Gate 表不是 Channel 新设计的默认先例；是否拆表必须回到生命周期、并发、恢复、查询和审计第一性判断。
