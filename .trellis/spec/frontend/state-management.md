# Frontend State Management

## Store Ownership

- Project/Story/Task/Lifecycle stores保存产品read model。
- AgentRun workspace state按`run_id + agent_id`保存当前产品shell、AgentFrame resource surface与Runtime inspect结果。
- Runtime feed hook保存authoritative snapshot baseline、当前 live connection lane与连接状态；
  它不复制后端或 concrete Agent state machine。Snapshot entries是committed baseline，live
  partial delta只在当前lane聚合。
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
- reconnect先重新读取authoritative snapshot，再建立新的live lane；duplicate event不重复reduce，
  connection/source变化删除旧lane partial贡献。failed/cancelled/lost item按identity终结原entry，
  snapshot terminal覆盖过程delta，terminal后的stale delta不再修改展示。
- canonical `TurnCompleted` 是 live overlay 的收敛边界：连接层立即读取 authoritative snapshot，
  用 committed history替换该回合的ephemeral partial；请求在途期间继续到达的canonical live records
  按 `presentation_id` 叠加到新baseline，避免标题等terminal后事实被较早的snapshot响应覆盖；若期间
  又收到后续回合的`TurnCompleted`，连接层在当前请求后再读取一次，保证每个terminal都完成durable收敛。
- 同target后台refresh继续使用当前committed workspace conversation作为命令与执行状态基线；
  `refreshing`只描述read请求，不覆盖`conversation.execution.status`或清空当前command set。
  只有target replace才隔离旧conversation，原因是取消/停止按钮必须在refresh期间保持与已提交
  Agent snapshot一致。
- UI允许thread-level ContextFrame在视觉上把同一turn切成多个presentation section。Section的React
  identity由首个canonical display item identity派生，而不是只用turn id；authoritative收敛替换掉
  live section时会得到新的identity，旧DOM不会与新section并存。
- `Platform(ContextFrameChanged)`只进入canonical feed与ContextFrame展示，不触发AgentRun
  workspace/hook runtime refetch；Product-owned Frame/resource变化由对应typed projection
  invalidation刷新，Workspace Module展示请求按自身currentness合同显式刷新一次。
- Backbone product/resource event只触发相应projection invalidate，不推进Runtime state。
- live 标准 `thread_name_updated` 触发 AgentRun workspace state 与 list 的重新查询；初始
  hydration replay boundary 内的历史名称事件不重复执行该副作用。UI 不直接用事件 payload
  patch shell，原因是重新查询会统一应用 explicit workspace title 与 Runtime name 的后端
  优先级。
- LifecycleGate waiting items作为Product事实单独展示；Agent input handoff不进入持久队列model。
  没有canonical endpoint的管理动作不进入model/intents。

必须测试target切换、stale snapshot、live lane重建、availability、presentation URI与layout稳定性；
命令式 presentation 还必须覆盖“历史request不打开”“live request先刷新current projection”
与“先打开 Tab、后执行 WorkspacePanel 首次初始化”的顺序。
后台refresh测试必须从`running_active + enabled cancel command`开始，断言refresh期间仍保持
running、cancel command与停止按钮派生条件；ContextFrame planner测试断言不产生workspace或hook
runtime refresh effect。
Runtime feed生命周期测试还必须覆盖StrictMode的`setup → cleanup → setup`：第一次load被取消、
第二次同target load完成时boundary从`null`变为该次`lastAppliedSeq`；后续重连继续保留原boundary。
Terminal convergence测试必须覆盖ephemeral overlay被committed snapshot替换，以及snapshot请求在途
收到的后续durable record仍保留在最终projection；连续回合的terminal必须排队完成下一次收敛读取。

## Scenario: Canonical turn refresh ownership

### 1. Scope / Trigger

修改AgentRun composer、canonical live副作用、Workspace查询或Canvas runtime panel时适用。

### 2. Signatures

```ts
planAgentRunLiveEvent(event: BackboneEvent): {
  effects: {
    refreshWorkspaceState?: boolean;
    refreshAgentRunListReason?: string;
  };
};
```

### 3. Contracts

