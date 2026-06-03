# Story / Task 运行时建模（SubjectRef + Lifecycle projection）

> Story / Task / LifecycleRun / LifecycleSubjectAssociation / RuntimeSession 的职责边界与关系拓扑。

---

## 核心定位

- **Story** 是 aggregate root，表达一条持久化的业务工作单元。Story 不绑定 `RuntimeSession`。
- **Task** 是 Story aggregate 下的 child entity，保存在 `stories.tasks` JSONB 列。无独立 repository、无独立表；Task 本体不拥有 runtime truth。
- **LifecycleRun** 是被追踪的执行生命过程 / control ledger。普通 Agent runtime 使用 `topology=graphless`，显式 Activity 工作流使用 `topology=workflow_graph` 并可以包含多个 `WorkflowGraphInstance`。
- **LifecycleSubjectAssociation** 是关联层实体，用 `(anchor_run_id, anchor_agent_id?, subject_kind, subject_id, role)` 显式表达 whole-run 或 agent-scoped subject 关系。
- **RuntimeSession** 是 runtime trace 容器：承载 event log、debug replay、agent 交互轨迹。不承载 ownership、permission scope 或 lifecycle progress truth。

---

## 职责边界

### Story

- 持有启动参数（title / priority / context / agent preference 等）和业务审计字段（status）。
- 持有 `Vec<Task>` 作为 aggregate 内 child entity 集合。
- 所有 task 变更必须通过 Story aggregate 方法（`add_task` / `remove_task`），由 `StoryRepository::update` 原子写回。
- **不持有** runtime truth；runtime truth 在 LifecycleRun / WorkflowGraphInstance / LifecycleAgent / AgentFrame / AgentAssignment / ActivityAttemptState。

### Task

- **Durable spec**：id / story_id / workspace_id / title / description / authoring preference / dispatch policy。
- **投影字段**：status / artifacts / current agent / latest attempt，由 `SubjectRef(kind=Task)`、`LifecycleSubjectAssociation`、`LifecycleAgent`、`AgentFrame` 与 lifecycle artifacts 派生；显式 Activity 工作流再通过 `AgentAssignment` 与 `ActivityAttemptState` 补充 attempt 投影，外部不可直接写为 runtime truth。
- **禁止**新增 runtime 字段：`executor_session_id`、`runtime_session_id`、`activity_key`、`attempt`、`execution_mode` 等属于 lifecycle / assignment / projection 层。
- Task execution 通过 `SubjectRef(kind=Task, id=task_id)` 进入 `ExecutionIntent`；默认使用 graphless run / `LifecycleAgent` / `AgentFrame` 控制面，显式 Activity 工作流才在同一 `LifecycleRun` 内追加或复用 `WorkflowGraphInstance`。

### LifecycleRun

- 不拥有 `RuntimeSession`；runtime session 到 run / agent / frame 的关系由 `RuntimeSessionExecutionAnchor` 索引。
- 业务归属通过 `LifecycleSubjectAssociation` 表达。
- `topology=graphless` 的运行态由 run / agent / frame / runtime session anchor 与 subject association 表达。`topology=workflow_graph` 的运行态按 `WorkflowGraphInstance` 分 namespace；activity state、claim、assignment、attempt key 必须包含 `graph_instance_id`。
- 推进规则见 [workflow/lifecycle-edge.md](./workflow/lifecycle-edge.md)。

### LifecycleSubjectAssociation

- anchor 只能是 whole run 或 `LifecycleAgent`。
- `subject_kind`：Story / Project / RoutineExecution / Task / LifecycleRun / External。
- `role`：Source / Subject / ProjectionTarget / ControlScope / Lineage。
- 一个 run 可拥有多个 association（如：Source=RoutineExecution + Subject=Story + ProjectionTarget=Task）。
- `anchor_agent_id != null` 表示某个 `LifecycleAgent` 正在处理或投影该 subject。
- Activity / ActivityAttemptState 不作为 subject anchor；执行证据来自 `AgentAssignment`、artifact 与 event。

### RuntimeSession（纯 runtime trace 容器）

- `SessionMeta` 持有 `project_id`（创建时确定，用于按项目查询 runtime trace 列表）。
- `RuntimeSession` 不通过任何 binding 表与业务实体关联。
- 业务上下文反查只能走 trace 链路：`runtime_session_id → RuntimeSessionExecutionAnchor → AgentFrame / LifecycleAgent / LifecycleRun → LifecycleSubjectAssociation`。
- capability / permission scope 由 `AgentFrame`、`PermissionGrant` 与 association 推导，不由 session owner 推导。

---

## 关系拓扑

| 关系 | 基数 | 绑定方式 |
|------|------|----------|
| Story ↔ LifecycleRun | 1:N | `LifecycleSubjectAssociation(anchor_run_id, subject_kind=Story, role=Subject)` |
| LifecycleRun ↔ WorkflowGraphInstance | 0:N | `WorkflowGraphInstance(run_id, graph_id, role)`；仅 `topology=workflow_graph` |
| LifecycleRun ↔ LifecycleAgent | 1:N | `LifecycleAgent(run_id)` |
| LifecycleAgent ↔ AgentFrame | 1:N | `AgentFrame(agent_id, revision)` |
| Story ↔ Task | 1:N | Story aggregate 持有 `Vec<Task>` |
| Task ↔ LifecycleAgent | 0..N | `LifecycleSubjectAssociation(anchor_agent_id, subject_kind=Task)` |
| ActivityAttemptState ↔ LifecycleAgent | 0..N | `AgentAssignment(graph_instance_id, activity_key, attempt, agent_id, frame_id)` |
| RoutineExecution → LifecycleRun | 1:N | `LifecycleSubjectAssociation(subject_kind=RoutineExecution, role=Source)` |
| Project ↔ RuntimeSession | 1:N | `SessionMeta.project_id` |

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

### 查找 Task 的执行视图

```text
task_id → SubjectRef(kind=Task, id=task_id)
        → lifecycle_subject_association_repo.list_by_subject(Task, task_id)
        → anchor agent / run
        → LifecycleAgent.current_frame / runtime anchors / artifacts
        → workflow_graph topology 时再进入 agent assignments / attempts
        → SubjectExecutionView.task_projection
```

### 查找 Project 下所有 RuntimeSessions

```text
project_id → SessionMeta query by project_id
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

- `start_task` / `continue_task` / `cancel_task` 等 facade 名字保留。
- 内部统一提交 `ExecutionIntent(subject_ref=SubjectRef(kind=Task, id=task_id), ...)`。
- Task / Routine execution response 返回 `AgentRuntimeRefs` envelope；run / agent / frame 是通用控制面，graph instance / assignment 只通过可选 Activity binding 暴露。
- **不允许**为新场景再开 Task-specific session 装配分支；Task runtime 进入统一 lifecycle dispatch 路径。
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

---

## Open Architecture Questions

以下问题不作为当前实现任务承诺，只作为后续 architecture review 的讨论入口：

- Agent Permission System（Request/Grant/Policy/Compiler）独立任务完成后，`CapabilityScope` 可全面替换为 Permission Grant 查询。
- WorkflowBindingKind 是否应全面替换为 launch scope / subject requirements / capability contract。
