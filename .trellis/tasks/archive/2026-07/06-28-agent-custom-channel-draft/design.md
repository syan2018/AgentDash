# Agent 自定义信道长期设计

## Draft Notes

本任务是长期 draft，当前不进入实现设计收敛。

## Candidate Architecture

```text
ChannelEvent
  -> AgentChannelBinding / SubscriptionResolver
  -> DeliveryPlan
  -> AgentRunMailboxMessage
```

全局 channel 系统只负责事件事实、Agent 绑定、订阅与 fan-out；AgentRun Mailbox 继续负责单个 AgentRun 的入站调度。

## Agent-Bound IM Group Case

IM 群聊信道可以作为验证案例：外部群聊消息进入 room event log，平台按 Agent 信道绑定、mention/关键词/digest/approval 等策略投递到 AgentRun mailbox。Agent 处理结果再通过受限 reply/publish 工具回到外部群聊。

待后续讨论：

- 外部 IM 用户、群聊、thread 与 AgentDash identity 的映射。
- mention / keyword / digest / explicit subscription / automatic subscription 的区别。
- 大量用户上下文进入 inbox 前如何裁剪、摘要和去重。
- Agent 发布消息是否默认对外部用户可见。
- 用户消息进入 Agent inbox 是否需要 approval。
- Agent 信道绑定应挂在 Agent preset、具体 Agent identity 还是运行中的 AgentRun。
