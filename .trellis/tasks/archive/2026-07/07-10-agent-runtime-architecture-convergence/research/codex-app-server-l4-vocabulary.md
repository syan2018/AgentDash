# Codex App Server Protocol 到 L4 ManagedContextRuntime 的词汇与操作合同映射

> 调研对象：`references/codex` 当前工作树，commit `da4c8ca57d40b074bdc1b5b1218851100150c56b`。
>
> 目标不是把 Codex 协议冻结成 AgentDash 的永久外部标准，而是从已经运行过大量真实 Agent 场景的协议中提取当前最有价值的 vocabulary、operation shape 与生命周期经验，为 AgentDash-owned L4 contract 和首批 Codex Adapter提供参考。AgentDash、企业 Agent Core 和 driver仍可协同演进；优先级是 module ownership、状态事实源和依赖方向干净。

## 1. 结论摘要

Codex App Server 最值得借鉴的不是具体 Rust struct，而是以下五组词汇与形状：

1. **Thread → Turn → Item**：Thread 表示可恢复、可分叉的对话历史；Turn 表示一次有终态的 Agent执行；Item 表示可独立流式更新和结算的输入、输出或动作。
2. **Client request / server request / notification**：协议是双向的，approval、user input、MCP elicitation、dynamic tool call都能由 runtime向host发起请求，而不是塞进普通输出事件。
3. **accepted response 与 lifecycle notification 分离**：`turn/start` 先返回 `InProgress` Turn，随后发 `turn/started`、item events和 `turn/completed`。
4. **expected active turn precondition**：`turn/steer` 明确要求 `expectedTurnId`，handler验证当前 active turn，避免把输入注入错误的 turn。
5. **typed final item authoritative，delta只是过程**：例如 Plan 的注释明确 completed item可以与delta拼接结果不同；这为 durable projection提供了正确事实优先级。

但 Codex 当前协议**还不是 AgentDash L4 ManagedContextRuntime 的充分合同**：

- `thread/read` 返回 metadata/turn历史视图，不是当前模型真正看到的 materialized context；其持久历史本身可能 lossy；
- `thread/compact/start` 只有 `{ threadId }`，response为空；`ContextCompaction` item也只有 `id`，没有 summary、replacement boundary、base/new snapshot或provenance；
- Thread ID不是 AgentDash durable runtime binding，不携带 Integration instance、driver、placement和source coordinates；
- request id只是连接级 JSON-RPC correlation；没有 durable operation id、idempotency、operation journal或accepted事务；
- server request pending callback主要保存在内存，虽然支持给新连接replay，但不是durable interaction aggregate；
- per-thread请求串行是进程内FIFO队列，不等于可恢复的命令顺序；
- `turn/completed`有状态但协议没有 exactly-one terminal保证，也没有 `Lost`；EOF/transport loss语义不在协议中；
- initialize只协商少量客户端能力和experimental开关，response没有runtime capability/guarantee描述；
- vendor-specific model/config/account/plugin/apps/process/fs等管理面与Agent runtime生命周期混在同一大ClientRequest union中。

因此建议：**借用 Thread/Turn/Item/Interaction/Delta/Accepted 等领域词汇与method风格，重建AgentDash-owned合同；Codex类型只留在Adapter。** L4必须补上 `RuntimeBinding`、`ContextCheckpoint`、compact prepare/activate、durable Operation、typed terminal和context fidelity。

## 2. 协议外壳、初始化、版本与experimental边界

### 2.1 Wire envelope

`references/codex/codex-rs/app-server-protocol/src/rpc.rs:1-82` 定义：

- `RequestId = String | Integer`；
- `JSONRPCRequest { id, method, params?, trace? }`；
- `JSONRPCNotification { method, params? }`；
- `JSONRPCResponse { id, result }`；
- `JSONRPCError { id, error { code, data?, message } }`。

文件开头明确说明它并不发送/期待 `jsonrpc: "2.0"` 字段，虽然保留 `JSONRPC_VERSION` 常量。因此值得借的是 request/response/notification 方向和correlation，而不是假设它完整遵循JSON-RPC 2.0。

`W3cTraceContext` 放在 request上是有价值的transport tracing设计，但trace不能替代 durable operation/session/turn ID。

### 2.2 Initialize

`references/codex/codex-rs/app-server-protocol/src/protocol/v1.rs:29-75`：

- `InitializeParams` 包含 `ClientInfo { name, title?, version }`；
- `InitializeCapabilities` 包含：
  - `experimental_api`；
  - `request_attestation`；
  - `mcp_server_openai_form_elicitation`；
  - `opt_out_notification_methods`；
- `InitializeResponse` 只返回 `user_agent`、`codex_home`、`platform_family`、`platform_os`。

`references/codex/codex-rs/app-server/src/request_processors/initialize_processor.rs:30-150` 保证一个connection只能初始化一次，并把experimental和client metadata存为connection state。

可借鉴：

- 客户端身份 + capability opt-in；
- initialize后才能处理其它请求；
- notification opt-out是连接级presentation优化，不改变业务事实。

不足：

- 没有明确 `protocolRevision`；
- 没有server runtime descriptor；
- 没有operation/profile/guarantee协商；
- experimental capability目前是per-connection，handler注释自己指出共享Thread的多client行为可能不一致（`initialize_processor.rs:45-55`）。

AgentDash首版只需要一个owned `protocol_revision` 与 `RuntimeDescriptor`，不必把它设计成永久兼容治理体系。descriptor应描述当前adapter真实支持的operations、context ownership/fidelity和transport constraints。

### 2.3 Experimental是显式代码标记，不等于注释里的“EXPERIMENTAL”

`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:72-103,383-413` 通过 `ExperimentalApi` trait和macro constants标记experimental method/field；`message_processor.rs:879-887` 在未opt-in时拒绝实际含experimental method/field的request。

