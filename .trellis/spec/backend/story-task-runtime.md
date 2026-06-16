# Story / Task 运行时建模（SubjectRef + SubjectContext + Lifecycle projection）

> Story / Task / SubjectContextAssignment / LifecycleRun / LifecycleSubjectAssociation / RuntimeSession 的职责边界与关系拓扑。

---

## 核心定位

- **Story** 是 Project 下的 subject / context aggregate，表达一条持久化的业务工作单元。Story 不绑定 `RuntimeSession`，也不拥有 Task domain facts。
- **Task** 是 `LifecycleRun` aggregate 内的计划项事实，保存在 `LifecycleRun.tasks` 结构化字段。Task 不做 Project/global 任务池，不拥有 runtime truth。
- **LifecycleRun** 是被追踪的执行生命过程 / control ledger。普通 Agent runtime 使用 `topology=graphless`；显式 workflow runtime 使用 `topology=workflow_graph`，并通过 `LifecycleRun.orchestrations[]` 承载 0..N 个内部编排实例。
- **LifecycleSubjectAssociation** 是关联层实体，用 `(anchor_run_id, anchor_agent_id?, subject_kind, subject_id, role)` 显式表达 whole-run 或 agent-scoped subject 关系。
- **SubjectContextAssignment** 是 `SubjectRef` 到 AgentFrame context / capability / VFS surface 的应用层解析结果。Story / Task 通过该模型作为 ProjectAgent 的 subject profile 注入上下文，不拥有独立 Agent owner。
- **RuntimeSession** 是 runtime trace 容器：承载 event log、debug replay、agent 交互轨迹。不承载 ownership、permission scope 或 lifecycle progress truth。

---

## 职责边界

### Story

- 持有启动参数（title / priority / context / agent preference 等）和业务审计字段（status）。
- Story Task 视图是 projection，由 Story-bound LifecycleRun、linked run 和可选 `story_ref` 推导。
- Story 状态通过明确 Story command 推进；LifecycleRun 的 terminal / failed / cancelled 事实只作为 UI 提示或 projection 输入。
- Story 作为 subject profile 被 ProjectAgent session 消费；快速创建会话入口复用 ProjectAgent session start 并携带 `subject_ref=story`。
- Runtime truth 在 `LifecycleRun` / `OrchestrationInstance` / `RuntimeNodeState` / `LifecycleAgent` / `AgentFrame` / `RuntimeSessionExecutionAnchor`。

### Task

- **Durable spec**：`id` / `title` / `body` / `status` / `priority` / `created_by_agent_id` / `owner_agent_id` / `assigned_agent_id` / `source_task_id` / `created_at` / `updated_at` / `archived_at` / optional `context_refs` / optional `story_ref`。
- **计划状态**：`open / active / review / blocked / done / dropped`。这些状态表达计划进度，不表达 runtime running / failed / cancelled。
- **执行投影**：current agent / latest runtime node / artifacts / linked runs 由 `SubjectRef(kind=Task)`、`LifecycleSubjectAssociation`、`LifecycleAgent`、`AgentFrame`、`RuntimeSessionExecutionAnchor`、`RuntimeNodeState` 与 lifecycle artifacts 派生；Task facts 不保存 execution status 或 artifacts。
- Task context 通过 `SubjectContextAssignment(subject_ref=Task)` 注入 ProjectAgent frame：Task value object、owning LifecycleRun、Project、effective Workspace、Task `context_refs`，以及 Story-bound run 或显式 `story_ref` 带来的 Story context 在 application 层一次解析成 `Contribution`。
- 执行器选择进入 assignment / launch hint / AgentRun launch command，不进入 Task facts。Task command 控制计划项事实，执行控制走统一 AgentRun / Lifecycle 控制面。

### SubjectContextAssignment

- 输入为 `project_id + SubjectRef(project|story|task)`，输出 `workspace`、`Vec<Contribution>` 和 `CapabilityScopeCtx`。
- Project subject 使用 ProjectAgent owner context 与 Project workspace 默认值。
- Story subject 解析 Story、Project、Story/default Project workspace 和 Story declared sources。
- Task subject 解析 owning LifecycleRun 内的 Task value object、effective Workspace、Task context refs，以及 Story-bound run / optional `story_ref` 带来的 Story context。
- Assignment 只构建 AgentFrame surface 所需画像；runtime session、LifecycleRun、LifecycleAgent 与 subject association 仍由 lifecycle dispatch / ProjectAgent session start 创建。

### LifecycleRun

