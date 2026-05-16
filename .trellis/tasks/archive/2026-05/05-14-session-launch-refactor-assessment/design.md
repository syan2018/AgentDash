# Design：Session 构建与 Launch 唯一数据流

## 设计原则

### 1. 先定业务边界，再拆模块

本重构以 session 构建边界和 launch 数据流为目标。模块拆分必须是业务边界清晰后的结果。

任何新类型或新 service 都必须回答一个明确问题：

- session 如何被构建；
- launch 需要消费哪些已解析信息；
- context/query/audit 如何投影同一份事实；
- runtime/connector/effects 如何消费该事实。

### 2. Turn 不是架构中心

Turn 的目标边界很薄：

- reservation；
- active execution；
- cancel；
- hook runtime handle；
- stream processor / adapter supervision；
- terminal release。

除 hooks 与 runtime supervision 外，Turn 不应承载 session 构建、owner、VFS、MCP、capability、context、restore、terminal effect 等业务边界。

### 3. SessionConstructionPlan 是唯一事实源

launch、context endpoint、audit/inspector 都必须来自同一份 `SessionConstructionPlan` 投影。禁止 route、task、workflow、routine、companion、local relay 各自重建类似逻辑。

### 4. LaunchExecution 是一次性执行计划

禁止把 `LaunchPlan` 作为新的长期事实源。如果再次出现这个名字，它只能等价于 `LaunchExecution`：一次 launch 的短生命周期执行计划。

### 5. 不为投影强造中间层

connector 输入是 `LaunchExecution` 的字段或子结构即可，不要求独立成 `ExecutionPlan` 传递层。`ExecutionContext` 只在 connector 边界投影生成，不能反向成为架构事实源。

## 目标数据流

```text
Source request
  -> SourceAdapter
  -> LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> RuntimeRegistry + ConnectorGateway + TurnSupervisor
  -> SessionEventWriter
  -> TerminalEvent
  -> TerminalEffectOutbox
```

读取 facts、解析 owner、构建 construction、解析 launch、生成 connector projection 可以作为实现细节存在，但不进入目标数据流图，也不要求形成一组跨模块 DTO。

## 核心类型草案

### `LaunchCommand`

来源入口的意图 DTO，只表达“我要启动/继续一个 session”：

```rust
pub struct LaunchCommand {
    pub session_id: SessionId,
    pub source: LaunchSource,
    pub user_input: UserPromptInput,
    pub identity: Option<AuthIdentity>,
    pub source_hints: LaunchSourceHints,
    pub overrides: LaunchOverrides,
}
```

不允许塞入最终 VFS、MCP、capability、context bundle、hook trigger 或 post-turn handler。

### `SessionConstructionPlan`

session 构建的唯一权威结果：

```rust
pub struct SessionConstructionPlan {
    pub session: SessionIdentityPlan,
    pub owner: Option<ResolvedSessionOwner>,
    pub source: SourceContractPlan,
    pub workspace: WorkspacePlan,
    pub execution_profile: ExecutionProfilePlan,
    pub surface: SessionSurfacePlan,
    pub context: ContextPlan,
    pub identity: IdentityPlan,
    pub projections: ConstructionProjections,
    pub trace: ConstructionResolutionTrace,
}
```

它负责：

- owner / binding；
- source contract；
- workspace / VFS / mount / typed working dir；
- executor config resolution；
- MCP / tools / capability state；
- context fragments / frames / bundle plan；
- identity / assignment / pending action frames；
- context endpoint / audit / inspector projections；
- fallback source trace。

它不负责：

- turn id；
- reservation / active runtime；
- connector accepted 状态；
- hook reload/refresh 的执行；
- repository restore / follow-up 的执行状态；
- terminal effect/outbox 状态；
- pending runtime command 的 applied/failed 状态。

### `LaunchExecution`

一次 launch 的执行计划：

```rust
pub struct LaunchExecution {
    pub launch_id: LaunchId,
    pub session_id: SessionId,
    pub source: LaunchSource,
    pub prompt: ResolvedPromptPayload,
    pub construction: SessionConstructionPlan,
    pub lifecycle: LifecycleLaunchPlan,
    pub restore: RestoreLaunchPlan,
    pub hooks: HookLaunchPlan,
    pub runtime_commands: RuntimeCommandLaunchPlan,
    pub connector_input: ConnectorInputFields,
    pub terminal_effects: TerminalEffectPlan,
    pub trace: LaunchExecutionTrace,
}
```

