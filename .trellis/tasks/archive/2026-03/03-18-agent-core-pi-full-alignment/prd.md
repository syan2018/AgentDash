# 彻底对齐 Agent Core 与 Pi Agent 语义

## Goal

对 `crates/agentdash-agent` 进行一次彻底的核心语义对齐，使其在不考虑兼容旧偏差行为的前提下，尽可能完整、严格地对齐 `references/pi-mono/packages/agent` 的 `Agent`、`agent-loop`、`types`、`proxy` 所定义的核心运行时契约。

本任务关注的是 `agent` package 内聚的核心逻辑，不是外围接入层的临时兜底。最终目标是让 `agentdash-agent` 本身成为一个语义上可以直接对照 Pi 参考实现理解、验证和演进的实现。

## Requirements

- 对齐 `Agent` 包装层生命周期语义：
  - `prompt()`、`continue()`、`abort()`、`reset()`、`wait_for_idle()` 的行为与状态流转必须与 Pi 核心语义一致。
  - 运行中必须具备明确的重入保护，避免并发 loop 破坏共享状态。
  - `Agent` 必须在内部持续消费 loop 事件并实时回写 `AgentState`，而不是仅在结束时一次性同步。
- 对齐 `continue()` 的完整恢复语义：
  - 在尾消息为 `assistant` 时，必须和参考实现一样优先尝试消费 queued steering / follow-up。
  - 必须保留 `one-at-a-time` 与 `all` 两种队列语义。
  - 对 assistant-tail、empty-context、tool-result-tail、user-tail 等路径给出完整且一致的行为定义。
- 对齐 `agent_loop` 事件生命周期：
  - `agent_start`、`turn_start`、`message_start`、`message_update`、`message_end`、`tool_execution_start`、`tool_execution_update`、`tool_execution_end`、`turn_end`、`agent_end` 的顺序和边界语义必须尽量与 Pi 一致。
  - `agent_end.messages` 与 loop 返回值必须表达“本轮新增消息”，不能错误返回全量历史。
  - `prompt` 路径与 `continue` 路径都必须符合参考实现定义。
- 对齐流式 assistant 事件语义：
  - 不仅要支持文本 delta，还要尽可能完整表达 text/thinking/toolcall 的 start、delta、end 族事件。
  - `MessageUpdate` 需要有足够的事件信息，让上层可以按 Pi 方式复原完整中间态。
  - 对无流式 delta、直接 done/error、代理流重建等情况也要给出一致行为。
- 对齐工具执行语义：
  - `beforeToolCall` 前必须进行工具参数校验，使 hook 和 tool execute 都拿到契约上已经校验过的参数。
  - `sequential` / `parallel` 的 prepare、execute、finalize 阶段顺序与结果聚合语义要尽可能一致。
  - `tool_execution_update`、`tool_execution_end` 与 tool result message 的发射顺序需与参考语义保持一致。
  - 需要重新审视工具结果 `content` / `details` / `is_error` 的表达与覆盖规则是否完整对齐。
- 对齐错误与取消语义：
  - 底层流错误、转换错误、取消、中止、tool 执行错误都要被规范化为一致的 assistant error / aborted 消息与事件序列。
  - 不能出现仅返回 `Err` 但消息历史和事件流不闭合的情况。
  - 需要明确哪些错误应该作为 loop error 抛出，哪些应该落成 assistant message。
- 对齐状态模型与消息模型：
  - 重新核对 `AgentState`、`AgentMessage`、`AgentEvent`、`AssistantStreamEvent`、`ToolCallInfo`、`AgentToolResult` 是否在核心语义上完整覆盖 Pi 契约。
  - 如当前 Rust 抽象存在过窄建模，应直接重构，不为现状妥协。
- 对齐低层与高层的一致性：
  - 低层 `agent_loop` 的语义、高层 `Agent` 的包装语义、`process_event` 的状态回写逻辑必须形成一个自洽闭环。
  - 不允许出现“低层像 Pi，高层不像 Pi”或“高层兜底掩盖低层偏差”的裂缝。
- 建立全面测试覆盖：
  - 至少覆盖参考实现中与核心语义有关的主要测试场景，包括事件顺序、continue 语义、队列模式、错误/取消、工具执行、流式更新。
  - 测试的目标不是验证当前实现，而是验证对齐后的契约。

## Acceptance Criteria

- [ ] `crates/agentdash-agent` 中与核心 agent 语义相关的实现已完成系统性对齐，而不是点状补丁。
- [ ] `Agent` 包装层在状态更新、重入保护、idle 管理、continue 恢复、错误闭环上与 Pi 参考语义一致。
- [ ] `agent_loop` 在 prompt/continue、turn 边界、消息边界、tool 执行边界、`agent_end` 返回值上与 Pi 参考语义一致。
- [ ] 流式 assistant 事件模型足以表达 Pi 参考实现中的核心中间态，至少不再只覆盖文本 delta。
- [ ] 工具调用前校验、hook 契约、并行/串行执行语义、工具事件顺序与 Pi 参考实现一致或更严格。
- [ ] 取消与错误路径会产出完整且闭合的消息/事件序列，不存在状态残缺和历史不一致。
- [ ] 有一组围绕 `references/pi-mono/packages/agent` 行为建立的对照测试，足以阻止后续回退。
- [ ] `crates/agentdash-agent/agent-design/PI_ALIGNMENT.md` 或相关设计文档已更新到“对齐后的真实状态”，不保留过期结论。

## Technical Notes

- 参考源以以下文件为准：
  - `references/pi-mono/packages/agent/src/agent.ts`
  - `references/pi-mono/packages/agent/src/agent-loop.ts`
  - `references/pi-mono/packages/agent/src/types.ts`
  - `references/pi-mono/packages/agent/src/proxy.ts`
  - `references/pi-mono/packages/agent/test/agent.test.ts`
  - `references/pi-mono/packages/agent/test/agent-loop.test.ts`
  - `references/pi-mono/packages/agent/test/e2e.test.ts`
- 本任务优先级是“语义正确性高于局部兼容性”。如果现有 Rust 抽象妨碍对齐，应直接重构。
- 本任务默认不把 `executor` / `connector` / 前端消费层适配视作主实现目标，但在核心契约收敛后，需要明确记录哪些外围模块将因此需要跟进适配。
- 在正式实现前，应先完成一轮系统化差异梳理，确保不存在仅凭已发现问题清单就低估偏差面的情况。
- 建议在实现过程中同步建立“Pi 事件序列 vs Rust 事件序列”的对照测试夹具，以降低后续回归风险。
