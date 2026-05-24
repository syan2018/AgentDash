# Prompt Pipeline 阶段边界收敛 Design

## Target Architecture

目标是把 session launch 收敛为一个显式子域：

```text
crates/agentdash-application/src/session/launch/
  mod.rs
  command.rs
  plan.rs
  planner.rs
  deps.rs
  orchestrator.rs
  preparation.rs
  connector_start.rs
  commit.rs
  ingestion.rs
  service.rs
```

`session/mod.rs` 继续暴露必要的 launch public surface，但实际实现不再放在 `prompt_pipeline.rs`、`launch_planner.rs`、`launch_service.rs` 这些平铺文件里。

目标调用链：

```text
SessionLaunchService::launch_command_with_outcome
  -> SessionLaunchOrchestrator::launch
    -> reserve_turn_and_load_runtime_facts
    -> build SessionConstructionPlan
    -> LaunchPlanner::plan
    -> TurnPreparer::prepare
    -> ConnectorStarter::start
    -> TurnCommitter::commit
    -> StreamIngestionAttacher::attach
```

事实源边界不变：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchPlan
  -> ExecutionContext
  -> SessionEvent / TerminalEffectOutbox
```

`LaunchPlan` 是原 `LaunchExecution` 的目标语义名称：它表达“本轮应如何启动”，不表达“已经执行完成”。真正执行状态由后续阶段类型表达。

## Before/After Alignment

### Before

```text
session/prompt_pipeline.rs
  SessionLaunchDeps
  SessionLaunchExecutor::execute_command
  SessionLaunchExecutor::execute_constructed_launch
    plan launch
    prepare tools/context frames
    activate turn
    apply runtime transitions
    call connector.prompt
    commit user/start/context/capability/meta/runtime-command
    spawn title generation
    attach processor + stream adapter
  impl SessionRuntimeInner hook runtime helpers
```

问题不是代码都在一个文件里，而是阶段语言不清楚：`execute_constructed_launch` 既是 planner caller，又是 preparer、starter、committer、ingestion attacher。任何后续能力都容易继续塞进这条大函数。

### After

```text
session/launch/
  command.rs
  plan.rs
  planner.rs
  orchestrator.rs
  preparation.rs
  connector_start.rs
  commit.rs
  ingestion.rs
  service.rs

SessionLaunchService
  -> SessionLaunchOrchestrator
    -> LaunchPlanner
    -> TurnPreparer
    -> ConnectorStarter
    -> TurnCommitter
    -> StreamIngestionAttacher
```

最终代码应让读者不打开实现细节也能看懂：哪些逻辑发生在 connector accepted 前，哪些逻辑必须等 accepted 后才提交，哪些逻辑只是 stream ingestion。

## Public Surface Policy

### Wire/API Surface

这些默认不改变：

- HTTP routes / DTO。
- Relay JSON protocol。
- BackboneEnvelope payload。
- NDJSON stream payload。
- connector prompt 接口的 wire 行为。

### Application Rust Surface

可以为清晰边界重命名，但不保留兼容 alias：

- `LaunchExecution` -> 推荐 `LaunchPlan`。
- `LaunchExecutionInput` -> 推荐 `LaunchPlanInput`。
- `LaunchExecutionTrace` -> 推荐 `LaunchPlanTrace`。
- `LaunchExecutionTraceEntry` -> 推荐 `LaunchPlanTraceEntry`。
- `ConnectorInputPlan` -> 推荐 `ConnectorContextPlan`。
- `RuntimeCommandLaunchPlan` -> 推荐 `RuntimeCommandApplyPlan`。
- `TerminalEffectPlan` -> 推荐 `TerminalEffectHandlingPlan`。
- `SessionLaunchExecutor` -> 推荐 `SessionLaunchOrchestrator`。
- `SessionLaunchPlanner` -> 推荐 `LaunchPlanner`。

需审慎评估：

- `LaunchCommandOutcome`：可以保留，因它确实是 command 返回结果；若重命名，推荐 `LaunchOutcome`。
- `LaunchStrictness`：当前 `Strict` 与 `Relaxed` 都要求 construction provider，命名容易误导。执行时应审计它是否仍有语义差异；若只是错误信息差异，应删除或改名为更准确的 source policy。
- `SessionLaunchDeps`：第一阶段可保留；阶段边界稳定后再拆成窄依赖，避免 rename 与依赖拆分同时扩大风险。

## Stage Types

### `LaunchPlan`

输入：

- `LaunchCommand`
- `SessionConstructionPlan`
- runtime facts：existing runtime、session meta、requested runtime commands

职责：

- resolved prompt payload。
- lifecycle / restore / follow-up plan。
- hook snapshot/reload plan。
- runtime command apply plan。
- terminal effect handling plan。
- connector context projection seed。
- launch trace/summary。

不允许：

- 不提交事件。
- 不更新 session meta。
- 不标记 runtime command applied。
- 不启动 stream processor。

### `PreparedTurn`

输入：

- `LaunchPlan`
- `SessionLaunchDeps`

输出建议：

```rust
struct PreparedTurn {
    plan: LaunchPlan,
    context: ExecutionContext,
    source: SourceInfo,
    capability_keys: Vec<String>,
    accepted_context_frames: Vec<ContextFrame>,
    pending_transition_application: PendingRuntimeContextApplication,
    hook_session: Option<SharedHookSessionRuntime>,
    post_turn_handler: Option<DynPostTurnHandler>,
    executor_config_for_meta: AgentConfig,
}
```

职责：

- 组装 runtime tools、direct MCP tools、relay MCP tools。
- activate turn runtime projection。
- apply pending runtime context transitions for this turn。
- 构造 identity / continuation / owner bootstrap / assignment / pending action / queued notices frames。
- dedupe context frames。
- enqueue transform_context notices。
- owner bootstrap 时触发 `SessionStart` hook preparation。

不允许：

- 不调用 `connector.prompt`。
- 不提交 accepted 后事件。
- 不标记 runtime command applied。

### `ConnectorAcceptedTurn`

输入：

- `PreparedTurn`

输出建议：

```rust
struct ConnectorAcceptedTurn {
    prepared: PreparedTurn,
    stream: ExecutionStream,
}
```

职责：

- 调用 `connector.prompt`。
- 把 `connector.prompt` 成功返回 stream 作为 accepted 边界。
- setup 失败时释放 turn/hook 并记录 failed terminal。

不允许：

- setup 失败时不能提交 user/start/context/capability/meta/runtime-command success side effects。

### `CommittedTurn`

输入：

- `ConnectorAcceptedTurn`
- mutable `SessionMeta`

职责：

- 持久化 user message envelopes。
- 持久化 `TurnStarted`。
- emit capability state changed events。
- emit accepted context frame events。
- 更新 session meta：running、last turn、executor config、bootstrap state、title hint。
- connector accepted 后提交 runtime command applied。
- connector accepted 后触发 title generation。

输出建议：

```rust
struct CommittedTurn {
    accepted: ConnectorAcceptedTurn,
    session_id: String,
    turn_id: String,
}
```

### `AttachedTurn`

输入：

- `CommittedTurn`

职责：

- spawn `SessionTurnProcessor`。
- register processor tx。
- spawn stream adapter。
- register stream adapter abort handle。

输出：

- turn id。

## Event And Side Effect Order

目标顺序：

```text
claim prompt
load session meta + requested runtime commands
build construction
plan launch
prepare turn runtime/context/tools
activate turn runtime projection
connector.prompt
  ├─ Err -> clear turn/hook + failed terminal, return error
  └─ Ok(stream) = accepted boundary
