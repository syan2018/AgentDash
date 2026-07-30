# Frontend Architecture

Frontend以产品路由与generated contracts组织：Project/Story/Task/Lifecycle是产品read models；AgentRun workspace通过`run_id + agent_id`消费Complete Agent canonical conversation/context的请求级视图。

## Invariants

- API client只使用generated Rust contracts；不手写Runtime/vendor DTO。
- AgentRun execution 与 command availability只来自 `AgentRuntimeView` 或
  `AgentRuntimeStreamFrame::Update.state`，不从
  产品status、Backbone、conversation record或executor kind推导。
- `AgentRuntimeConnection` 以`AgentRuntimeView.observation.conversation`建立baseline，并把update
  presentations直接交给同一个Session reducer增量归约；live presentation不写回authoritative conversation。
  canonical record再进入 `useSessionStream -> sessionStreamReducer -> SessionEntry ->
  toolCardRegistry`。target切换隔离旧state。
- `AgentRuntimeConnection` 是同一 target 的唯一 Runtime 连接 owner。Feed、Composer 与
  Interaction UI 读取同一 view；Workspace 只提供 Product shell、静态 command binding、
  model/resource/waiting facts。
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
type AgentRuntimeStreamFrame =
  | { kind: "baseline"; connection_epoch: RuntimeU64; view: AgentRuntimeView }
  | {
      kind: "update";
      connection_epoch: RuntimeU64;
      lane_sequence: RuntimeU64;
      state: AgentObservationState | null;
      presentations: CanonicalConversationRecord[];
    }
  | {
      kind: "reset_required";
      connection_epoch: RuntimeU64;
      reason: AgentRuntimeResetReason;
      last_sequence: RuntimeU64 | null;
    };
```

### 3. Contracts

- transport只接收 generated `AgentRuntimeStreamFrame`。每个 connection epoch 先接收一次
  `Baseline`，再接收有序 `Update`；`ResetRequired`结束当前 live lane。
- `AgentRuntimeView.observation.conversation`只在baseline replacement时完整重建；普通update只归约本batch records。
- execution、active turn、command availability与interactions只在update携带owner state时替换，不从
  presentation event type推导。
- 显式 refresh 与 live lane 并行时，connection暂存 refresh期间已经交付的ordered updates。
  refresh baseline返回后先replacement，再以durable presentation identity与state revision重放baseline
  尚未确认的部分；ephemeral presentation始终属于live overlay。新stream baseline使用generation
  fence使旧refresh失效；reset同时丢弃失效lane的
  overlay。原因是authoritative read与process-local update回答不同时间问题，较旧read不能覆盖读取
  期间已经观察到的事实。
- `AgentDashThreadItem.type`直接决定消息、reasoning或tool/resource card。
- durable terminal batch携带同一owner边界的idle state并直接收敛会话；正常terminal不触发例行reload。
  只有显式refresh或lane失效才替换baseline。
- 同turn terminal关闭已有message/reasoning streaming状态，并吸收迟到的ephemeral
  message/reasoning/tool progress；新turn不受旧terminal identity影响。baseline replacement从
  authoritative conversation重建terminal集合。

### 4. Validation & Error Matrix

| 条件 | 行为 |
| --- | --- |
| live缺少canonical record | 拒绝并报告连接错误 |
| live frame解析失败 | 报告target、connection epoch与最后成功lane sequence后重连 |
| presentation id重复 | 覆盖同一record |
| item completed | 终结该item，不终结turn |
| update.execution=idle | receiving=false |
| 显式refresh期间收到后续live record | baseline replacement后按原lane顺序保留未被baseline确认的record |
| refresh期间收到新epoch baseline | 新stream baseline生效；旧refresh返回值被generation fence丢弃 |
| reset期间存在refresh overlay | 丢弃失效lane overlay，等待新epoch baseline |

### 5. Good / Base / Bad Cases

- Good：工具start/update/complete与final assistant按一个ordered record流渲染。
- Base：刷新页面后从durable history恢复同一内容。
- Bad：把generic item一律送入tool renderer，导致agent message显示为未知工具。

### 6. Tests Required

- transport current/removed shape边界测试。
- `presentation_id`合并测试。
- Runtime update控制状态替换测试。
- tool + final assistant真实浏览器tracer和reload恢复。

### 7. Wrong vs Correct

```ts
// Wrong：presentation 不能承担控制事实。
isReceiving = hasActiveCanonicalTurn(view.conversation);

// Correct
isReceiving = view.execution.status === "active";
```

## Data Flow

```text
React intent
  -> typed service
  -> AgentRun API/facade
  -> Runtime operation receipt
  -> AgentRuntimeView / AgentRuntimeStreamFrame
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
- command-state availability、target isolation与connection lane tests。
- session presentation parity覆盖message/reasoning/plan/tool/context/Companion/usage/error/interaction、item terminal与transient generation切换。
- service URL/encoding、Draft create、Runtime command/context/interaction tests。
- Draft tracer必须断言导航早于首个Agent output与turn terminal，首条用户消息由canonical Agent
  history/live产生且只提交一次。
- Workspace presentation、Canvas/VFS surface与Runtime Lost UI tests。
- Canvas 资源测试必须覆盖 Runtime Lost 但 current `workspace_modules` 仍含 Canvas 时用户入口
  和既有 tab 保持可打开，以及 Project 资产删除后历史 presentation 不重新打开该 Canvas。
