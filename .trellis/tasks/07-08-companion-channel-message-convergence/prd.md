# Companion 与 Channel message 语义收束

## Goal

修复 Companion 协作输入被投递为 `system_delivery` context frame / `system_message` 的语义偏差，并把 session timeline 的可见消息收束到 channel message 视角：消息本体保留模型投递语义，来源、参与者、route 与 UI 差分由 channel/source identity 描述。

本任务先完成规划与方案评估，不启动实现。后续实现需要同时更新协议、后端投递、前端渲染和相关 spec。

## Background

Companion 任务中的协作请求、回执、parent response 等内容本质上仍是需要 Agent 处理的输入，不应被塞入 system message frame。前端可以标注它来自 Companion 或 channel，但模型侧不应把它提升到 system authority。

已确认代码事实：

- `AgentRunMessageDelivery` 已使用 `Vec<UserInputBlock>` 承载消息输入，`MailboxMessageOrigin::Companion` 会进入 `LaunchCommand::companion_parent_resume_input`。证据：`crates/agentdash-application-agentrun/src/agent_run/message_delivery.rs`。
- `UserInputBlock` 是项目 canonical 用户输入单元，`PromptPayload::Input` 可结构化映射到 `ContentPart`，图片等多模态输入不会被拍平成文本。证据：`crates/agentdash-agent-protocol/src/backbone/user_input.rs`、`crates/agentdash-spi/src/connector/mod.rs`。
- 当前 `TurnPreparer` 会把 Companion launch source 生成为 `system_delivery` context frame，并把实际 prompt 替换成固定文本 `Continue from the AgentDash system delivery context for this turn.`。证据：`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs`。
- 当前 `TurnCommitter` 对 Companion 不发 `UserInputSubmitted`，而是持久化 `PlatformEvent::SessionMetaUpdate(key="system_message")`，`kind=companion_delivery`。证据：`crates/agentdash-application-runtime-session/src/session/launch/commit.rs`。
- Mailbox scheduler 对 non-user origin 也会生成 `system_message` projection，Companion steering / launch 仍可能通过这条路径显示为 system delivery。证据：`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs`。
- `channel` 模块已有 `ChannelAddress`、`ChannelMessage`、`ChannelDeliveryIntent`，并能把 `ChannelAddress` 映射到 `MailboxSourceIdentity`。证据：`crates/agentdash-domain/src/channel/mod.rs`、`crates/agentdash-application/src/channel.rs`。
- 旧任务 `06-28-integration-channel-mailbox-convergence` 已完成 mailbox source identity 底座，推荐使用 `namespace/kind/source_ref/correlation_ref/actor/route/display_label_key/metadata` 表达来源，而不是继续扩 enum。
- 现有 spec `backend/session/agentrun-mailbox.md` 仍写着 non-user mailbox delivery 进入 `system_message` / `system_delivery`，这是本任务需要修订的旧结论。

## Requirements

- R1: Companion 协作请求、结果、parent request、parent response、human response、parent resume 等 AgentRun-facing 输入必须保持 user-input 模型通道，由 Agent 作为本轮输入处理。
- R2: Companion 输入必须携带 channel/source provenance，至少表达 `namespace`、`kind`、`source_ref`、`correlation_ref`、`actor`、`route`、`display_label_key` 和可选 metadata。
- R3: `UserInputSubmitted` 必须从“人类用户输入事件”收束为“模型 user-role 输入事件”，并能区分 human user、companion、canvas、local relay、channel binding 等来源。
- R4: 前端 session timeline 必须基于 channel/source provenance 做展示差分：Companion 输入不能渲染成普通当前用户输入，也不能渲染成 system event；应以独立 Companion/channel 样式展示。
- R5: AgentRun mailbox scheduler 与 runtime-session launch commit 必须共享同一套投递分类，避免一条路径修正为 UserInput，另一条路径仍发 `system_message`。
- R6: `system_delivery` context frame 只保留给真正的平台/运行期控制事实，不承载需要 Agent 响应的 Companion 协作输入。
- R7: Transcript restore、context projection 和 fork/replay 必须把 Companion channel user input 当作 user-role message 恢复，确保模型在冷启动和历史恢复时看到同一语义。
- R8: Backbone Protocol、generated TypeScript、前端类型和测试必须同步更新；项目未上线，不需要保留旧字段兼容层。
- R9: 更新相关 Trellis spec，记录为什么 channel/source identity 是消息来源事实源，模型 role 是独立投递维度。

## Acceptance Criteria

- [ ] `design.md` 明确 channel message 视角下的消息维度：模型通道、来源身份、actor、route、UI presentation 分离。
- [ ] `design.md` 明确 Companion 输入为何进入 `UserInputSubmitted` 而不是 `system_delivery` context frame。
- [ ] `design.md` 给出 `UserInputSubmitted` 增加 source/channel provenance 的推荐字段和数据流。
- [ ] `design.md` 覆盖 runtime-session launch commit 与 AgentRun mailbox scheduler 两条投递路径。
- [ ] `design.md` 覆盖前端 session feed 的差分渲染策略。
- [ ] `implement.md` 给出后续实现顺序、测试范围、生成命令和 spec 更新点。
- [ ] `implement.jsonl` 和 `check.jsonl` 包含真实 spec / 既有设计上下文，支持后续子代理实现和检查。

## Out Of Scope

完整外部 IM broker、全局 channel registry UI、第三方 provider room/thread 绑定和跨项目 channel 搜索不纳入本轮实现；本轮只把 AgentRun/session timeline 已经消费的消息投影改成 channel/source aware。

Hook / workflow / routine 的模型通道是否全部重分级不在本轮一次性完成；本轮只为它们保留 source identity 的扩展路径，并优先修复 Companion 这类明确应为 user-role input 的协作消息。

## Open Questions

无阻塞问题。推荐方案是先做“channel/source provenance + UserInputSubmitted 语义扩展”的窄切口，不先引入完整全局 channel broker。
