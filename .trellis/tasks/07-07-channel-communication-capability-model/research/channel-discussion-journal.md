# Channel 通信能力讨论 Journal

## 背景

本记录沉淀 2026-07-07 关于 Channel 长期模型的一轮预评估讨论。目标是保留一等判断，避免后续上下文压缩或实现切片时丢失关键领域边界。

## 核心定位

Channel 不是队列，也不是 `namespace=channel` 的超级来源。Channel 是 AgentDash 的通信能力面：统一广播、绑定、通用入栈、回复/发布、capability 暴露和 runtime materialization。

```text
Channel / ChannelAddress = 通信来源、参与者、可回复地址、广播/入栈语义
Mailbox = run_id + agent_id 下的消费、排队、steer/launch、恢复状态机
LifecycleGate = wait/adoption/request result authority
PermissionGrant / Broker = platform/system 决策事实源
RuntimeSession / Terminal = trace、terminal state、output refs
Capability = Agent 当前能看到/操作哪些 channel surface
```

## ChannelAddress 与 MailboxSourceIdentity

所有进入 Mailbox 的来源都可以被视作 channel，包括用户 composer 输入、Companion、Terminal wake、Human response、Platform response 和 external IM。正确方向不是把 Mailbox source 写成 `namespace=channel`，而是把现有 `MailboxSourceIdentity` 的字段形态提升为 canonical `ChannelAddress` 基础：

```text
namespace
kind
source_ref
correlation_ref
actor
route
display_label_key
metadata
```

因此 `namespace=companion`、`namespace=core`、`namespace=terminal`、`namespace=platform`、`namespace=im.slack` 本身就是 channel family。对于有运行时实体的 channel，再通过 `source_ref` 或 metadata 挂 lifecycle channel ref / external room ref。

推荐后续提取独立 domain value object：

```text
ChannelAddress
  -> MailboxSourceIdentity 使用同构字段或嵌入 / 投影
```

这样能避免 `MailboxSourceIdentity` 名字把通用 Channel 地址限制在 Mailbox 语义下。

## Capability 决策

长期 Channel 应是一等 capability dimension，即 `CapabilityState.channel` 是正确方向。它类似 Workspace Module：不是单个工具，而是一套 Agent 可见/可操作能力面。

短期不需要先做独立可配置的 channel cap，因为第一版主要验证 Companion runtime channel。短期可以通过 runtime effect 暴露 channel ref；长期再支持 Project / Story / AgentTeam / IM 配置式 capability。

建议模型：

```text
ChannelCapabilityState.visible_channels[]
  channel_ref
  aliases
  operations: receive | reply | send | broadcast | publish_external | subscribe | manage
  ingress_policy
  egress_policy
  readiness
```

`ToolCapability` 只控制工具是否可见，例如 `companion_request`、`companion_respond`、未来 `channel_reply`；具体能否访问某个 channel / operation，应由 Channel capability / admission 决定。

## Companion 决策

Companion 工具本质是使用 Channel 的语义 facade：

```text
companion_request = channel request facade
companion_respond = channel reply facade
```

第一版 Agent-facing 工具面选择 facade-only：保留 `companion_request` / `companion_respond` 等语义工具，底层走 Channel；不急着暴露通用 `channel_*` 工具，避免模型负担和 prompt/tool 去重复杂度。

`target=sub` 是特例：它不是绕过 Channel，而是一个带 provision 副作用的 Channel request target resolution。

```text
companion_request target=sub
  -> create child AgentRun
  -> create/resolve lifecycle-scoped temporary private channel
  -> create parent-child relation
  -> expose channel ref to parent/child current AgentFrame via runtime effect
  -> deliver first message / gate / reply contract
```

## Companion Channel 第一版范围

第一版 companion channel 不是 Project 全局资产。它是 lifecycle-scoped temporary channel，生命周期绑定 `LifecycleRun`，用于 parent-child/control tree 下的通信关系和审计。

已锁定方向：

```text
module boundary: agentdash-domain::channel
scope v1: lifecycle/runtime-scoped temporary channel
lifetime: LifecycleRun
capability exposure: RuntimeCapabilityEffect / AgentFrame transition
message log v1: 不做完整 ChannelMessage/Delivery log
```

第一版可持久化最小 channel ref / participants / binding。消息事实仍优先保留在 Gate / Mailbox / Terminal owner 中；Mailbox source metadata 引用 lifecycle channel ref。后续 AgentTeam / IM 需要广播和 fan-out 时，再引入 `ChannelMessage` / `ChannelDelivery` log。

## 模型上下文决策

Channel capability 不默认全量进模型上下文。第一版选择 task-local exposure：

```text
只在当前派发 / 等待 / 回复 prompt 中暴露短 alias / ReplyContract
不提供全局 visible channel roster
不让 Agent 看到 channel_id / message_id / delivery_id / gate_id / runtime_session_id
```

这延续当前 Companion reply contract：模型只提交业务 `payload`，多目标时才提交短 `reply_to` alias；真实目标由 resolver 解析。

## Mailbox Attribution 决策

Channel delivery materialize 到 Mailbox 时，第一版不改成 `namespace=channel`。对 Companion 应保留原来源家族：

```text
MailboxSourceIdentity {
  namespace: "companion",
  kind: "...",
  source_ref / correlation_ref: gate / dispatch / request refs,
  metadata: {
    lifecycle_channel_ref: "..."
  }
}
```

长期统一思想是：Mailbox source identity 就是 ChannelAddress 的投递 attribution 形态，而不是 Channel 的下游附属字段。

## 外部 IM 长期方向

外部 IM 是未来 Project asset channel 的代表：

