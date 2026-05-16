# Story / Task 运行时建模（Story-as-durable-session）

> Story / Task / Session / LifecycleRun 的职责边界与关系拓扑。

---

## 核心定位

- **Story** 是 aggregate root，表达一条持久化的业务工作单元。每个 Story 对应一个 Story session（通过 `SessionBinding(owner_type=Story)`）
- **Task** 是 Story aggregate 下的 child entity，保存在 `stories.tasks` JSONB 列。无独立 repository、无独立表
- **LifecycleRun** 是独立 domain entity，1:1 挂在 Story session 上，记录 workflow step 运行态
- **child session** 是 Story session 的子 session（companion 对话、step 远程执行等），通过 `parent_session_id` 关联

---

## 职责边界

### Story

- 持有启动参数（title / priority / context / agent_binding 等）和业务审计字段（status）
- 持有 `Vec<Task>` 作为 aggregate 内 child entity 集合
- 所有 task 变更必须通过 Story aggregate 方法（`add_task` / `remove_task`），由 `StoryRepository::update` 原子写回
- **不持有** runtime 真相；runtime 真相在 Story session event stream + LifecycleRun/step state

### Task

- **Durable spec**：id / story_id / workspace_id / lifecycle_step_key / title / description / agent_binding
- **投影字段**（由 step state 反投射，外部不可直接写）：status / artifacts
- **禁止**新增 runtime 字段（`executor_session_id`、`execution_mode`、retry policy 等属于 session / lifecycle step 层）
- Task 通过 `lifecycle_step_key` 指向 LifecycleRun 中对应 step

### LifecycleRun

- `session_id` 指向 Story session（1:1）
- `steps: Vec<LifecycleStepState>` 只表达 step 运行态，不携带 Task id
- Step state 变化 → 投影到 Task.status / Task.artifacts
- 推进规则见 [workflow/lifecycle-edge.md](./workflow/lifecycle-edge.md)

---

## 关系拓扑

| 关系 | 基数 | 绑定方式 |
|------|------|----------|
| Story ↔ Story session | 1:1 | `SessionBinding(owner_type=Story, label="companion")` |
| Story session ↔ LifecycleRun | 1:1（活跃） | `LifecycleRun.session_id` |
| Story ↔ Task | 1:N | Story aggregate 持有 `Vec<Task>` |
| LifecycleStep ↔ Task | 0..1:1 | `Task.lifecycle_step_key` |
| Story session ↔ child session | 1:N | `parent_session_id` |

---

## 事件真相源

- Story 内一切状态变更的唯一审计源是 **Story session event stream**
- `state_changes` 表降级为跨 session 全局游标索引，由投影器自动维护，业务代码不直接写入
- Task.status / Task.artifacts 作为只读投影从 step state 反投射

---

## 对外 API 规范

- `start_task` / `continue_task` / `cancel_task` 等 facade 名字保留
- 内部统一委托 `StoryStepActivationService::activate_story_step(story_id, step_key, ...)`
- **不允许**为新场景再开 Task-specific 装配分支（`compose_task_runtime` 已删除）

---

## SessionBinding.label 值域

| owner_type | label | 语义 |
|------------|-------|------|
| `Story` | `"companion"` | Story session root 绑定 |
| `Task` | `"execution"` | Task 对应的 execution child session |
| `Project` | `"execution"` | Project session |

---

## 待演进

1. **Story.status 定位**：当前为业务审计字段（非 runtime projection），可由 runtime 给出 suggested transition
2. **Task 投影字段类型机制**：`TaskSpec + TaskView` 拆分或 setter 私有化，待实现阶段选择
3. **`state_changes` 表去留**：若引入 `session_events.global_seq`，可进一步废弃
