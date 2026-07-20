# Frontend / Backend Contracts

## 1. Scope / Trigger

本规范约束浏览器与 API 之间的共享 DTO、AgentRun control plane、Runtime stream、Workspace Module/Canvas presentation，以及跨端资源引用。新增 endpoint、生成类型、命令按钮、事件 reducer 或资源坐标时必须复核。

## 2. Contract Crate Shape

```text
agentdash-contracts
  -> product/resource DTOs
  -> packages/app-web/src/generated/*

agentdash-agent-runtime-contract
  -> Runtime command/snapshot/event/profile DTOs
  -> packages/app-web/src/generated/agent-runtime-contracts.ts

agentdash-agent-runtime-wire
  -> Cloud/Local Driver transport DTOs
  -> packages/app-web/src/generated/agent-runtime-wire.ts
```

- Rust 类型与生成器是 wire shape 的事实源；TypeScript 不复制手写同名 DTO。
- Runtime Contract、RuntimeWire 与 Backbone/product contracts 是三套独立合同，不能因字段相似而互相反序列化中转。
- JSON 使用 `snake_case`；可选字段由 Rust serde/TS 导出共同定义。

## 3. AgentRun Runtime Contract

### Execution Profile discovery

- 执行器选择器读取的是产品级 `ExecutionProfileDto`，其稳定 identity 来自受信 Integration definition；该 DTO 只表达名称、availability 与 unavailable reason，不携带 RuntimeOffer、service instance、generation 或 placement credential。
- Native `PI_AGENT` 与 Codex `CODEX` 是独立 execution profile。definition 已注册但尚未首次 provision RuntimeOffer 是合法状态；discovery 不以当前 offer 数量决定 profile 是否存在。
- Native discovered-options 从 LLM Provider effective catalog 投影 provider/model 与精确不可用原因；Codex profile 不伪造 Native Provider/model 列表。
- ProjectAgent create/update 与 discovery 使用同一 profile-to-definition catalog 校验，避免 UI 可选值与 API 可保存值产生第二套枚举。
- Rust contracts 及生成 TypeScript 是 discovery/options DTO 的事实源，前端 feature model 不复制同名字段结构。

### Signatures

```text
GET  /agents/discovery
GET  /agents/discovered-options/stream?executor={PI_AGENT|CODEX}
GET  /projects/{project_id}/agent-runs?limit={limit}&cursor={cursor}
POST /projects/{project_id}/agents/{project_agent_id}/agent-runs
POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit
POST /agent-runs/{run_id}/agents/{agent_id}/cancel
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/context
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/events/stream/ndjson
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/context/compact
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/interactions/{interaction_id}/respond
```

```rust
CreateProjectAgentRunRequest {
    input,
    client_command_id,
    model_selection?: {
        provider_id?,
        model_id?,
        agent_id?,
        thinking_level?,
    },
    backend_selection?,
    subject_ref?,
}

AgentRunAcceptedRefs {
    run_ref,
    agent_ref,
    frame_ref?,
    runtime_thread_id?,
    runtime_operation_id?,
}

AgentRunCommandReceipt {
    client_command_id,
    status,
    duplicate,
    accepted_runtime_operation_id?,
    message?,
}
```

### Contracts

- Project Agent create 先建立 Lifecycle run/agent/frame 产品事实，再通过
  `AgentRunProductInputDeliveryPort` 同步交接首条 Agent input。响应只有在 concrete Agent
  返回 operation receipt 后才报告 accepted。
- ProjectAgent 决定 executor/Integration identity并提供默认运行参数；create-run 使用独立的 `model_selection` 与 `backend_selection` 表达逐 Run 意图，不暴露完整 executor config。`model_selection` 聚合 Provider、model、agent variant 与 thinking level；admission 在 provision 前将这些 generated contract 分片与 ProjectAgent defaults 编译成 effective config并写入 AgentFrame execution profile。这些意图不是无状态 HTTP override，也不改写 ProjectAgent defaults。
- Composer submit 成功返回 concrete Agent `OperationReceipt`；重复 `client_command_id` 命中同一
  Agent effect，不存在 queued response 或 Product background delivery。
