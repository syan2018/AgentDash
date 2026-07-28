# 当前链路证据

## 领域边界

- `CONTEXT.md` 将 AgentRun Product 定义为产品 aggregate，将 concrete Complete Agent 定义为
  history、execution 与 effect 的事实 owner，将 Agent Runtime 定义为进程内协调、协议映射与
  live broadcast 机制。
- `.trellis/tasks/07-20-agent-runtime-persistence-authority-convergence/` 已完成 Runtime
  repository、协调账本和跨 owner revision gate 的删除。当前任务不能重新建立 Runtime-owned
  durable aggregate。
- `.trellis/spec/project-overview.md` 仍把 Managed Agent Runtime 描述为唯一执行事实源，与当前
  glossary、代码和已完成架构任务冲突。

## 后端事实链

- `ManagedRuntimeSnapshot` 定义于
  `crates/agentdash-agent-runtime-contract/src/managed_projection.rs`，当前包含 lifecycle、
  interactions、command availability 和 canonical conversation history。
- `project_authoritative_agent_snapshot` 位于
  `crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs`，证明上述 snapshot 是从
  Complete Agent authority 即时生成的 normalized read model。
- `AgentChangePayload::SourceObservation` 位于
  `crates/agentdash-agent-service-api/src/snapshot.rs`，其合同明确要求一次 source observation
  原子携带 state 与 presentation records。
- `AgentLiveEvent` 位于 `crates/agentdash-agent-service-api/src/live.rs`，合同明确说明它只是
  process-local presentation data，sequence 可在 service 重启后重置。
- `AgentRunProductProjectionGateway` 位于
  `crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs`，负责从
  Product association 解析 Complete Agent service/source，并将 Agent snapshot normalize 成
  application-facing view。
- Workspace query 位于
  `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs`。它当前把 normalized
  snapshot 还原为 `AgentObservation`，再重新派生 execution、commands 和 Workspace state。

## 前端故障链

- `useSessionStream.ts` 按 `conversation_history` 数组位置生成 `event_seq = index + 1`。
- `SessionChatView.tsx` 使用跨 render 保留的 `lastLiveEventSeqRef` 过滤 live side effects。
- 权威 terminal reload 收缩 presentation history 后，下一轮记录的数组序号可能小于旧 cursor；
  `turn_started` 因此被过滤。
- Workspace Control Plane 依赖该 `turn_started` 触发 Workspace reload，导致 execution 保持
  `ready`，Composer 不显示停止按钮。
- 最小诊断测试已证明：旧 cursor 为 24、收敛后 `turn_started` 序号为 18 时，当前 dispatcher
  不派发任何事件。

## 影响面

- `ManagedRuntime` 词汇命中约 52 个 Rust、TypeScript、schema 和 spec 文件。
- snapshot/feed 词汇命中约 33 个文件。
- Workspace conversation execution/commands/control refresh 命中约 15 个文件。
- 三组文件存在重叠；预计实际修改 55–70 个文件，其中行为关键文件约 20–30 个，其余为生成物、
  fixtures、测试和规范同步。