`references/codex/codex-rs/app-server-protocol/src/export.rs:93-134,193-262` 在生成TS/JSON schema时会删除标记的method、field和依赖type。schema fixture测试位于 `app-server-protocol/tests/schema_fixtures.rs:1-90`。

这是值得借鉴的工程机制：Rust类型、TS、JSON Schema和运行时gating来自同一事实。

但不能用文档标签推断稳定性：

- `ToolRequestUserInput` 的doc写着 EXPERIMENTAL，server request variant却没有 `#[experimental]`（`common.rs:1463-1473`）；stable schema仍包含 `item/tool/requestUserInput`；
- `AppsListParams` doc写着 EXPERIMENTAL（`v2/apps.rs:8-25`），`app/list`也存在于stable `ClientRequest.ts`；
- `ThreadItem::Plan` doc写着 EXPERIMENTAL，却不是被schema filter移除的experimental variant（`v2/item.rs:246-253`）。

AgentDash应只用owned annotation/profile metadata做gating，不继承Codex注释与实际schema之间的历史差异。

### 2.4 Schema/export

`references/codex/codex-rs/app-server-protocol/src/export.rs:1-60,93-150,193-247` 导出：

- root `ClientRequest` / `ServerRequest` / `ClientNotification` / `ServerNotification`；
- 每个params/response/notification type；
- bundled schema `codex_app_server_protocol.schemas.json`；
- flat v2 schema `codex_app_server_protocol.v2.schemas.json`；
- stable与 `--experimental` 两种输出。

`rawResponseItem/completed` 被排除出public JSON schema（`export.rs:60`），说明它确实是内部vendor surface。

建议AgentDash保留“owned Rust contract -> TS + JSON Schema + runtime validators”的同源生成，但输出只包含AgentDash vocabulary；Codex schema由Adapter测试使用。

## 3. Thread operation逐项映射

Client request全集在 `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:466-1191`；Thread handler在 `app-server/src/request_processors/thread_processor.rs`。

| Codex method | 当前合同与handler事实 | L4判断 | AgentDash建议 |
|---|---|---|---|
| `thread/start` | Params包含model/provider/tier/cwd、approval/sandbox、instructions/personality、environment、dynamic tools、capability roots等；response返回Thread及effective配置（`v2/thread.rs:44-207`） | **借词汇与start/response形状**；字段不可原样提升 | canonical `ThreadStart`只携带binding、profile、configuration/context/tool revision refs；vendor config留Adapter |
| `thread/resume` | 支持thread id、experimental history、experimental path；running thread会rejoin；非running优先history/path/id（`v2/thread.rs:324-447`，handler `thread_processor.rs:2615-3030`） | **借明确resume词汇** | 只允许RuntimeThreadId + pinned binding；history/path是Codex Adapter内部restore strategy |
| `thread/fork` | 可按thread/path分叉，可指定inclusive `lastTurnId`；response返回新Thread，Thread含`forkedFromId`（`v2/thread.rs:494-608`） | **借fork和fork-through-boundary** | 使用source thread + source checkpoint/turn + expected revision；禁止对in-progress boundary fork |
| `thread/read` | `includeTurns`可读metadata+历史；loaded走live store，unloaded走persisted store（`v2/thread.rs:1275-1288`，handler `thread_processor.rs:2181-2265`） | **仅作Thread/Transcript read** | 保留 `thread/read`，但另设 `thread/context/read`；不能把turns视为模型上下文 |
| `thread/list` | cursor、limit、sort、provider/source/archive/cwd/search/parent filters（`v2/thread.rs:1070-1198`） | 可选 `ThreadCatalog` profile | application/管理查询，不是L4最小driver能力 |
| `thread/loaded/list` | 返回当前进程内loaded thread ids（`v2/thread.rs:1229-1248`） | **vendor runtime内部** | 用binding/live-runtime registry query替代，不进入canonical driver contract |
| `thread/archive` | archive目标及spawn subtree；会unload并逐个通知，descendant失败可warn继续（handler `thread_processor.rs:1357-1454`） | 词汇可借，subtree/partial语义不借 | application management operation；事务/子树策略由AgentDash定义 |
| `thread/unarchive` | 返回恢复后的Thread并发notification（`v2/thread.rs:724-739,932-936`） | 可选管理能力 | 不属于ManagedContext最低保证 |
| `thread/delete` | 删除持久Thread | 可选管理能力 | application拥有删除授权与级联，不下放为核心Agent行为 |
| `thread/unsubscribe` | 取消当前connection对loaded thread事件订阅，返回NotLoaded/NotSubscribed/Unsubscribed（`v2/thread.rs:653-672`） | **transport/vendor内部** | Relay/SSE subscription管理，不是Thread业务状态 |
| `thread/name/set` | `{ threadId, name }`，发 `thread/name/updated`（`v2/thread.rs:716-739,1475+`） | 借metadata词汇 | 可选 `ThreadMetadata` profile；AgentRun title通常仍由application拥有 |
| `thread/settings/update` | async queued update；response为空，真正effective settings通过`thread/settings/updated`通知；fields会影响后续turn（`v2/thread.rs:215-308`，`turn_processor.rs:640-660`注释） | **借command->applied event形状** | 增加base/settings revision、operation id和applied revision；不要空ack无correlation |
| `thread/compact/start` | `{ threadId }`，只提交`Op::Compact`并立即空response（handler `thread_processor.rs:1771-1782`） | **借compact operation名，不借保证** | common facade可保留此名；driver contract增加prepare/checkpoint activate与typed result |
| `thread/rollback` | deprecated；按`numTurns`截尾，不回滚文件；response Thread历史明确lossy（`v2/thread.rs:1046-1066`） | **不进入canonical** | 用checkpoint activate / explicit rewind取代；资源副作用另有补偿语义 |
| `thread/turns/list` | experimental分页turn read，支持items view（`v2/thread.rs:1306-1335`） | 可选 `HistoryPagination` profile | 是比巨型`thread/read`更好的read shape，可在owned合同中稳定化 |
| `thread/items/list` | experimental按thread/turn分页item（`v2/thread.rs:1340-1368`） | 可选 `HistoryPagination` profile | 同上；读取projection，不等于context snapshot |
| `thread/inject_items` | 向模型可见history追加raw Responses API JSON（`v2/thread.rs:1292-1302`） | **vendor/provider内部** | 以typed context import/checkpoint operation替代，不暴露raw Responses items |
| goal set/get/clear | Thread goal含objective/status/budget/usage/time（`v2/thread.rs:747-834`） | AgentRun/domain可选 | 不作为L4 managed context最低能力 |
| metadata update | 当前主要patch Git info（`v2/thread.rs:835-887`） | Adapter/application metadata | 不污染核心Thread context contract |
| memory mode/reset | Codex memory实现开关 | vendor内部/可选memory profile | AgentDash memory/context source通过自己的capability/context模块表达 |
| shellCommand / backgroundTerminals / guardian action | 直接shell、进程管理、Guardian内部事件；多项experimental（`v2/thread.rs:951-1043`） | vendor execution surface | 放入Workspace/Process/Guardian可选profile，不属于L4 core |

