# Lifecycle Activity/Executor 系统设计

## 1. 核心判断

Lifecycle 应被定义为工作过程实例，而不是主 session 的生命周期。

新的抽象层次：

```text
WorkflowDefinition
  单个 Agent 活动的行为契约

LifecycleDefinition
  可复用 Activity Graph 定义

LifecycleRun
  某次 Activity Graph 的运行实例，挂在 root session 上

ActivityDefinition
  图中的工作单元

ActivityExecutorSpec
  Activity 的执行方式

ActivityAttempt
  Activity 在某次 run 内的一次执行尝试

ActivityTransition
  Activity 之间的控制与数据流转规则
```

设计原则：

- Activity 是图中的唯一节点语义。
- Executor 是 Activity 的执行策略，不是节点种类。
- Session 是执行载体之一，不是 Lifecycle 本体。
- 回退 / 重试 / 审批退回通过新 attempt 表达，不重置历史状态。
- 条件分支通过 Transition condition 表达，不依赖孤立字符串 DSL。

重构后的成功状态不是“功能更多”，而是“系统解释力更强”：同一套 Activity/Executor/Attempt/Transition 模型可以同时解释 child session 执行、root session 继续执行、平台函数执行和人工审批。新增能力时优先扩展 executor，而不是继续扩展 node type。

## 2. 领域模型草案

### 2.1 LifecycleDefinition

```rust
pub struct LifecycleDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub binding_kinds: Vec<WorkflowBindingKind>,
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    pub transitions: Vec<ActivityTransition>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

持久化层可以继续使用 `steps` / `edges` 字段名作为过渡，但 domain / API / TS 类型应尽快切到 `activities` / `transitions` 的语义。

### 2.2 ActivityDefinition

```rust
pub struct ActivityDefinition {
    pub key: String,
    pub description: String,
    pub executor: ActivityExecutorSpec,
    pub input_ports: Vec<InputPortDefinition>,
    pub output_ports: Vec<OutputPortDefinition>,
    pub completion_policy: ActivityCompletionPolicy,
    pub iteration_policy: ActivityIterationPolicy,
}
```

`workflow_key`、`function` 不再作为顶层 optional 字段存在，而是进入 executor spec。

### 2.3 ExecutorSpec

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityExecutorSpec {
    Agent(AgentActivityExecutorSpec),
    Function(FunctionActivityExecutorSpec),
    Human(HumanActivityExecutorSpec),
}

pub struct AgentActivityExecutorSpec {
    pub workflow_key: String,
    pub session_policy: AgentSessionPolicy,
}

#[serde(rename_all = "snake_case")]
pub enum AgentSessionPolicy {
    SpawnChild,
    ContinueRoot,
    AttachExisting,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FunctionActivityExecutorSpec {
    ApiRequest(ApiRequestExecutorSpec),
    BashExec(BashExecExecutorSpec),
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HumanActivityExecutorSpec {
    Approval(HumanApprovalExecutorSpec),
}
```

映射关系：

| 当前模型 | 新模型 |
| --- | --- |
| `agent_node` | `executor.kind = agent`, `session_policy = spawn_child` |
| `phase_node` | `executor.kind = agent`, `session_policy = continue_root` |
| `function_node` | `executor.kind = function` |

### 2.4 CompletionPolicy

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityCompletionPolicy {
    OutputPorts {
        required_ports: Vec<String>,
    },
    ExecutorTerminal,
    HumanDecision {
        decision_port: String,
    },
    HookGate {
        hook_key: String,
    },
}
```

首版推荐：

- Agent / Function 默认使用 `OutputPorts + ExecutorTerminal` 的组合语义。
- Human Approval 默认使用 `HumanDecision`。
- HookGate 保留现有 gate 能力，但不作为所有分支的唯一表达方式。

### 2.5 IterationPolicy 与 Attempt

```rust
pub struct ActivityIterationPolicy {
    pub max_attempts: Option<u32>,
    pub artifact_alias: ArtifactAliasPolicy,
}

#[serde(rename_all = "snake_case")]
pub enum ArtifactAliasPolicy {
    Latest,
    PerAttempt,
    LatestAndHistory,
}

