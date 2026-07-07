# Channel 通信能力长期模型预评估执行记录

## Current State

本任务只记录预评估，不进入 implementation phase。不要运行 `task.py start`，除非后续用户明确要求把某个 MVP 切片进入实现。

## Suggested Future Phases

1. **Glossary / Spec**
   - 收敛 Channel、Participant、Message、Binding、Delivery、DeliveryPolicy、ReplyAddress、PublishOutbox、ChannelCapability。
   - 明确 Channel 与 Mailbox、LifecycleGate、PermissionGrant、RuntimeSession、Terminal output 的事实源边界。

2. **Channel Capability Dimension**
   - 已决策（2026-07-07）：新增 `CapabilityState.channel` 一等 dimension，`AccumulationPolicy::Accumulate`，effect 形态对齐 VFS `apply_mount_operations`/`MountDirective`（`ChannelDirective::{Expose,Revoke}`）。不做 projection-only surface，不对齐 Workspace Module 现状（现状是历史权宜的三段式混合实现）。
   - 定义 visible channel refs、aliases、operations、readiness、ingress/egress policy。
   - 待实现阶段确定：`CapabilityState` 新增必填字段的 serde 迁移策略（参照 `workspace_module` 字段引入先例）；是否需要 `PermissionGrant` 驱动的 channel 可见性（目前无通用机制，需另设计）。

3. **Ingress / DeliveryPlan Prototype**
   - 先在 application 内部定义 Channel envelope / delivery intent / materializer。
   - 覆盖 Companion reply、Terminal wake、Human response 等已有异步行为，验证进入 Mailbox 前的标准结构。

4. **Mailbox Materializer**
   - 将需要 AgentRun 消费的 ChannelDelivery 统一 materialize 为 AgentRunMailboxMessage。
   - 让 `ChannelAddress`（原 `MailboxSourceIdentity`，已决策整体重定位、不留别名）引用 Channel message / delivery attribution。

5. **Companion Facade Migration**
   - 将 `companion_request` / `companion_respond` 视为 Channel request/reply facade。
   - `target=sub` 作为在同一个 `LifecycleRun` 下新增 `lifecycle_agents` 行、并向 `LifecycleRun.channels` append 一条 `LifecycleChannel` 的 target resolver（不创建新 run_id）。

6. **Multi-Agent LifecycleRun / Project / Story Channel**
   - 验证 internal broadcast、role routing、shared channel context 和 multi-agent fan-out。（此前称 "AgentTeam Channel"；已决策 AgentTeam 不是独立实体，只是一个 `LifecycleRun` 下多 Agent 协作的说法。）

7. **External IM Adapter**
   - 引入 external binding、identity mapping、provider idempotency、publish outbox、approval/rate limit。

## Research Anchors

- `.trellis/tasks/07-07-channel-communication-capability-model/research/channel-discussion-journal.md`
- `.trellis/tasks/07-07-channel-communication-capability-model/research/v1-decision-evidence-and-open-items.md`（注意：其中 D3/D5 的表设计建议已被 design.md "Resolved" 最终版本覆盖，实现时以 design.md 为准）
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/tasks/06-28-agent-custom-channel-draft/design.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/design.md`

## Not Ready For Implementation Until

全部已决策（2026-07-07，经五轮对齐，其中持久化模型三次纠正）：

- [x] MVP 切片范围：Companion / SubAgent lifecycle-scoped temporary channel，parent/child 共享同一个 `LifecycleRun`（不创建新 run_id）。
- [x] Channel 持久化形态：`LifecycleRun.channels: Vec<LifecycleChannel>`，作为结构化字段随 `ALTER TABLE lifecycle_runs ADD COLUMN` 挂在既有 `lifecycle_runs` 表上（对齐 `orchestrations`/`execution_log` 先例），不新建 `channels`/`lifecycle_channels`/`channel_participants` 表。参与关系由各参与方 `CapabilityState.channel.visible_channels` 的引用表达。
- [x] `ChannelOwner` 模型：`AgentRun{run_id,agent_id}` 与 `AgentTeam{team_id}` 合并为 `LifecycleRun{run_id}`。
- [x] `ChannelAddress` 与 `MailboxSourceIdentity` 的关系：直接重定位为 `agentdash-domain::channel::ChannelAddress`，全部调用点迁移，不留别名/re-export。
- [~] 与 extension protocol channel 的命名边界：方向已决策（新 Channel 保留命名，Extension Protocol Channel 是重命名/收束候选），已写入本任务 `design.md`；写入全局 `.trellis/spec/` 是后续实现任务 Phase 3.3（Spec Update）的工作，不在预评估阶段做。

本预评估任务的阻塞性决策已经全部清空。下一步是另开一个可实施的 MVP 子任务，把这些决策转成真正的 `prd.md`/`design.md`/`implement.md` 执行计划（需要用户对是否创建新任务给出许可，规划仍先于实现）。
