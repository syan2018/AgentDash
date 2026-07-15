# 设计：以 main 行为为外部契约，重建 Runtime 内部边界

## 1. 设计目标

本任务不撤销 Agent Runtime 分层，也不恢复旧 connector/session 双事实。目标是让新 Runtime 内部结构继续成立，同时恢复以下不变量：

1. Session presentation 的 protected `BackboneEvent` 与 pinned main-reference 完全等价；只有 Runtime carrier/wrapper允许不同。
2. Runtime operation、turn、item、interaction、tool call 与 outbox各自只有一个 authority，并通过显式坐标关联。
3. 工具目录/Schema是 immutable surface；工具执行上下文在 invocation 时由 canonical binding/turn/frame 解析，不能冻结 bootstrap placeholder。
4. Platform ToolBroker始终拥有 canonical execution/policy/idempotency；外部 presentation producer按 binding/profile选择且恰好一个。
5. presentation/projection是可观察结果，不是工具参数裁决或 Agent loop存活的 authority。
6. 前端继续消费现有 Session feed/reducer/card，不引入新会话 UI或兼容层。

## 2. 关键职责分离

### 2.1 Canonical tool lifecycle 与 presentation lifecycle分离

Platform ToolBroker始终负责：

- canonical Runtime Item / ToolBrokerCall；
- binding/generation/tool-set fencing；
- permission、VFS、credential、timeout/cancel与idempotency；
- owner executor调用与结果收敛。

Session presentation的 producer则由 bound surface决定：

- `VendorStream`：Native/Codex/具备完整标准流的Remote connector负责输出 main/Codex App Server等价 item lifecycle，ToolBroker不重复输出；
- `ToolBroker`：只有 driver明确没有对应 presentation capability时，Broker用 owner projector输出。

这两个概念不得再共用一个模糊的“ToolBroker拥有全部 lifecycle”表述。Runtime internal item ID与presentation item ID也不得复用：

```rust
struct BoundToolPresentationRoute {
    producer: ToolPresentationEmitter,
    family: ToolProtocolProjection,
}

struct ToolCallCoordinates {
    runtime_item_id: RuntimeItemId,
    presentation_item_id: PresentationItemId,
    runtime_turn_id: RuntimeTurnId,
    presentation_turn_id: PresentationTurnId,
    source_turn_id: Option<DriverTurnId>,
    source_item_id: Option<DriverItemId>,
    // binding / generation / tool-set...
}
```

具体类型名可在实现中按现有contract收束，但必须保留“internal identity”和“presentation identity”两条明确坐标，不能通过字符串前缀猜测。

`ToolPresentationEmitter` 必须贯穿：

```text
ToolContribution
  -> profile/binding求交得到effective route
  -> persisted bound tool catalog
  -> DriverToolDefinition
  -> Native/Codex/Remote mapper
  -> ToolBroker journal
```

Native profile在支持的 message/reasoning/tool family上选择 `VendorStream`，从而直接复用 pinned main mapper；ToolBroker仍写 internal canonical tool state但不写第二套card。

### 2.2 Definition compile 与 invocation execution分离

删除“Surface compile时构造 `DynAgentTool` 并冻结到 binding”的语义。目标模型：

```rust
struct PlatformToolRegistration {
    contribution: ToolContribution,
    executor: Arc<dyn PlatformToolOwnerExecutor>,
}

struct PlatformToolExecutionContext {
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Uuid,
    runtime_thread_id: RuntimeThreadId,
    runtime_turn_id: RuntimeTurnId,
    runtime_item_id: RuntimeItemId,
    binding_id: RuntimeBindingId,
    binding_generation: RuntimeDriverGeneration,
    tool_set_revision: ToolSetRevision,
    identity: Option<AuthIdentity>,
    hook_runtime: SharedHookRuntime,
    vfs: Vfs,
    vfs_access_policy: RuntimeVfsAccessPolicy,
    runtime_backend_anchor: Option<RuntimeBackendAnchor>,
}
```

Business Surface compiler只产生定义、schema、projector、capability、scope requirements、digest与owner route。`RegistryToolExecutor` 每次调用时：

1. 由 `ToolExecutionRequest` 取得真实 thread/turn/item/binding/generation；
2. 由 binding registry取得 run/agent/frame、identity、HookRuntime与persisted surface；
3. 校验 invocation 与 binding/surface/tool-set完全一致；
4. 构造 typed `PlatformToolExecutionContext`；
5. 路由到业务 owner executor。

