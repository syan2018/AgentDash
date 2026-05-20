# Activity Lifecycle 后端契约

> Activity lifecycle 的运行核心是 `ActivityLifecycleRunState`。Scheduler 负责 durable claim 与 executor 启动，executor 只能通过事件把结果交还给 `LifecycleEngine`。

## Scenario: Function Executor Immediate Completion

### 1. Scope / Trigger

- Trigger: `ActivityExecutorSpec::Function::{ApiRequest,BashExec}` 接入 Activity scheduler。
- Scope: `agentdash-application/src/workflow/{scheduler.rs,agent_executor.rs,engine.rs}`。
- Why: Function Activity 没有 Agent session terminal，因此启动后必须在同一次 scheduler pass 内把 completed/failed event 交给状态机。

### 2. Signatures

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

### 3. Contracts

- Scheduler still records `ExecutorStarted` before applying any `immediate_events`.
- Agent/Human executors return `ActivityExecutorStartResult::started(...)` with no immediate events.
- Function executors return `ExecutorRunRef::FunctionRun { run_id }` plus exactly one terminal event for the attempt:
  - success -> `ActivityEvent::ActivityCompleted`
  - failure -> `ActivityEvent::ActivityFailed`
- Function output values are mapped to declared `activity.output_ports`; completion policy validation remains owned by `LifecycleEngine`.

Template context for Function specs:

```json
{
  "lifecycle": { "id": "...", "key": "..." },
  "activity": { "key": "collect", "attempt": 1 },
  "run": { "id": "..." },
  "inputs": { "<port_key>": "<value>" },
  "outputs": { "<activity>.<port>": "<value>" }
}
```

### 4. Validation & Error Matrix

- Unknown activity in claim -> scheduler returns bad request before executor start.
- Function template render failure -> `ActivityFailed`.
- API request transport/read failure -> `ActivityFailed`.
- API non-2xx status -> `ActivityFailed`.
- Bash process spawn failure -> `ActivityFailed`.
- Bash non-zero exit -> `ActivityFailed`.
- Missing required output port -> `LifecycleEngine::CompletionPolicyRejected`.

### 5. Good/Base/Bad Cases

- Good: Function `ApiRequest` returns 2xx, output artifact is written, successor Agent activates by artifact transition.
- Base: Function `BashExec` exits 0 and maps stdout/stderr/exit_code JSON to declared output ports.
- Bad: Function `BashExec` exits 7; attempt becomes failed and successor is not activated.

### 6. Tests Required

- `workflow::agent_executor`: API request success/failure and Bash success/failure produce immediate terminal events.
- `workflow::scheduler`: immediate completion event is applied after executor started.
- `workflow`: Function -> successor activation remains covered through engine/scheduler workflow tests.
- Gate: `cargo test -p agentdash-application workflow`, `cargo check -p agentdash-api`.

### 7. Correct Contrast

Function executor completion should be represented as Activity events:

```rust
ActivityExecutorStartResult::with_events(
    ExecutorRunRef::FunctionRun { run_id },
    vec![ActivityEvent::ActivityCompleted { activity_key, attempt, outputs, summary }],
)
```

This preserves one state transition path through `LifecycleEngine`; Function execution does not mutate `LifecycleRun.activity_state` directly.
