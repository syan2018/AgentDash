# Session Construction / Launch 唯一数据流

## 目标

用一条数据流收口所有 session 启动与查询逻辑：

```text
Source Request
  -> SourceAdapter
  -> LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> RuntimeRegistry + ConnectorGateway + TurnSupervisor
  -> SessionEventWriter
  -> TerminalEvent
  -> TerminalEffectOutbox
```

这条链路只描述稳定数据边界，不把每个内部计算步骤都提升成架构层。读取 facts、解析 owner、构建 construction、解析 launch、生成 connector projection 可以是模块内函数或 port，但不能成为必须跨层传递的中间对象。

## LaunchCommand

来源入口的意图输入：

- session id；
- source；
- user input；
- auth identity；
- env / source hints / explicit follow-up；
- source-level overrides。

不包含最终 VFS、MCP、capability、context bundle、hook trigger、post-turn handler。

## SessionConstructionPlan

session 构建事实源：

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

包含：

- session meta summary；
- owner / binding；
- source contract；
- project / story / task / workflow / routine / companion facts；
- workspace / VFS / mount / typed working dir；
- executor config resolution；
- MCP / tools / capability state；
- context fragments / frames / bundle plan；
- identity / assignment / pending action frames；
- context endpoint / audit / inspector projections；
- construction fallback trace。

不包含：

- turn id；
- runtime reservation；
- connector accepted 状态；
- hook reload/refresh 执行结果；
- repository restore / executor follow-up 执行状态；
- terminal effect / outbox 状态；
- runtime command applied / failed 状态。

## LaunchExecution

一次 launch 的执行计划。它是短生命周期对象，只服务本次启动，不作为 context query 或 audit 的长期事实源：

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

`connector_input` 是 `LaunchExecution` 的内部字段或子结构，不要求独立成新的传递层。它包含 connector 执行所需的最终输入：

- executor config；
- working directory；
- env；
- MCP servers；
- VFS；
- capability state；
- context frames；
- restored state；
- runtime tools。

`construction` 表示同一次 `SessionConstructionPlan` 的共享输入或快照引用，不要求复制出第二份长期数据。

`ExecutionContext` 只在 connector 边界由 `LaunchExecution.connector_input + SessionConstructionPlan` 投影生成。

## RuntimeTurn

运行中 turn 状态：

- reservation；
- active execution；
- hook runtime handle；
- cancel token；
- processor / stream adapter supervision；
- terminal release。

Turn 不处理 owner、VFS、MCP、capability、context、follow-up fallback。

## TerminalEffectOutbox

terminal event 先落库，effect 再进入 outbox：

- typed kind + JSON payload；
- idempotency key；
- finite retry；
- dead-letter；
- manual / background replay。

## 字段归属规则

| 问题 | 边界 |
|---|---|
| session 属于谁 | ownership / construction |
| agent 能看到什么 workspace、VFS、MCP、capability、context | construction |
| context endpoint 和 audit 展示什么 | construction projection |
| 本次 prompt payload 是什么 | launch command / launch execution |
| 本次是否 bootstrap、restore、follow-up、hook reload | launch execution |
| connector 需要什么执行输入 | launch execution connector input |
| 当前 turn 是否运行、能否取消 | runtime turn |
| turn 结束后触发什么业务动作 | terminal effect outbox |
| capability/runtime transition 是否已应用 | runtime command event/projection |