- UI 命令可用性来自 authoritative Agent snapshot 经内存 mapper 得到的当前 view；Lifecycle
  status、executor kind、HTTP success 与任何持久 presentation cache 都不能推导命令权限。
- LifecycleAgent owner document 中的 association 是 `run_id + agent_id` 到 concrete Agent
  service/source 的稳定 Product 坐标。浏览器不接触 source ID、Host generation、callback route
  或 placement credential。
- Session baseline 来自 Complete Agent authoritative `read`。随后连接只消费该 source 的
  process-local live delta；live delta 不携带跨重启 durable cursor。
- browser reducer 以 snapshot 为 committed baseline，以当前连接 lane 归约 partial delta。
  连接断开、source切换、Lagged 或 sequence gap 时丢弃 partial lane并重新读取 snapshot。
- Complete Agent 真正支持 ordered durable `changes` 时可以用 Agent-owned cursor增量读取；
  Snapshot-only Agent 不由平台伪造 change tail。
- 所有直接使用 `fetch` 的NDJSON客户端必须通过 `buildApiPath(agentRunScopedPath(...))` 构造URL；`resolveApiUrl`只拼origin，不会注入`/api`。
- AgentRun cutover必须维护route ledger：每个前端service方法都要对应仍注册的HTTP route、application owner、generated contract与至少一个contract test。删除router入口时，必须在同一变更中迁移消费者或删除service/contract；文件级替换router不代表cutover完成。
- Project AgentRun列表使用generated `ProjectAgentRunListView` /
  `AgentRunListEntryView` / `AgentRunListChildView`。title/activity/subject/lineage来自Product
  facts；Agent status 是 optional enrichment。
- Agent resolve/read 失败不能清空 Product list/workspace。前端展示 Product shell 与 typed
  unavailable，再在 Agent 恢复后重新查询。
- context、compaction、interaction 与 tool approval 均通过 Product/Agent facade，最终由
  concrete Agent command/inspection证明；不存在独立 session command 或 vendor DTO 路径。
- Interaction response使用generated `InteractionResponse` union；approval、user input、MCP elicitation与dynamic tool result共用一个`/respond` route。UI只有在刷新后的Runtime snapshot声明`interaction_respond=available`时才启用对应控件。
- Runtime context popup直接读取`RuntimeContextView`的active head、materialized checkpoint、blocks与fidelity；target切换以`run_id + agent_id`为request generation，旧target迟到响应不能覆盖当前popup。
- RuntimeWire/Relay 只承载 Complete Agent transport；其 connection epoch、route 与 generation
  不进入浏览器合同或 Product persistence。
- LifecycleGate 等 Product owner 的 waiting facts单独展示；Agent input handoff不形成 mailbox
  projection。没有真实 Agent/Product command 的管理动作不进入 UI。

### Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| execution profile definition 未进入最终 Host inventory | discovery 保留 profile 并返回 `available=false + unavailable_reason`；ProjectAgent 写入拒绝未知 profile |
| create-run Provider/model override 合法 | 与 ProjectAgent defaults 合并，保留 ProjectAgent executor，写入新 AgentFrame revision后再 provision |
| create-run 携带旧 `executor_config` 或分片包含未知字段 | `400 Bad Request`；不得静默忽略或建立兼容映射 |
| explicit backend 有匹配 activated offer | 只绑定该 backend placement并持久化 binding coordinates |
| explicit backend 无匹配 offer | typed unavailable；不得回退任意 backend或 InProcess instance |
| `PI_AGENT` 没有 executable Provider | profile 可见但 disabled；options 返回 Provider 诊断，不依赖 RuntimeOffer |
| `CODEX` definition 已注册 | profile 可选；options 不伪造 Native Provider/model |
| options executor 未知 | `400 Bad Request`，不探测 Connector 或任意 offer |
| AgentRun target 不存在或跨 Project | not found/authorization error before Runtime side effect |
| client command id 为空 | `400 Bad Request` |
| Agent turn/interaction coordinate 已过期 | typed stale error；前端刷新 authoritative snapshot |
| interaction event已到但Runtime inspect尚未刷新 | 控件保持disabled；`interaction_requested`触发inspect refresh后按availability启用 |
| context target A响应晚于target B | A响应丢弃；popup只提交与当前target key匹配的结果 |
| Driver回报与`runtime_turn_id`不同的Turn | critical protocol violation；matching identity只作为Observed ack |
| command availability=false | UI 禁用且 API 在副作用前拒绝 |
| Agent unavailable | 当前请求 typed unavailable；无 queued Product row |
| command duplicate | 返回原 operation receipt |
| live connection断开 | 清除 partial lane并重读 snapshot；Product shell保持可用 |
| NDJSON URL 未经过 `buildApiPath` | frontend contract test失败；不得请求缺少`/api`的同名页面路由 |
| live source/connection变化或sequence重复 | 重建partial lane；同连接重复sequence丢弃 |
| broadcast Lagged | 输出typed retryable error并断流；浏览器重新读取authoritative snapshot |
| presentation envelope合法但protected body无法通过generated validator | 拒绝该frame并显式报protocol error；不降级为文本消息或generic tool card |
| workspace/list route在cutover中移除但service仍存在 | route ledger/contract test失败；同一变更迁移projection或删除consumer |
| Runtime thread为`active`但没有`active_turn_id` | 列表显示idle/ready，不伪造running |
| Runtime thread为`suspended` | 列表显示独立paused/suspended状态；不得折叠为turn interrupted或据此生成命令权限 |
| Runtime thread为`closed` | 使用Lifecycle终态区分completed/failed/cancelled，不恒定显示completed |

