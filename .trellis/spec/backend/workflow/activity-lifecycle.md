# Activity Lifecycle Backend Contract

Activity lifecycle 是 workflow 运行的唯一模型。旧 Step 概念已完全清除（P3b + P4 收口）。运行核心是 `ActivityLifecycleRunState`。Scheduler 负责 durable claim 与 executor 启动，executor 只能通过事件把结果交还给 `LifecycleEngine`。模块不变量见 [Workflow Architecture](./architecture.md)。

## Core Runtime Contract

- `/lifecycle-runs` resolves `ActivityLifecycleDefinition` by project-scoped id/key。
- run 初始化通过 `LifecycleEngine::initialize`。
- 持久化使用 `LifecycleRun::new_activity`。
- 启动 ready attempts 由 `ActivityLifecycleRunService::launch_ready_attempts` 负责。
- Agent session terminal callbacks 只解析 Activity session bindings。
- Successor activation 委托给 Activity scheduler。
- Hook evaluation 可以报告 completion metadata，但 durable state advancement 仍由 ActivityEvent application 拥有。
- Task 启动/续跑统一经 Activity `activate_story_step` 路径（P1 解耦）。
- 投影层从 `step_states` 切换到 `activity_state.attempts`（P2 投影层改造）。

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

Agent executor 的 output port 内容是 lifecycle artifact 值，必须写入 JSON 内容。`complete_lifecycle_node` 推进完成态时只读取 activity 已声明的 output ports，并把每个 port 的文件内容解析为 `serde_json::Value`；解析失败表示 artifact contract 无效，activity 不进入 completed。这样后继 artifact binding、gate evaluation 与 workflow projection 消费的是结构化值，而不是由 orchestrator 猜测的自由文本。

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

---

## Scenario: Drop-Step Migration（P1-P4 收口）

### 1. Scope / Trigger

- 旧 Step lifecycle 模型全面替换为 Activity lifecycle 唯一模型
- 删除 `lifecycle_definitions` 表的 `entry_step_key`、`steps`、`edges` 列
- 删除 `lifecycle_runs` 表的 `step_states` 列
- Task 启动/续跑解耦到 Activity 唯一信道
- 投影层改用 `activity_state.attempts` 代替 `step_states`

### 2. Signatures

#### Database Schema Changes

```sql
-- migrations/0068_drop_step_lifecycle_columns.sql
ALTER TABLE lifecycle_definitions DROP COLUMN IF EXISTS entry_step_key;
ALTER TABLE lifecycle_definitions DROP COLUMN IF EXISTS steps;
ALTER TABLE lifecycle_definitions DROP COLUMN IF EXISTS edges;
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS step_states;
```

#### Removed Domain Types (P3b)

原位于 `agentdash-domain::workflow` 的以下类型已删除：
- `LifecycleStep` / `LifecycleStepState` / `LifecycleStepDefinition`
- `StepActivationService` 的直接 step 启动路径
- `workflow/validation.rs` 中的 step 拓扑校验

#### 保留的 Activity-Only Runtime 类型

```rust
// crates/agentdash-domain/src/workflow/value_objects/lifecycle_def.rs
pub struct ActivityLifecycleDefinition {
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    pub transitions: Vec<ActivityTransition>,
}
```

### 3. Contracts

| Before (Step Model) | After (Activity-Only Model) |
|---------------------|-----------------------------|
| `lifecycle_definitions.entry_step_key` | 删除（使用 `entry_activity_key`） |
| `lifecycle_definitions.steps` (JSONB) | 删除 |
| `lifecycle_definitions.edges` (JSONB) | 删除（使用 `transitions`） |
| `lifecycle_runs.step_states` (JSONB) | 删除（使用 `activity_state`） |
| `StepActivationService::activate_step` | `StoryStepActivationService::activate_story_step` |
| Task.lifecycle_step_key → step lookup | Task.lifecycle_step_key → activity key（语义不变，底层模型变了） |

#### 投影层契约变更 (P2)

```rust
// Before: view_projector 从 step_states 投影 task 状态
let step_state = run.step_states.get(task.lifecycle_step_key);

// After: 从 activity_state.attempts 投影
let activity = run.activity_state.activities.get(task.lifecycle_step_key);
let latest_attempt = activity.and_then(|a| a.attempts.last());
```

### 4. Validation & Error Matrix

| Condition | Result |
|-----------|--------|
| 旧 DB 含 step columns | migration 0068 DROP IF EXISTS — 幂等 |
| 应用层访问已删除列 | 不会发生 — P3b 已清除所有 step 引用 |
| Task.lifecycle_step_key 指向不存在的 activity | activity scheduler 返回 "activity not found" error |

### 5. Good / Base / Bad Cases

#### Good: Task 启动走 Activity 唯一路径

```
StoryStepActivationService::activate_story_step(story_id, step_key)
→ resolve ActivityLifecycleDefinition
→ find activity by step_key
→ LifecycleEngine::advance (创建 attempt)
→ ActivityExecutorLauncher::start
```

#### Base: 投影层使用 Activity attempts

```
ViewProjector::project_task_status(run, task)
→ run.activity_state.activities[task.lifecycle_step_key].attempts.last()
→ map attempt.status → TaskViewStatus
```

#### Bad: 试图访问已删除的 step_states

```rust
// BAD — 编译不通过，类型已删除
let states: &StepStates = &run.step_states;
```

### 6. Tests Required

| Level | Target | Assertion |
|-------|--------|-----------|
| Unit | `ActivityLifecycleDefinition` serde roundtrip | 无 step 相关字段 |
| Integration | Migration 0068 on existing DB with step columns | Columns removed, no data loss in activity columns |
| Integration | `StoryStepActivationService::activate_story_step` | Resolves activity, creates attempt |
| Integration | `ViewProjector` with activity_state | Projects task status correctly |

### 7. Wrong vs Correct

#### Wrong: Reading step_states for task projection

```rust
// WRONG — step_states column deleted, type removed
let task_status = run.step_states
    .get(&task.lifecycle_step_key)
    .map(|s| s.status);
```

#### Correct: Reading from activity_state.attempts

```rust
// CORRECT — Activity is the sole lifecycle model
let task_status = run.activity_state.activities
    .get(&task.lifecycle_step_key)
    .and_then(|a| a.attempts.last())
    .map(|attempt| attempt.status);
```
