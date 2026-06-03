# WorkspacePanel / Session / Lifecycle 状态归属规划

## 目标

Session 页保持用户可理解的会话入口；WorkspacePanel 展示当前会话对应 Agent 运行表面；LifecycleRun 提供运行账本与 Workflow 进度观察。三者的连接由 `runtime_session_id -> AgentFrame -> LifecycleAgent -> LifecycleRun` 解析链路建立，而不是让 Session 自身承载业务执行状态。

## 当前修复范围

本轮先恢复两条既有链路：

- Session header 在解析到当前会话对应的 run 后显示“运行详情”，进入现有 `/run/:runId` detail 页面。
- Session 右侧 WorkspacePanel 的 pinned `context` tab 接收当前 run projection，使既有 `ContextOverviewTab` 能展示 Workflow / run / attempt / progress。

这能让用户仍从 Session 出发查看运行状态，同时不引入新的一级 Lifecycle 导航概念。

## 状态归属

| 状态 | 归属 | 原因 |
| --- | --- | --- |
| Session title、stream/feed、event history、tab layout | Session / session UI store | 这些是用户可理解的消息壳和阅读状态。 |
| delivery runtime session id | Session route + trace reference | 它定位当前消息流，但不表达业务执行所有权。 |
| AgentFrame runtime surface、capability、context slice、VFS/MCP surface | AgentFrame runtime projection | WorkspacePanel 的可执行表面来自 frame revision。 |
| LifecycleAgent identity、role、current frame、delivery runtime ref | lifecycleStore agents | 它表达当前会话背后的 Agent 执行身份。 |
| LifecycleRun、WorkflowGraphInstance、activity attempts、execution log | lifecycleStore run projection | 它表达运行账本、Workflow 进度和可观察状态。 |
| Subject association | lifecycleStore subject/run projection | 它表达 Story/Task/Project 与运行账本之间的业务归属。 |

## WorkspacePanel 锚点

WorkspacePanel 的根锚点应是当前 Session 解析出的 `LifecycleAgent + AgentFrame`，因为它展示的是“当前会话能看到和能操作的运行表面”。`LifecycleRun` 应作为观察投影传入 context tab，用于展示 run status、activity attempts、progress 和 workflow injection，而不是作为 WorkspacePanel 的 owner root。

推荐前端 view model 收敛为：

```ts
interface WorkspaceLifecycleTarget {
  runtimeSessionId: string;
  runId: string | null;
  agentId: string | null;
  frameId: string | null;
}

interface WorkspaceRuntimeData {
  sessionId: string | null;
  lifecycleTarget: WorkspaceLifecycleTarget | null;
  frameRuntime: AgentFrameRuntimeView | null;
  lifecycleRun: LifecycleRunView | null;
  // existing workspace / extension / context fields
}
```

当前的 `lifecycleRuns: LifecycleRunView[]` 是迁移中间态。它服务 pinned context tab 的运行观察，后续应收敛为单个 `lifecycleRun` 加明确 `lifecycleTarget`，避免在 Session 页面里暗示多个 run 是 WorkspacePanel 的主状态。

## 推进步骤

### Step 1: 当前补链路

- 从 `useSessionRuntimeState` 获得 `AgentFrameRuntimeView`。
- 使用 hook active workflow metadata 或 `LifecycleAgentView.agent_ref.run_id` 解析 run id。
- 加载对应 `LifecycleRunView` 并传给 WorkspacePanel context tab。
- header 只在解析成功时展示“运行详情”。
- 投影刷新作为发送成功后的后台更新，不影响消息发送成功语义。

### Step 2: 收敛 WorkspaceRuntimeData

- 引入 `WorkspaceLifecycleTarget`。
- 将 `WorkspaceRuntimeData.lifecycleRuns` 收敛为 `lifecycleTarget`、`frameRuntime`、`lifecycleRun`。
- `ContextOverviewTab` 直接消费单 run projection；需要多 run 观察的页面从 subject/run 列表页进入，而不是从当前 Session 的 WorkspacePanel 推导。
- WorkspacePanel tab 注册和 extension bridge 继续以 Project/session/backend context 获取执行环境，但当前 frame surface 是 Session 工作区面板的主要 runtime input。

### Step 3: Session 状态瘦身

- Session store 保留消息壳状态、stream/feed、title、tab layout。
- run/agent/frame refs 在页面层通过 resolver 派生，或由 lifecycleStore 缓存提供。
- route state 可作为导航体验优化，但直接打开 `/session/:runtime_session_id` 时仍以 frame-runtime 反查为准。

### Step 4: Detail 页面整合

- `/run/:runId` 保持完整运行账本页面。
- `/agent/:agentId` 保持 Agent/frame runtime 详情页面。
- Session header 的“运行详情”默认进 run detail；需要 Agent/frame 细节时从 run detail 继续进入 Agent detail。
- Context tab 展示 compact status，不复制完整 detail 页面。

## Review 清单

- WorkspacePanel 的可操作能力是否来自 AgentFrame runtime surface。
- LifecycleRun 是否只作为运行观察投影进入 WorkspacePanel。
- Session 是否只保存消息壳和阅读状态。
- 发送成功路径是否与 projection refresh 失败解耦。
- 直接打开 Session URL 时是否仍通过 frame-runtime 反查，而不是依赖 route state。
- 普通 runtime trace 是否保持只读/不可发送状态。
