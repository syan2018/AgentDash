# Compaction 上下文与前端状态完整性评估

## Goal

完整审计 Compaction 的上下文正确性、状态协议与前端行为，并以 concrete Complete Agent
authority 为唯一事实源，收敛 Compaction 的状态面、控制面和展示/查询面。

用户价值：

- 压缩期间能看到真实、可恢复的 activity，不会遇到按钮可点但 owner 拒绝的假状态。
- 压缩完成、失败、lost、取消、重载和重连后，命令状态与当前模型上下文自动收敛。
- Native 用户看到的 Exact context recipe 与下一轮 provider输入一致。
- Codex 无法证明的 provider-private context明确标为Observed/Opaque。

## Background

相邻任务 `.trellis/tasks/07-28-runtime-session-state-chain/prd.md` 已确认：

- concrete Complete Agent 是 execution、history、effect 的唯一事实 owner；
- Agent Runtime 是进程内协调、normalize、连接与广播机制，不拥有durable aggregate；
- 同一 AgentRun target应只有一条Agent projection connection；
- Feed、Composer与Agent状态UI应从同一次Agent `read/changes` observation收敛；
- Product Workspace只拥有Product shell、AgentFrame、resource surface、subject/lineage与
  Workspace Module，不重复拥有Runtime execution/commands。

本任务沿用上述边界，不建立独立Compaction store、第二条Runtime connection或
Workspace event-refresh补丁。

## Confirmed Facts

### Context correctness

- P0：`BridgeDashCompactor` 当前并非接续原 Session 的 Compaction Turn，而是从 history
  手工重建一套 summary transcript，再用独立 system prompt发起无状态 bridge request；
  这既破坏正常 Turn 的精确上下文前缀，也使 provider prefix cache无法稳定复用。
- P0：`crates/agentdash-integration-native-agent/src/bridge_execution.rs:297-362`
  的compactor输入只物化用户/助手文本，遗漏`ToolCall/ToolResult`；被cut且不在retained tail的
  工具事实会从后续模型输入真实消失。
- P0：`crates/agentdash-integration-native-agent/src/canonical_projection.rs:197-207`
  把`CompactionFailed/Lost`投影为成功`ItemCompleted`；
  `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:53-67`
  又把该item当作有效context boundary。
- P1：Native provider materializer按`retained_from`恢复suffix：
  `crates/agentdash-agent/src/dash/service.rs:2070-2191`；Product projector却按completed event
  位置截断，因此用户视图遗漏模型仍使用的retained messages。
- P1：协议已有typed `CompactionSummary` section：
  `crates/agentdash-agent-protocol/src/backbone/context_frame.rs:530-558`；producer仍生成
  `SystemNotice`：`crates/agentdash-agent/src/dash/history.rs:224-268`。

### State/control convergence

- Dash fold已保存`active_compaction`：
  `crates/agentdash-agent/src/dash/history.rs:531-548`，但公共
  `AgentSnapshot`没有active activity：
  `crates/agentdash-agent-service-api/src/snapshot.rs:97-120`。
- Runtime command availability只从canonical active Turn派生：
  `crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs:119-132,202-249`。
- `AgentChangePayload::SourceObservation` 已定义state与presentation的原子observation：
  `crates/agentdash-agent-service-api/src/snapshot.rs:189-198`。
- 当前Frontend Runtime feed只把live event合入`conversation_history`：
  `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts:73-110`。
- Product Workspace又从Workspace HTTP snapshot中的conversation构造execution/commands：
  `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:206-230`。
- Conversation presentation event反向驱动Workspace refresh：
  `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:157-188`。
- compaction不是Turn，因此现有planner既不刷新control state；给planner增加compaction特判也只会
  延续Runtime feed与Workspace HTTP state的双状态面。

### Frontend behavior

- `item_started(contextCompaction)`已经到达reducer，但
  `packages/app-web/src/features/session/model/types.ts:616-633`固定返回`completed`。
- popup pending只覆盖本地HTTP promise：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:251-307`。
- context projection请求没有target/revision commit fence：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:471-509`。
- Runtime feed只把`turn_completed`当authoritative reload boundary：
  `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts:46-48`。

## Requirements

- R1：concrete Complete Agent必须是Compaction activity、checkpoint、context recipe、terminal
  outcome与command admission的唯一事实owner。
- R1a：Compaction必须作为原 Agent Session中的正式 Turn执行，直接复用正常 Turn 的精确
  context materialization，只在稳定prefix末尾追加一次synthetic compaction instruction；
  禁止手工维护第二套summary transcript。
- R1b：Compaction provider output只作为checkpoint candidate；synthetic instruction与候选
  summary不得持久化成普通conversation records。
- R2：Compaction state与canonical presentation必须由同一次`SourceObservation`原子投影；
  presentation只负责timeline/audit，不得反向充当control事实。
- R3：统一Agent Runtime connection/view必须承载active activity、commands、source/context
  revision、authority/fidelity和canonical presentation；不得建立Compaction专用连接或状态owner。
