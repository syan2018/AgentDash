# Story / Task 运行时建模（Story-as-thin-business-scope）

> Story / Task / LifecycleRun / LifecycleRunLink / Session 的职责边界与关系拓扑。

---

## 核心定位

- **Story** 是 aggregate root，表达一条持久化的业务工作单元。Story 本身不再 1:1 绑定 Session。
- **Task** 是 Story aggregate 下的 child entity，保存在 `stories.tasks` JSONB 列。无独立 repository、无独立表。
- **LifecycleRun** 是独立 domain entity，通过 `LifecycleRunLink` 与 Story/Task/RoutineExecution 等业务对象显式关联。
- **LifecycleRunLink** 是关联层实体，用 `(run_id, subject_kind, subject_id, role)` 四元组显式表达 Run 与业务对象的关系。
- **Session** 降级为 runtime substrate：承载 event log、debug replay、agent 交互轨迹。不再作为 Story 业务查询入口。

---

## 职责边界

### Story

- 持有启动参数（title / priority / context / agent_binding 等）和业务审计字段（status）
- 持有 `Vec<Task>` 作为 aggregate 内 child entity 集合
- 所有 task 变更必须通过 Story aggregate 方法（`add_task` / `remove_task`），由 `StoryRepository::update` 原子写回
- **不持有** runtime 真相；runtime 真相在 LifecycleRun 和 LifecycleRunLink

### Task

- **Durable spec**：id / story_id / workspace_id / lifecycle_step_key / title / description / agent_binding
- **投影字段**（由 step state 反投射，外部不可直接写）：status / artifacts
- **禁止**新增 runtime 字段（`executor_session_id`、`execution_mode`、retry policy 等属于 session / lifecycle step 层）
- Task 通过 `lifecycle_step_key` 指向 LifecycleRun 中对应 step

### LifecycleRun

- `session_id: Option<String>` — runtime session association（可选，仅表示当前活跃的 agent session）
- 业务归属通过 `LifecycleRunLink` 表达，不再由 `session_id` 推断
- Activity step 运行态：`activity_state` + `execution_log`
- 推进规则见 [workflow/lifecycle-edge.md](./workflow/lifecycle-edge.md)

### LifecycleRunLink

- `subject_kind`：Story / Project / RoutineExecution / Task / LifecycleRun / External
- `role`：Source / Subject / ProjectionTarget / ControlScope / SpawnedBy
- 一个 Run 可拥有多个 Link（如：Source=RoutineExecution + Subject=Story + ProjectionTarget=Task）

### Session（runtime substrate）

- SessionBinding 仍然存在，用于 runtime 关联和 debug
- `SessionBinding(Story, "companion")` 保留为 runtime trace，但不再是 Story→Run 的查询入口
- `SessionBinding(Task, "execution")` 保留为 Task 执行的 runtime session 关联

---

## 关系拓扑

| 关系 | 基数 | 绑定方式 |
|------|------|----------|
| Story ↔ LifecycleRun | 1:N | `LifecycleRunLink(subject_kind=Story, role=Subject)` |
| LifecycleRun ↔ Session | 0..1:1（runtime） | `LifecycleRun.session_id`（optional） |
| Story ↔ Task | 1:N | Story aggregate 持有 `Vec<Task>` |
| LifecycleStep ↔ Task | 0..1:1 | `Task.lifecycle_step_key` |
| RoutineExecution → LifecycleRun | 1:N | `LifecycleRunLink(subject_kind=RoutineExecution, role=Source)` |

---

## 查询路径

### 查找 Story 的所有 Runs（业务查询）

```
story_id → lifecycle_run_link_repo.list_by_subject(Story, story_id)
         → run_ids → lifecycle_run_repo.list_by_ids(run_ids)
```

API 端点：`GET /stories/{story_id}/runs`

### 查找 Story 的活跃 Run

```
story_id → list_by_subject_and_role(Story, story_id, Subject)
         → filter(status == Running || status == Ready)
```

API 端点：`GET /stories/{story_id}/runs/active`

### Task 执行 Session（runtime 查询）

```
task_id → SessionBinding(owner_type=Task, label="execution") → session_id
```

API 端点：`GET /tasks/{task_id}/session`（保留）

---

## 对外 API 规范

- `start_task` / `continue_task` / `cancel_task` 等 facade 名字保留
- 内部统一委托 `StoryStepActivationService::activate_story_step(story_id, step_key, ...)`
- **不允许**为新场景再开 Task-specific 装配分支；Task runtime 进入统一 Story step activation 路径
- Run-oriented API（`/stories/{id}/runs`、`/lifecycle-runs/{id}/links`）是 Story 业务查询的主路径
- Session API（`/stories/{id}/sessions`）保留为 runtime/debug 用途

---

## SessionBinding.label 值域

| owner_type | label | 语义 |
|------------|-------|------|
| `Story` | `"companion"` | Story runtime session（debug/trace） |
| `Task` | `"execution"` | Task 对应的 execution child session |
| `Project` | `"execution"` | Project session |

---

## CapabilityResolver 过渡

- `CapabilityResolverInput` 包含 `capability_context: Option<CapabilityContext>`
- `CapabilityContext` 携带 `run_links` 信息，允许根据 Run 的业务关联做能力可见性决策
- 当前 `SessionOwnerCtx` 保留为兼容路径，后续由 Agent Permission System 全面接管

---

## Open Architecture Questions

以下问题不作为当前实现任务承诺，只作为后续 architecture review 的讨论入口：

- Agent Permission System（Request/Grant/Policy/Compiler）独立任务完成后，`SessionOwnerCtx` 可全面替换为 Permission Grant 查询。
- WorkflowBindingKind 是否应全面替换为 launch scope / subject requirements / capability contract。
- `LifecycleRun.session_id` 最终目标是重命名为 `runtime_session_id` 以明确语义。
