# 前端 Session 渲染扩展设计

> 在 lifecycle session 模型下，前端如何在一个会话视图中完整展示关联 session 的内容。

## 1. 当前前端渲染架构

### 1.1 消息流渲染管线

```
Raw Events (WebSocket + DB)
  → useAcpStream（事件归约为 AcpDisplayEntry[]）
    → useAcpSession（聚合同类型 tool call → AggregatedEntryGroup）
      → SessionChatView（布局 + 输入）
        → AcpSessionEntry（分发到具体 card）
          → AcpMessageCard / AcpToolCallCard / AcpSystemEventCard / ...
```

### 1.2 现有的嵌套/展开模式

已有多种可展开内容模式：

| 模式 | 组件 | 行为 |
|---|---|---|
| Tool call 折叠 | `AcpToolCallCard` | header 可点击展开 input/output |
| Aggregated tool group | `AggregatedToolGroupEntry` | 一行摘要，展开显示所有 tool call |
| Thinking 折叠 | `AggregatedThinkingGroup` | 折叠为一行 "TH"，展开显示思考过程 |
| System event card | `EventStripCard` / `EventFullCard` | Strip 紧凑 / Full 展开详情 |
| Companion request card | `AcpCompanionRequestCard` | 交互式卡片（按钮/输入框） |

### 1.3 Session 列表中的 Parent-Child 嵌套

`active-session-list.tsx` 已实现：
- Root session 作为主卡片
- Companion session 缩进显示（`ml-3 border-l`），前缀 "↳"
- 通过 `parent_session_id` 分组

### 1.4 Companion 事件类型

前端已识别的 companion 系统事件：
- `companion_dispatch_registered` — 子 session 已创建
- `companion_result_available` — 子 session 结果已收到
- `companion_result_returned` — 结果已回传
- `companion_human_request` — 向用户提问
- `companion_human_response` — 用户已回答
- `companion_review_request` — 向 parent 发送 review

## 2. 目标体验（修订后：无 host session 模型）

> 经讨论后确认：lifecycle session 不是某个 agent session 的消息流 + 嵌套子 session。
> 而是 lifecycle run 的一级视图，每个 agent node 是并列的卡片。

### 2.1 Lifecycle Session 视图

**没有"主消息流"**。Lifecycle session 是一个 node 列表视图，每个 node 有自己的 session 消息流。

类比：
- ~~旧模型：一个长对话，中间穿插子 session 展开区域~~
- **新模型：一个项目看板，每个 node 是一张卡片，点开卡片看到该 agent 的完整对话**

