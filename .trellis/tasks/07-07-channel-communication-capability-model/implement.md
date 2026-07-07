# Channel 通信能力长期模型预评估执行记录

## Current State

本任务只记录预评估与文档修订，不进入 implementation phase。不要运行 `task.py start`，除非后续用户明确要求把某个切片进入实现。

最新对齐已经推翻 lifecycle-only temporary channel MVP 作为主模型的结论。下一步实现任务应从通用 Channel domain / ChannelService skeleton 开始，再用 Companion/SubAgent 和 Project IM 两条代表链路验证。

## Suggested Future Phases

1. **Channel Domain / Service Skeleton**
   - 新增 `agentdash-domain::channel` 领域模型：Channel、Participant、Binding、Message、Delivery、DeliveryPolicy、ReplyAddress、PublishOutbox、ChannelAddress。
   - 新增 application 层 `ChannelService` 边界：维护 channel、participants、bindings、broadcast policy、message ingress、delivery planning、mailbox/gate/outbox materialization intent。
   - 明确 `LifecycleRun` 是 `ChannelOwner::LifecycleRun` 的 scope，不引入一等 `LifecycleChannel`。

2. **Project / External IM Capable Asset Model**
   - 设计 Project-owned persistent Channel 与 External IM binding。
   - 定义 external workspace / room / thread / user / message identity normalization。
   - 定义 publish outbox、approval、audit、rate-limit 与 identity mapping 的最小合同。
   - 决定 Channel 持久化形态：独立表、Project asset，或二者组合。

3. **Channel Capability Dimension**
   - 新增 `CapabilityState.channel` 一等 dimension，`AccumulationPolicy::Accumulate`。
   - 将 Channel facts / participant policy 投影为 AgentFrame visible channel refs、aliases、operations、readiness、ingress/egress policy。
   - 明确 capability projection 不保存 membership，不成为 Channel 事实源。
   - 细化 ProjectAgent / Project channel assignment / PermissionGrant 对 Channel capability 的输入关系。

4. **Ingress / DeliveryPlan Prototype**
   - 定义 Channel ingress envelope、delivery intent、materializer。
   - 覆盖 IM inbound、Companion reply、Terminal wake、Human response 等已有异步行为。
   - `ChannelAddress` 从 `MailboxSourceIdentity` 抽象出来，作为 source/delivery attribution 值对象。

5. **Mailbox Materializer**
   - 将需要 AgentRun 消费的 `ChannelDelivery` materialize 为 AgentRunMailboxMessage。
   - Mailbox source identity / dedup key 引用 Channel message、delivery、binding 或 provider event refs。
   - 保持 mailbox scheduler 不按 channel source 分支决定消费语义。

6. **Companion / SubAgent Facade Migration**
   - 将 `companion_request` / `companion_respond` 视为 Channel request/reply facade。
   - `target=sub` 创建或解析 LifecycleRun-scoped runtime Channel，参与者包含 parent/child AgentRun refs。
   - Gate 继续拥有 wait/result state；Channel 只规划消息、reply address 和 delivery materialization。

7. **Old Path Cleanup**
   - 清理 Companion / Terminal / system wake 中绕过 ChannelService 的 ad hoc delivery 路径。
   - 清理把系统/子 Agent 通知伪装成普通 human input 的路径。
   - 保持 06-28 Mailbox source identity 收束成果，避免 ChannelService 变成第二个 mailbox。

## Research Anchors

- `.trellis/tasks/07-07-channel-communication-capability-model/research/channel-service-first-principles-realignment.md`
- `.trellis/tasks/07-07-channel-communication-capability-model/research/channel-discussion-journal.md`
- `.trellis/tasks/07-07-channel-communication-capability-model/research/v1-decision-evidence-and-open-items.md`（其中 lifecycle-only 最终决策已被最新 realignment 推翻；代码证据仍可参考）
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/tasks/06-28-agent-custom-channel-draft/design.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/design.md`

## Decisions To Preserve

- Channel 是一等领域与 `ChannelService` 主干；Lifecycle 只是 runtime-scoped channel 的一种 owner/scope。
- Project 公共 Channel / 企业 IM 接入是明确需求，应从架构起点纳入，而不是后续补丁。
- Channel participants、binding、broadcast policy、message/delivery planning 是 Channel 事实。
- `CapabilityState.channel` 是 AgentFrame 可见操作投影，不是 membership 或 policy 事实源。
- `ChannelAddress` 从 `MailboxSourceIdentity` 抽象出来的方向保留，但它只负责 source/delivery attribution。
- Mailbox 只负责 AgentRun durable consumption；LifecycleGate 只负责 wait/result authority。

## Not Ready For Implementation Until

- [ ] Channel 持久化方案明确：独立 `channels`/`channel_participants`/`channel_bindings` 表、Project asset，或混合方案。
- [ ] Project Channel assignment 与 Agent capability projection 的输入关系明确。
- [ ] 外部 IM 的 first slice 明确：只做 schema/service skeleton，还是包含某个 provider 的 bounded adapter。
- [ ] `ChannelAddress` 迁移影响面重新核对，尤其是 frontend 是否依赖 `display_label_key` 字符串前缀。
- [ ] 与 Extension Protocol Channel 的命名边界在后续 spec update 中写清；本任务只记录方向，不顺手改 extension runtime。

当前文档修订后，阻塞性问题不是"是否要 ChannelService"，而是下一步可执行任务如何切分持久化与 Project/IM 首个落地范围。