### Tests Required

- Contract generation/check 覆盖 product refs、Runtime snapshot/event/profile 与 RuntimeWire。
- Production composition test 断言最终 `IntegrationDriverHost` inventory 包含动态装配的 Native definition 和已注册的 Codex definition。
- Discovery/API tests 覆盖 Native/Codex 独立 availability、未知 profile、Provider diagnostic 与 options NDJSON。
- Selector tests 断言不可用 profile/Provider 保持可见、disabled 且展示原因。
- Service tests 覆盖 URL encoding、create/composer/cancel/context/generic interaction endpoints。
- Command-state tests 证明 availability 只取 Runtime snapshot。
- Feed tests 覆盖 authoritative snapshot baseline、duplicate live event、disconnect/gap、reconnect
  与 typed stream error。
- Interaction feed tests保留`interaction_id/kind/prompt/terminal`并证明response控件只消费刷新后的availability；context popup tests覆盖target切换迟到响应。
- Feed URL test断言完整AgentRun scoped snapshot/live endpoint，不请求已删除的 mailbox 或
  Runtime change-tail endpoint。
- Stream state测试覆盖target切换、连接lane变化、重复sequence、terminal与Lagged后snapshot
  recovery。
- Route ledger test至少枚举AgentRun list/workspace/composer/cancel/runtime/context/events/approval的前端consumer与Axum route，防止cutover静默删入口。
- Project列表测试覆盖service URL、generated DTO消费、status presentation与state分页/失效刷新；真实产品验证覆盖侧栏、完整列表及列表行导航。
- Project Agent create E2E 覆盖 lifecycle facts → Agent create/source association → synchronous
  first input → operation response → live delta → reconnect snapshot。
- Create-run contract generation test断言 generated TypeScript 只暴露 `model_selection` 与 `backend_selection`，不重新引入可覆盖 executor 的请求字段。

## 4. Companion and Workflow Product Facts

- Companion/subagent dispatch 以 Lifecycle run/agent/frame、assignment/activity attempt 与 canonical Runtime thread/operation refs表达。
- Workflow、Gate、Task、Story 只保存产品编排与 evidence 坐标；Runtime terminal 通过 canonical Runtime event/snapshot 投影，不保存另一份执行 session 状态。
- 等待状态由 LifecycleGate 持久化；gate result通过同步 Agent input handoff唤醒目标 Agent。
  Gate owner只保存 waiting fact 与 handoff/operation coordinate作为下游证据。
- UI 可以展示 Runtime trace link，但不得把 trace metadata当作 AgentRun command authority。

## 5. Workspace Module, Canvas and VFS

