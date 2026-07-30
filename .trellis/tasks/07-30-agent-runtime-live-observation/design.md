# 技术设计：Agent Runtime Snapshot / Live Lane 收束

## 1. 设计目标

将当前“每条 live event + 事后完整 snapshot”的混合模型替换为两个时间语义明确的数据面：

```text
Authoritative Snapshot Plane
  Complete Agent read
  -> AgentRuntimeView
  -> initial / reconnect / reset / explicit refresh

Process-local Live Lane
  owner transition / Core partial / tool progress
  -> AgentLiveBatch
  -> AgentRuntimeStreamFrame::Update
  -> incremental frontend reducer
```

snapshot回答“现在已经提交了什么”；live回答“当前连接按什么顺序观察到什么”。
任何类型都不再同时假装自己回答这两个问题。

## 2. 核心合同

### 2.1 共享轻量状态

从 `AgentObservation` 提取不含 conversation 的状态：

```rust
pub struct AgentObservationState {
    pub revision: AgentSnapshotRevision,
    pub context: AgentContextCoordinate,
    pub lifecycle: AgentLifecycleStatus,
    pub execution: AgentExecutionSnapshot,
    pub command_availability: BTreeMap<AgentControlKind, AgentControlAvailability>,
    pub interactions: Vec<AgentInteractionSnapshot>,
    pub thread_name: Option<AgentThreadNameSnapshot>,
    pub source_info: AgentSourceInfo,
}

pub struct AgentObservation {
    pub state: AgentObservationState,
    pub conversation: Vec<CanonicalConversationRecord>,
}
```

具体字段以现有 `AgentObservation` 为准进行无损提取；不得复制第二套控制字段。

### 2.2 Complete Agent live batch

```rust
pub struct AgentLiveBatch {
    pub source: AgentSourceCoordinate,
    pub sequence: RuntimeU64,
    pub state: Option<AgentObservationState>,
    pub presentations: Vec<CanonicalConversationRecord>,
}
```

约束：

- `sequence` 对当前 service/source进程内单调；
- `presentations` 非空，保持 owner投影顺序；
- ephemeral Core partial的 `state = None`；
- durable history commit把 exact committed suffix放进一个 batch；
- durable batch的 `state` 必须从该 commit完成后的 folded owner state直接投影；
- producer不得为了构造 batch再次调用 `read`；
- 一个 commit只分配一个 lane sequence，不能把同一原子状态转移拆成多个可独立投递的 update。

### 2.3 浏览器 stream frame

```rust
pub enum AgentRuntimeStreamFrame {
    Baseline {
        connection_epoch: RuntimeConnectionEpoch,
        view: AgentRuntimeView,
    },
    Update {
        connection_epoch: RuntimeConnectionEpoch,
        lane_sequence: RuntimeU64,
        state: Option<AgentObservationState>,
        presentations: Vec<CanonicalConversationRecord>,
    },
    ResetRequired {
        connection_epoch: RuntimeConnectionEpoch,
        reason: AgentRuntimeResetReason,
        last_sequence: Option<RuntimeU64>,
    },
}
```

`ResetReason`至少包含 `lagged`、`sequence_gap`、`source_mismatch`、
`protocol_error`、`binding_replaced`。错误详情进入服务端 diagnostics；wire只暴露可操作的typed code。

`GET /runtime/view` 保留用于命令后的显式 refresh和诊断读取。
`GET /runtime/updates` 首帧改为 `Baseline`，后续为 `Update/ResetRequired`。
服务端处理顺序固定为：

```text
resolve binding
-> attach live subscription
-> read authoritative baseline
-> send Baseline
-> drain subscription in source order
```

订阅期间 baseline已经包含的 durable record由 presentation identity去重；
baseline之后发生的 ephemeral event不会落入 read/attach窗口。

## 3. Native owner 发布

### 3.1 Durable history

`DashHistoryCallbacks::committed` 已经持有 commit history、entry suffix和逐步 folded state。
它负责：

1. 使用 canonical projector投影本次 exact suffix；
2. 从最终 folded state直接投影 `AgentObservationState`；
3. 一次性发布一个 durable `AgentLiveBatch`。

这样 `InputAccepted + TurnStarted`、interaction变化、usage、tool terminal、turn terminal等
状态转移都带有同一 commit后的控制状态。

### 3.2 Ephemeral execution

`DashExecutionCallbacks` 发布 provider waiting、text、reasoning和工具过程 presentation，
不携带 observation state。其顺序严格位于相邻 durable commit batches之间。

删除 `AgentRuntimeObservation::reconcile_live` 的 source read职责。Runtime update stream只：

- 校验 source；
- 附加 Product thread/connection identity；
- 转发 batch。

## 4. 工具执行流

### 4.1 深模块合同

工具 callback改为事件流：

```rust
pub enum AgentToolExecutionEvent {
    Started,
    Progress(AgentToolProgress),
    Completed(AgentToolResult),
}

#[async_trait]
pub trait AgentToolExecutionStream: Send {
    async fn next(
        &mut self,
    ) -> Result<Option<AgentToolExecutionEvent>, AgentHostCallbackError>;
}

#[async_trait]
pub trait AgentHostCallbacks {
    async fn invoke_tool(
        &self,
        invocation: AgentToolInvocation,
    ) -> Result<Box<dyn AgentToolExecutionStream>, AgentHostCallbackError>;
}
```

