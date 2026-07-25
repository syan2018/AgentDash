# Frontend Architecture

Frontend以产品路由与generated contracts组织：Project/Story/Task/Lifecycle是产品read models；AgentRun workspace通过`run_id + agent_id`消费Complete Agent canonical conversation/context的请求级视图。

## Invariants

- API client只使用generated Rust contracts；不手写Runtime/vendor DTO。
- AgentRun command availability只来自Runtime snapshot，不从产品status、Backbone或executor kind推导。
- Runtime feed按snapshot `conversation_history` baseline与process-local live canonical records消费同一协议；只以`presentation_id`合并，不建立第二份turn/item store。canonical record再进入`useSessionStream -> sessionStreamReducer -> SessionEntry -> toolCardRegistry`。target切换隔离旧state。
- 会话运行态只由canonical `TurnStarted/TurnCompleted`推导。message delta、tool item或provider round结束不能单独停止receiving状态。
- `AgentRuntimeFeed`等平行renderer不存在；AgentRun workspace只提供Runtime target、inspect、command availability与product projection，不拥有第二套会话UI。
- Workspace Module/Canvas tab以concrete presentation URI为identity；layout按AgentRun product key持久化。
- VFS/resource surface来自current AgentFrame/Business Surface；Runtime binding只提供typed execution coordinate。
- Canvas 用户可打开项直接来自 `AgentRunWorkspaceView.workspace_modules` 的 ready Canvas
  entries。该服务端投影已经组合当前 canonical VFS Canvas mounts、workspace-module授权与可访问Project资产，
  因此菜单和 presentation validation 不再各自拼装事实；`runtimeStatus` 只控制执行命令
  可用性，Lost/terminal 不隐藏仍在该 current projection 中的资源。
- 持久化的Canvas tab只是布局偏好，不是资源事实。current `workspace_modules` ready后按
  concrete presentation URI清理失效tab；异步布局恢复不得覆盖这次currentness校验。
- UI intent必须对应真实API/facade command；无canonical endpoint的按钮、service与contract必须一起删除。
- errors保持typed code/diagnostic；stale command触发inspect refresh，不静默retry不同语义命令。
- ProjectAgent Draft提交先调用target creation API，收到`run_id + agent_id`后立即导航；原始
  composer intent通过navigation transition交给目标页。目标页只在canonical history baseline与
  live lane就绪后消费一次transition，并调用与follow-up相同的composer command。创建API不执行
  首条输入，Draft页不预测Agent source identity。

## Canonical Conversation Boundary

### 1. Scope / Trigger

修改live transport、Session reducer、消息/工具渲染或composer receiving状态时适用。

### 2. Signatures

```ts
type AgentLiveEvent = {
  source: AgentSourceCoordinate;
  sequence: AgentServiceU64;
  record: CanonicalConversationRecord;
};

function hasActiveCanonicalTurn(
  records: readonly CanonicalConversationRecord[],
): boolean;
```

### 3. Contracts

- transport只接收generated `AgentLiveEvent`形态。
- `ManagedRuntimeSnapshot.conversation_history`是渲染输入；live只向该数组覆盖/追加canonical record。
- `AgentDashThreadItem.type`直接决定消息、reasoning或tool/resource card。
- `TurnCompleted`触发reload，以Complete Agent durable history替换ephemeral overlay；reload期间到达的
  后续canonical records继续fold到新baseline；期间再次出现`TurnCompleted`时排队下一次reload，
  因此网络响应顺序和连续回合都不会创建第二套会话事实。

### 4. Validation & Error Matrix

| 条件 | 行为 |
| --- | --- |
| live缺少canonical record | 拒绝并报告连接错误 |
| presentation id重复 | 覆盖同一record |
| item completed | 终结该item，不终结turn |
| turn completed | receiving=false |
| terminal snapshot请求期间收到后续live record | snapshot替换旧overlay后继续保留该record |
| terminal snapshot请求期间下一回合也完成 | 当前收敛结束后再读取一次authoritative snapshot |

### 5. Good / Base / Bad Cases

- Good：工具start/update/complete与final assistant按一个ordered record流渲染。
- Base：刷新页面后从durable history恢复同一内容。
- Bad：把generic item一律送入tool renderer，导致agent message显示为未知工具。

### 6. Tests Required

- transport current/removed shape边界测试。
- `presentation_id`合并测试。
- first output与TurnCompleted运行态测试。
- tool + final assistant真实浏览器tracer和reload恢复。

### 7. Wrong vs Correct

```ts
// Wrong
isReceiving = snapshot.active_turn_id != null;

// Correct
isReceiving = hasActiveCanonicalTurn(snapshot.conversation_history);
```

## Data Flow

```text
React intent
  -> typed service
  -> AgentRun API/facade
  -> Runtime operation receipt
  -> snapshot/events refresh
  -> view model
```

Draft首条输入：

```text
Draft composer
  -> create AgentRun target
  -> navigate(run_id, agent_id, pending composer intent)
  -> target history/live baseline ready
  -> canonical composer input handoff
  -> UserInputSubmitted / TurnStarted / partial output
```

## Tests Required

- generated contract check与TypeScript typecheck。
- command-state availability、target isolation与stream cursor tests。
- session presentation parity覆盖message/reasoning/plan/tool/context/Companion/usage/error/interaction、item terminal与transient generation切换。
- service URL/encoding、Draft create/composer/cancel/context/approval tests。
- Draft tracer必须断言导航早于首个Agent output与turn terminal，首条用户消息由canonical Agent
  history/live产生且只提交一次。
- Workspace presentation、Canvas/VFS surface与Runtime Lost UI tests。
- Canvas 资源测试必须覆盖 Runtime Lost 但 current `workspace_modules` 仍含 Canvas 时用户入口
  和既有 tab 保持可打开，以及 Project 资产删除后历史 presentation 不重新打开该 Canvas。