- 不拥有 `RuntimeSession`；runtime session 到 run / agent / frame 的关系由 `RuntimeSessionExecutionAnchor` 索引。
- 业务归属通过 `LifecycleSubjectAssociation` 表达。
- 拥有 `tasks` 计划项事实集合；repository create / update / select 需要对该集合做整体 roundtrip。
- `topology=graphless` 的运行态由 run / agent / frame / runtime session anchor 与 subject association 表达。`topology=workflow_graph` 的运行态按 `OrchestrationInstance.orchestration_id` 分 namespace；runtime node key 必须包含 `orchestration_id + node_path + attempt`。
- 推进规则见 [workflow/lifecycle-edge.md](./workflow/lifecycle-edge.md)。

### LifecycleSubjectAssociation

- anchor 只能是 whole run 或 `LifecycleAgent`。
- `subject_kind`：Story / Project / RoutineExecution / Task / LifecycleRun / External。
- `role`：Source / Subject / ProjectionTarget / ControlScope / Lineage。
- 一个 run 可拥有多个 association（如：Source=RoutineExecution + Subject=Story + ProjectionTarget=Task）。
- `anchor_agent_id != null` 表示某个 `LifecycleAgent` 正在处理或投影该 subject。
- Runtime node 不作为 subject anchor；执行证据来自 `RuntimeSessionExecutionAnchor`、orchestration journal、artifact 与 event。

### RuntimeSession（纯 runtime trace 容器）

- `SessionMeta` 只持有 runtime trace shell：title projection、event sequence head、delivery status 和 last turn 指针。
- `RuntimeSession` 不通过任何 binding 表与业务实体关联。
- 业务上下文反查只能走 trace 链路：`runtime_session_id → RuntimeSessionExecutionAnchor → AgentFrame / LifecycleAgent / LifecycleRun → LifecycleSubjectAssociation`。
- capability / permission scope 由 `AgentFrame`、`PermissionGrant` 与 association 推导，不由 session owner 推导。

---

## 关系拓扑

| 关系 | 基数 | 绑定方式 |
|------|------|----------|
| Story ↔ LifecycleRun | 1:N | `LifecycleSubjectAssociation(anchor_run_id, subject_kind=Story, role=Subject)` |
| LifecycleRun ↔ OrchestrationInstance | 0:N | `LifecycleRun.orchestrations[]`；仅显式 workflow / script / append orchestration runtime |
| LifecycleRun ↔ Task | 1:N | `LifecycleRun.tasks[]` aggregate field |
| LifecycleRun ↔ LifecycleAgent | 1:N | `LifecycleAgent(run_id)` |
| LifecycleAgent ↔ AgentFrame | 1:N | `AgentFrame(agent_id, revision)` |
| Story → Task projection | 0:N | Story-bound LifecycleRun / linked run / optional `story_ref` |
| Task ↔ LifecycleAgent | 0..N | `LifecycleSubjectAssociation(anchor_agent_id, subject_kind=Task)` |
| RuntimeNodeState ↔ LifecycleAgent | 0..N | `RuntimeSessionExecutionAnchor(orchestration_id, node_path, attempt, agent_id, frame_id)` 与 frame/current-agent refs |
| RoutineExecution → LifecycleRun | 1:N | `LifecycleSubjectAssociation(subject_kind=RoutineExecution, role=Source)` |
| Project ↔ RuntimeSession | 1:N | `RuntimeSessionExecutionAnchor.run_id → LifecycleRun.project_id` read model |

---

## 查询路径

### 查找 Story 的所有 Runs（业务查询）

```text
story_id → lifecycle_subject_association_repo.list_by_subject(Story, story_id)
         → run_ids → lifecycle_run_repo.list_by_ids(run_ids)
```

API 端点：`GET /stories/{story_id}/runs`。

### 查找 Story 的活跃 Run

```text
story_id → list_by_subject_and_role(Story, story_id, Subject)
         → filter(status == Running || status == Ready)
```

API 端点：`GET /stories/{story_id}/runs/active`。

### 查找 Story 的 Task projection

```text
story_id → lifecycle_subject_association_repo.list_by_subject(Story, story_id)
         → Story-bound run ids
         → lifecycle_run_repo.list_by_ids(run_ids)
         → flatten LifecycleRun.tasks
         → include linked run tasks and explicit story_ref matches
         → StoryTaskProjection
```

Story projection 只解释 Task 为什么对该 Story 可见，不表达 Story 对 Task 的所有权。

### 查找 AgentRun workspace 的 Task plan

```text
run_id → lifecycle_run_repo.get(run_id)
       → LifecycleRun.tasks
       → filter by created_by_agent_id / owner_agent_id / assigned_agent_id / archived_at
       → RunScopedTaskPlanView
```