### 3.1 Thread数据词汇

`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs:170-229` 的 `Thread` 包含：

- `id`：注释声明Codex-generated ID是UUIDv7；
- `session_id`：同一session tree内共享；
- `forked_from_id`、`parent_thread_id`；
- preview、ephemeral、history mode、provider、timestamps/status/path/cwd/CLI version/source；
- name、git info；
- turns只在resume/rollback/fork/read(includeTurns)等响应填充。

可借鉴：

- Thread是可fork的conversation history节点；
- tree/fork provenance是Thread的一等字段；
- metadata read与history detail可以分离；
- response不总是携带完整turn列表。

不可混淆：

- Codex `session_id`不是AgentDash `RuntimeBindingId`；
- Thread path/CLI version/source属于Codex rollout实现；
- `turns`是presentation/persisted history view，不是current context checkpoint。

`ThreadStatus`（`v2/thread.rs:1253-1272`）为 `NotLoaded / Idle / SystemError / Active{WaitingOnApproval, WaitingOnUserInput}`。Active flags很适合read model，但AgentDash还需把binding/placement health与Thread runtime status分开。

## 4. Turn operation与configuration

### 4.1 `turn/start`

`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:68-166`：

- required：`thread_id`、typed `input`；
- optional：`client_user_message_id`、metadata、additional context、environments、cwd/workspace roots；
- sticky override：approval/reviewer/sandbox/permissions/model/tier/effort/summary/personality；
- output schema、collaboration mode。

response返回 `Turn { status = InProgress }`。handler先验证input和Thread，提交core op后立即返回，真正started/completed由notification推进（`turn_processor.rs:430-552`）。

值得借：

- `threadId + input -> InProgress Turn`；
- client user message id用于input idempotency/correlation；
- output schema作为turn request；
- accepted response不等于terminal。

不应原样借：

- 放在turn/start上的大量配置会“影响本turn及后续turn”，使一次消息同时变成隐式Thread mutation；
- `additionalContext: HashMap<String, {value, untrusted|application}>` 太弱，无法表达AgentDash context frame authority、source refs、delivery plan与checkpoint revision。

AgentDash建议把sticky变更放入带revision的 `thread/settings/update`；TurnStart只引用已生效的 `settingsRevision/contextCheckpointId/toolSetRevision`，或携带一个在同一operation内原子生效的typed revision proposal。

### 4.2 `turn/steer`

`v2/turn.rs:172-202` 要求：

- thread id；
- input；
- **required `expected_turn_id`**；
- optional client message id、metadata、additional context。

handler拒绝空expected id，并由core验证没有active turn、expected mismatch、Review/Compact非steerable等情况（`turn_processor.rs:849-920`）。

这是可以几乎原样借鉴的领域操作：

- steer不创建新Turn；
-必须针对精确active RuntimeTurnId；
-不同命名空间的driver turn id由Adapter/binding映射，application绝不能直接比较。

### 4.3 `turn/interrupt`

`v2/turn.rs:206-214` 为 `{ threadId, turnId }`。handler验证active turn；普通turn的response会等到`TurnAborted`，startup interrupt则立即ack（`turn_processor.rs:1343-1398`）。

operation名与target shape值得借，但AgentDash不应保留两种ack语义。建议：

- interrupt command在durable journal accepted后统一返回Accepted；
- 最终是否Interrupted只看exactly-one TurnTerminal；
- driver的“提交interrupt成功”不能被application当作terminal；
- process kill只可映射为弱`AbortOnly` capability，不能冒充protocol interrupt。

### 4.4 Turn data与terminal

`thread_data.rs:231-278`：

- Turn ID注释声明Codex-generated是UUIDv7；
- `items` + `TurnItemsView(NotLoaded/Summary/Full)`；
- `TurnStatus = Completed / Interrupted / Failed / InProgress`（`v2/turn.rs:30-37`）；
- error、start/completion timestamp、duration。

`turn/started`和`turn/completed` payload都携带Thread ID和完整Turn摘要（`v2/turn.rs:375-399`）。`ErrorNotification`另有`will_retry`，retrying error不会终止Turn（`v2/notification.rs:37-56`）。

可借：

- Error telemetry与terminal分开；
- final Turn携带status/error/timing；
- completed notification可以表示failed/interrupted settlement。

AgentDash补充：

- 增加 `Cancelled` 与 `Lost`，或统一 `TurnTerminalKind`；
- exactly-one terminal；
- EOF before terminal = Lost；
- terminal必须携带RuntimeTurnId、operation id和可选driver coordinates；
- accepted/start/terminal之间的合法顺序由Business Runtime验证，而非相信Adapter。

## 5. User input vocabulary

`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:289-370` 定义：

