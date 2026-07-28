# Compaction 控制面与状态面收束评估

## 参考任务

本评估参考相邻任务：

- `.trellis/tasks/07-28-runtime-session-state-chain/prd.md`

该任务确认的核心方向是：

- concrete Complete Agent 是 execution、history、effect 的唯一事实 owner；
- Agent Runtime 只负责进程内解析、协调、normalize、连接与广播，不形成新的 durable aggregate；
- 同一 AgentRun target 应只有一条 Agent projection connection；
- Feed、Composer、Agent状态 UI 从同一次 Agent read/change observation 收敛；
- Product Workspace 保留 Product shell、AgentFrame、resource surface、subject/lineage 与 Workspace Module，不重复拥有 Runtime execution/commands。

Compaction 必须沿用这条链，不能建立单独的 compaction store、Workspace refresh planner 或 context event side-effect lane。

## 当前平行链路

### Runtime presentation/state lane

`useSessionStream` 内部创建 `useManagedRuntimeFeed`：

- `packages/app-web/src/features/session/model/useSessionStream.ts:98-108`
- `packages/app-web/src/features/agent-run-runtime/model/useManagedRuntimeFeed.ts:30-86`

feed baseline 是 `ManagedRuntimeSnapshot`，live event目前只合并
`conversation_history`：

- `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts:73-110`
- `packages/app-web/src/features/agent-run-runtime/model/agentLiveProjection.ts:22-43`

`useSessionStream` 又按当前数组位置生成 `event_seq = index + 1`：

- `packages/app-web/src/features/session/model/useSessionStream.ts:109-139`

因此这条 lane适合presentation projection，却没有稳定承载跨快照控制事实的坐标。

### Product Workspace control lane

`useAgentRunWorkspaceControlPlane` 从 Workspace HTTP snapshot 的
`workspaceControl.conversation` 构造 execution 与 commands：

- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:206-230`
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:271-292`

它通过 Conversation Feed presentation event反向决定何时刷新 Workspace snapshot：

- `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:157-188`
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:306-333`

Product backend自身又把 `ManagedRuntimeSnapshot` 重新还原成较弱的
`AgentObservation`：

- `crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs:396-438`

该还原只从 conversation history推导 active Turn，再由 Workspace query再次派生
execution：

- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:397-435`

这形成：

```text
Complete Agent read
  -> Runtime normalized snapshot
     ├─ frontend Runtime feed -> presentation
     └─ Product Workspace query -> reconstructed observation
                                -> execution/commands
                                -> frontend Workspace HTTP state

presentation turn event
  -> frontend planner
  -> refresh Product Workspace HTTP state
```

Compaction item不是Turn，所以现有planner不会刷新commands；即使加
`ContextCompactionStarted/Completed`特判，双状态面的时序、staleness与重复投影仍然存在。

## SourceObservation 已提供正确原子单位

`AgentChangePayload::SourceObservation` 明确规定：

> One source observation may update normalized service state and append zero or more immutable presentation records. Runtime must preserve both parts atomically.

证据：

- `crates/agentdash-agent-service-api/src/snapshot.rs:189-198`

因此 Compaction 的正确 observation 应同时携带：

```text
state:
  active activity / terminal outcome
  command availability basis
  context revision/checkpoint evidence

presentation:
  ContextCompaction item lifecycle
  CompactionSummary ContextFrame
```

状态与presentation来自同一次source observation，但消费职责不同：

- state驱动Composer、Compact、Fork、Cancel、Context query revision；
- presentation只驱动timeline/audit；
- presentation不能再次触发Workspace HTTP refresh来获得控制事实。

## 三个逻辑平面

### 1. Authority/state plane

Owner：concrete Complete Agent。

内容：

- lifecycle；
- active activity；
- interactions；
- operation outcome；
- command availability基础事实；
- source/context revision；
- current checkpoint/context recipe；
- canonical presentation records。

Runtime只建立可重建的normalized view，不拥有这些事实。

### 2. Control plane

Owner：仍是concrete Complete Agent command admission。

前端通过同一 Agent Runtime view读取：

- 当前可执行命令；
- stale guard/source revision；
- activity phase与cancellable；
- command operation receipt。