Task 创建、更新、归档和 assignment 的第一入口是 AgentRun workspace / run-scoped command。

### 查找 Task 的执行视图

```text
task_id → SubjectRef(kind=Task, id=task_id)
        → lifecycle_subject_association_repo.list_by_subject(Task, task_id)
        → anchor agent / run
        → LifecycleAgent.current_frame
        → artifacts
        → workflow_graph topology 时进入 LifecycleRun.orchestrations[] / RuntimeNodeState
        → SubjectExecutionView.task_projection
```

### 查找 Project 下所有 AgentRun Workspaces

```text
project_id → LifecycleRun(project_id)
           → LifecycleAgent / AgentFrame
           → AgentRunWorkspaceView(shell, conversation.commands, delivery_trace_meta?)
```

`conversation.commands` 使用 AgentRun workspace DTO，原因是 Project/Story/Task 页面打开的是
可继续交互的 AgentRun 工作台；`delivery_trace_meta` 只提供 RuntimeSession trace/detail 下钻。

### 查找 Project 下所有 RuntimeSession traces

```text
project_id → RuntimeSessionExecutionAnchor
           → LifecycleRun(project_id)
           → SessionMeta by runtime_session_id
```

### 查找 RuntimeSession 的业务上下文（trace 反查）

```text
runtime_session_id → RuntimeSessionExecutionAnchor
                   → launch_frame_id / agent_id / run_id
                   → lifecycle_subject_association_repo.list_by_anchor(run/agent)
                   → derive trace projection
```

---

## 对外 API 规范

- ProjectAgent session start 接收可选 `subject_ref`。省略时为 Project context；传入 Story/Task 时由 SubjectContextAssignment 动态补齐 subject context。
- Story 快速创建会话是 ProjectAgent session start 的薄入口：选择 ProjectAgent 后携带 `subject_ref=story`，返回同一套 run / agent / frame / runtime session refs。
- Task plan API 面向 Run / AgentRun workspace scope：list / create / update / archive Task 都以 LifecycleRun 或 AgentRun workspace 为作用域。
- Agent-facing Task 工具面使用 runtime tools：`task_read` 负责 overview/list/detail/context/execution/projection，`task_write` 负责 create/update/status/reorder/drop/context refs。
- Story Task projection API 返回由 Story-bound run / linked run / optional `story_ref` 推导出的 projection DTO。
- Task 执行面向 read projection：subject-oriented API 返回 `SubjectExecutionView`，包含 association、current agent、latest runtime node 和 artifacts。执行视图统一使用 `/subjects/task/{id}/execution`；Task plan DTO 不返回 runtime status 或 artifacts。
- Subject / agent / run-oriented API 是 Story / Task 业务查询的主路径；session route 只提供 RuntimeTrace。

---

## CapabilityScope 与能力可见性

- `CapabilityScope` enum（Project / Story / Task）不从 session owner 推导，而从 subject association / agent frame / permission grant 推导。
- `CapabilityVisibilityRule.allowed_scopes` 定义每个 well-known capability 的硬边界。
- `CapabilityScope` 推导顺序：
  - agent-level Task association → Task scope。
  - run-level Story association → Story scope。
  - run/agent Project ControlScope association → Project scope。
- 后续 Agent Permission System 将全面接管，替换当前的静态规则。

## Scenario: LifecycleRun Task Plan Facts Contract

### 1. Scope / Trigger

- Trigger: 新增或修改 Task plan facts、Run-scoped Task API、Story Task projection、Task status、Task assignment 或 Task execution projection。
- Scope: `LifecycleRun` aggregate、PostgreSQL migration / repository roundtrip、Rust contract DTO、generated TypeScript、Story projection、AgentRun workspace Task UI、runtime Task tools。

### 2. Signatures

Domain aggregate:

```rust
pub struct LifecycleRun {
    pub tasks: Vec<LifecycleTaskPlanItem>,
    // other lifecycle fields omitted
}

pub struct LifecycleTaskPlanItem {
    pub id: TaskId,
    pub title: String,
    pub body: Option<String>,
    pub status: TaskPlanStatus,
    pub priority: Option<TaskPriority>,
    pub created_by_agent_id: Option<LifecycleAgentId>,
    pub owner_agent_id: Option<LifecycleAgentId>,
    pub assigned_agent_id: Option<LifecycleAgentId>,
    pub source_task_id: Option<TaskId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub context_refs: Vec<ContextRef>,
    pub story_ref: Option<SubjectRef>,
}

pub enum TaskPlanStatus {
    Open,
    Active,
    Review,
    Blocked,
    Done,
    Dropped,
}
```