- R4：Composer、Compact、Fork、Close、Cancel与interaction response只消费统一Runtime view的
  command availability和stale guard。
- R5：Product Workspace不得重复拥有Runtime execution、Compaction activity或commands；
  Workspace reload/error不得改变Composer control state。
- R6：Context inspector按Runtime view发布的`context_revision/recipe_digest`查询完整
  `AgentContextSnapshot`；不得从timeline event位置重建context membership。
- R7：Native的Exact recipe必须与provider capture的成员、顺序、tool pairing和digest一致；
  Codex必须保留Observed/Opaque fidelity。
- R8：只有成功Applied checkpoint推进context revision；failed/lost/cancelled不改变active recipe。
- R9：生产summary frame必须使用typed `CompactionSummary` section并暴露checkpoint provenance；
  retained conversation仍保持canonical records，不复制成ContextFrame。
- R10：Compaction执行必须可durable恢复；Started后进程退出不得永久保留Accepted/active source。
- R11：manual queue、deferred input、cancel与command matrix遵循既有`07-17`契约，前后端语义一致。
- R12：不引入兼容双读、旧shape fallback、定时轮询或presentation事件refresh特判。

## Planning Deliverables

- [x] Backend、Runtime、Product、Frontend与ContextFrame三路审计完成。
- [x] 问题严重度、代码证据、场景矩阵和既有任务覆盖完成。
- [x] 控制面、状态面、presentation/query plane边界完成收敛。
- [x] `design.md`与`implement.md`已按统一Runtime state chain修订。
- [x] 阻塞产品问题为空。

## Target Acceptance Criteria

- [ ] AC1：Compaction activity从Complete Agent source observation进入同一Agent Runtime view；
  reload任一phase可恢复同一operation id与phase。
- [ ] AC1a：Compaction以包含`ContextCompaction` item的正式Turn出现，canonical lifecycle完整；
  failed/lost以Turn error终止，不能伪装成成功item。
- [ ] AC2：同一AgentRun target只有一个Agent projection connection；Feed、Composer、Compaction UI
  与Agent状态UI消费同一observation。
- [ ] AC3：Product Workspace不再拥有Runtime execution/commands副本，也不通过presentation event
  refresh获得control事实。
- [ ] AC4：manual/automatic compaction期间，提交、重复压缩、Fork、Close、Cancel等行为与owner
  command matrix一致；草稿编辑和只读浏览保持可用。
- [ ] AC5：failed/lost/cancelled显示正确terminal，且active context recipe/revision保持不变。
- [ ] AC6：Native Compaction Turn request与同一时刻正常Turn context拥有结构完全相同的prefix，
  包含retained messages、tool call/result pairs与active ContextFrames；只额外追加一次压缩指令，
  且本轮不开放structured tools。
- [ ] AC7：typed `CompactionSummary` frame完整暴露checkpoint identity、边界、revision、digest、
  strategy、统计和usage freshness。
- [ ] AC8：context inspector按target与required revision自动收敛；旧target/旧revision响应不能提交。
- [ ] AC9：Started、provider side effect、Applied和outer receipt各崩溃点均不会留下永久Accepted；
  outcome确定为succeeded/failed/lost/cancelled。
- [ ] AC10：Codex页面明确显示Observed/Opaque，不声称Exact context。
- [ ] AC11：生产顺序fixture、provider capture、reload/reconnect、crash injection和浏览器E2E通过。
- [ ] AC12：相关spec只记录最终owner、state/control/presentation边界与设计原因。

## Scope Ownership

相邻 `07-28-runtime-session-state-chain` 负责：

- 统一`AgentRuntimeConnection/View`基础设施；
- Complete Agent baseline/change tail、gap reload和presentation overlay；
- 去除Workspace Runtime execution/commands副本；
- Composer/Stop接入与control cursor修复。

本任务负责：

- 原 Session精确上下文上的Compaction Turn执行语义；
- typed Compaction activity、terminal、checkpoint与context recipe；
- compaction command matrix与durable execution/recovery；
- Compaction state observation和typed presentation；
- Runtime view所需的Compaction字段；
- revision-bound Context inspector与Compaction纵向测试。

## Out of Scope

- 为Compaction单独重做相邻任务拥有的Runtime connection基础设施。
- 把所有页面交互在压缩期间整体锁死。
- 把conversation records复制成ContextFrame。
- 为不可观察的provider-private context制造推测值。
- 保留Product/Runtime双command规则或旧context projection兼容路径。

## Deferred, Non-blocking

- 当前`LlmBridge`没有server-side thread/session coordinate。Slice 1先保证同一精确上下文、
  稳定prefix与prefix cache语义；未来provider-native remote compaction作为能力实现同一
  Compaction Turn契约，不引入平行fallback设计。
- `AgentRuntimeView/AgentRuntimeConnection`的最终公开命名由相邻任务收敛；本任务依赖的是其
  “单一可重建Runtime read/change view”语义，不依赖临时DTO名称。
- 本轮结论来自静态源码审计；真实LLM、PostgreSQL crash-injection、Codex pinned schema fixture
  与浏览器E2E在实施验证阶段完成。
