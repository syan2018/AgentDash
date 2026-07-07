# Channel 通信能力长期模型预评估

## Goal

建立 AgentDash 长期 Channel 系统的预评估与领域边界：Channel 作为一套面向 Project / Story / AgentTeam / 外部 IM / Companion / Terminal 等来源的通用通信能力面，负责广播、绑定、标准化入栈、回复与发布；Mailbox 继续作为 AgentRun 消费输入的 durable scheduler。

本任务当前只进入规划与预评估，不启动实现。

## Background

当前讨论已形成几个关键判断：

- Channel 显然不应演进成队列；它应是一套广播与通用入栈机制。
- Companion 工具本质上应是使用 Channel 能力的工具；`target=sub` 是带创建 Channel、创建 child AgentRun 和建立 parent/child 关系副作用的特例。
- Channel 应被视为类似 Workspace Module 的通用能力面：Agent 能否接入某个 IM、某个 Project team channel 或某个 Story collaboration channel，本质上应由 capability surface / admission 决定。
- 既有 Companion、Terminal 等异步消息行为需要在进入 Mailbox 前拥有标准结构；后续同种构造应能以通用路径入 box。

仓库中已有相关基础：

- AgentRun Mailbox 已经具备开放的 `MailboxSourceIdentity`，用于 delivery attribution、dedup、correlation 和 projection。
- Capability 系统已经支持开放 string key、dimension module 和 Workspace Module 可见性投影，可作为 Channel capability 的参照。
- Workspace Module 已经表达“某类通用能力面通过 capability 投影给 Agent，再通过 descriptor / operation / readiness 暴露操作”的模式。
- 最近的 Companion reply contract 已收敛为 `payload + optional reply_to`，它可以视为 Channel reply contract 的早期局部形态。
- 既有长期任务 `.trellis/tasks/06-28-agent-custom-channel-draft` 已记录外部 IM / 自定义信道方向；`.trellis/tasks/06-28-integration-channel-mailbox-convergence` 已记录 Mailbox source identity 与 Companion/Routine 入站收束方向。

## Requirements

- R1: 定义 Channel 作为一等通信能力面的领域语言，至少覆盖 Channel、Participant、Message、Binding、Delivery、DeliveryPolicy、ReplyAddress、PublishOutbox 和 ChannelCapability。
- R2: 明确 Channel 与 Mailbox 的边界：Channel 负责广播、绑定、入栈标准化和回复/发布能力；Mailbox 只负责某个 AgentRun 如何 durable 消费一条输入。
- R3: 明确 Channel 与 LifecycleGate / PermissionGrant / RuntimeSession / Terminal output 的边界，避免 Channel 接管等待结果、平台决策或运行 trace 的事实源。
- R4: 设计 Channel capability 的长期方向：AgentFrame / AgentRun effective capability 应能表达 visible channel refs、allowed operations、ingress/egress policy、aliases 和 readiness。
- R5: 设计 Agent Team / Project / Story 层级多 Agent 协作的 Channel 语义，支持广播、角色路由、shared channel context 和多 Agent delivery fan-out。
- R6: 设计外部 IM 接入语义：外部 workspace/room/thread/user/message identity normalization、ChannelBinding、delivery policy、publish outbox、权限审计和 rate limit。
- R7: 设计 Companion 迁移方向：`companion_request` / `companion_respond` 作为 Channel 工具 facade；subagent 创建作为带 provision 副作用的 Channel request target resolution。
- R8: 设计 Terminal / async producer 入栈方向：Terminal 保留 output/state owner，Channel 只承载 bounded event / refs / wake delivery。
- R9: 明确 Agent-facing tool / prompt 只能暴露最小有效合同，例如 payload、短 alias、operation intent；内部 channel/message/delivery/gate/runtime refs 由 resolver 持有。
- R10: 给出分阶段落地路径，避免一次性实现完整外部 IM 和全量持久化 Channel 系统。

## Acceptance Criteria

- [ ] `design.md` 记录 Channel 的长期领域模型、核心边界和与 Mailbox / Gate / Capability / Workspace Module 的关系。
- [ ] `design.md` 记录 Project / Story / AgentTeam broadcast、外部 IM、Companion、Terminal 四类代表链路的候选数据流。
- [ ] `design.md` 明确 Channel capability surface 的建议方向，以及它与 ToolCapability / WorkspaceModule capability / PermissionGrant 的区别。
- [ ] `design.md` 记录短期不做的内容，避免把本任务误解为立即实现外部 IM、全局队列或重写 Companion。
- [ ] `implement.md` 只记录未来分阶段探索路线，不进入可执行实现计划，直到用户确认后续 MVP 范围。
- [ ] `implement.jsonl` 与 `check.jsonl` 包含真实 spec / 既有任务上下文条目，便于后续研究或检查。

## Out Of Scope

- 本轮不启动实现、不运行 `task.py start`。
- 本轮不创建数据库 migration。
- 本轮不把 Companion 当前实现迁移到 Channel。
- 本轮不实现外部 IM provider、身份映射 UI 或 publish outbox。
- 本轮不重写 AgentRun Mailbox scheduler。

## Open Questions

- OQ1: Channel 第一阶段 MVP 应优先验证“内部 AgentTeam / Project / Story 广播”，还是优先验证“Companion / Terminal 等既有异步行为统一入栈”？
- OQ2: Channel capability 应作为新的 capability dimension 进入 `CapabilityState`，还是先以 Workspace Module 类似的 projection-only surface 试验？
- OQ3: 外部 IM 的持久化 ChannelEventLog 是否应从第一版就设计进 schema，还是等内部 Channel 模型稳定后再引入？