- Workspace Module presentation payload 的 concrete URI 是 tab identity；浏览器不根据 view key 猜测资源 URI。
- `AgentRunWorkspaceView.workspace_modules` 是 AgentRun 页面当前可见 Workspace Module 的唯一
  UI 投影。后端按当前精确 AgentFrame 的 runtime module refs 与已授权 Project 资产组合该
  字段；菜单、展示事件校验和 renderer 都消费这一份响应，原因是它们必须对“当前可打开资源”
  得出相同结论。
- `workspace_module_presentation_requested` 是独立的 typed Backbone 产品事件。journal
  持久化它是为了审计与 feed 展示；命令式打开只消费 live 边界后的请求，原因是“曾请求展示”
  不等于“当前观察者仍应被强制切换界面”。
- `ControlPlaneProjectionChanged` 只表达投影失效，`reason` 只决定需要重新查询哪些 read
  model。展示请求不嵌入 projection change，避免同一个事件同时承担状态同步和 UI 命令。
- Agent-facing operation 只来自 generated operation catalog。panel-only action 不自动成为 Agent tool。
- Canvas runtime snapshot、VFS resource surface 与 Agent tool 使用同一当前 AgentFrame/Business Surface projection；Frame 是产品期望，不是 Runtime lifecycle authority。
- Runtime-bound Canvas/extension invocation 以 `run_id + agent_id` 进入 API，后端通过 canonical `AgentRunRuntimeBinding` 获取 thread/binding coordinate。
- Backend placement 与 VFS mount access 是资源 facts；它们约束 Business Surface/Tool Broker，但不创建 Runtime capability guarantee。
- iframe/webview 只发送声明的 action/channel key 与 input；父页面补齐 AgentRun/Project identity，API 完成 authorization 与 binding resolution。

### Scenario: Typed Live Workspace Module Presentation

#### 1. Scope / Trigger

当 Agent tool 提交 Workspace Module 展示请求，或前端修改 AgentRun journal hydration、
live-event dispatcher、WorkspacePanel imperative handle、tab store workspace scope 时适用。

#### 2. Signatures

```ts
dispatchLiveSessionEvents(
  rawEvents,
  afterSeq,
  historyReplayBoundarySeq,
  onLiveEvent,
): number;

planAgentRunLiveEvent(event: BackboneEvent): AgentRunLiveEventPlan;

isWorkspaceModulePresentationCurrent(
  presentation: WorkspaceModulePresentation,
  modules: readonly WorkspaceModuleDescriptor[],
): boolean;

openOrActivateInWorkspace(
  workspaceKey: string | null,
  typeId: string,
  uri: string,
  options?: WorkspaceTabLayoutOptions,
): string;
```

```text
GET /agent-runs/{run_id}/agents/{agent_id}/workspace
  -> AgentRunWorkspaceView {
       ...,
       workspace_modules: WorkspaceModuleDescriptor[],
     }
```

#### 3. Contracts

- backend 从 workspace snapshot 定位当前精确 AgentFrame，读取该 frame 的
  `visible_workspace_module_refs`，再与当前用户可访问的 Project Workspace Module 资产组合
  `workspace_modules`。Canvas 只有同时具备 Project 资产与精确 runtime ref 才进入 AgentRun
  投影；Project 中已删除的 Canvas 不会由历史 frame ref 重新制造出来。
- `WorkspacePanel` 的“可打开 Canvas”菜单直接选择
  `AgentRunWorkspaceView.workspace_modules` 中 ready 的 Canvas entry，不再建立页面级 Project
  catalog 缓存或在浏览器中与 resource surface 二次求交。这样刷新完成即得到一个原子版本的
  模块 identity、状态、view、renderer 与 URI。
- backend 在 canonical AgentRun journal 中持久化独立 typed
  `WorkspaceModulePresentationRequested`；payload 携带
  `module_id`、`view_key`、`renderer_kind`、`presentation_uri`、`title` 与 `payload`。
- 初次 hydration 只把 durable request 恢复为 feed 审计卡片，不执行 panel/tab 命令。
  `dispatchLiveSessionEvents` 从 `historyReplayBoundarySeq` 后按 seq 顺序把完整 typed
  `BackboneEvent` 交给页面唯一 live planner；turn、task、projection 与 presentation
  不再各自扫描同一 raw event 数组。