Task/Workspace Module/Companion/Wait/Workflow不得再从 `HookRuntime.session_id` 或 `ExecutionSessionFrame.turn_id` 反推业务 owner。HookRuntime只负责真实 Hook snapshot/evaluation；run/agent/frame/thread/turn由 typed execution context直接提供。

若实现需要过渡性内部步骤，可在同一工作项内先按 invocation实时重建 tool handle作为短桥，但最终提交不得保留 `surface-bootstrap-*` executable或 binding级捕获的 per-turn `DynAgentTool`。

### 2.3 恢复 immutable owner surface

Runtime invocation context必须来自严格的 current AgentFrame/binding surface：

- capability/VFS/HookPlan缺失或非法时 provision typed fail，不得 `unwrap_or_default()`；
- 恢复 frame-scoped permission grants到 `RuntimeVfsAccessPolicy` 与 Broker permission/VFS gate；
- `RegistryToolBrokerPolicy::authorize_permission/authorize_vfs` 不得无条件 Allowed；
- launch evidence frame、orchestration/node/attempt provenance必须由不可变binding/anchor持久化并可恢复，不能拿current frame伪装launch frame；
- identity若采用binding级事实，必须由 invocation context显式读取binding surface，不能依赖旧 tool capture。

这些不是额外产品功能，而是 main 工具执行语义与新 Runtime authority对齐所需的 owner facts。

## 3. Driver command acceptance 与 terminal

### 3.1 三段状态

```text
Pre-acceptance validation
  -> Driver/Core accepts request_id
  -> delivery receipt becomes idempotent
  -> stream/tool execution
  -> TurnTerminal + OperationTerminal
```

- Unsupported、stale generation、无效surface、active-turn冲突只允许在 Core接受prompt前返回 `DriverError`。
- `agent.prompt(...)` 成功即登记同一 `request_id` 的 `DriverDispatchReceipt`，之后 duplicate dispatch只能返回该receipt。
- 接受后的 provider/tool/mapper/sink错误必须通过 Runtime events终结，不得把整条 `TurnStart` 重新暴露为 pre-acceptance error。
- outbox在 delivery acceptance后ack；business terminal不控制outbox retry。

当前 trait可以先保持不变，但实现必须把 receipt登记点前移，并保证 post-acceptance路径返回已接受receipt。若采用真正异步dispatch，driver必须持有stream task、sink与active-turn fence并提供可恢复inspect；不能把任务生命周期留在局部stack。

### 3.2 Operation correlation

`DriverCommandEnvelope` 必须显式携带 owning `RuntimeOperationId`，或由contract提供等价的非字符串猜测关联。对会产生turn的命令：

- Runtime admission产生 `OperationAccepted + TurnStarted`；
- Driver terminal回报包含同一 runtime turn与operation坐标；
- successful/failed/cancelled/lost turn各自映射恰好一个 `OperationTerminal`；
- Runtime state machine阻止下一条普通 TurnStart与悬空active operation发生模糊归属；
- critical violation只终结真正受影响的active operation，不把已成功的旧operation一起改写为Lost。

Outbox错误分类至少区分：

- pre-acceptance retryable unavailable；
- pre-acceptance nonretryable rejected/unsupported/protocol error；
- post-acceptance terminal failure；
- binding lost/rebind recovery。

## 4. Native canonical / presentation mapper

Native mapper以 main-reference `stream_mapper.rs` 为行为oracle，但分为两个输出面：

1. canonical internal Runtime facts；
2. immutable presentation facts。

规则：

- application-owned User输入不再由 Native internal mapper创建 AgentMessage；
- ToolResult消息不创建 AgentMessage；
- assistant canonical text/reasoning才创建 assistant internal item；
- tool-only assistant不创建空 AgentMessage；
- tool start/update/end只由effective presentation route允许的producer输出；
- `ToolCallStart` 与 `ToolExecutionStart` 共享状态机，不能重复 started；
- Native Agent与mapper共享 `ToolResultRefContext/address_provider`，确保第2个工具、后续turn及截断大结果的start/end item ID一致；
- main的 optional/null canonical form保持不变。

### 4.1 Projection错误等级

展示阶段接受流式partial args：

