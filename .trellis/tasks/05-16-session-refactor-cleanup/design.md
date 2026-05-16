# Session 重构彻底收尾 Design

## Target Architecture

目标仍以 review 文档为准：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> LaunchExecutor / TurnSupervisor -> TerminalEffectRouter
```

`SessionHub` 符号已删除。跨 crate 入口只保留 `SessionRuntimeBuilder` 与
`SessionRuntimeServices`；application 内部的 `SessionRuntimeInner` 是 crate-private
装配细节。每个生产 service 持有自己的显式依赖：

- launch：`SessionLaunchDeps`
- hooks：hook provider + runtime registry + audit bus
- capability/runtime transition：runtime registry + stores + tool builder deps
- effects：terminal effect store + hook trigger dispatcher + terminal callback provider + auto-resume scheduler
- title/core/eventing/control：各自 store/service deps

## Workstreams

### 1. Terminal Effect Router 去 Hub 化

原 `SessionTerminalEffectDispatcher` 通过 `TerminalEffectDeps { hub: &SessionHub }` 访问：

- terminal effect store；
- hook trigger dispatch；
- terminal callback；
- hook effect handler registry；
- hook auto-resume scheduler。

目标改为：

```rust
pub struct TerminalEffectRouterDeps {
    pub terminal_effects: Arc<dyn SessionTerminalEffectStore>,
    pub hook_trigger: Arc<dyn TerminalHookTriggerPort>,
    pub terminal_callback: Arc<RwLock<Option<DynSessionTerminalCallback>>>,
    pub hook_effect_handler_registry: Arc<RwLock<Option<DynTerminalHookEffectHandlerRegistry>>>,
    pub auto_resume: Arc<dyn TerminalAutoResumePort>,
}
```

`SessionEffectsService` 持有上述 deps，不持有内部 runtime 装配对象。

### 2. LaunchPlanner Hook Runtime 纯化

当前 planner 调 `deps.hooks.resolve_hook_session(...).await`，失败时直接
`turn_supervisor.clear_turn_and_hook(...)`。目标是：

- planner 可以准备 hook plan，但不负责清理 turn；
- executor 调 planner 失败时统一清理 claim；
- hook resolve 属于 launch preparation effect，失败语义由 `execute_constructed_launch` 管理。

第一步先移除 planner 内部 clear；后续再把 hook resolve 函数整体移动到 executor 准备阶段。

### 3. Session Runtime 业务入口退场

API `ServiceSet` 不再公开 `session_hub` 给 route/use-case 使用。本机 runtime 也不再把
内部 runtime 装配对象放进 WebSocket config / command handler，而是持有 `SessionRuntimeServices`。
AppState 构造期间使用 `SessionRuntimeBuilder` 完成延迟注入，builder 不进入业务服务集合。

### 4. Runtime Command / DB 清理

现有 `session_runtime_commands` 已经替代 meta pending transitions。彻底项包括：

- 对外命名从 pending capability transition 收敛到 runtime command request；
- 如果 schema 字段名仍表达旧含义，新增 migration 改名或增加新字段；
- repository API 需要保留 apply-once、failed、replay 查询语义。

runtime command 状态事实使用 `requested / applied / failed`。PostgreSQL 通过新增
`0037_runtime_command_requested_status.sql` 把旧 `pending` 数据一次性迁为 `requested`；
SQLite 初始化路径同样修正本地既有库状态。

## Migration Strategy

- 小步提交，每步保持编译与关键测试可跑。
- 先去 Hub 依赖最重但边界最清楚的 terminal effects。
- 再处理 planner 清理副作用。
- 最后处理 AppState/ServiceSet 暴露面和 DB 命名。

## Risk

- terminal effect replay 依赖 durable handler registry，去 Hub 化必须保持 replay 能找到 callback/handler。
- hook auto-resume 不能丢失限流语义。
- planner 清理副作用移走后，所有失败路径必须仍释放 claimed turn。