- `Text { text, textElements }`；
- `Image { url, detail? }`；
- `LocalImage { path, detail? }`；
- `Skill { name, path }`；
- `Mention { name, path }`。

映射建议：

| Codex input | AgentDash canonical判断 |
|---|---|
| Text | 原样借鉴；text elements可作为presentation annotations |
| Image URL / local image | 借typed multimodal block；local path必须通过VFS/resource coordinate，不让cloud/remote误解宿主路径 |
| Skill | 概念借鉴，但映射为AgentDash Capability/SkillRef，不暴露Codex `SKILL.md` path |
| Mention | 概念借鉴为typed ResourceRef/SubjectRef，不使用任意字符串path |

AgentDash canonical input不能继续type alias Codex `UserInput`；应拥有自己的blocks并由Codex Adapter显式转换。

## 6. Item全集与delta映射

### 6.1 ThreadItem全集

`references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs:221-399`：

| Codex ThreadItem | 当前内容 | L4归类 |
|---|---|---|
| `UserMessage` | id、clientId、typed content | **最小canonical** |
| `HookPrompt` | hook fragments + hookRunId | Adapter/Hook optional profile；canonical更适合SystemContext item |
| `AgentMessage` | text、phase、memory citation | **最小canonical**，但content应保留structured blocks |
| `Plan` | final text | 可选 `Planning` profile |
| `Reasoning` | summary/content arrays | 可选 `ReasoningTelemetry` profile；需visibility policy |
| `CommandExecution` | command/cwd/process/source/status/actions/output/exit/duration | 常用可选 `WorkspaceActions` profile |
| `FileChange` | changes + patch status | 常用可选 `WorkspaceActions` profile |
| `McpToolCall` | server/tool/status/arguments/app context/result/error | 统一ToolCall canonical的MCP origin；MCP细节留extension |
| `DynamicToolCall` | namespace/tool/args/status/output/success/duration | 统一ToolCall canonical的HostDynamic origin |
| `CollabAgentToolCall` | spawn/send/resume/wait/close与agent states | 可选 `MultiAgent` profile |
| `SubAgentActivity` | started/interacted/interrupted | 可选 `MultiAgent` telemetry |
| `WebSearch` | query + search/open/find action | vendor/builtin tool-specific；可映射generic ToolCall或可选typed item |
| `ImageView` | local path | vendor tool-specific；resource item |
| `Sleep` | duration | vendor scheduling/tool-specific |
| `ImageGeneration` | status/prompt/result/path | 可选 `MediaGeneration` profile |
| `EnteredReviewMode` | review text | 可选 `Review` profile |
| `ExitedReviewMode` | review text | 可选 `Review` profile |
| `ContextCompaction` | **只有id** | 名称/Item lifecycle值得借；payload不足L4 |

L4最小Item不需要复制全部Codex variants。建议最小union：

```text
UserMessage
AgentMessage
ToolCall { origin, name, arguments, status, output/error }
ContextCompaction { operation/checkpoint/provenance }
SystemContextChange { source/revision }
```

Planning、Reasoning、WorkspaceActions、Review、MultiAgent、Media等做正交profile。Codex Adapter可保留更细的source payload作为typed extension/diagnostic，但不能把vendor enum提升成domain union。

### 6.2 Lifecycle与delta全集

`common.rs:1623-1667` 与 `v2/item.rs:1243-1448` 定义：

- generic：`item/started`、`item/completed`；
- message：`item/agentMessage/delta`；
- plan：`item/plan/delta`；
- reasoning：summary text delta、summary part added、raw reasoning text delta；
- command：output delta、terminal interaction；
- file change：patch updated；`fileChange/outputDelta` deprecated且不再发；
- MCP：tool call progress；
- dynamic tool只有started/completed item与server request；
- turn-level：diff updated、plan updated。

所有item delta都携带thread/turn/item id，这是值得借的。`Plan`注释明确final completed item authoritative，不能假设deltas拼接等于final（`v2/item.rs:246-253,1348-1358`）。

AgentDash建议统一基础事件：

```rust
ItemStarted { item, started_at }
ItemUpdated { item_id, item_revision, delta: TypedItemDelta }
ItemTerminal { item, terminal_status, completed_at, error? }
```

各kind仍可有typed delta，但必须共享：

- monotonic item revision或delta sequence；
- final item authoritative；
- started后exactly-one terminal；
- duplicate ingest幂等；
- terminal后delta非法；
- failed tool/file/command是settled item status，不等于整个Turn必然失败。

## 7. Approval、user-input、MCP elicitation与server request-response

### 7.1 Server request全集

`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:1452-1518`：

| Server -> Client request | 含义 | L4判断 |
|---|---|---|
| `item/commandExecution/requestApproval` | command、cwd、actions、reason、approval id、policy amendment、可选决策 | canonical Interaction kind |
| `item/fileChange/requestApproval` | file change approval、reason、grant root | canonical Interaction kind |
| `item/tool/requestUserInput` | questions/options/secret/auto-resolution | canonical Interaction kind |
| `item/permissions/requestApproval` | requested permission profile + grant scope | canonical Interaction kind |
| `mcpServer/elicitation/request` | form/openai form/url，accept/decline/cancel | optional MCP Interaction kind |
| `item/tool/call` | host执行dynamic tool并返回content/success | optional DynamicTools execution request |
| auth token refresh / attestation / current time | client-host设施请求 | vendor/host integration内部 |
| legacy patch/exec approval | deprecated | 不借 |

Command/File decisions区分Accept、AcceptForSession、Decline、Cancel，command还支持policy amendment（`v2/item.rs:54-112`）。这说明“拒绝但继续turn”和“拒绝并interrupt”必须是不同决定，值得进入AgentDash ApprovalDecision。

Tool user input支持：question id/header/question、optional choices、other、secret、auto-resolution，以及question-id到answers映射（`v2/item.rs:1604-1654`）。这是比当前空答案处理完整得多的交互词汇。

