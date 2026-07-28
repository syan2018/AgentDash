# Frontend State Management

## Store Ownership

- Project/Story/Task/Lifecycle stores保存产品read model。
- AgentRun workspace state按`run_id + agent_id`保存当前Product shell、AgentFrame resource surface、
  model configuration、waiting items与静态Runtime command binding，不保存execution、active turn或
  动态command availability。
- `AgentRuntimeConnection`保存authoritative `AgentRuntimeView` baseline、当前update lane、
  presentation overlay与连接状态。Feed、Composer与Interaction UI从同一个connection读取selectors，
  因而history收缩与Workspace刷新不会形成第二条控制状态链。
- 初次连接、重连和lane gap恢复都把权威view中的presentation identity登记为hydration baseline；
  只有当前lane直接交付、且不属于恢复baseline的presentation才进入typed副作用dispatcher。副作用按
  `presentation_id`去重，数组位置不承担跨快照identity。
- Workspace tab/layout store按AgentRun product key持久化用户布局，concrete presentation URI作为tab identity。
- 命令式 Tab 展示必须携带目标 workspace key，并通过
  `openOrActivateInWorkspace(workspaceKey, typeId, uri, options)` 在一次 store 操作中先绑定
  workspace 再打开 Tab。WorkspacePanel 的被动初始化 effect 必须读取 store 最新状态，
  原因是 history hydration 可能在 sibling effect 挂载前已经提交 presentation；使用首帧
  捕获的旧 workspace key 会把刚打开的 Tab 重置。

## Runtime Rules

- command enabled只来自`AgentRuntimeView.command_availability`。
- target变化立即隔离旧view/update lane/resource surface；loading期间不泄漏前一target状态。
- reconnect和lane gap先重新读取authoritative view，再归约当前连接的新lane；duplicate update不重复reduce，
  connection/source变化删除旧lane partial贡献。failed/cancelled/lost item按identity终结原entry，
  authoritative terminal覆盖过程delta，terminal后的stale delta不再修改展示。
- canonical `TurnCompleted` 是 live overlay 的收敛边界：connection立即读取authoritative view，
  用 committed history替换该回合的ephemeral partial；请求在途期间继续到达的canonical live records
  按 `presentation_id` 叠加到新baseline，避免标题等terminal后事实被较早的snapshot响应覆盖；若期间
  又收到后续回合的`TurnCompleted`，连接层在当前请求后再读取一次，保证每个terminal都完成durable收敛。
- 同target Workspace后台refresh只影响Product projection；Composer继续使用当前Runtime control
  selector。只有target replace才隔离旧Runtime view，原因是取消/停止按钮必须持续对应同一个
  Complete Agent observation。
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
- canonical `item_completed` 中成功完成的 `task_write` 通过统一control-plane effect planner
  重新读取当前AgentRun的Task owner；进行中、失败及其他工具不触发。Task状态由owner read收敛，
  因而live reducer不保存optimistic Task副本。
- LifecycleGate waiting items作为Product事实单独展示；Agent input handoff不进入持久队列model。
  没有canonical endpoint的管理动作不进入model/intents。

必须测试target切换、stale view、live lane重建、availability、presentation URI与layout稳定性；
命令式 presentation 还必须覆盖“历史request不打开”“live request先刷新current projection”
与“先打开 Tab、后执行 WorkspacePanel 首次初始化”的顺序。
后台refresh测试必须从`active execution + available interrupt`开始，断言refresh期间仍保持
  active、interrupt command与停止按钮派生条件；ContextFrame planner测试断言不产生workspace或hook
  runtime refresh effect。
AgentRuntimeConnection生命周期测试还必须覆盖StrictMode的`setup → cleanup → setup`、target fence、
重连与gap恢复baseline；恢复view中的历史presentation不得重新执行一次性UI命令。
Terminal convergence测试必须覆盖ephemeral overlay被committed view替换，以及view请求在途
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

