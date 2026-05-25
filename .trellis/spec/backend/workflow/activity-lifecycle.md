# Activity Lifecycle Backend Contract

Activity lifecycle 的运行核心是 `ActivityLifecycleRunState`。Scheduler 负责 durable claim 与 executor 启动，executor 只能通过事件把结果交还给 `LifecycleEngine`。模块不变量见 [Workflow Architecture](./architecture.md)。

## Core Runtime Contract

- `/lifecycle-runs` resolves `ActivityLifecycleDefinition` by project-scoped id/key。
- run 初始化通过 `LifecycleEngine::initialize`。
- 持久化使用 `LifecycleRun::new_activity`。
- 启动 ready attempts 由 `ActivityLifecycleRunService::launch_ready_attempts` 负责。
- Agent session terminal callbacks 只解析 Activity session bindings。
- Successor activation 委托给 Activity scheduler。
- Hook evaluation 可以报告 completion metadata，但 durable state advancement 仍由 ActivityEvent application 拥有。

## Executor Launcher

Trait contract:

```rust
#[async_trait]
pub trait ActivityExecutorLauncher {
    async fn start(
        &self,
        definition: &ActivityLifecycleDefinition,
        state: &ActivityLifecycleRunState,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError>;
}

pub struct ActivityExecutorStartResult {
    pub executor_run: ExecutorRunRef,
    pub immediate_events: Vec<ActivityEvent>,
}
```

## Function Executor

Function Activity 没有 Agent session terminal，因此启动后必须在同一次 scheduler pass 内把 terminal event 交给状态机。

Function execution port:

```rust
async fn execute_function_activity(
    definition: &ActivityLifecycleDefinition,
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    spec: &FunctionActivityExecutorSpec,
    state: &ActivityLifecycleRunState,
) -> Result<FunctionExecutionResult, String>;
```

Contract:

- Scheduler 先记录 `ExecutorStarted`，再应用 `immediate_events`。
- Agent/Human executors 返回 started result，不带 immediate events。
- Function executors 返回 `ExecutorRunRef::FunctionRun { run_id }` plus exactly one terminal event。
- success -> `ActivityEvent::ActivityCompleted`。
- failure -> `ActivityEvent::ActivityFailed`。
- Function output values 映射到 declared `activity.output_ports`。
- completion policy validation 由 `LifecycleEngine` 拥有。

Template context:

```json
{
  "lifecycle": { "id": "...", "key": "..." },
  "activity": { "key": "collect", "attempt": 1 },
  "run": { "id": "..." },
  "inputs": { "<port_key>": "<value>" },
  "outputs": { "<activity>.<port>": "<value>" }
}
```

## Activity Session Binding

`complete_lifecycle_node` 通过如下 session binding 定位当前 work：

```text
lifecycle_activity:{run_id}:{activity_key}#{attempt}
```

找到绑定后提交 `ActivityCompleted` 或 `ActivityFailed`。

## ProjectAgent Default Lifecycle

ProjectAgent single-workflow defaults create a one-activity lifecycle:

- executor: `ActivityExecutorSpec::Agent`
- session policy: `AgentSessionPolicy::ContinueRoot`

## Workflow Template Asset Contract

Workflow template assets 进入 Shared Library 或从 Marketplace 安装/更新时，必须使用 normalized Activity payload。

Contract:

- Workflow template payloads normalized to `template.lifecycle.entry_activity_key`、`activities`、`transitions` before deserialization or persistence repair。
- Shared Library startup repair rewrites builtin workflow template assets to normalized shape and recomputes `payload_digest`。
- Project install/update commits workflow definitions and activity lifecycle definition in one database transaction。
- Overwrite install preserves project resource ids and `created_at`，increments `version`，updates installed source metadata together。
- Runtime active workflow projection resolves from Activity session bindings and Activity lifecycle definitions。
- Missing Activity binding means no active workflow projection。