MCP elicitation的turn id可空，源码注释说明MCP request自身identity不是turn，turn只是correlation（`v2/mcp.rs:297-318`）。AgentDash Interaction也应允许session-scoped请求，但必须明确scope。

### 7.2 `serverRequest/resolved`

`v2/notification.rs:58-64` 定义 `{threadId, requestId}`；`thread_lifecycle.rs:751-779` 在请求被其它client解决时通知订阅者。

`app-server/src/outgoing_message.rs:274-470`：

- pending callbacks存于进程内map；
- request id由递增integer生成；
- pending request可以按thread replay给新connection；
- client response/error会移除callback；
- close/cancel会清理pending requests。

可借：

- server-side request是独立可correlate对象；
- reconnect后重放未决请求；
- resolved notification防止多client重复处理。

不足：它仍是connection/runtime-memory lifecycle，不是durable business fact。

AgentDash canonical建议不用JSON-RPC callback本身充当事实，而使用durable `Interaction`：

```text
InteractionRequested { interactionId, threadId, turnId?, itemId?, kind, payload, expiresAt? }
interaction/respond { interactionId, operationId, expectedState, response }
InteractionResolved { interactionId, resolution, resolvedBy, operationId }
InteractionExpired / InteractionCancelled
```

Codex Adapter把vendor `RequestId`绑定到`RuntimeInteractionId`；Relay只传typed Interaction。这样approval在进程重启、多client、远端断连后仍可恢复。

## 8. Dynamic tools、MCP、Apps、Skills

### 8.1 Dynamic tools

`references/codex/codex-rs/protocol/src/dynamic_tools.rs:13-67`：

- `DynamicToolSpec = Function | Namespace`；
- function包含name、description、input schema、defer loading；
- runtime通过server request `item/tool/call`让client执行，response为text/image content + success。

ThreadStart的`dynamic_tools`目前experimental，且没有稳定的thread tool replace/revision operation（`v2/thread.rs:128-137`）。

可借词汇：ToolDescriptor、namespace、deferred loading、host-executed tool call。

AgentDash补充：

- `ToolSetId/ToolSetRevision`；
- `thread/tools/replace { expectedRevision, tools }`；
- `thread/tools/updated { revision, digest }`；
- TurnStart引用固定tool revision；
- tool call是Item + Interaction，不只是临时callback；
- tool schema属于context fidelity的一部分。

L4即使没有DynamicTools profile，也应在checkpoint中记录空或固定tool set revision，保证恢复语义不漂移。

### 8.2 MCP

`v2/mcp.rs` 暴露：

- status list（server info、tools、resources、templates、auth）；
- resource read；
- direct tool call；
- OAuth login/reload/startup status；
- tool progress；
- typed elicitation forms与response。

这些是优秀的MCP bridge词汇，但不是ManagedContextRuntime的最低合同。建议作为 `McpBridge` 正交profile；canonical ToolCall/Interaction保留通用字段，MCP-specific server/resource/auth/meta放extension。

### 8.3 Apps

`v2/apps.rs:8-178` 是Codex connector/app catalog、branding、install/accessibility/config state与list update。它属于Codex产品生态和catalog，不应进入AgentDash Agent Runtime core。AgentDash已有Integration/Extension/Capability Pack taxonomy，应用自己的catalog模型。

### 8.4 Skills、Plugins、Hooks、Marketplace

`v2/plugin.rs:21-145,406-540,764-882` 包含：

- skills list/extra roots/config write/changed invalidation；
- Skill metadata/interface/dependencies/scope；
- hooks list；
- marketplace/plugin install/read/share/uninstall。

SkillRef作为UserInput/Capability选择值得借，但Codex plugin marketplace与AgentDash taxonomy不同：这些操作留在Capability Pack/Shared Library/application模块，不放进L4 driver interface。

`SelectedCapabilityRoot`（`codex-rs/protocol/src/capabilities.rs:12-31`）只支持environment path。AgentDash应继续使用自己的resource surface/VFS/context source coordinates，不能退化成远端无法解释的local path。

## 9. Model、configuration、account、review、command等操作面

### 9.1 Model

`common.rs:852-867` 与 `v2/model.rs:29-176`：

- model list：display、reasoning efforts、input modalities、personality、service tiers、default；
- provider capabilities read目前只有namespace tools/image generation/web search；
- rerouted、verification、moderation/safety notifications。

判断：`ModelCatalog`是executor discovery可选profile；具体Codex fields与risk telemetry留Adapter。它不能替代Agent Runtime capability descriptor。

### 9.2 Configuration

`common.rs:1104-1148` 与 `v2/config.rs:244-430,772-833`：

- config read包含effective config、origins、layers；
- value/batch write有merge strategy、expected version、typed conflict/error data；
- requirements read描述组织policy；
- external Agent config detect/import属于迁移工具。

值得借：expected version、layer provenance、effective value、write conflict。归属上它们是Integration instance/host configuration模块，不是Thread/Turn L4 core。

### 9.3 Account

`common.rs:995-1043,1143-1148` 与 `v2/account.rs` 包含login/cancel/logout/read、rate limits、usage、workspace messages、token refresh等。它是Codex/OpenAI account Adapter管理面，不进入canonical Agent runtime。AgentDash只需要DriverInit从credential refs得到授权，以及typed `AuthenticationUnavailable` failure。

### 9.4 Review

`review/start` 支持working tree/base branch/commit/custom target，并可inline或detached到新Thread（`v2/review.rs:17-74`）。这是好的 `Review` 正交profile：Review是特殊Turn kind，可能创建detached Thread；它不应伪装成普通prompt。

### 9.5 Command/process/fs

`command/exec`、write/terminate/resize，experimental `process/*`，FS read/write/watch，以及`thread/shellCommand`都是Codex host utility/desktop能力。它们可以映射到AgentDash Workspace/VFS/Process modules或ToolCall items，但不是L4 ManagedContextRuntime的最小operation set。