Composer、Compact、Fork、Close、interaction response不得从Product Workspace
conversation副本或presentation event推导可用性。

本地pending只表示请求发送中，不代表Agent operation状态。

### 3. Presentation/query plane

Conversation Feed渲染canonical records；Context inspector查询当前
`AgentContextSnapshot`。二者都以同一target/source/context revision为坐标，但不成为
control事实owner。

Context payload可能较大，不必塞入每次Runtime baseline。推荐：

- Agent Runtime view暴露`context_revision/recipe_digest/fidelity`；
- Context inspector按这些坐标查询完整`AgentContextSnapshot`；
- response必须匹配target与required revision；
- frame或item presentation事件只用于即时展示overlay，不作为revision authority。

## Compaction 对统一 Runtime view 的最小扩展

相邻任务目标中的 `AgentRuntimeView` 至少应能表达：

```text
AgentRuntimeView {
  target
  source
  source_revision
  connection_status

  lifecycle
  active_activity
  last_compaction_outcome?
  command_availability

  context_revision?
  context_recipe_digest?
  context_fidelity

  canonical_conversation
}
```

`AgentRuntimeConnection`负责：

- authoritative baseline；
- Agent source change tail；
- gap/reconnect reload；
- process-local presentation overlay；
-同一 source observation中 state与presentation的原子投影；
- target切换隔离。

它不持久化第二份aggregate，也不让每个调用方建立独立连接或游标。

## 对原 Compaction 方案的修正

### 不再新增独立 frontend compaction store

原设计中的 `activity/lastCompactionOutcome/commandAvailability` 应成为统一
Agent Runtime view的selectors，不是Session feature自己的第二份状态。

Session只保留：

- 命令请求的短暂transport pending；
- context query的loading/error/result；
- presentation UI局部展开状态。

### 不再依赖 control-plane event planner刷新commands

Compaction state observation直接更新统一 Runtime view。Workspace planner不需要新增
`context_compaction`事件特判。

Product Workspace若需要展示Agent状态，只读取同一 view selector；其HTTP snapshot不再拥有
execution/Runtime commands。

### Context inspector是revision-bound query

完整recipe不应由conversation history/event position重建，也不应塞入Workspace snapshot。

```text
Runtime view.context_revision changes
  -> context query(required_revision)
  -> Agent owner materializes AgentContextSnapshot
  -> target/revision/digest commit fence
  -> inspector render
```

### Terminal convergence不按presentation类型枚举

当前 `managedRuntimeFeedConnection.ts:46-48` 只把 `turn_completed` 当权威reload边界。
目标Connection应消费Agent source change/state revision；凡state observation或gap需要reload时，
按revision协议收敛，不再维护“Turn terminal、Compaction terminal、未来activity terminal”的
presentation类型清单。

## 任务关系

`07-28-runtime-session-state-chain` 应拥有：

- `AgentRuntimeConnection/View` 的统一链路；
- 去除Workspace execution/commands副本；
- 去除presentation数组下标作为control cursor；
- Composer/Stop对统一view的接入。

本 Compaction 任务应拥有：

- Complete Agent 的typed Compaction activity/checkpoint/context recipe；
- activity对应的command matrix；
- Compaction state observation与typed presentation；
- Runtime view所需的compaction字段；
- revision-bound Context inspector；
- compaction-specific vertical tests。

依赖关系不是“两个任务各自修一遍前端”：

```text
Runtime state-chain foundation
  -> Compaction activity + context contract
  -> Compaction UI selectors/query
```

若并行实施，Compaction任务先完成后端contract与fixtures，前端接线必须等待统一
AgentRuntimeView seam稳定，不能临时接回Workspace conversation state。

## 结论

Compaction前端问题的最终修复不应是：

- 给 `controlPlaneModel` 增加几个compaction事件；
- 给Session增加一个独立`isCompacting`；
- terminal时同时刷新多个HTTP snapshot；
- 从ContextFrame或timeline推断context revision。

正确收束是：

- concrete Complete Agent拥有状态与命令事实；
- 一次source observation原子携带state与presentation；
- 一个Agent Runtime connection/view服务Feed、Composer与Agent状态UI；
- Product Workspace退出Runtime execution/command owner角色；
- Context inspector按统一view发布的context revision查询Exact/Observed recipe。