### connector input 字段

connector-facing 输入作为 `LaunchExecution` 的字段组存在，可在实现中内联，不要求抽成独立类型。字段包括 executor config、working directory、env、MCP servers、VFS、capability state、context frames、restored state、runtime tools。

不把它升级为独立主链路层，除非后续实现证明多个 connector/use case 需要复用同一个稳定 application model。

`LaunchExecution.construction` 表示同一次 `SessionConstructionPlan` 的共享输入或快照引用，不要求复制出第二份长期数据。

### `TerminalEffectPlan`

terminal event 之后进入 durable outbox 的 effect 计划：

```rust
pub struct TerminalEffectEnvelope {
    pub effect_id: EffectId,
    pub kind: TerminalEffectKind,
    pub idempotency_key: String,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub payload: serde_json::Value,
}
```

effect 至少执行一次，有限重试后进入 dead-letter；handler 必须幂等。

## 目标模块

### `session/core`

Session durable facts：meta、title、event seq、execution status projection、create/get/delete、typed meta update。

不负责 owner 选择、surface 构建、launch 策略、connector、effects。

### `session/ownership`

唯一 owner/binding 解析入口。输出 `ResolvedSessionOwner`，供 construction、launch、context query、权限展示共同使用。

### `session/construction`

读取 session facts、owner facts、source domain facts、runtime profile、events、pending command projection，输出 `SessionConstructionPlan`。

这是原先 context / VFS / capability / MCP / identity 组装逻辑的目标归宿；命名上避免和 domain `SessionComposition` 混淆。

### `session/launch`

消费 `LaunchCommand + SessionConstructionPlan + runtime facts`，输出 `LaunchExecution` 并交给 executor。

launch 不重建 owner/surface/context；只解析一次性 launch 策略：prompt payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect。

### `session/runtime`

运行中 turn registry / supervisor：reserve、activate、release、cancel、stall、processor/adapter task handle。

### `session/eventing`

session event append、projection、broadcast、backlog/page、terminal domain event。

### `session/effects`

terminal event -> durable outbox -> typed effect handler。替代 `terminal_callback`、`PostTurnHandler.execute_effects` 的内存即时路径。

### `session/pending`

runtime command requested/applied/failed domain events + 可重建 projection。projection 是查询索引，不是事实源。

### `session/adapters`

HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 只把来源输入转换成 `LaunchCommand`，不构造最终执行上下文。

## 状态轴

| 状态轴 | 回答的问题 | 权威 |
|---|---|---|
| BindingState | session 属于谁 | ownership store |
| ConstructionState | session 当前可见 surface 是什么 | `SessionConstructionPlan` |
| BootstrapPhase | owner 首轮上下文是否完成 | session projection / construction |
| LaunchLifecycle | 本次 launch 是 bootstrap / restore / plain | `LaunchExecution` |
| HookLaunchPlan | 本次 hook reload / refresh / none | `LaunchExecution` |
| TurnLifecycle | 当前 turn claimed / active / terminal | runtime registry |
| ExecutionStatus | 最近执行终态 | event projection |
| RuntimeCommandState | pending/applied/failed runtime command | domain event + projection |
| TerminalEffectState | effect queued/running/retry/dead-letter | durable outbox |

## 不变量

- `PromptSessionRequest` 从生产主链路删除。
- `SessionConstructionPlan` 是 launch/query/audit 的唯一事实源。
- `LaunchExecution` 不承载长期 session 事实。
- `LaunchResolution`、`ExecutionPlan`、`ExecutionProjector` 不作为目标态必需层保留。
- `ExecutionContext` 只是 connector SPI 投影。
- runtime 不临时 fallback owner/VFS/MCP/capability/context。
- context endpoint 不在 route 层重建 session surface。
- terminal fact 先持久化，effect 后进入 durable outbox。
- `SessionHub` 不作为业务能力入口存在；最终代码中不能承载业务判断。