特别是`ThreadShellCommandParams`注释明确它unsandboxed并保留shell语法（`v2/thread.rs:951-963`），不能因位于thread namespace就提升为canonical Agent operation。

### 9.6 其它完整ClientRequest家族

为避免遗漏，`common.rs:466-1191` 还包含：

- environment add/info；
- Windows sandbox setup/readiness；
- feedback upload；
- remote control enable/disable/pairing/client management；
- realtime start/audio/text/speech/stop/voices；
- experimental feature list/set；
- permission profile list；
- collaboration mode list；
- fuzzy file search；
- deprecated v1 conversation summary/git diff/auth status。

这些均不应因为与app-server同处一个request union而进入L4 core；只按实际AgentDash module和可选profile吸收。

## 10. Notification全集与归属

Server notification union位于 `common.rs:1607-1701`：

### 可提升为canonical lifecycle

- Thread started/status/archived/unarchived/closed/name updated；
- Turn started/completed；
- Item started/completed；
- typed item deltas；
- Error/Warning（但要typed scope/code）；
- ServerRequestResolved -> InteractionResolved。

### 可选profile notifications

- turn diff/plan；
- hook started/completed；
- MCP progress/status/OAuth；
- account/rate limit；
- apps/skills/fs changes；
- model reroute/verification/safety；
- realtime；
- Guardian auto-review；
- config warning/deprecation。

### 仅vendor/internal

- `rawResponseItem/completed`；
- Codex-specific moderation/guardian/account细节；
- process/fs/remote-control implementation notifications。

`thread/compacted` 已在协议注释中deprecated，v2 canonical是ContextCompaction item（`common.rs:1664-1667`；handler `bespoke_event_handling.rs:926-929`直接忽略legacy event）。AgentDash应同样选择Item/lifecycle作为主事实，但必须扩展Item payload，不能只复制ID。

## 11. Errors、IDs与并发语义

### 11.1 Errors

`app-server/src/error_code.rs:1-32` 使用标准式codes：invalid request/method/params/internal，加overloaded；`JSONRPCErrorError`支持arbitrary data。Config write另把domain error code写进data（`config_processor.rs:558-575`）。

Turn侧有 `TurnError { message, codexErrorInfo?, additionalDetails? }`，`CodexErrorInfo`包括context window、budget、usage、overload、auth、stream disconnected、rollback、sandbox、not steerable等（`v2/shared.rs:71-154`）。

可借：transport error与turn error分层、typed code、data、willRetry。

AgentDash canonical error至少需要：

```text
code
scope = runtime|binding|thread|turn|item|interaction|operation|transport
retryable
message
runtime coordinates
driver/source coordinates
source data
```

Codex-specific `CodexErrorInfo`只在Adapter source data中；Adapter映射为AgentDash code。

### 11.2 IDs

Codex：

- RequestId：string/integer，连接级correlation；
- Thread/Turn：wire上String，注释说明server生成UUIDv7；
- Item：String，各item producer自定；
- Approval有时只有item id，zsh子command额外approval id（`v2/item.rs:1440-1470`）；
- Dynamic tool有call id；MCP elicitation有其协议request identity。

AgentDash必须使用不同newtype：

```text
RuntimeBindingId
RuntimeThreadId / RuntimeTurnId / RuntimeItemId
RuntimeInteractionId
RuntimeOperationId
ContextCheckpointId / ContextCandidateId / ContextCompactionId
DriverThreadId / DriverTurnId / DriverItemId / DriverRequestId
```

Codex Adapter负责映射，任何application expected-turn比较只使用RuntimeTurnId。

### 11.3 Request serialization

`common.rs:106-184,466-1191` 为request声明serialization scope：global、thread、path、process、watch、MCP OAuth等。`app-server/src/request_serialization.rs:1-150` 将同key exclusive request做FIFO，不同key并行，shared reads可并发。

这是很好的并发分类参考，但实现是进程内队列：重启会丢queue，也没有durableaccepted sequence。

AgentDash应提升为：

- 每Thread mutation有单调operation sequence；
- read可按snapshot revision并发；
- accepted operation持久化后才dispatch；
- same thread的start/resume/fork/settings/turn/compact/activate按明确规则串行；
- steer/interaction response可与active turn并行进入，但按operation sequence和expected turn/revision校验；
- transport queue只是delivery implementation，不是顺序事实源。

## 12. L4 ManagedContextRuntime 最小必需集

L4是当前AgentDash内部能力层级，不是永恒外部标准。以下是首版最小集。

### 12.1 核心vocabulary

```text
RuntimeDescriptor
RuntimeBinding
Thread
ThreadConfigurationRevision
Turn
Item
Interaction
Operation
ContextCheckpoint
ContextCompaction
ToolSetRevision
Runtime/Driver coordinates
```

### 12.2 最小operations

| family | 必需method | 最小保证 |
|---|---|---|
| runtime | `initialize` / `runtime/describe` | 返回owned protocol revision、operations、context ownership/fidelity与当前driver constraints |
| thread | `thread/start` | 创建RuntimeThread并建立driver binding coordinates |
| thread | `thread/resume` | pinned binding上恢复同一logical Thread；不把fork当resume |
| thread | `thread/fork` | 从terminal Turn或activated checkpoint创建新Thread，保留provenance |
| thread | `thread/read` | 读取Thread metadata/status/turn projection；明确不是context read |
| settings | `thread/settings/update` | expected revision、applied revision、typed event |
| turn | `turn/start` | typed input + fixed settings/context/tool revisions；accepted后exactly-one terminal |
| turn | `turn/steer` | required expected RuntimeTurnId；ordered injection |
| turn | `turn/interrupt` | accepted与terminal分离；最终Interrupted/Failed/Lost |
| item | lifecycle notifications | started、typed updates、exactly-one terminal；final item authoritative |
| interaction | `interaction/respond` | durable request/resolution；approval/user-input至少完整闭环 |
| context | `thread/context/read` | 返回active checkpoint和materialized context/fidelity，不返回presentation transcript冒充context |
| compaction | `thread/compact/start` | durable ContextCompaction operation；running时可schedule，idle时maintenance turn |
| compaction/internal driver | `thread/compact/prepare` | 生成candidate但不切换live canonical context |
| context/internal driver | `thread/checkpoint/activate` | expected active checkpoint/revision CAS；idempotent activate |
| operations | `operation/read` | accepted/running/terminal状态、outcome event、retry/lost诊断 |