- canonical `TurnStarted`与`TurnCompleted`只进入AgentRuntimeConnection的presentation/control归约，
  不使Product Workspace失效。execution和command availability已随同一次Runtime update到达。
- connection在`TurnCompleted`上读取Complete Agent authoritative view完成durable收敛。composer
  command完成只清理本地输入状态，不触发Workspace reload。
- Workspace Module用户打开只改变tab layout；菜单已经来自current `workspace_modules`，打开动作不反向
  使同一Workspace失效。Canvas向Agent提交输入后的状态收敛仍沿canonical turn边界完成。
- Canvas runtime load identity由`run_id + agent_id + project_id + canvas_mount_id`和显式
  `refreshRevision`组成。父Workspace返回语义相同的新对象时，现有iframe与runtime snapshot保持不变。

### 4. Validation & Error Matrix

| 输入 | Workspace读取 | conversation reload | Canvas iframe |
| --- | ---: | ---: | --- |
| `TurnStarted` | 0 | 0 | 保持 |
| `TurnCompleted` | 0 | 由connection执行1次 | 保持 |
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

- planner测试断言`TurnStarted`、`TurnCompleted`与terminal metadata都不产生Workspace effect。
- terminal connection测试断言authoritative reload由connection owner执行且并发terminal按顺序收敛。
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
// TurnStarted -> AgentRuntimeConnection update
// TurnCompleted -> AgentRuntimeConnection update + authoritative view convergence
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

- 只有不属于connection恢复baseline、且`presentation_id`尚未消费的live
  `thread_name_updated`执行副作用。
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
if (isCurrentLaneThreadNameUpdated(event) && consumePresentationId(event.presentationId)) {
  refreshAgentRunWorkspaceState();
  refreshProjectAgentRunList();
}
```

## Scenario: Task tool owner refresh

### 1. Scope / Trigger

修改canonical item事件、AgentRun control-plane effect planner、Task store或Session状态栏时适用。

### 2. Signatures

```ts
type AgentRunControlPlaneEffectPlan = {
  refreshTaskPlan: boolean;
  // other typed effects
};

type AgentRunControlPlaneEffectExecutor = {
  refreshTaskPlan(): Promise<void>;
};
```

### 3. Contracts

- 只有成功终结的`item_completed`，且item family为`dynamicToolCall`、tool name为`task_write`时，
  planner才产生`refreshTaskPlan`。
- executor使用事件所属的当前`run_id + agent_id`调用Task owner read；planner不解析tool output来
  patch本地Task。
- hydration/reconnect baseline不重复执行该effect；live重复记录由presentation identity去重。
- Task刷新与workspace、title等effect在同一typed plan中合并执行，Session组件不建立第二条事件扫描线。

### 4. Validation & Error Matrix

| 事件 | `refreshTaskPlan` |
| --- | ---: |
| successful completed `task_write` | true |
| running/pending `task_write` | false |
| failed completed `task_write` | false |
| successful completed其他tool | false |
| hydration boundary内历史记录 | false |

### 5. Good / Base / Bad Cases

- Good：Agent提交Task变更后，planner触发一次owner read，状态栏显示后端已提交状态。
- Base：Task read暂时失败，现有Task snapshot保持；后续typed invalidation或重新进入页面可再次读取。
- Bad：组件从tool result文本猜测Task状态并局部patch，会绕过Task owner校验并与重连结果漂移。

### 6. Tests Required

- planner单测覆盖表中五种事件。
- executor单测断言只使用当前`run_id + agent_id`调用一次Task fetch。
- control-plane组合测试断言Task effect与workspace/title effect可合并，且target切换后的旧异步结果
  不覆盖新target。

### 7. Wrong vs Correct

```ts
// Wrong: Session组件维护第二条raw event扫描并猜测Task结果。
if (tool.name === "task_write") taskStore.patch(parseToolOutput(tool.output));

// Correct: canonical planner只发owner refresh effect。
if (isSuccessfulCompletedTaskWrite(event)) {
  plan.refreshTaskPlan = true;
}
```