- 参数未完整时输出 main等价 provisional/dynamic item；
- typed字段可用后升级为FsRead/FsGlob/Command/FileChange等；
- apply_patch不完整时fallback dynamic，不中止Agent；
- non-displayable result part按main规则过滤或降级diagnostic；
- schema/executor仍负责真实调用参数验证。

任何 presentation-only错误都不能调用 `agent.abort()`。只有 canonical lifecycle无法建立、sink无法持久化authoritative fact等错误才可以让turn失败，并且仍走post-acceptance terminal而非outbox重投。

## 5. Session API与前端

保持当前 main等价链路：

```text
Runtime journal presentation fact
  -> Session API carrier/wrapper
  -> inner BackboneEvent validator
  -> useSessionFeed
  -> sessionStreamReducer
  -> existing SessionEntry / ToolCallCard
```

不修改 reducer以合并两套错误identity，也不新增 AgentRuntimeFeed。ToolBroker wrapper必须补齐与Native相同的 source/runtime/presentation turn映射；inner payload继续使用协议规定的 thread/turn/item ID。

## 6. Connector边界

| Connector | 输入/工具桥接 | Presentation route | 必须验证 |
|---|---|---|---|
| Native | `DriverToolInvocation -> PlatformToolBroker` | 支持family为`VendorStream`，pinned main mapper唯一输出 | 多工具、partial args、截断结果、继续到final |
| Codex | Codex App Server标准dynamic tool/MCP/command callback接PlatformToolBroker | 标准server notification优先VendorStream；AgentDash扩展仍在owned wrapper | 标准body不被二次投影；Codex 0.144.1 nullable |
| Enterprise Remote | RuntimeWire callback携带完整canonical/source coordinates | 由offer/profile决定VendorStream或ToolBroker，不能默认双发 | generation fence、重放、remote terminal |

connector只负责协议与坐标转换，不决定Task scope、workspace visibility、permission或VFS业务规则。三者进入公共Broker后必须消费同一 invocation context resolver。

## 7. 验证设计

### 7.1 Pinned main行为oracle

在现有七场景 isolated mapper fixture之外，新增组合场景：

- User -> assistant -> 三个并行工具 -> 两个业务错误 -> provider继续 -> final assistant；
- `fs_glob` name/空args/partial args/完整args；
- 第2/第3工具与后续turn的大结果/`readable_ref`；
- shell、apply_patch、MCP、dynamic tool；
- reasoning、usage、provider retry/error、compaction、rewind、ContextFrame；
- optional omitted与explicit null。

fixture生成必须固定 main-reference commit/source hash；current composition输出只剥离允许的 Runtime carrier，protected body与顺序零ignore-list比较。

### 7.2 Production composition + PostgreSQL

使用真实 AgentFrame、binding、surface source、六类 production provider、PlatformToolBroker、Native/Codex/Remote adapter和embedded PostgreSQL。至少断言：

- 每个logical tool恰好一个presentation started/update*/completed；
- user/tool result不成为internal AgentMessage；
- operation/turn/outbox全部终结；
- post-acceptance projection failure不重跑provider/tool副作用；
- Task、Workspace Module、Workflow、Companion、Wait与VFS至少各一条真实scope调用；
- grant/VFS deny在side effect前发生；
- malformed surface closure provision失败；
- restart/rebind/surface adopt重建executor，旧generation被fence；
- journal经真实Session API进入前端 reducer后只形成一个tool card并继续显示final assistant。

### 7.3 禁止弱断言

- production catalog test不得丢弃 `tool.execute()` 的 Result；
- 不得只断言“存在一个terminal”而不查 operation/outbox/active entity；
- 不得只测 mapper或只测 ToolBroker；
- 不得用mock surface source代替 production `AgentBusinessSurfaceSource`证明工具接线完成。

## 8. 需要同步修订的规范

- `backend/agent-runtime-native-adapter.md`：区分 internal canonical lifecycle owner与presentation producer，补dispatch acceptance/operation terminal。
- `backend/agent-runtime-surface-tool-broker.md`：将 executable invocation context改为typed per-call owner route，补effective presentation route。
- `backend/agent-runtime-kernel.md` / `agent-runtime-persistence.md`：补operation/outbox post-acceptance不变量。
- `cross-layer/backbone-protocol.md`：固定wrapper可变、protected body main等价与single presentation producer。

规范只记录目标职责与选择理由，不记录临时错误实现。