`thread/list/archive/unarchive/name`不是L4 context最低条件，可由ThreadCatalog/Management profile提供。Dynamic tool执行也可选，但L4 checkpoint必须记录确定的ToolSetRevision（可以为空）。

### 12.3 最小Item

```text
UserMessage
AgentMessage
ToolCall
SystemContextChange
ContextCompaction
```

CommandExecution/FileChange虽非常常用，仍可属于WorkspaceActions profile；否则L4会被“是否具有shell/文件工具”不必要地定义。

### 12.4 最小Interactions

```text
CommandApproval
FileChangeApproval
PermissionApproval
UserInputRequest
```

若某driver不产生对应kind可为空，但L3/L4 Interactive保证要求host能够durable接收、恢复并resolve其宣称支持的kind。

## 13. 正交可选profiles

| Profile | Codex参考 | AgentDash归属 |
|---|---|---|
| `ThreadCatalog` | list/search/archive/unarchive/name/delete | application/session management |
| `HistoryPagination` | turns/list、items/list、TurnItemsView | read projection |
| `WorkspaceActions` | CommandExecution、FileChange、diff、terminal interaction | tools/workspace runtime |
| `DynamicTools` | DynamicToolSpec + item/tool/call | ToolSet/host tool execution |
| `McpBridge` | MCP status/resource/tool/OAuth/elicitation | MCP Adapter/profile |
| `Planning` | Plan item/delta、turn plan updated | optional UI/runtime planning |
| `ReasoningTelemetry` | Reasoning summary/raw deltas | visibility-controlled telemetry |
| `Review` | review/start inline/detached、mode items | special Turn kind |
| `MultiAgent` | collab tool calls、subagent activity、thread tree | multi-agent runtime |
| `MediaGeneration` | image generation/view | media tool profile |
| `Realtime` | thread/realtime methods/notifications | realtime transport/profile |
| `ModelCatalog` | model/list、provider caps、reroute | executor discovery |
| `RuntimeAdministration` | config/account/skills/apps/plugins/environment | Integration/application admin，不进入AgentRun runtime facade |

Profile是当前module组合与availability输入，不需要引入长期certificate治理。每个Adapter用共享behavior tests证明当前实现与声明一致即可。

## 14. AgentDash-owned extension methods与事件

### 14.1 Runtime describe

Codex initialize response缺能力描述，建议：

```rust
RuntimeDescribeResponse {
    protocol_revision,
    driver_kind,
    supported_operations,
    context_ownership,
    snapshot_fidelity,
    profiles,
    limits,
}
```

这只是owned module当前协作合同，可随仓库整体重构演进，不需要承诺未知第三方永久兼容。

### 14.2 Operation metadata

所有mutating request共享：

```rust
OperationMeta {
    operation_id: RuntimeOperationId,
    idempotency_key: String,
    expected_thread_revision: Option<u64>,
    actor,
}
```

response共享 `OperationAccepted { operationId, acceptedSequence, target }`。operation journal是Business Runtime持久事实；driver JSON-RPC request id仅是delivery correlation。

### 14.3 Context read

```text
thread/context/read
  -> activeCheckpointId
     revision
     throughEventSequence
     ownership/fidelity
     materializedContext or opaqueSameDriverRef
     settingsRevision
     toolSetRevision
     contentHash
```

必须与`thread/read`分开，防止UI transcript、rollout history和模型context再次混用。

### 14.4 Compaction prepare/activate

```rust
ThreadCompactPrepareParams {
    thread_id,
    operation_id,
    base_checkpoint_id,
    expected_context_revision,
    policy,
}

ContextCheckpointCandidate {
    candidate_id,
    base_checkpoint_id,
    summary,
    replacement_boundary,
    materialized_context_ref,
    settings_revision,
    tool_set_revision,
    content_hash,
    provenance,
}

ThreadCheckpointActivateParams {
    thread_id,
    operation_id,
    candidate_id,
    expected_active_checkpoint_id,
    expected_context_revision,
}
```

prepare不改变live context。Business Runtime先原子持久化checkpoint/segments/head/operation event，再调用idempotent activate。activate失败则binding进入Desynchronized/Lost，禁止继续新Turn。

这正是Codex `ContextCompaction {id}` 缺失的语义。

### 14.5 Tool revision

```text
thread/tools/replace { operationId, expectedRevision, tools }
thread/tools/updated { operationId, revision, digest }
```

TurnStart引用已生效revision；不允许默认no-op成功。

### 14.6 Typed events

建议owned event最小集：

```text
operation/accepted
operation/terminal

runtime/bindingEstablished
runtime/bindingLost
runtime/protocolViolation

thread/started
thread/resumed
thread/forked
thread/statusChanged
thread/settingsUpdated

turn/started
turn/terminal

item/started
item/updated
item/terminal

interaction/requested
interaction/resolved
interaction/expired

thread/context/checkpointPrepared
thread/context/checkpointActivated
thread/compaction/terminal
```

Codex Adapter可以把`turn/completed`映射成`turn/terminal`，把server requests映射成Interaction，把ContextCompaction item映射为`NativeOpaque` telemetry；只有拿到完整candidate/checkpoint信息时才能产生checkpoint events。

## 15. L4不变量

