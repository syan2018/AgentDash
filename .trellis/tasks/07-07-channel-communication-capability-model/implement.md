# Channel 通信能力长期模型预评估执行记录

## Current State

本任务只记录预评估，不进入 implementation phase。不要运行 `task.py start`，除非后续用户明确要求把某个 MVP 切片进入实现。

## Suggested Future Phases

1. **Glossary / Spec**
   - 收敛 Channel、Participant、Message、Binding、Delivery、DeliveryPolicy、ReplyAddress、PublishOutbox、ChannelCapability。
   - 明确 Channel 与 Mailbox、LifecycleGate、PermissionGrant、RuntimeSession、Terminal output 的事实源边界。

2. **Channel Capability Prototype**
   - 评估是否新增 `CapabilityState.channel` 或 projection-only surface。
   - 定义 visible channel refs、aliases、operations、readiness、ingress/egress policy。
   - 对齐 Workspace Module capability 模式。

3. **Ingress / DeliveryPlan Prototype**
   - 先在 application 内部定义 Channel envelope / delivery intent / materializer。
   - 覆盖 Companion reply、Terminal wake、Human response 等已有异步行为，验证进入 Mailbox 前的标准结构。

4. **Mailbox Materializer**
   - 将需要 AgentRun 消费的 ChannelDelivery 统一 materialize 为 AgentRunMailboxMessage。
   - 让 `MailboxSourceIdentity` 引用 Channel message / delivery attribution。

5. **Companion Facade Migration**
   - 将 `companion_request` / `companion_respond` 视为 Channel request/reply facade。
   - `target=sub` 作为带 channel / AgentRun provision 副作用的 target resolver。

6. **AgentTeam / Project / Story Channel**
   - 验证 internal broadcast、role routing、shared channel context 和 multi-agent fan-out。

7. **External IM Adapter**
   - 引入 external binding、identity mapping、provider idempotency、publish outbox、approval/rate limit。

## Research Anchors

- `.trellis/tasks/07-07-channel-communication-capability-model/research/channel-discussion-journal.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/tasks/06-28-agent-custom-channel-draft/design.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/design.md`

## Not Ready For Implementation Until

- 第一阶段 MVP 切片从 Companion / SubAgent lifecycle temporary channel 开始的范围已被用户最终确认。
- `LifecycleChannel` 最小实体 / 表命名已决策。
- `ChannelAddress` 与 `MailboxSourceIdentity` 是嵌入、映射还是同构双类型已决策。
- Channel message persistence 的第一版范围已决策。
- 与 extension protocol channel 的命名边界已写入 spec。
