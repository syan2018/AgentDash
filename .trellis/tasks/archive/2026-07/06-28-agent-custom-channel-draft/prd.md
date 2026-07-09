# Agent 自定义信道长期设计

## Goal

长期探索 AgentDash 如何支持用户为 Agent 定义可接收的外部通信信道，使 Agent 能被绑定到 IM 群聊、客服群、组织频道等外部对话场所，并将大量用户上下文按策略进入 Agent inbox。该任务作为长期 draft 挂起，不进入短期实现。

## Background

短期工作优先收束 Routine 单会话与 Companion 入 box；本任务承载更远期的 Agent 自定义信道问题，避免过早把 Host Integration、IM 群聊接入和 mailbox scheduler 混成一个过大的短期重构。

长期目标不是让 integration 直接控制 AgentRun mailbox，而是让平台提供可治理的信道模型：

- 用户或管理员为某类 Agent / 某个 Agent 配置可接收信道。
- 外部 IM 群聊或频道绑定到该 Agent 信道。
- 群聊中的用户消息、上下文片段、mention、文件或 thread 摘要可按策略进入目标 AgentRun mailbox。
- Agent 可以通过受限工具向绑定的外部信道回复或发布结构化消息。
- 外部系统可以通过 Host Integration 贡献 IM adapter、事件源和 normalization，但 routing、permission、fan-out 和 mailbox materialization 仍归平台。

## Candidate Case: Agent 绑定 IM 群聊

一个可用于验证模型的长期案例：

```text
Agent 可接收信道
  -> 绑定 IM 群聊 / 组织频道
  -> 群聊中的大量用户上下文被采集、过滤、摘要或按 mention 触发
  -> 平台按信道策略将消息 materialize 到目标 AgentRun mailbox
  -> Agent 处理后可通过受限 reply/publish 工具回到群聊
```

该案例能同时验证：

- 外部 IM 用户身份、群聊身份与 AgentDash Agent 身份的映射边界。
- 大量用户上下文如何被裁剪、摘要、去重或按触发条件进入 Agent inbox。
- AgentRun mailbox 如何接收来自外部群聊的消息。
- Agent 是否需要显式 mention、关键词、频道订阅、角色过滤或人工 approval 才能入 box。
- Agent 回复外部群聊时如何受权限、审计和速率限制约束。

## Requirements To Explore

- R1: 定义 Agent 可接收信道、外部 room binding、subscription、participant、publisher、delivery policy 的产品语义。
- R2: 评估 Agent preset、Agent identity、AgentRun、Companion relationship 作为订阅/接收 scope 的差异；Project 不作为主验证案例。
- R3: 设计 IM 群聊输入进入 Agent inbox 的触发策略，例如 mention、关键词、thread 摘要、定时 digest、人工 approve。
- R4: 设计 Agent 发布/回复工具的最小权限边界，避免任意 Agent 向任意外部群聊发言。
- R5: 设计信道事件事实与 per-AgentRun mailbox delivery row 的关系，避免大群上下文在 fan-out 时被重复存储。
- R6: 设计外部 integration adapter 的边界：贡献 IM adapter、事件 normalization、room metadata 和身份映射，不直接控制 mailbox。
- R7: 评估群聊信道如何接入用户 UI、AgentRun workspace、notification 和 audit。
- R8: 明确与 Routine / Companion 短期收束后的复用关系。

## Acceptance Criteria

- [ ] 形成长期 channel 模型的概念设计。
- [ ] 明确 Agent 绑定 IM 群聊案例的端到端数据流。
- [ ] 明确 Host Integration 在自定义信道中的边界。
- [ ] 明确哪些能力依赖短期 Routine/Companion mailbox 收束完成后再推进。
- [ ] 拆出后续可进入规划的 MVP 子任务。

## Out Of Scope

- 不影响短期 Routine/Companion 收束任务。
- 不直接实现 integration SPI 或 IM 群聊 UI。
- 不讨论 extension protocol channel。
- 不要求现在确定所有外部系统 provider。
- 不以 Project / Story 作为自定义信道的主 case。