```
┌──────────────────────────────────────────────────────────┐
│  Lifecycle: trellis_dev_task                             │
│  Status: running  |  Progress: 2/3 nodes                 │
│  [research] ✅ ──▶ [implement] 🔄 ──▶ [check] ⏳        │
│                                                          │
│  ┌── research (completed) ────────────────────────────┐  │
│  │  Agent: research-agent  |  Duration: 3m            │  │
│  │  Artifacts: [session_summary]                      │  │
│  │  ▼ 展开完整对话                                     │  │
│  │  ┌────────────────────────────────────────────────┐│  │
│  │  │ [AI] 我来调研相关代码模式...                     ││  │
│  │  │ [TOOL] Grep "lifecycle" ...                     ││  │
│  │  │ [AI] 调研完成，核心发现如下...                   ││  │
│  │  └────────────────────────────────────────────────┘│  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌── implement (running) ─────── [📝 可输入] ─────────┐  │
│  │  Agent: implementation-agent  |  Running...         │  │
│  │  ▼ 展开完整对话                                     │  │
│  │  ┌────────────────────────────────────────────────┐│  │
│  │  │ [AI] 我来分析需求并开始实现...                   ││  │
│  │  │ [TOOL] Edit src/workflow/entity.rs              ││  │
│  │  │ ...                                             ││  │
│  │  │ [输入框: 用户可以直接和 implement agent 对话]    ││  │
│  │  └────────────────────────────────────────────────┘│  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌── check (pending) ─────────────────────────────────┐  │
│  │  等待前置 node 完成...                               │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

### 2.2 三种展示层级（不变）

| 层级 | 默认状态 | 内容 |
|---|---|---|
| **摘要行** | 始终可见 | node 名称 + 状态 badge + agent 名称 + 耗时/消息计数 |
| **产物摘要** | 折叠 | artifact 列表（类型 + 标题） |
| **完整对话** | 折叠 | 该 node 的 agent session 完整消息流（复用 `AcpSessionList`） |

当前活跃 node 可默认展开对话 + 提供输入框。

### 2.3 实时更新（不变）

当 agent node session 正在执行时：
- 摘要行实时更新状态（running → completed）
- 消息/工具计数实时递增
- 如果用户展开了完整对话，实时流式显示该 session 的消息

## 3. 组件设计

### 3.1 新增页面级组件：`LifecycleSessionView`

这不是嵌入 `AcpSessionEntry` 的子组件，而是一个 **独立的页面级视图**（替代或并列于 `SessionChatView`）。

```tsx
// 概念结构
function LifecycleSessionView({ runId }: Props) {
  const run = useLifecycleRun(runId);  // 获取 lifecycle run + node states

  return (
    <div className="flex flex-col gap-4">
      {/* 顶部：lifecycle 进度概览 */}
      <LifecycleProgressBar run={run} />

      {/* 主体：node 卡片列表 */}
      {run.nodeStates.map((node) => (
        <LifecycleNodeCard
          key={node.nodeKey}
          node={node}
          isActive={node.status === "running"}
        />
      ))}
    </div>
  );
}
```

### 3.2 Node 卡片组件：`LifecycleNodeCard`

```tsx
function LifecycleNodeCard({ node, isActive }: Props) {
  const [expanded, setExpanded] = useState(isActive);

  return (
    <div className="border rounded-lg">
      {/* 摘要行 - 始终可见 */}
      <NodeHeader
        nodeKey={node.nodeKey}
        status={node.status}
        agentName={node.agentName}
        duration={node.duration}
        onToggle={() => setExpanded(!expanded)}
      />

      {/* Artifact badges */}
      <NodeArtifactBadges artifacts={node.artifacts} />

      {/* 展开后：嵌入该 node 的 agent session 消息流 */}
      {expanded && node.sessionId && (
        <div className="border-t">
          <AcpSessionList sessionId={node.sessionId} />
          {/* 活跃 node 提供输入框 */}
          {isActive && <SessionInput sessionId={node.sessionId} />}
        </div>
      )}
    </div>
  );
}
```

### 3.3 复用 `AcpSessionList`

`AcpSessionList` 已经是一个独立的消息流渲染组件，内部调用 `useAcpSession` 获取流数据。直接嵌入到 `LifecycleNodeCard` 中，只需传入 node 的 `sessionId`。

最大复用点——不需要重新实现消息渲染。

### 3.4 Lifecycle 进度条

顶部的进度条/DAG 视图组件。初期可以是线性进度条，后续 Phase 2 演进为 DAG 可视化。

已有雏形：`TaskWorkflowPanel` 已在展示 lifecycle run 状态和 step 列表，可以在此基础上演进。

## 4. 数据流设计

### 4.1 Node Session 事件订阅

每个 node 卡片展开后，需要该 node 的 agent session 实时消息流。

直接为展开的 node 创建独立的 `useAcpStream` 实例（`AcpSessionList` 已封装此逻辑）。展开时建立 WebSocket 连接，折叠时断开。多个 node 同时展开就有多个连接——简单直接，无需后端改造。

### 4.2 Lifecycle Run 状态获取

lifecycle 面板需要 lifecycle run 的实时状态。可以通过：

1. 现有的 `workflowStore.fetchRunsByTarget` 轮询
2. 或利用 SSE event store 监听 lifecycle run 状态变更事件（如果后端支持）

初期用方案 1（轮询），后续需要时加 SSE 推送。

## 5. Companion 渲染的关系（修订后）

由于 lifecycle session 不再是某个 agent session 的消息流，companion 事件渲染的问题简化了：

- **Lifecycle 视图层面**：不渲染 companion 事件，渲染的是 node 卡片
- **Node 内部展开后**：该 node 的 agent session 消息流中，companion 事件按现有方式渲染（subagent dispatch、human request 等）

唯一的新需求：如果 agent node session 内部有 companion subagent，在 node 卡片的消息流中可以看到嵌套的 companion session（已有能力）。

## 6. Phase 的渲染（修订后）

Phase 是 agent node 内部的概念。在该 node 的 session 消息流中，通过系统事件标记 phase 边界：

```
[node: implement 的 session 消息流]
── Phase: analyze ──────────────────────────
[AI] 让我先分析需求...
[TOOL] Read prd.md
── Phase: analyze completed ────────────────

── Phase: code ─────────────────────────────
[AI] 现在开始写代码...
[TOOL] Edit entity.rs
...
```

用现有的 `EventStripCard` 渲染分隔标记即可，不需要新组件。

## 7. 待确认的 UX 问题

1. **活跃 node 默认展开还是折叠？** — 建议活跃 node 默认展开对话 + 输入框
2. **lifecycle 视图的路由** — 独立路由（`/lifecycle/{run_id}`）还是嵌入 task/story 详情页？
3. **三层嵌套展示** — lifecycle → agent node session → companion subagent session，如何避免嵌套过深
4. **Lifecycle 进度条的形态** — Phase 1 用线性进度条，Phase 2 用 DAG 可视化（reactflow 等）