- `historyReplayBoundarySeq`表示当前target第一次成功完成的journal history load，不表示某次
  React effect是否同时执行了source reset。首次成功load以幂等方式填充空boundary；同target
  reconnect保留原值。这个边界把审计事实 hydrate 与 live 命令执行明确分开，也让StrictMode
  取消第一次setup后仍能由下一次成功load完成初始化。
- `context_frame_changed` 是 current workspace projection 的 canonical invalidation event；
  页面收到后刷新 workspace state，使 SurfaceAdopt 产生的新 module refs 同步进入列表。
- live planner 先按 typed payload 与 concrete presentation URI 生成 registry tab target；
  executor 随后等待 workspace refresh，并要求 `module_id + view_key + renderer_kind +
  presentation_uri` 精确匹配当前 ready descriptor 后才打开。request 决定“现在尝试展示”，
  current workspace 投影决定“该资源现在是否可打开”。
- imperative UI owner 必须携带当前 AgentRun workspace key；tab store 在打开前原子切换到
  该 scope。WorkspacePanel 首次 effect 从 store 读取最新 workspace key，使 hydration 与
  mount effect 的先后顺序不影响最终 active tab。

#### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| typed presentation request 位于 `seq <= historyReplayBoundarySeq` | 渲染审计卡片，不执行 workspace refresh、侧栏展开或 tab open |
| 任意一次性事件位于 hydration 边界内 | 重建 feed/read model 展示，不重复执行页面命令 |
| 收到 `context_frame_changed` | invalidate/refetch AgentRun workspace，列表从新 `workspace_modules` 原子更新 |
| live `workspace_module_presentation_requested` | 刷新 workspace，校验 current descriptor 后执行 presentation open |
| `control_plane_projection_changed` | 只按 projection/reason 刷新 read model，不产生 presentation 命令 |
| runtime ref 存在但 Project Canvas 资产已删除 | `workspace_modules` 不含该 Canvas；菜单不可见，live presentation 校验失败 |
| 当前 module/view/renderer/URI 任一不匹配事件 payload | 保留审计事件，不执行 tab open |
| 当前 module status 不是 `ready` | 菜单不提供入口，presentation 不打开 |
| workspace refresh 失败 | 不执行 presentation open；错误由 workspace state owner 呈现 |
| StrictMode第一次history load被cleanup取消，第二次同target load成功 | 第二次load建立boundary；其后到达的live request进入dispatcher |
| 同target在boundary建立后重连并回放到更高sequence | 保留原hydration boundary；只按side-effect cursor消费新增事件 |
| Canvas `presentation_uri` 为空或仅为 `canvas://` | mapper 拒绝生成无资源 identity 的 Canvas target |
| tab store 当前 workspace 与命令目标不同 | 先初始化目标 workspace，再打开并激活 tab |
| presentation 先于 WorkspacePanel 首次 effect | effect 识别已绑定的 workspace，保留刚打开的 tab |

#### 5. Good / Base / Bad Cases

- Good：history hydration 完成后收到新的 Canvas request，workspace refresh 返回同一 ready
  descriptor，随后打开 `canvas://{mount_id}`，侧栏展开、tab 激活且 renderer 可见。
- Base：historical request 只恢复成功审计卡片；live presentation 走同一 dispatcher、
  typed planner、imperative owner 与 scoped store，
  `context_frame_changed` 走通用 workspace invalidation，不需要单独的 Canvas handler。
- Bad：把 request 塞进 projection invalidation，再为历史 presentation 添加例外回放；这会让
  一次性 UI 命令随刷新、重连和组件重挂载重复执行。

#### 6. Tests Required

- hydration dispatcher 回归：typed request 在 boundary 内只渲染、不执行命令；boundary 后
  的完整 typed events 按 seq 唯一分发。
- Runtime feed生命周期回归：StrictMode取消首次setup后，后续成功history load仍建立boundary；
  同target reconnect不移动既有boundary。
- typed planner 回归：live presentation 先等待 workspace refresh，再精确匹配 current
  descriptor；空投影不会打开 Canvas。
- control-plane 回归：`context_frame_changed` 必须刷新 AgentRun workspace。
- backend visibility 回归：Canvas 只有同时存在 Project asset 与 runtime ref 时进入
  `workspace_modules`。
