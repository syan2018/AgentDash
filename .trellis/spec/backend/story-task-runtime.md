# Story / Task 运行时建模（Story-as-thin-business-scope）

> Story / Task / LifecycleRun / LifecycleRunLink / Session 的职责边界与关系拓扑。

---

## 核心定位

- **Story** 是 aggregate root，表达一条持久化的业务工作单元。Story 不绑定 Session。
- **Task** 是 Story aggregate 下的 child entity，保存在 `stories.tasks` JSONB 列。无独立 repository、无独立表。
- **LifecycleRun** 是独立 domain entity，通过 `LifecycleRunLink` 与 Story/Task/RoutineExecution 等业务对象显式关联。
- **LifecycleRunLink** 是关联层实体，用 `(run_id, subject_kind, subject_id, role)` 四元组显式表达 Run 与业务对象的关系。
- **Session** 是纯 runtime 容器：承载 event log、debug replay、agent 交互轨迹。不承载 ownership 或 business 归属语义。

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

### Session（纯 runtime 容器）

- `SessionMeta` 持有 `project_id`（创建时确定，用于按项目查询 session 列表）
- Session 不再通过任何 binding 表与业务实体关联
- 业务上下文由 `LifecycleRun.session_id` 反查获得：`session_id → run → links → subjects`
- `CapabilityScope`（Project/Story/Task）用于 session 级能力可见性判断

---

## 关系拓扑

| 关系 | 基数 | 绑定方式 |
|------|------|----------|
| Story ↔ LifecycleRun | 1:N | `LifecycleRunLink(subject_kind=Story, role=Subject)` |
| LifecycleRun ↔ Session | 0..1:1（runtime） | `LifecycleRun.session_id`（optional） |
| Story ↔ Task | 1:N | Story aggregate 持有 `Vec<Task>` |
| LifecycleStep ↔ Task | 0..1:1 | `Task.lifecycle_step_key` |
| RoutineExecution → LifecycleRun | 1:N | `LifecycleRunLink(subject_kind=RoutineExecution, role=Source)` |
| Project ↔ Session | 1:N | `SessionMeta.project_id` |

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

### 查找 Task 的执行 Session

```
task_id → lifecycle_run_link_repo.list_by_subject(Task, task_id)
        → runs → filter(active) → run.session_id
```

### 查找 Project 下所有 Sessions

```
project_id → SessionMeta query by project_id
```

### 查找 Session 的业务上下文（SessionRunContext）

```
session_id → lifecycle_run_repo.find_by_session(session_id)
           → run → lifecycle_run_link_repo.list_by_run(run_id)
           → links → derive { project_id, story_id, task_id, scope }
```

---

## 对外 API 规范

- `start_task` / `continue_task` / `cancel_task` 等 facade 名字保留
- 内部统一委托 `StoryStepActivationService::activate_story_step(story_id, step_key, ...)`
- **不允许**为新场景再开 Task-specific 装配分支；Task runtime 进入统一 Story step activation 路径
- Run-oriented API（`/stories/{id}/runs`、`/lifecycle-runs/{id}/links`）是 Story 业务查询的主路径

---

## CapabilityScope 与能力可见性

- `CapabilityScope` enum（Project / Story / Task）替代了原 `SessionOwnerType`
- `CapabilityVisibilityRule.allowed_scopes` 定义每个 well-known capability 的硬边界
- `CapabilityScope` 由 session 的 `LifecycleRunLink` 推导：
  - 有 Task link → Task scope
  - 有 Story link（无 Task） → Story scope
  - 仅 Project link → Project scope
- **Agent Permission System 已实现**：active grants 优先覆盖静态规则（`CapabilityContext.granted_capability_keys`）

---

## Agent Permission System

`agentdash-domain::permission` + `agentdash-application::permission` 构成完整链路。

### 核心模型

- **PermissionGrant** — 聚合根，10 状态机（Created → PendingPolicy → Approved/PendingUserApproval → Applied → ScopeEscalated/Expired/Revoked）
- **PermissionPolicyService** — Agent role (`auto_grantable_capabilities`) ∩ Lifecycle contract (`requestable_capabilities`) → auto/user/reject
- **PermissionGrantCompiler** — grant.requested_paths → `RuntimeCapabilityTransition` (Add directives)
- **ScopeEscalationCoordinator** — post-action hook: create LifecycleRunLink(ControlScope) + unlock secondary paths

### 数据流

```
companion_request(capability_grant_request)
  → PermissionGrantService.request()
    → PolicyService.evaluate()
    → [auto_approved] → Compiler.compile() → apply RuntimeCapabilityTransition
    → [needs_user]   → persist PendingUserApproval → wait UI approve
    → [rejected]     → return error
```

### Scope Escalation

Grant 携带 `scope_escalation_intent`（target_subject_kind + unlocked_paths）。
Agent 执行 scope-creating action 后 → `ScopeEscalationCoordinator.try_escalate()`:
1. 查找匹配的 active escalation grant
2. 创建 LifecycleRunLink(ControlScope, new_subject)
3. 标记 grant → ScopeEscalated
4. 编译 unlocked_paths → secondary RuntimeCapabilityTransition

### API

- `GET /permission-grants?session_id=&run_id=`
- `GET /permission-grants/:id`
- `POST /permission-grants/:id/approve`
- `POST /permission-grants/:id/reject`
- `POST /permission-grants/:id/revoke`

---

## Open Architecture Questions

以下问题不作为当前实现任务承诺，只作为后续 architecture review 的讨论入口：

- WorkflowBindingKind 是否应全面替换为 launch scope / subject requirements / capability contract。
- `LifecycleRun.session_id` 最终目标是重命名为 `runtime_session_id` 以明确语义。
- Permission Grant TTL 过期需要后台 job 或 lazy check（session 活跃时检查）。