Database column:

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS tasks text DEFAULT '[]'::text NOT NULL;
```

Command shape:

```text
POST /lifecycle-runs/{run_id}/tasks
PATCH /lifecycle-runs/{run_id}/tasks/{task_id}
POST /lifecycle-runs/{run_id}/tasks/{task_id}/archive
GET /lifecycle-runs/{run_id}/tasks
GET /stories/{story_id}/task-projection
GET /subjects/task/{task_id}/execution
runtime tool: task_read
runtime tool: task_write
```

### 3. Contracts

- `LifecycleRun.tasks` is the durable Task plan facts source.
- Task id is unique inside the owning LifecycleRun; API responses that cross run boundaries include the owning run ref.
- Task status values are only `open / active / review / blocked / done / dropped`.
- `story_ref` is a projection hint for cross-run Story visibility; it does not create ownership.
- `assigned_agent_id` is a plan-layer assignment hint. Runtime evidence comes from subject association, Agent lineage and runtime anchors.
- Task plan DTOs do not include `dispatch_preference`, execution status or artifacts.
- `SubjectExecutionView` remains the runtime projection contract for linked runs, latest runtime node and artifacts.
- Story page reads Task projection; AgentRun workspace owns Task create / update / archive / assignment commands.
- Agent runtime uses `task_read` and `task_write` as the two Task tools. Status updates, reorder, drop and context refs are write operations, not separate tools.
- `companion_request(target=sub, payload.task_id=...)` can attach Task context to the child prompt and write `assigned_agent_id` after companion launch.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `lifecycle_runs.tasks` is missing in migrated schema | readiness / repository integration fails |
| `lifecycle_runs.tasks` contains invalid JSON | repository returns `DomainError` with `lifecycle_runs.tasks` context |
| Task status is not one of the six plan states | request validation fails |
| Task update addresses an id not present in the owning run | command returns NotFound |
| Story projection sees a Task from an unrelated run without `story_ref` | projection excludes it |
| Task artifact is submitted through a Task facts command | route through Lifecycle / SubjectExecution artifact path |
| `task_write` receives a `run_id` outside current project | tool execution failed |
| `task_write` receives unknown context ref enum | invalid arguments |
| Contract generation leaves old Task status or artifact fields in generated TS | `pnpm run contracts:check` fails |

### 5. Good/Base/Bad Cases

- Good: AgentRun workspace creates `open` Task in its owning LifecycleRun; Story-bound projection shows it because the run has `SubjectRef(kind=story)`.
- Good: Assigned subagent writes runtime artifacts; Task remains a plan item, and artifacts appear through `SubjectExecutionView`.
- Base: Task without `story_ref` appears only in its owning run workspace unless the run is Story-bound.
- Bad: Story detail page writes Task plan facts without a LifecycleRun scope.
- Bad: Task DTO exposes `running`, `failed`, `cancelled`, `dispatch_preference` or `artifacts` as Task facts.

### 6. Tests Required

- Migration guard and clean DB initialization cover `lifecycle_runs.tasks`.
- LifecycleRun repository roundtrip covers default tasks, update, archive and invalid JSON error context.
- LifecycleRun aggregate tests cover Task create / update / archive / status transition.
- Story projection tests cover Story-bound run visibility, linked run visibility and unrelated run exclusion.
- SubjectExecutionView tests cover Task subject association -> latest runtime node / artifacts.
- Contract check asserts generated TypeScript contains only the plan status enum and omits Task artifact / dispatch fields.
- Runtime tool tests assert `task_read` modes and `task_write` patch/snapshot operations write `LifecycleRun.tasks`.
- Companion tests assert `payload.task_id` adds Task context and writes `assigned_agent_id`.

### 7. Wrong vs Correct

#### Wrong

```text
Business page command -> mutate Task facts outside LifecycleRun -> Task DTO carries runtime artifacts
```

#### Correct

```text
AgentRun workspace command -> mutate LifecycleRun.tasks
Task subject association -> SubjectExecutionView -> linked runs / runtime artifacts
Story page -> Story Task projection
```

---

## Open Architecture Questions

以下问题不作为当前实现任务承诺，只作为后续 architecture review 的讨论入口：

- Agent Permission System（Request/Grant/Policy/Compiler）独立任务完成后，`CapabilityScope` 可全面替换为 Permission Grant 查询。
- WorkflowBindingKind 是否应全面替换为 launch scope / subject requirements / capability contract。