pub struct ActivityAttemptState {
    pub activity_key: String,
    pub attempt: u32,
    pub status: ActivityAttemptStatus,
    pub executor_run: Option<ExecutorRunRef>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
}
```

Activity 的“退回前一阶段”不是回滚状态，而是创建新的 attempt：

- `plan#1` 完成后保留历史。
- `approval#1` rejected 后触发 `plan#2`。
- 后续默认读取 `plan.proposal@latest`。
- 审计可查看 `plan.proposal#1`、`plan.proposal#2`。

### 2.6 Transition

```rust
pub struct ActivityTransition {
    pub from: String,
    pub to: String,
    pub kind: ActivityTransitionKind,
    pub condition: TransitionCondition,
    pub artifact_bindings: Vec<ArtifactBinding>,
}

#[serde(rename_all = "snake_case")]
pub enum ActivityTransitionKind {
    Flow,
    Artifact,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransitionCondition {
    Always,
    ArtifactFieldEquals {
        activity: String,
        port: String,
        path: String,
        value: serde_json::Value,
    },
    HumanDecisionEquals {
        activity: String,
        decision_port: String,
        value: String,
    },
    AgentSignalEquals {
        activity: String,
        signal_key: String,
        value: serde_json::Value,
    },
}
```

首版不引入通用表达式 DSL。条件来源必须是结构化 output artifact、human decision 或 agent signal。

### 2.7 JoinPolicy

默认 join 仍为 `All`，即一个 Activity 的所有入边依赖满足后才 Ready。

预留：

```rust
#[serde(rename_all = "snake_case")]
pub enum ActivityJoinPolicy {
    All,
    Any,
    First,
    NOfM { n: u32 },
}
```

MVP 不实现 `Any / First / NOfM`，但 schema 设计应避免阻塞未来扩展。

## 3. LifecycleRun 与 root session

`LifecycleRun.session_id` 应明确命名或注释为 root session / host session。

```text
root session
  └─ LifecycleRun
       ├─ ActivityAttempt(plan, #1)       -> child session or root turn
       ├─ ActivityAttempt(approval, #1)   -> human decision
       ├─ ActivityAttempt(plan, #2)       -> child session or root turn
       └─ ActivityAttempt(implement, #1)  -> child session / function / root turn
```

root session 的职责：

- 承载用户启动与观察 LifecycleRun 的交互。
- 提供 project / workspace / owner / permission 的上下文锚点。
- 接收 LifecycleEngine 的系统消息或摘要。
- 在 `AgentSessionPolicy::ContinueRoot` 时作为执行载体。

约束建议：

- 同一个 root session 同一时刻只能有一个 running `ContinueRoot` Activity。
- SpawnChild Agent Activity 创建 child session，并通过 executor_run ref 关联到 attempt。
- Function Activity 创建 function run / job，并通过 executor_run ref 关联到 attempt。
- Human Activity 创建 pending decision，并通过 executor_run ref 或 decision id 关联到 attempt。

## 4. 推进模型

