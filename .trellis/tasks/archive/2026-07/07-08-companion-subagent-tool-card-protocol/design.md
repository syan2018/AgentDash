# Companion subagent 工具卡片与协议收束 - Design

## Boundary

本任务跨后端 Companion 工具结果、Backbone / Codex item 解析、前端 session tool card rendering、AgentRun projection 消费和 spec 记录。

业务坐标分层：

- Agent ref: `agent_id` 是 child subagent 的闭环产品坐标；父子 Agent 天然处于同一个 lifecycle run 下时，Agent 可见协议不需要重复携带 `run_id`。
- Lifecycle/workspace context: `run_id` 是当前工作区上下文，用于前端构造 AgentRun workspace 路径或从 lineage projection 解析状态，不作为模型可消费工具结果的必要字段。
- Frame/gate ref: `frame_id` / `gate_id` 是 dispatch、wait、result adoption 的业务诊断坐标。
- Journal access ref: Agent 和用户访问 child agent 完整 journal 的闭环入口，应稳定出现在工具结果中。
- Delivery runtime session id: 内部 delivery binding / scheduler / runtime trace 坐标，不进入 Agent 可见工具结果。
- Raw collab protocol: `senderThreadId` / `receiverThreadIds` 可保留为上游协议 raw 字段，但 AgentDash presentation 先解析为产品坐标。

## Proposed Contracts

后端 `companion_request target=sub` 返回 details 使用如下产品形状：

```json
{
  "kind": "companion_subagent_dispatch",
  "dispatch_id": "...",
  "wait": false,
  "companion_label": "...",
  "child": {
    "agent_id": "...",
    "frame_id": "...",
    "gate_id": "..."
  },
  "journal": {
    "uri": "lifecycle://agent-runs/<agent-id>/sessions/messages"
  },
  "mailbox": {
    "message_id": "...",
    "outcome": "launched"
  },
  "status": "running",
  "summary": "..."
}
```

等待完成和超时返回保留同一 `kind` 与 `child` / `journal`，通过 `status`、`summary`、`timed_out`、`result_preview` 表达结果。

`collabAgentToolCall` 解析策略：

- 保留原始 `senderThreadId` / `receiverThreadIds` 字段供 raw protocol 调试。
- AgentDash parser 将 receiver 映射为 `agent_id`。
- 前端通过当前 lifecycle/workspace context 或 children/list projection，用 `agent_id` 解析同 run child 的状态与跳转路径。
- UI 不以 raw thread id 为默认标题、目标或跳转参数。

## Frontend Flow

1. 在 session model/UI 层新增 Companion subagent presentation parser。
2. parser 支持：
   - `dynamicToolCall` with `tool=companion_request` and `arguments.target=sub`
   - `collabAgentToolCall` with `tool=spawnAgent`
3. parser 输出：
   - `childAgentId`
   - `title`
   - `status`
   - `summary`
   - `journalUri`
   - `openPath`
   - `rawProtocolRefs` for debug-only display
4. `openPath` 由当前 lifecycle/workspace `runId` 加 `childAgentId` 构造；若当前上下文不可用，则从 AgentRun list/lineage projection 解析。
5. `CompanionSubagentDispatchCardBody` 渲染卡片和跳转按钮。
6. `SessionEntry` / `threadItemKind` 将 subagent dispatch 从普通 tool burst 中分离，避免派发点被折叠成普通工具调用。
7. AgentRun projection 刷新沿用现有 `control_plane_projection_changed` 和 `useAgentRunWorkspaceState` / `useAgentRunListState`。

## Backend Flow

1. `CompanionChildDispatchOutcome` 继续内部持有 `delivery_runtime_session_id`，用于 mailbox delivery、gate wait、runtime launch。
2. `CompanionRequestTool` 构造 `AgentToolResult` 时只输出 child `agent_id`、业务诊断坐标和 journal access ref。
3. `CompanionRespondTool` 对 parent/child gate resolve 的返回也收束 runtime session id 暴露，保留 gate、agent、frame、journal/accepted turn 等闭环信息。
4. Hook evaluation/debug payload 如需 delivery runtime session id，应保持在 hook/runtime internal trace，不进入 Agent tool result。

## Tradeoffs

- 复用 AgentRun projection 可避免新增 subagent 状态事实源，但前端卡片需要处理 projection 尚未刷新的短暂空窗。
- raw collab protocol 保留能兼容 Codex 上游协议，但 UI 必须将 raw 字段和产品字段分开，避免再次把实现坐标当业务入口。
- journal URI 沿用当前 `lifecycle://agent-runs/{agent_id}/sessions/messages` 语义；在同一 lifecycle run 内，`agent_id` 已能唯一确定 child journal，避免把 workspace 上下文重复灌进 Agent 可见协议。

## Validation

- Rust tests: Companion 工具结果不包含 delivery runtime session id；保留 child `agent_id`、业务诊断 ref 和 journal access ref。
- TS tests: parser 识别 dynamic Companion 和 collabAgentToolCall；跳转路径生成正确；raw receiver thread 不默认展示。
- UI tests: 卡片渲染状态、按钮、摘要；普通 tool burst 不吞掉 subagent dispatch。