- scoped tab store 回归：presentation 先执行、WorkspacePanel 初始化后执行，目标 tab 保持
  active。
- production 页面验证：真实消息产生新 request 后同时断言审计事件、菜单 entry、侧栏展开、
  concrete active tab 与 renderer 内容；收起侧栏再刷新时历史 request 不重复打开。

#### 7. Wrong vs Correct

```ts
// Wrong: 多个 effect 各自扫描 rawEvents，并通过字符串伪类型猜测副作用。
dispatchTurnEffects(rawEvents);
dispatchPlatformEffects(rawEvents);
dispatchTaskEffects(rawEvents);

// Correct: history boundary 后只分发一次完整 typed event，由页面 planner 裁决。
lastLiveSeq = dispatchLiveSessionEvents(
  rawEvents,
  lastLiveSeq,
  historyReplayBoundarySeq,
  (event) => execute(planAgentRunLiveEvent(event)),
);
```

## 6. MCP and External Resource Contracts

- MCP preset contract 分离 declaration、credential refs、placement requirement 与 probe result。secret 不进入共享 DTO。
- Runtime tool availability 是 Business Surface required contribution 与 bound Runtime profile 的交集；MCP catalog 存在不等于 Driver 能原生或精确消费。
- Remote/local resource references 使用 typed owner/mount/backend coordinate；浏览器不发送本机绝对路径作为业务身份。
- 外部 service/provider 不可用时返回 typed diagnostic；不选择任意在线 backend 或另一 provider fallback。

## 7. Good / Base / Bad Cases

- Good：Draft 创建返回 run/agent/frame 与 Runtime thread/operation；页面随后从 runtime inspect/events 渲染 transcript，并从 snapshot availability启用 interrupt。
- Good：Project列表在无active turn时显示就绪，点击generated list entry的run/agent坐标进入同一AgentRun详情。
- Good：首次运行前 RuntimeOffer 表为空，selector 仍从最终 Host definition inventory 展示 `PI_AGENT`/`CODEX`。
- Base：没有 executable Provider 时 `PI_AGENT` disabled 并展示凭据诊断，`CODEX` availability 独立计算。
- Bad：API 读取 composition 前的临时 definition registry，导致动态装配的 Native definition 在真实启动中消失。
- Base：Agent 暂时不可用，首条消息请求返回 typed unavailable；使用同一 client identity重试后
  返回 concrete Agent operation receipt。
- Bad：前端调用已经没有后端实现的管理 endpoint，或根据 `execution_status=running` 自行启用 cancel。
- Bad：把Runtime `active`直接映射为running，或把`closed`直接映射为completed，会把thread lifecycle误当成turn/产品终态。
- Bad：把 live partial delta 当成 durable history；断线后会缺失未提交内容或重复追加。
- Good：Canvas presentation 先刷新 `AgentRunWorkspaceView.workspace_modules`，精确匹配
  `canvas://{mount_id}` 后打开 tab；同一响应同时驱动“可打开 Canvas”菜单。
- Bad：把 RuntimeWire frame转成 Backbone JSON 再由 UI 推导 Runtime terminal。

## 8. Wrong vs Correct

```ts
// Wrong
const canCancel = lifecycleAgent.status === "running";

// Correct
const canCancel = runtimeInspect.snapshot
  ?.command_availability.interrupt?.available === true;
```

```rust
// Wrong
let thread_id = request.protocol_turn_id;

// Correct
let binding = agent_run_runtime_binding_repo.load(&target).await?;
let receipt = agent_run_runtime.send_message(command).await?;
```

```rust
// Wrong: composition 前 registry 不是生产 Host inventory
let profiles = app_state.runtime_definition_registry.definitions();

// Correct: discovery、ProjectAgent validation 与 Relay trust 共用最终 Host
let profiles = app_state.services.agent_runtime_host.definitions();
```

```ts
// Wrong：前端复制一份请求形状，并把 executor 混入逐 Run 参数
type StartConfig = { executor: string; provider_id?: string; model_id?: string };

// Correct：直接消费 Rust 生成的分片合同
import type {
  AgentRunModelSelectionRequest,
  AgentRunRuntimeOptionsRequest,
  CreateProjectAgentRunRequest,
} from "../generated/project-agent-contracts";
```

