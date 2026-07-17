# Frontend State Management

## Store Ownership

- Project/Story/Task/Lifecycle stores保存产品read model。
- AgentRun workspace state按`run_id + agent_id`保存当前产品shell、AgentFrame resource surface与Runtime inspect结果。
- Runtime feed hook保存snapshot baseline、durable/transient cursor与连接状态；它不复制后端state machine。Snapshot entries标记为durable，live transient按generation lane聚合。
- Runtime feed的`historyReplayBoundarySeq`由当前target第一次成功完成journal history load时建立，
  并在同target重连期间保持不变。source state是否需要reset只决定read model隔离，不决定
  history load是否已经成功；这样React StrictMode取消第一次effect setup后，第二次真正完成的
  load仍会发布boundary。boundary以内只恢复feed/read model，boundary以后才进入唯一的
  typed live-event副作用dispatcher。
- Workspace tab/layout store按AgentRun product key持久化用户布局，concrete presentation URI作为tab identity。
- 命令式 Tab 展示必须携带目标 workspace key，并通过
  `openOrActivateInWorkspace(workspaceKey, typeId, uri, options)` 在一次 store 操作中先绑定
  workspace 再打开 Tab。WorkspacePanel 的被动初始化 effect 必须读取 store 最新状态，
  原因是 history hydration 可能在 sibling effect 挂载前已经提交 presentation；使用首帧
  捕获的旧 workspace key 会把刚打开的 Tab 重置。

## Runtime Rules

- command enabled只来自canonical Runtime snapshot availability。
- target变化立即隔离旧snapshot/feed/resource surface；loading期间不泄漏前一target状态。
- reconnect携带durable cursor与transient generation/sequence，duplicate event不重复reduce；generation变化删除旧lane纯transient贡献但保留durable final，Lost和retention gap显示typed diagnostic。
- failed/cancelled/lost item按item identity终结原entry/card；final durable item覆盖过程delta，terminal后的stale delta不再修改展示。
- Backbone product/resource event只触发相应projection invalidate，不推进Runtime state。
- live 标准 `thread_name_updated` 触发 AgentRun workspace state 与 list 的重新查询；初始
  hydration replay boundary 内的历史名称事件不重复执行该副作用。UI 不直接用事件 payload
  patch shell，原因是重新查询会统一应用 explicit workspace title 与 Runtime name 的后端
  优先级。
- mailbox只显示queued intent与accepted Runtime operation；没有canonical endpoint的管理动作不进入model/intents。

必须测试target切换、stale snapshot、cursor replay、availability、presentation URI与layout稳定性；
命令式 presentation 还必须覆盖“历史request不打开”“live request先刷新current projection”
与“先打开 Tab、后执行 WorkspacePanel 首次初始化”的顺序。
Runtime feed生命周期测试还必须覆盖StrictMode的`setup → cleanup → setup`：第一次load被取消、
第二次同target load完成时boundary从`null`变为该次`lastAppliedSeq`；后续重连继续保留原boundary。

## Scenario: Runtime conversation name invalidation

### 1. Scope / Trigger

修改session side-effect dispatcher、AgentRun control-plane planner、workspace query或列表store时，
必须保持Runtime名称事件只负责live invalidation。

### 2. Signatures

```ts
type ThreadNameRefreshReason = "thread_name_updated";

planAgentRunControlPlaneRefresh(event): {
  refreshWorkspaceState: boolean;
  refreshAgentRunListReason: ThreadNameRefreshReason | null;
};
```

### 3. Contracts

- 只有`seq > historyReplayBoundarySeq`的live `thread_name_updated`执行副作用。
- planner同时刷新当前AgentRun workspace与project AgentRun list；store收到现有
  `agent_run_list/title_changed` product invalidation时也重新查询列表。
- payload不直接patch shell/list；refetch结果应用后端统一的
  explicit title > Runtime name > `新会话`规则。

### 4. Validation & Error Matrix

| 条件 | 必须结果 |
| --- | --- |
| hydration boundary内名称事件 | 保留会话展示归约；workspace/list refetch次数为0 |
| live set/replace/clear | workspace与list各进入一次合并后的refresh plan |
| product `title_changed` invalidation | list store重新查询；不信任事件携带title |
| target在异步refresh期间切换 | 旧target结果被currentness fence，不覆盖新workspace |

### 5. Good / Base / Bad Cases

- Good：live clear触发refetch，后端返回显式标题或`新会话`，workspace与list一致。
- Base：页面初始history包含旧名称事件，只恢复feed，不重复网络副作用。
- Bad：直接写`workspace.title = event.payload.threadName`，会绕过显式标题优先级与stale-target fence。

### 6. Tests Required

- Session dispatcher断言hydration历史事件无副作用、live事件输出名称refresh reason。
- Control-plane测试断言名称reason同时刷新workspace/list，并与其他reason合并而不重复请求。
- List store测试断言`title_changed`触发重新查询；target切换测试断言旧响应不能覆盖新target。

### 7. Wrong vs Correct

```ts
// Wrong
workspaceStore.patchTitle(event.payload.threadName ?? "新会话");

// Correct
if (isLiveThreadNameUpdated(event, historyReplayBoundarySeq)) {
  refreshAgentRunWorkspaceState();
  refreshProjectAgentRunList();
}
```