1. RuntimeBinding与Thread是不同实体：binding固定service/driver/placement，Thread表达conversation history。
2. Resume保持同一RuntimeThreadId；Fork创建新RuntimeThreadId并保留source checkpoint/turn provenance。
3. 同一Thread最多一个active Turn；Review/Compact等non-steerable kind显式可见。
4. 每个accepted Turn恰好一个terminal；EOF/driver消失before terminal = Lost。
5. 每个started Item恰好一个terminal；final item authoritative；terminal后无delta。
6. `turn/steer`必须携带expected RuntimeTurnId；Adapter只能在binding内翻译DriverTurnId。
7. Interrupt accepted不等于Turn terminal。
8. Interaction requested/resolved持久化；同一Interaction最多resolve一次；response必须校验Thread/Turn/Item scope。
9. 所有mutation先durable accept再driver side effect；operation idempotency覆盖replay。
10. Per-thread operation sequence是持久事实；transport FIFO不是事实源。
11. `thread/read`与`thread/context/read`不可互相替代。
12. ContextCheckpoint一旦activated不可原地修改；新状态产生新revision/checkpoint。
13. Compact candidate必须基于明确base checkpoint；activate使用CAS；失败不得回退或覆盖head。
14. Compaction成功只在checkpoint和projection事务提交后成立。
15. settings/tool revisions是context checkpoint的一部分；恢复时不能使用“当前最新配置”偷偷改写历史语义。
16. NativeOpaque compaction不推进平台checkpoint。
17. Error telemetry若`retryable=true`不终止Turn；最终仍需terminal。
18. Unknown非关键vendor notification可记录diagnostic；关键lifecycle payload malformed使对应operation/turn Lost或Failed，不能静默跳过。

## 16. Module ownership与依赖方向

### 16.1 `agent-runtime-contract`

拥有：

- Thread/Turn/Item/Interaction/Operation/Checkpoint词汇；
- AgentDash typed IDs；
- operations/events/errors；
- profiles与context fidelity；
- TS/JSON schema生成。

依赖：serde/schema等基础库；**不依赖Codex protocol、application、domain repositories或transport**。

### 16.2 Business Agent Runtime

拥有：

- operation journal和per-thread sequence；
- Thread/Turn/Item/Interaction状态机；
- context construction/materialization；
- checkpoint/compaction prepare-persist-activate编排；
- settings/tool revisions；
- terminal guarantee和projection。

### 16.3 Executor/Driver Adapter

拥有：

- AgentDash operations到Codex methods的映射；
- runtime/driver ID binding；
- Codex server request到Interaction；
- vendor error/item/notification translation；
- unsupported检测。

Codex structs只存在于此module。Native/企业driver可直接协同owned contract，不需要模仿Codex内部rollout/path/config字段。

### 16.4 Transport

stdio/WebSocket/Relay只传owned frames与correlation。Relay是placement transport，不是Agent service/能力主体；它不能改变service vocabulary或提升profile。

### 16.5 Application/AgentRun

调用AgentRun-first facade：send/compact/steer/interrupt/resolve approval/read context。它不调用ThreadCompactPrepare、driver resume path、Codex account/config或MCP OAuth等Adapter内部operation。

## 17. Native、Codex、Relay、企业Agent套用

### Native Pi

- 直接实现owned Thread/Turn/Item；
- PlatformManaged context；
- clean core生成compaction candidate；Business Runtime持久化后activate；
- 可作为L4 reference implementation。

### Codex App Server

- `thread/start/resume/fork/read`、`turn/start/steer/interrupt`、item/server requests具有高复用价值；
- thread path/history/config/account/plugin/process等留Adapter；
- 当前compact只能声明NativeOpaque；
- `thread/read`只能提供transcript/history fidelity，不能声明canonical context read；
- 若未来Adapter能从Codex取得精确model context和replacement provenance，再实现prepare/activate或导入checkpoint。

### Relay placement

- 透明承载owned request/response/event；
- 保留service provenance和runtime IDs；
- transport断连before terminal -> Lost；
- 不把Codex JSONRPC Value再嵌进另一层untyped event。

### 企业Agent

- 可与AgentDash一起调整Core/driver，直接实现owned operations；
- 不必复制Codex所有vendor管理面；
- 最小实现L4核心，按需增加WorkspaceActions、MCP、MultiAgent等profile；
- 当前仓库内协同演进优先，不需要为了假想永久外部标准保留不正确shape。

## 18. 最终建议

### 原样借鉴的领域词汇

- Thread / Turn / Item；
- start / resume / fork / read；
- turn start / steer / interrupt；
- item started / delta / completed（AgentDash命名可收敛为terminal）；
- server-initiated request、resolved；
- expected active turn；
- final item authoritative；
- typed request/response/notification与同源schema。

### 只留Codex Adapter的概念

- rollout path/history precedence、CLI version、loaded list、unsubscribe；
- raw Responses API items；
- OpenAI account/rate limit/auth refresh；
- Codex config layers、apps/plugins/marketplace；
- Codex-specific sandbox/Guardian/remote control/realtime/process/fs；
- CodexErrorInfo和vendor model telemetry；
- `thread/rollback`；
- vendor `thread/compacted`与只有ID的ContextCompaction item。

### AgentDash必须补齐

- RuntimeBinding及runtime/driver typed coordinates；
- owned runtime descriptor/profile/fidelity；
- durable Operation ID、idempotency与journal；
- durable Interaction；
- `thread/context/read`；
- ContextCheckpoint；
- compact prepare/persist/activate；
- settings/tool revisions进入checkpoint；
- exactly-one Turn/Item terminal与Lost；
- typed protocol violation；
- per-thread durable sequence与CAS。

最合适的总体定位是：**Codex App Server约等于L4操作词汇的优秀第一参考Adapter，但不是L4事实源。** AgentDash应借它已经成熟的会话语言和交互形状，同时把它在durability、context fidelity和ownership上的缺口补进自己的Business Runtime，而不是继续直接re-export vendor types。