`Started` 可以由 Core在 before-tool通过且即将调用 owner时产生，也可以由 execution stream首帧确认；
最终实现必须固定唯一 owner，避免两个 started。推荐由 Core拥有 canonical execution started：

```text
ToolCallRequested/internal proposal
-> before_tool
-> emit ToolCallStarted
-> consume owner progress stream
-> emit ToolCallProgress*
-> emit ToolCallCompleted
```

before-tool deny同样产生 started + failed completed，保证 provider tool call具有闭合 item。

### 4.2 Tool Broker 与现有 AgentTool

`RuntimeToolExecutor` 与平台 `AgentTool` 使用统一 progress reporter/stream adapter。
已有 `ToolUpdateCallback` 通过有界 mpsc适配为 `AgentToolExecutionStream`，随后删除只返回 final result
的生产入口。

progress必须是 typed owner output，包含稳定 invocation/item identity和递增 update index；
projector负责把它映射为：

- command：`CommandOutputDelta`；
- file/apply patch：file change过程 update；
- MCP：`McpToolCallProgress`；
- dynamic/其他工具：`ItemUpdated`。

前端继续只消费 canonical Backbone events，不消费 callback envelope。

### 4.3 RuntimeWire

RuntimeWire为 tool callback引入 correlated progress frame：

```text
ToolCallbackRequest(correlation)
ToolCallbackProgress(correlation, index, payload)*
ToolCallbackResponse(correlation, terminal)
```

progress与response共享有序 inbound queue；response不得越过未提交的 progress。
disconnect在 terminal response前发生时返回 lost/unavailable，不能伪造 completed。

## 5. 前端连接与 reducer

### 5.1 Connection state

`AgentRuntimeConnection` 内部维护：

- 当前 connection epoch与last lane sequence；
- 当前 authoritative baseline；
- 当前 `AgentObservationState`；
- baseline尚未确认的 durable presentation overlay；
- lifecycle/error/reset状态。

它不把 ephemeral presentations写回 `AgentRuntimeView.observation.conversation`。

observer/hook接口区分：

```ts
onBaseline(view)
onUpdate({ state, presentations, laneSequence })
onReset(reason)
```

### 5.2 Session incremental fold

`useSessionStream`：

- `onBaseline`：从 durable conversation创建新的 reducer state；
- `onUpdate`：通过 functional state update只归约当前 presentations；
- `onReset`：丢弃当前 ephemeral state，等待新 baseline；
- target变化：销毁旧 connection和全部 scoped state。

移除每次 render对完整 records的 `map + reduceStreamState(createInitial..., events)`。
presentation id只用于 transport/baseline去重；item id继续用于消息和工具生命周期合并。

### 5.3 Thinking

Thinking派生输入为：

```ts
activeTurn = runtimeState.execution.active_turn
providerAttemptStateByRound
reasoningEntriesByTurn
```

规则：

- 没有 active turn时没有 streaming Thinking；
- provider waiting的 turn必须等于active turn；
- active turn变化时清除其他turn的 ephemeral attempt状态；
- first text/reasoning/tool output或provider phase推进后关闭对应 waiting；
- terminal durable batch携带 idle state并在同一 update清除 waiting；
- reducer额外保持 terminal吸收规则，防御 adapter错误或同batch旧 telemetry。

## 6. 正常终态与恢复

正常终态不再触发例行完整 refresh：

```text
ephemeral deltas
-> durable ItemCompleted(full body)
-> durable TurnCompleted(full terminal)
-> state.execution = idle
```

Session reducer用相同 item identity完成卡片，idle state结束 receiving/thinking。

authoritative read只在以下场景发生：

- 初始/重连 baseline；
- typed reset/gap/lag；
- command response要求显式 refresh；
- 用户显式 refresh；
-诊断或其他独立查询。

## 7. Backpressure 与错误

- Complete Agent producer不等待浏览器；有界 broadcast允许lag。
- Runtime consumer不再执行 per-event read，因此正常 token流不会自发制造积压。
- Lagged转换为 `ResetRequired::Lagged { skipped }`，记录 diagnostics后关闭当前 epoch。
- API serialization error必须记录 target/source/epoch/sequence；若还能写入，发送 protocol reset，
  否则关闭连接并由客户端按 transport error恢复。
- 前端同一 epoch只启动一次 recovery，重复 close/reset不产生并发 baseline读取。

## 8. 一致性与删除

实施完成后删除：

- `AgentRuntimeObservation::reconcile_live` 的 read-then-wrap模型；
- 每 update完整 observation conversation；
- terminal presentation触发的常规 refresh；
- result-only生产工具 callback；
- front-end全 conversation逐 update重放；
- parity inventory中没有真实 production test支撑的覆盖声明；
- 被新 generated contracts替代的旧类型和兼容 parser。

## 9. Spec 更新

至少更新：

- `backend/agent-runtime-kernel.md`
- `backend/agent-runtime-native-adapter.md`
- `backend/agent-runtime-driver-host.md`
- `backend/agent-runtime-agentrun-facade.md`
- `cross-layer/backbone-protocol.md`
- `cross-layer/agent-runtime-wire-relay.md`
- `frontend/architecture.md`
- `frontend/hook-guidelines.md`

核心原因应记录为：snapshot与live回答不同时间问题；只有 owner发生点能形成一致的状态与presentation
batch，事后读取当前snapshot不能补齐旧event。