### 4.1 ActivityEvent

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityEvent {
    ExecutorStarted {
        activity_key: String,
        attempt: u32,
        executor_run: ExecutorRunRef,
    },
    AgentSessionCompleted {
        activity_key: String,
        attempt: u32,
        session_id: String,
        summary: Option<String>,
    },
    AgentSessionFailed {
        activity_key: String,
        attempt: u32,
        session_id: String,
        error: String,
    },
    FunctionCompleted {
        activity_key: String,
        attempt: u32,
        outputs: Vec<PortOutputValue>,
        summary: Option<String>,
    },
    FunctionFailed {
        activity_key: String,
        attempt: u32,
        error: String,
    },
    HumanDecisionSubmitted {
        activity_key: String,
        attempt: u32,
        decision: serde_json::Value,
    },
}
```

所有推进入口统一提交 ActivityEvent。LifecycleEngine 负责：

1. 校验 event 是否匹配当前 running / ready attempt。
2. 写入 output artifacts。
3. 评估 completion policy。
4. 更新 attempt status。
5. 记录 execution log。
6. 评估 outgoing transitions。
7. 创建后继 ActivityAttempt。
8. 将 Ready attempt 交给 scheduler。

### 4.2 LifecycleEngine / ExecutorScheduler 边界

LifecycleEngine：

- 纯工作流状态机。
- 持有 run、definition、attempt、transition、artifact、execution log 的规则。
- 不直接发 HTTP 请求，不直接执行 Bash，不直接启动 session。

ExecutorScheduler：

- 根据 ActivityExecutorSpec 启动具体执行。
- Agent executor：SpawnChild / ContinueRoot / AttachExisting。
- Function executor：API / Bash / 后续 JSON transform 等。
- Human executor：创建 approval request / decision form。
- 启动完成后回写 `ExecutorStarted`。
- 执行结束后提交对应 ActivityEvent。

## 5. 审批退回 case

Human Approval 是本轮 MVP 的一等验证场景。它不是附属 UI 弹窗，而是正式 Human executor：会创建 pending decision，消费 proposal artifact，产出 structured decision artifact，并通过 typed transition 决定进入后续实现还是创建下一轮规划 attempt。

### 5.1 Definition

```json
{
  "entry_activity_key": "plan_draft",
  "activities": [
    {
      "key": "plan_draft",
      "executor": {
        "kind": "agent",
        "workflow_key": "workflow.plan_draft",
        "session_policy": "spawn_child"
      },
      "input_ports": [
        { "key": "feedback", "description": "上一轮审批反馈" }
      ],
      "output_ports": [
        { "key": "proposal", "description": "固定格式规划方案" }
      ],
      "iteration_policy": {
        "max_attempts": 5,
        "artifact_alias": "latest_and_history"
      }
    },
    {
      "key": "approval_gate",
      "executor": {
        "kind": "human",
        "type": "approval",
        "form_schema_key": "approval.plan_review"
      },
      "input_ports": [
        { "key": "proposal", "description": "待审批方案" }
      ],
      "output_ports": [
        { "key": "decision", "description": "结构化审批结果" }
      ],
      "iteration_policy": {
        "max_attempts": 5,
        "artifact_alias": "latest_and_history"
      }
    },
    {
      "key": "implement",
      "executor": {
        "kind": "agent",
        "workflow_key": "workflow.implement",
        "session_policy": "spawn_child"
      },
      "input_ports": [
        { "key": "approved_plan", "description": "已审批方案" }
      ],
      "output_ports": [
        { "key": "implementation_summary", "description": "实现摘要" }
      ]
    }
  ],
  "transitions": [
    {
      "from": "plan_draft",
      "to": "approval_gate",
      "condition": { "kind": "always" },
      "artifact_bindings": [
        { "from_port": "proposal", "to_port": "proposal", "alias": "latest" }
      ]
    },
    {
      "from": "approval_gate",
      "to": "implement",
      "condition": {
        "kind": "human_decision_equals",
        "activity": "approval_gate",
        "decision_port": "decision",
        "value": "approved"
      },
      "artifact_bindings": [
        { "from_activity": "plan_draft", "from_port": "proposal", "to_port": "approved_plan", "alias": "latest" }
      ]
    },
    {
      "from": "approval_gate",
      "to": "plan_draft",
      "condition": {
        "kind": "human_decision_equals",
        "activity": "approval_gate",
        "decision_port": "decision",
        "value": "rejected"
      },
      "artifact_bindings": [
        { "from_port": "decision", "to_port": "feedback", "alias": "latest" }
      ]
    }
  ]
}
```

### 5.2 Runtime

```text
plan_draft#1 Ready
plan_draft#1 Running -> child session s1
plan_draft#1 Completed -> proposal#1

approval_gate#1 Ready
approval_gate#1 Running -> pending decision d1
approval_gate#1 Completed(rejected) -> decision#1

plan_draft#2 Ready(feedback = decision#1)
plan_draft#2 Running -> child session s2
plan_draft#2 Completed -> proposal#2

approval_gate#2 Ready(proposal = proposal#2)
approval_gate#2 Completed(approved) -> decision#2

