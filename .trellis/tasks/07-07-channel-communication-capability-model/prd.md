# Channel 通信能力长期模型预评估

## Goal

建立 AgentDash 长期 Channel 系统的预评估与领域边界：Channel 作为一套面向 Project / Story / AgentTeam / 外部 IM / Companion / Terminal 等来源的通用通信能力面，负责广播、绑定、标准化入栈、回复与发布；Mailbox 继续作为 AgentRun 消费输入的 durable scheduler。

本任务当前只进入规划与预评估，不启动实现。

## Background

当前讨论已形成几个关键判断：

- Channel 显然不应演进成队列；它应是一套广播与通用入栈机制。
- Companion 工具本质上应是使用 Channel 能力的工具；`target=sub` 是带创建 Channel、创建 child AgentRun 和建立 parent/child 关系副作用的特例。
- Channel 能否被 Agent 接入某个 IM、某个 Project team channel 或某个 Story collaboration channel，本质上应由 capability surface / admission 决定——这个产品语义参照 Workspace Module（能力面由 capability 决定可见性），但**不**照搬 Workspace Module 当前的技术实现（见下方代码核实结论）。
- 既有 Companion、Terminal 等异步消息行为需要在进入 Mailbox 前拥有标准结构；后续同种构造应能以通用路径入 box。

仓库中已有相关基础：

- AgentRun Mailbox 已经具备开放的 `MailboxSourceIdentity`（`namespace/kind/source_ref/correlation_ref/actor/route/display_label_key/metadata`），用于 delivery attribution、dedup、correlation 和 projection。
- Capability 系统的 `CapabilityDimensionModule` + `AccumulationPolicy` 机制已经支持”运行时累积授予、可撤销”的 dimension（VFS 的 `apply_mount_operations`/`MountDirective::{AddMount,RemoveMount,...}` 是直接可复刻的先例），可作为 Channel capability dimension 的参照。
- **代码核实纠正（2026-07-07 二轮对齐）**：Workspace Module 并不是干净的”能力面通过 capability 投影、再通过 descriptor/operation/readiness 暴露操作”的参照实现——它的声明式部分挂在 `CapabilityState.workspace_module` 但从未注册进 `CapabilityDimensionRegistry`，运行时曝光部分完全走独立的 `AgentFrame.visible_workspace_module_refs_json` 列绕开 registry，两者只在读取时由专用 resolver OR 合并。这是历史权宜实现，Channel 改为直接复刻 VFS dimension 的 `Accumulate` 模式，不复刻 Workspace Module 现状。
- **代码核实纠正**：“Companion reply contract 已收敛为 `payload + optional reply_to`，是 Channel reply contract 的早期局部形态”这个类比不够准确——`CompanionReplyContract` 的真实字段是 `route/request_id/channel/aliases/model_instruction`，`namespace/kind/source_ref/correlation_ref` 这套词汇实际属于 `MailboxSourceIdentity`，不是 `CompanionReplyContract`。方向判断（Companion 回复合同未来可以收敛进 Channel 体系）仍然成立，但字段层面不是同一个结构的早期形态。
- 既有长期任务 `.trellis/tasks/06-28-agent-custom-channel-draft` 已记录外部 IM / 自定义信道方向；`.trellis/tasks/06-28-integration-channel-mailbox-convergence` 已记录 Mailbox source identity 与 Companion/Routine 入站收束方向（该任务 work-items W0-W8 已全部 `implemented`/`done`，但 `task.json` 仍是 `in_progress`，尚未收尾归档）。

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

- [x] `design.md` 记录 Channel 的长期领域模型、核心边界和与 Mailbox / Gate / Capability / Workspace Module 的关系。
- [x] `design.md` 记录 Project / Story / AgentTeam broadcast、外部 IM、Companion、Terminal 四类代表链路的候选数据流。
- [x] `design.md` 明确 Channel capability surface 的建议方向，以及它与 ToolCapability / WorkspaceModule capability / PermissionGrant 的区别。（2026-07-07：capability 落地路线已决策为一等 dimension；PermissionGrant 集成仍是明确记录的 Still Open 项，不是未处理的遗漏。）
- [x] `design.md` 记录短期不做的内容，避免把本任务误解为立即实现外部 IM、全局队列或重写 Companion。
- [x] `implement.md` 只记录未来分阶段探索路线，不进入可执行实现计划，直到用户确认后续 MVP 范围。（2026-07-07：MVP 范围已确认为 Companion/SubAgent lifecycle-scoped temporary channel；是否据此拆分新的可执行子任务留给下一步决定，本文档仍保持路线图形态。）
- [x] `implement.jsonl` 与 `check.jsonl` 包含真实 spec / 既有任务上下文条目，便于后续研究或检查。

## Out Of Scope

- 本轮不启动实现、不运行 `task.py start`。
- 本轮不创建数据库 migration。
- 本轮不把 Companion 当前实现迁移到 Channel。
- 本轮不实现外部 IM provider、身份映射 UI 或 publish outbox。
- 本轮不重写 AgentRun Mailbox scheduler。

## Open Questions

- ~~OQ1: Channel 第一阶段 MVP 应优先验证"内部 AgentTeam / Project / Story 广播"，还是优先验证"Companion / Terminal 等既有异步行为统一入栈"？~~ **已决策（2026-07-07）**：优先 Companion / SubAgent lifecycle-scoped temporary channel。
- ~~OQ2: Channel capability 应作为新的 capability dimension 进入 `CapabilityState`，还是先以 Workspace Module 类似的 projection-only surface 试验？~~ **已决策（2026-07-07）**：作为一等 `CapabilityState.channel` dimension，`AccumulationPolicy::Accumulate`，对齐 VFS `apply_mount_operations`/`MountDirective` 的现有实现模式，不走 Workspace Module 式 projection-only 路线。详见 `design.md` "Channel Capability" 一节。
- OQ3: 外部 IM 的持久化 ChannelEventLog 是否应从第一版就设计进 schema，还是等内部 Channel 模型稳定后再引入？（本轮未重新讨论，journal.md 现有倾向"等内部模型稳定后再引入"继续有效，但未经用户重新确认，不算新决策。）