- canonical `TurnStarted`与`TurnCompleted`分别使Product Workspace失效一次，用于读取当前执行命令和
  Product shell；普通command promise完成、composer清理和terminal展示metadata不建立平行刷新入口。
- conversation feed在`TurnCompleted`上自行读取Complete Agent authoritative snapshot。composer
  command完成只清理本地输入状态，不再次调用feed reload。
- Workspace Module用户打开只改变tab layout；菜单已经来自current `workspace_modules`，打开动作不反向
  使同一Workspace失效。Canvas向Agent提交输入后的状态收敛仍沿canonical turn边界完成。
- Canvas runtime load identity由`run_id + agent_id + project_id + canvas_mount_id`和显式
  `refreshRevision`组成。父Workspace返回语义相同的新对象时，现有iframe与runtime snapshot保持不变。

### 4. Validation & Error Matrix

| 输入 | Workspace读取 | conversation reload | Canvas iframe |
| --- | ---: | ---: | --- |
| `TurnStarted` | 1 | 0 | 保持 |
| `TurnCompleted` | 1 | 由feed执行1次 | 保持 |
| terminal `SessionMetaUpdate` | 0 | 0 | 保持 |
| 等值Workspace/bridge对象重渲染 | 0 | 0 | 保持 |
| 用户显式点击Canvas刷新 | 0 | 0 | 重载1次 |

### 5. Good / Base / Bad Cases

- Good：执行开始后composer切换到停止语义，终止后直接恢复发送；打开的Canvas不进入loading。
- Base：标题或resource surface发生独立typed invalidation时Workspace正常重读，Canvas bridge坐标未变
  因而iframe保持。
- Bad：command service、composer success、hook别名与terminal各自读取整个Workspace，使终止成为多次
  页面刷新并重建Canvas。

### 6. Tests Required

- planner测试断言`TurnStarted`与`TurnCompleted`各产生一个Workspace effect，terminal metadata不产生
  effect。
- terminal feed测试断言authoritative reload由connection owner执行且并发terminal按顺序收敛。
- 真实浏览器回归在已打开Canvas的会话发送无工具输入，断言运行期和终止后Canvas loading次数、
  iframe缺失次数、reconnect状态次数均为0。

### 7. Wrong vs Correct

```ts
// Wrong: command完成后从多个观察者重复收敛相同事实。
await submitComposer(intent);
await runtimeFeed.refresh();
refreshAgentRunWorkspace();

// Correct: command只提交输入；canonical边界分别驱动各自owner收敛。
await submitComposer(intent);
// TurnStarted -> Product Workspace invalidation
// TurnCompleted -> Product Workspace invalidation + Agent feed authoritative reload
```

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
- planner同时刷新当前AgentRun workspace与Project AgentRun list；store收到
  `agent_run_list/title_changed` product invalidation时也重新查询列表。
- payload不直接patch shell/list；refetch结果读取Product-owned
  `LifecycleAgent.workspace_title`，缺省展示`新会话`。

### 4. Validation & Error Matrix

| 条件 | 必须结果 |
| --- | --- |
| hydration boundary内名称事件 | 保留会话展示归约；workspace/list refetch次数为0 |
| live set/replace/clear | workspace与list各进入一次合并后的refresh plan |
| product `title_changed` invalidation | list store重新查询；不信任事件携带title |
| 普通Project `StateChanged` | list store不查询；该事件没有声明list projection已变化 |
| target在异步refresh期间切换 | 旧target结果被currentness fence，不覆盖新workspace |

### 5. Good / Base / Bad Cases

- Good：live clear触发refetch，后端返回显式标题或`新会话`，workspace与list一致。
- Base：页面初始history包含旧名称事件，只恢复feed，不重复网络副作用。
- Bad：直接写`workspace.title = event.payload.threadName`，会绕过显式标题优先级与stale-target fence。

### 6. Tests Required

- Session dispatcher断言hydration历史事件无副作用、live事件输出名称refresh reason。
- Control-plane测试断言名称reason同时刷新workspace/list，并与其他reason合并而不重复请求。
- List store测试断言`title_changed`触发重新查询、普通Project `StateChanged`不查询；
  target切换测试断言旧响应不能覆盖新target。

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