commit user messages
commit TurnStarted
emit capability events
emit context frame events
update session meta
mark runtime commands applied
spawn title generation if needed
attach SessionTurnProcessor
attach stream adapter
return turn id
```

这条顺序是验收契约。实现可以通过阶段类型让错误路径更难写错。

## Module Migration Shape

当前 Rust 模块迁移建议：

```text
session/launch.rs
  -> session/launch/mod.rs
  -> session/launch/command.rs
  -> session/launch/plan.rs

session/launch_planner.rs
  -> session/launch/planner.rs

session/launch_service.rs
  -> session/launch/service.rs

session/prompt_pipeline.rs
  -> session/launch/deps.rs
  -> session/launch/orchestrator.rs
  -> session/launch/preparation.rs
  -> session/launch/connector_start.rs
  -> session/launch/commit.rs
  -> session/launch/ingestion.rs
```

`session/mod.rs` 需要保持外层使用路径清晰：

```rust
pub mod launch;
pub use launch::{LaunchCommand, LaunchOutcome, LaunchSource, SessionLaunchService};
```

是否保留 `LaunchCommandOutcome` 由命名审计决定。

## Dependency Strategy

第一步可保留宽 `SessionLaunchDeps`，避免同时处理结构迁移与依赖收窄。

第二步按阶段拆窄依赖：

```text
LaunchConstructionDeps
TurnPreparationDeps
ConnectorStartDeps
TurnCommitDeps
StreamIngestionDeps
```

拆依赖的目标不是多造类型，而是让每个阶段的测试不必构造无关 service。

## Hook Runtime Placement

当前 `prompt_pipeline.rs` 中仍含 `impl SessionRuntimeInner` 的 hook runtime helper。目标是：

- hook session reload/refresh 决策进入 `hooks_service` 或 `hook_runtime` 相关模块。
- launch preparation 只调用 hook service，不直接扩展 `SessionRuntimeInner`。
- `SessionRuntimeInner` 保持 wiring / ready gate / runtime registry owner，不再被 launch 文件补方法。

## Testing Strategy

需要分三层测试：

### Characterization Tests

用于锁现有行为，优先保留或补齐：

- connector setup failure 不提交 `TurnStarted`。
- connector setup failure 不把 bootstrap pending 改成 bootstrapped。
- connector setup failure 不把 requested runtime command 标成 applied。
- runtime command applied commit 失败时转 failed。
- owner bootstrap 初始 context frames 仍被提交。
- pending action / identity frame transform_context 过滤规则不变。
- stream close/error/cancel terminal kind 不变。

### Stage Unit Tests

拆阶段后新增：

- `TurnPreparer` 只准备 context/tools，不提交事件。
- `ConnectorStarter` 成功返回 `ConnectorAcceptedTurn`，失败走 cleanup。
- `TurnCommitter` 只能消费 `ConnectorAcceptedTurn`。
- `StreamIngestionAttacher` 只做 processor/adapter wiring。

### Integration Tests

保留现有 hub tests，并确保这些高价值用例通过：

- `start_prompt_records_current_turn_state`
- `build_tools_filters_relay_mcp_with_initial_capability_state`
- `start_prompt_records_failed_terminal_when_connector_setup_fails`
- `connector_setup_failure_does_not_commit_bootstrap_or_requested_commands`
- `start_prompt_releases_claim_when_session_meta_is_missing`
- `cancel_marks_running_turn_interrupted`
- relay connector early event buffering test

## Risk Controls

- 一次只迁移一个阶段；每个阶段迁移后跑相关测试。
- rename 与 logic move 尽量分开提交/检查点处理。
- 如果某阶段抽取导致 lifetime/ownership 复杂度明显上升，优先调整阶段结果结构，而不是回退到大函数。
- 不引入兼容 alias；旧名字删除后由编译器帮助找齐调用点。
- 不改变 wire/API payload；任何跨层语义变化另立任务。