```text
IM provider event
  -> normalize workspace / room / thread / user / message
  -> channel family namespace, e.g. im.slack
  -> persistent Project channel asset / external binding
  -> delivery policy: mention | keyword | digest | approval
  -> materialize to mailbox / notification / gate
  -> publish outbox for outbound reply
```

给特定 Agent 支持 IM，本质是添加对应 channel capability：

```text
channel = im.company.room:platform-team
operations = receive mention, reply thread, publish with approval
aliases = ["platform-team", "team-room"]
ingress_policy = mention | assigned_thread | digest
egress_policy = thread_reply_only | approval_required | rate_limited
```

## 第一版建议路线

1. 更新任务设计文档，记录 `MailboxSourceIdentity -> ChannelAddress` 的判断。
2. Domain 引入轻量 `channel` value model，但 v1 只支持 lifecycle/runtime-scoped channel。
3. Capability 设计为长期 `CapabilityState.channel` dimension；v1 通过 runtime effect expose lifecycle channel ref。
4. Companion `target=sub` 改造成 channel provision + runtime exposure + mailbox/gate materialization 的 tracer。
5. 不做通用 `channel_*` 工具，不做全量 ChannelMessage log，不做外部 IM provider。
6. 后续再做 Project / Story / AgentTeam persistent channel asset、broadcast/fan-out、external IM binding 和 publish outbox。

## 仍需细化的问题

- `LifecycleChannel` 最小表/实体字段如何命名：使用通用 `channels` + owner scope，还是先用 `lifecycle_channels`。
- 是否在第一版就把 composer input 显式称为 `namespace=core, kind=composer` 的 built-in channel family。
- ChannelAddress 是直接复用 `MailboxSourceIdentity` 类型，还是提取 domain value object 后让 Mailbox source identity 映射/嵌入它。当前推荐后者。
- Channel message persistence 的第一版最小 schema（channel ref / participants / binding 具体字段）尚未设计，只确认了"不做完整 event log"。
- Channel capability 通过 `PermissionGrant` 累积可见性的通用机制尚未设计（现有 grant 判定硬编码基于 Tool 维度）。

## 2026-07-07 二轮对齐：代码核实 + 用户决策

在第一轮讨论基础上，派 4 个只读 Explore agent 核实 design.md 的关键假设是否与代码现状一致，随后与用户逐项对齐。核实与决策结果：

**核实纠正的事实**（已同步回 `prd.md`/`design.md`，此处只留摘要，细节见两处正文）：

- `MailboxSourceIdentity` 确实已是开放结构，`MailboxMessageSource` closed enum 已在 migration `0032`（commit `f6e2406e6`）修复消除，06-28 任务记录的 `canvas_action` drift 是历史问题、已解决。
- `platform` namespace 目前只有 spec/design 里的前瞻占位，代码里没有任何路径真正构造它；`target=platform` 现状是 missing-broker 诊断。
- Workspace Module **不是**干净的 "projection-only 先例"——声明式部分挂在 `CapabilityState.workspace_module` 但未注册进 `CapabilityDimensionRegistry`；运行时曝光部分完全走独立的 `AgentFrame.visible_workspace_module_refs_json` 列，绕开 registry；两者只在读取时由专用 resolver OR 合并。这是历史权宜实现。
- `AccumulationPolicy::Accumulate` 已有完整先例：VFS dimension 的 `apply_mount_operations`/`MountDirective::{AddMount,RemoveMount,ReplaceMount,AddLink,RemoveLink}`（`runtime_capability.rs:822-838`）实现了"运行时累积、可撤销"；`Accumulate` 的文档注释本身就点名"canvas mount append"为典型场景（`session_persistence.rs:148-149`）。不需要新的 AccumulationPolicy。
- `companion_request target=sub` 首条任务确实已经走 AgentRun Mailbox（W3 属实），但 design.md 提议的 lifecycle-scoped temporary channel、`RuntimeCapabilityEffect(dimension=channel)`、AgentFrame 可见的 channel alias/reply surface——代码里完全不存在，是全新机制，不是迁移。
- `CompanionReplyContract` 真实字段（`route/request_id/channel/aliases/model_instruction`）与 design.md 引用的 `namespace/kind/source_ref/correlation_ref` 词汇不是同一个结构；后者属于 `MailboxSourceIdentity`。
- Extension Protocol Channel（`channel_key`/`protocol_channels`）是贯穿 domain→contracts（有 TS codegen）→relay→runtime-gateway→前端 bridge→UI 文案的完整上线功能，与 design.md 提议的 Agent 通信 Channel 存在真实的心智模型冲突（都叫 Channel、都是 AgentRun-scoped、都是 capability 门禁的 invoke 语义），但当前没有具体标识符重名。

**用户决策**：

- MVP 切片范围confirmed：Companion / SubAgent lifecycle-scoped temporary channel。
- Capability 路线：**不**采用"仿 Workspace Module 现状"的 AgentFrame 侧信道方案；直接把 Channel 做成一等 `CapabilityState.channel` dimension，理由是长期扩展到全局 channel 时这一步逃不掉，现在直接建对形态比以后migrate 更省。用户明确指出 VFS/canvas mount 的 accumulate 先例可以直接复用，纠正了最初"没有 append 先例、需要新 policy"的过度保守判断。
- 命名边界：新 Channel 保留 `Channel` 命名；Extension Protocol Channel 使用面不大，是重命名或收束进统一 Channel 体系（作为未来某个 `ChannelMedium`）的候选，而不是让新概念让路。这不是 v1（Companion lifecycle channel）范围内要处理的事。
- 下一步：先把上述纠正写回 `design.md`/`prd.md`/`implement.md`（已完成），再规划可执行的 MVP 子任务。
