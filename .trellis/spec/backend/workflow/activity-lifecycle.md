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

## Scenario: Activity Run Startup And Advancement

### 1. Scope / Trigger

- Trigger: explicit `/lifecycle-runs` creation, ProjectAgent default lifecycle startup, Agent child session terminal, and `complete_lifecycle_node`.
- Scope: `agentdash-api/src/routes/workflows.rs`, `agentdash-api/src/routes/project_agents.rs`, `agentdash-application/src/workflow/{activity_run.rs,orchestrator.rs,tools/advance_node.rs}`.
- Why: Runtime progression must have one authority. LifecycleRun creation initializes `ActivityLifecycleRunState`; all later progression enters `LifecycleEngine` through ActivityEvent.

### 2. Contracts

- `/lifecycle-runs` resolves `ActivityLifecycleDefinition` by project-scoped id/key, initializes the run through `LifecycleEngine::initialize`, persists with `LifecycleRun::new_activity`, then invokes `ActivityLifecycleRunService::launch_ready_attempts`.
- ProjectAgent single-workflow defaults create a one-activity lifecycle with `ActivityExecutorSpec::Agent` and `AgentSessionPolicy::ContinueRoot`.
- `complete_lifecycle_node` locates the current work by `lifecycle_activity:{run_id}:{activity_key}#{attempt}` session binding and submits `ActivityCompleted` or `ActivityFailed`.
- Agent session terminal callbacks only resolve Activity session bindings. Successor activation is delegated to the Activity scheduler; orchestrator code does not branch on `LifecycleNodeType`.
- Hook evaluation can still report completion metadata, while durable state advancement remains owned by ActivityEvent application.

### 3. Tests Required

- `cargo test -p agentdash-application workflow`
- `cargo check -p agentdash-api`
- `pnpm --filter app-web test workflow`
- `pnpm --filter app-web typecheck`

## Scenario: Workflow Template Asset Migration And Install

### 1. Scope / Trigger

- Trigger: builtin/plugin/user workflow template assets entering Shared Library, and project install/update from Marketplace.
- Scope: `agentdash-domain/src/shared_library`, `agentdash-application/src/shared_library`, `agentdash-infrastructure/src/persistence/postgres/{shared_library_repository,workflow_repository}.rs`.
- Why: Workflow template assets are part of the Activity lifecycle contract. Marketplace status, installed source, and project definitions must compare the normalized Activity payload, not a stale pre-Activity shape.

### 2. Contracts

- Workflow template payloads are normalized to `template.lifecycle.entry_activity_key`, `activities`, and `transitions` before deserialization or persistence repair.
- Shared Library startup repair rewrites builtin workflow template assets to the normalized shape and recomputes `payload_digest` from that shape.
- Project install/update commits workflow definitions and the activity lifecycle definition in one database transaction.
- Overwrite install preserves project resource ids and `created_at`, increments `version`, and updates installed source metadata for every workflow definition and the activity lifecycle definition together.
- A failed workflow template update must leave project resources and installed source metadata unchanged.
- Runtime active workflow projection resolves from `lifecycle_activity:{run_id}:{activity_key}#{attempt}` session bindings and Activity lifecycle definitions. Missing Activity binding means no active workflow projection.
- Frontend Marketplace workflow drawers consume only the Activity lifecycle shape; unexpected old shape means the repository normalization did not run.

### 3. Tests Required

- Domain payload normalization test for legacy workflow template lifecycle JSON.
- Infrastructure transaction test: conflict without overwrite leaves versions/source unchanged; overwrite bumps workflow and lifecycle versions.
- Frontend typecheck/workflow tests after removing old payload UI parsing.
- Browser smoke: resource market renders workflow template details, update action does not emit constraint errors, project definitions report `source_status = up_to_date`.