implement#1 Ready(approved_plan = proposal#2)
```

这里没有任何节点被“撤回完成状态”。历史 attempt 是审计事实，后续 attempt 是新的运行实例。

## 6. UI 心智

### Editor

- DAG canvas 展示 Activity，不展示 executor 作为 node type。
- Activity Inspector 顶层字段：
  - key / description
  - executor kind
  - input / output ports
  - completion policy
  - iteration policy
  - transitions
- Agent executor panel：
  - workflow contract
  - session policy
  - capability / injection / hook
- Function executor panel：
  - function kind
  - placement
  - timeout / mapping / redaction
- Human executor panel：
  - approval form schema
  - decision schema
  - decision output mapping

### Run View

- 左侧按 Activity 展示当前状态。
- 展开 Activity 后显示 attempts。
- 每个 attempt 显示 executor run：
  - child session
  - root session turn
  - function run
  - human decision
- Artifact 面板默认显示 latest，同时可查看历史。
- 审批退回链路显示为 attempt timeline，而不是图结构反复闪烁。

## 7. 迁移策略

项目处于预研期，建议直接演进 schema，不做旧兼容。

建议步骤：

1. Domain 新增 Activity 模型。
2. 删除或废弃 `LifecycleNodeType`。
3. 将 `LifecycleStepDefinition.workflow_key` 移入 Agent executor。
4. 将 planned `function` spec 移入 Function executor。
5. 将 `edges` 语义升级为 transitions。
6. 将 `LifecycleRun.step_states` 升级为 activity attempts / activity summaries。
7. 更新 seed / builtin lifecycle JSON。
8. 更新前端 type / store / editor / run view。

如果需要降低单次实现风险，可以内部仍读写 `steps` 字段，但 API 和领域命名先改为 Activity。

## 8. MVP 建议

首版实现范围：

- ActivityDefinition + ActivityExecutorSpec。
- Agent executor：SpawnChild、ContinueRoot。
- Function executor：保留接口，具体 API/Bash 可由 Function Node task 承接。
- Human executor：实现最小 ApprovalGate，包含 pending decision、固定 form schema、decision artifact、approved/rejected transition。
- Transition condition：Always + HumanDecisionEquals。
- Attempt：支持 latest_and_history artifact alias。
- Join：仅 All。
- UI：能创建 Plan -> Approval -> Implement，能运行 rejected -> replanning -> approved。

不进入首版：

- 通用表达式 DSL。
- Any / First / NOfM join。
- 并行 ContinueRoot。
- 自动补偿事务。
- cyclic graph 通用运行时。
- Secret 管理完整产品化。

## 9. 风险

- `ContinueRoot` 会把 root session 同时作为交互入口和执行载体，必须限制并发。
- Attempt 模型会影响 run state、artifact URI、execution log、UI 查询结构。
- Transition condition 如果过早引入 DSL，会让校验和可观测性变差。
- Function/Human/Agent executor 都需要统一事件入口，否则 engine 会再次分裂。
- Migration 会触及 domain、application、api、frontend、builtin JSON，必须分阶段落地。

## 10. 可执行化约束

本节把设计落到实现必须遵守的不变量。后续任务如果没有满足这些约束，不应进入 orchestrator / frontend 大范围改造。

### 10.1 先决工程修复

Activity 重构前必须先修正当前 lifecycle 基础链路的硬边界：

- 所有 workflow / lifecycle 按 key 解析必须带 `project_id`，禁止在运行链路使用全局 `get_by_key` 决定当前 project 的 definition。
- definition 的 create / update / delete / get / list 路由必须执行 project permission gate。
- `LifecycleRun` 持久化 schema 与 repository insert 列必须一致，避免新 run 在空库或新环境启动时依赖历史 migration 副作用。
- Function 能力不再以 `LifecycleNodeType::FunctionNode` 方式继续扩展；通用功能节点任务应改为等待 `ActivityExecutorSpec::Function` 落地。

这些修复不是 Activity 模型的一部分，但它们是后续重构的地基。若不先处理，新的 engine 仍会继承跨 project 串定义、权限面过宽、调度状态不可信的问题。

### 10.2 ActivityAttempt 状态机

首版只允许以下状态与转移：

| From | Event | To | 说明 |
| --- | --- | --- | --- |
| `pending` | dependency satisfied | `ready` | 所有 required incoming transition 满足 |
| `ready` | scheduler claim accepted | `claiming` | durable claim 已写入，尚未启动 executor |
| `claiming` | executor started | `running` | 写入 `ExecutorRunRef` |
| `claiming` | scheduler failed before start | `ready` / `failed` | 可重试错误回到 ready；不可重试错误进入 failed |
| `running` | completion event accepted | `completed` | completion policy 通过，写 artifacts / log |
| `running` | failure event accepted | `failed` | executor 明确失败或 gate 达到失败阈值 |
| `running` | cancel requested | `cancelled` | 后续是否生成补偿 attempt 由 transition 决定 |
| `completed` | outgoing transition selects same activity | `completed` + new attempt `ready` | 不重置旧 attempt |
| `failed` | retry policy creates retry | `failed` + new attempt `ready` | 失败 attempt 保留 |

禁止从 `completed` 回到 `ready`。审批退回、自动重试、人工修订都必须创建新的 attempt。

### 10.3 Run 汇总状态

`LifecycleRun.status` 是 attempts 的派生摘要，不直接驱动业务逻辑：

| Run status | 条件 |
| --- | --- |
| `ready` | 存在 ready attempt，且无 running / claiming attempt |
| `running` | 存在 running / claiming attempt |
| `blocked` | 无 ready / running / claiming attempt，且仍存在 pending attempt |
| `completed` | 所有 terminal attempts 已完成，且没有待创建的后继 |
| `failed` | 存在不可恢复 failed attempt，且没有 retry / rejection transition 可创建新 attempt |
| `cancelled` | run 被显式取消 |

Engine 判断是否可推进时读取 attempt 状态，不以 run status 作为唯一事实源。

### 10.4 Transition 与循环修订

首版 transition 采用 typed condition，不引入通用 DSL：

- `Always`
- `HumanDecisionEquals`
- `ArtifactFieldEquals`

Loop 只允许通过“有条件 transition 指回已有 activity，并创建新 attempt”表达。校验规则必须要求这类 transition 至少满足一个条件：

- 目标 activity 配置了 `max_attempts`；或
- transition 自身声明 `max_traversals`；或
- condition 来源是人工 decision / structured artifact，而不是无条件自环。

无条件自环和纯循环 DAG 不进入首版。

### 10.5 Scheduler Claim 与幂等

ExecutorScheduler 启动执行器前必须先写 durable claim。建议最小模型：

```rust
pub struct ActivityExecutionClaim {
    pub run_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
    pub claim_id: Uuid,
    pub executor_kind: String,
    pub status: ActivityExecutionClaimStatus,
    pub idempotency_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

`idempotency_key = "{run_id}:{activity_key}:{attempt}"`，同一 attempt 同时只能有一个 active claim。Scheduler 重试时先按 idempotency key 查询已有 claim：

- `claiming` 超时：可接管或标记 failed 后重新 claim。
- `running` 且有 executor_run：不得重复启动。
- `succeeded`：不得重复启动。
- `failed` 且可重试：创建新 claim attempt 或重新 claim 同 attempt，具体由 retry policy 决定。

Agent SpawnChild 启动必须把“创建 child session / 创建 binding / 写 executor_run / launch prompt accepted”纳入同一 claim 生命周期。若 prompt 未 accepted，attempt 不能停留在 running。

### 10.6 Persistence Shape

首版可以继续使用 JSONB 字段，但 domain / API 类型必须表达新语义：

```text
lifecycle_definitions
  activities_json
  transitions_json

lifecycle_runs
  root_session_id
  status
  attempts_json
  execution_log_json

activity_execution_claims
  run_id
  activity_key
  attempt
  claim_id
  idempotency_key
  executor_kind
  executor_run_ref_json
  status

inline_files
  owner_kind = lifecycle_run
  container = port_outputs
  path = {activity_key}/{attempt}/{port_key}
  latest alias path = {activity_key}/latest/{port_key}
```

如果为了降低迁移成本暂时沿用 `steps` / `edges` / `step_states` 列名，必须在同一 PR 内保证 API / TS 类型已经使用 Activity 命名，避免新调用方继续依赖旧心智。

### 10.7 Project Scope 与权限

所有新服务入口必须显式携带或解析 `project_id`：

- `LifecycleEngine::apply_event(project_id, run_id, event)`
- `ExecutorScheduler::schedule_ready(project_id, run_id)`
- `WorkflowCatalogService::{get_by_project_and_key, validate_lifecycle_definition}`

按 id 读取 definition / run 后必须校验其 `project_id` 与调用上下文一致。前端只传 key 时，后端必须以当前 project scope 解析。

### 10.8 最小可交付路径

可执行落地不以“大重构一次完成”为目标，而以以下检查点推进：

1. 当前 lifecycle 基础边界修复完成。
2. Activity domain schema 和 validation 可单独运行。
3. LifecycleEngine 作为纯状态机通过 approval-replan 单测。
4. Scheduler claim 能防止同一 attempt 重复启动。
5. Agent SpawnChild 映射旧 AgentNode 流程。
6. ContinueRoot 映射旧 PhaseNode 的 capability transition，且限制并发。
7. Human Approval MVP 跑通 rejected -> replan -> approved。
8. 前端能编辑并观察 attempts。

只有第 1-4 项完成后，才进入 executor 和 UI 大规模替换。