```ts
// Wrong：把上一次连接的partial sequence当成跨重启history cursor
connectLiveEvents(target, { after: previousConnectionSequence });

// Correct：先读authoritative snapshot，再为当前连接建立新的partial lane
const snapshot = await getAgentRunConversation(target);
connectLiveEvents(target, { baselineRevision: snapshot.sourceRevision });
```

```ts
// Wrong: approval卡片调用独立vendor/tool route并从event存在推断可响应。
approveToolCall(interactionId);

// Correct: event提供identity，canonical snapshot提供命令authority。
if (runtimeSnapshot.command_availability.interaction_respond?.status === "available") {
  await respondAgentRunInteraction(target, interactionId, { kind: "approved" });
}
```

```ts
// Wrong: thread lifecycle直接伪造turn/product状态。
const status = runtime.thread_status === "active" ? "running" : "completed";

// Correct: active turn与Lifecycle终态共同形成纯展示状态；命令仍只读availability。
const status = agentRunListPresentationStatus(
  runtime?.thread_status,
  runtime?.active_turn_id,
  entry.lifecycle_status,
);
```

## 9. Schema-generated Owned Conversation Protocol

### 9.1 Scope / Trigger

修改Codex revision、conversation item/event/interaction、Rust/TypeScript生成器或跨层nullable/number语义时适用。标准Codex payload由固定上游schema机械生成AgentDash-owned类型；vendor crate只允许出现在protocol codegen工具与Codex Integration。

### 9.2 Signatures

```powershell
cargo run -p agentdash-agent-protocol-codegen -- write
cargo run -p agentdash-agent-protocol-codegen -- check
```

生成锁记录upstream tag/commit、schema hash、root types、generator version、schema override identity与variant-qualified nullable paths，例如`CommandExecution.durationMs`。

### 9.3 Contracts

- 上游standard字段和variant不手抄；局部generator缺陷只能通过固定schema hash与路径约束的机械override处理。
- nullable允许空间按`Variant.field`审计。已声明nullable的字段同时接受omitted/null并输出稳定canonical form；同名非nullable字段不能被全局替换影响。
- JSON wire整数在TypeScript中统一为`number`，generated outputs不得出现`bigint`；`RequestId`保持`string | number`。
- write删除所有managed root中的stale extra文件；check分别拒绝missing、changed与extra。
- generated owned protocol不得依赖Codex vendor crate。Integration admission先vendor typed deserialize，再strict transcode为owned type。

### 9.4 Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| schema hash与override baseline不一致 | codegen失败并要求审查override |
| nullable审计出现missing/extra qualified path | codegen失败，不扩大为字段名全局规则 |
| 同字段名在不同variant中nullable语义不同 | 分variant生成；required/nonnullable branch保持原shape |
| generated TS出现`bigint` | generation/check失败 |
| managed root存在stale extra | check失败；write删除后重建 |
| vendor payload无法进入owned type | typed protocol mismatch，无JSON/text fallback |

### 9.5 Good / Base / Bad Cases

- Good：`CommandExecution.durationMs`接受null并canonical输出null，`Sleep.durationMs`仍为必填number。
- Base：上游新增nullable path时allowlist双向审计失败，由协议升级显式决定是否接纳。
- Bad：对`durationMs`做全局文本替换，导致非nullablevariant也变成optional。

### 9.6 Tests Required

- codegen执行write→check，并通过临时extra文件证明Rust、TypeScript与schema三个managed root的check/write行为。
- nullable fixtures覆盖omitted、null与canonical output，同时保留同名nonnullable字段的拒绝测试。
- generated TS执行no-`bigint`断言；前端typecheck验证consumer边界。
- vendor→owned strict admission覆盖全部admitted item/notification/request family、unknown method/item与显式typed no-op。
- `cargo tree -i codex-app-server-protocol --edges normal`证明直接owner只有codegen与Codex Integration。

### 9.7 Wrong vs Correct

```text
Wrong: nullable_fields = { "durationMs" } -> 全局修改每个variant
Correct: nullable_paths = { "CommandExecution.durationMs", ... } -> 只修改对应discriminator branch
```
