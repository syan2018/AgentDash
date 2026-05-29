# Session 外露与 Story/Lifecycle 控制面架构 Review Design

## Architecture Direction

目标架构把产品对象、权限对象、运行实例和 runtime 对象分层：

```text
Story
  -> explicit run association
    -> LifecycleRun
      -> Activity / Step
        -> Attempt
          -> RuntimeSession
```

Story 表达业务工作单元。LifecycleRun 表达 lifecycle definition 的一次运行实例。Activity / Step 表达运行中的编排节点。Attempt 表达某次执行尝试。RuntimeSession 表达该 attempt 的 turn/event/tool/runtime 日志。

Story 与 LifecycleRun 原则上是多对多关系，通过显式关联层表达：

```text
LifecycleRunSubject / LifecycleRunLink / StoryRunLink
- run_id
- subject_kind: story | project | routine_execution | task | external
- subject_id
- role: source | subject | projection_target | spawned_by | control_scope
- metadata
- created_at
```

当前实现模型需要先说清楚：

- Story 当前本体可以独立创建，`POST /stories` 不创建 SessionBinding 或 LifecycleRun。
- Story runtime spec 仍是 `Story-as-durable-session`：Story root session 通过 `SessionBinding(owner_type=Story, label="companion")` 表达。
- LifecycleRun 当前 core 持有 `session_id`。历史 migration 已从 `binding_kind + binding_id` 迁到 `session_id`，也就是 run 跟 session 走，而不是直接跟 Story/Task 走。
- Task 启动链路当前是 `story_id -> Story companion SessionBinding -> lifecycle_runs by session -> Task.lifecycle_step_key`。
- workflow / lifecycle definition 当前用 `binding_kinds = project | story` 做挂载范围；Task owner 映射到 Story binding。
- capability visibility 当前由 `SessionOwnerType` 的 `allowed_owner_types` 硬切工具可见性。
- RoutineExecution 当前可记录 `session_id`，Routine executor 仍偏 session prompt dispatch，而不是 LifecycleRun-first。

目标模型不是在这个现状上加 Story 外键，而是把业务关系、运行关系、权限关系、runtime session 关系拆开。

管理权限进入 Agent permission system：

```text
Actor(User | ProjectAgent | System)
  -> Agent assignment / Project scope permission
    -> story management tools
      -> AgentPermissionRequest / AgentPermissionGrant
        -> Story scope capabilities
        -> task / companion / lifecycle dispatch authority
```

SessionBinding 继续有价值，但它应是 runtime association：把 RuntimeSession 标注到 Project / Story / Task / ActivityAttempt 上，帮助 context construction、debug trace、timeline 回放和权限审计定位，不再作为 Story 是否可运行、Agent 是否拥有控制权的事实源。

WorkRun / StoryRun 暂不作为必选抽象。只有当产品需要把一个业务轮次聚合多个 LifecycleRun，并以该轮次作为 UI/API 主语时，再引入产品层索引。当前文档主线优先使用显式 run association，避免把新 wrapper 提前固化为必要模型。

## Target Responsibilities

### Story

Story 是薄业务壳：

- 持有用户可理解的业务语义：标题、描述、优先级、类型、标签、来源上下文。
- 持有或索引 Story 级 context source、container、巡检摘要、外部触发来源。
- 暴露当前可见状态投影：相关 run、整体进度、摘要、风险。
- 作为前端与 API 的业务入口，将执行动作转发到 Story 权限状态、LifecycleRun association、Activity dispatch 服务。

Story 不承载执行器选择、session 创建、turn launch、tool 注入、companion 派发等 runtime 业务。这些进入 LifecycleRun / Activity / Capability / Session runtime 层。

### LifecycleRun

LifecycleRun 是 lifecycle definition 的运行实例：

- `id`
- `project_id`
- `lifecycle_id`
- `status`
- `active_node_keys`
- `step_states` / `activity_state`
- `execution_log`
- `created_at` / `updated_at` / `last_activity_at`

LifecycleRun 不直接持有 Story 外键、permission grant 外键、业务轮次或 projection target。Story 相关查询通过 run association 完成；管理权限通过 AgentPermissionGrant 与 actor/context 完成；RuntimeSession 通过 ActivityAttempt association 完成。

外部 API 和前端应围绕 Story、LifecycleRun、Activity、Attempt 的业务语义组织，session id 只在 runtime detail / debug / trace 中出现。

### Run Association

显式 run association 负责表达 LifecycleRun 与业务对象之间的多对多关系：

- `source`：run 的触发来源，例如 RoutineExecution、webhook、manual command。
- `subject`：run 正在处理的对象，例如 Story、Project、外部实体。
- `projection_target`：run 输出需要投影到的对象，例如 Story view、Task view。
- `spawned_by`：run 由另一个 run 或 activity 派生。
- `control_scope`：run 内 actor 可请求控制权的业务范围。

该层替代通过 `LifecycleRun.session_id` 或 Story companion session 反查业务语义的路径。

### Activity / Step / Attempt

Activity / Step 是 workflow definition 的运行节点。Attempt 是一次可重试或可回放的执行：

- Attempt 可以由 Agent、Function、Human decision 等 executor 完成。
- Agent attempt 可以创建或复用 RuntimeSession。
- Attempt 与 session 的关系是实现细节，Timeline 可展示 session trace，但业务动作不以 session id 为主键。
- Task 与 activity / lifecycle step 的关系需要重新研究。`Task.lifecycle_step_key` 目标上应删除或迁移到 run/activity association，因为 Step 概念已在迁移中，Task 不应继续绑定具体 lifecycle step key。

### RuntimeSession

Session 保留为 runtime substrate：

- append-only event log
- turn / tool call / stream / terminal state
- connector resume state
- context compaction projection
- runtime capability transition records
- debug / audit / replay view

Session 可以被 activity attempt 索引，也可以被 fork/lineage 系统管理，但产品主路径不直接暴露“打开某个 session 来操作业务”。

### Story Permissions

Story 管理能力来自 Agent permission system。Agent 能否创建 Story，首先取决于 ProjectAgent assignment 或 active permission grant 是否授予了 `project.story.create` 这类 permission cap，并由 resolver 解析出对应 story management tool capability。进入某个 Story 之后，管理能力通过 scoped AgentPermissionGrant 表达，而不是通过 SessionBinding 或 OwnedAgent 概念推断。

Agent permission state 应至少能表达：

```text
AgentPermissionRequest / AgentPermissionGrant
- id
- project_id
- agent_id
- scope_kind: project | story | task | lifecycle_run | backend | workspace | mcp_server
- scope_id
- context_kind: lifecycle_run | manual | system
- context_id
- permission_caps
- compiled_tool_capabilities
- tool_filters
- status: requested | active | rejected | released | revoked | expired
- requested_at
- acquired_at
- released_at
- reason / source_ref
```

OwnedAgent 不作为目标模型中的核心概念存在。Story 层只需要能按 active AgentPermissionGrant + runtime association 捞到有权限的 Agent 会话，并据此展示“哪些 Agent 当前能管理/协作这个 Story”。这只是查询投影，不需要独立生命周期。

详细 Agent 权限系统见 `agent-permission-system.md`。该文档把 Agent 主动申请、审批/policy、permission cap 到 tool cap 的解析、CapabilityState runtime apply、撤销/过期都纳入同一条链路。

## Current Tension Evidence

### Story-as-durable-session Spec

`.trellis/spec/backend/story-task-runtime.md` 当前规定：

- Story 与 Story session 1:1。
- LifecycleRun 1:1 挂在 Story session 上。
- Story 内状态变更的唯一审计源是 Story session event stream。
- Task runtime 通过 Story step activation 路径进入。

这个模型使 session 成为 Story 运行事实源。目标架构需要把 Story 与 LifecycleRun 的关系提升为显式 association，把 session 降级为 Attempt runtime log。

### Story Creation

`crates/agentdash-api/src/routes/stories.rs` 的 `create_story` 当前只创建 Story 和 inline files，不创建 SessionBinding 或 LifecycleRun。这说明 Story 本体已经可以先于 session 存在，代码事实与 1:1 Story session 不变量存在偏差。

### Story Session API

`crates/agentdash-api/src/routes/story_sessions.rs` 当前只允许 Story root session 使用 `label=companion`，并在新 session 创建后启动 freeform lifecycle。这把“伴随对话”和“Story 控制入口”合并到同一条 API 路径。

### Task Activation

`crates/agentdash-application/src/task/service.rs` 当前 `activate_story_step` 通过 `Story -> SessionBinding(Story, "companion") -> active LifecycleRun -> Task.lifecycle_step_key` 定位执行。这让 Task 执行依赖 Story companion session 的存在。

### LifecycleRun Session Binding

`crates/agentdash-domain/src/workflow/entity.rs` 当前 `LifecycleRun.session_id` 的注释仍描述 Story root session 模型。`LifecycleRunRepository::list_by_session` 也把 session 作为 run 查询入口。目标模型中 run 的业务关联通过 association 查询，session 查询只服务 runtime/detail/debug。

### Workflow Binding Kind

`WorkflowBindingKind/binding_kinds` 当前用 project/story 表达 workflow 与 lifecycle definition 的挂载范围，并在 MCP、repository、frontend contract 中传播。它适合作为早期 catalog filter，但不足以表达：

- run 启动时的 subject requirements
- actor 能力来源
- lifecycle contract 对 Story / Project / Task 的细粒度需求
- activity attempt 与 runtime session 的关联
- Story 控制权申请范围

后续应审计它是否迁移为 launch scope、subject requirements 与 capability contract 的组合。

### Session Owner Resolution

`crates/agentdash-application/src/session/ownership.rs` 当前通过 `Task > Story > Project` 选择 primary owner。该逻辑适合构建单个 session 的上下文，但不足以表达 actor 对 Story 的控制权、授权来源、角色和有效期。

### Capability Visibility

`crates/agentdash-spi/src/platform/tool_capability.rs` 当前以 `SessionOwnerType` 做能力硬边界。目标模型中 Story 创建来自 Agent assignment / active grant 中的 permission cap，并由 resolver 解析成 story management tool capability；Story 内管理能力来自 scoped AgentPermissionGrant。当前矩阵无法表达“Agent 权限赋予工具，Agent 可申请权限，审批后动态更新工具面”的链路。

### Routine Dispatch

`crates/agentdash-application/src/routine/executor.rs` 已有 RoutineExecution、system routine identity 和 ProjectAgent session dispatch。目标链路中 Routine 负责触发 LifecycleRun；Story 创建来自 LifecycleRun 内巡检 Agent 通过 Agent permission system 获得的 story management tool capability，Story 后续管理能力通过 scoped AgentPermissionGrant 进入 Story scope。

## Target Routine Flow

Routine 巡检 Agent 的目标流程：

```text
Routine trigger
  -> start LifecycleRun for inspection workflow
  -> inspection Agent runs inside LifecycleRun
  -> Agent emits result / calls story management tool or permission request tool
  -> application service validates Agent permission grant + run context + policy
  -> create Story when needed
  -> request/acquire scoped AgentPermissionGrant when Story-scope management is needed
  -> create Task specs under Story
  -> dispatch task/companion execution through lifecycle/activity services
  -> RuntimeSession exists only for attempt/event/debug runtime
```

这个流程让自动化 Agent 能通过明确的 Agent permission grant、run context 和 Story scope permission 进入 Story scope，而不是让 Routine 越过 Agent 执行上下文直接操作 Story，也不是绕过 Project session 或 Story companion session 取得能力。

## Redundant Association Audit Targets

首轮 review 需要覆盖这些具体张力：

- `SessionBinding(owner_type, owner_id, label)`：当前同时承担 runtime association、产品归属、查询入口、能力上下文；目标是收敛为 runtime/debug/context association。
- `LifecycleRun.session_id`：当前把 run 与 root session 绑定；目标是 run 与 runtime session 通过 activity/attempt association 连接。
- `WorkflowBindingKind/binding_kinds`：当前用 project/story 表达 workflow 可挂载范围；目标是拆成 launch scope、subject requirements、capability contract。
- `SessionOwnerType` capability matrix：当前按 Project/Story/Task owner 硬切工具可见性；目标是 Agent assignment/base permission -> permission request/grant -> permission cap compiler -> tool capability -> runtime CapabilityState 的链路。
- `Task.lifecycle_step_key`：当前用字符串把 Story Task 直连 lifecycle step；目标是研究删除或迁移到 run/activity association，不再保留 Step 绑定语义。
- `RoutineExecution.session_id`：当前 Routine 执行仍可落到 session；目标是审计 Routine -> LifecycleRun 的正确索引路径。
- `story_sessions` / `companion` label：当前把 Story companion session 当业务入口；目标是 companion 只作为一种 runtime/协作 attempt。

详细耦合评估见 `coupling-assessment.md`。该文档把 Story 周边字段和关联分为 P0/P1/P2/P3，并区分“必须迁出 core”、“可保留为 projection/cache”、“需要后续设计决策”三类，作为后续拆分任务的依据。

## API Shape Direction

产品 API 应收敛到 Story + LifecycleRun / Activity 主语：

```text
POST /stories/{story_id}/permissions/requests
POST /stories/{story_id}/permissions/{request_id}/grant
DELETE /stories/{story_id}/permissions/{grant_id}
GET  /stories/{story_id}/runs
POST /lifecycle-runs/{run_id}/subjects
GET  /lifecycle-runs/{run_id}
GET  /lifecycle-runs/{run_id}/timeline
POST /lifecycle-runs/{run_id}/activities/{activity_key}/dispatch
POST /lifecycle-runs/{run_id}/activities/{activity_key}/complete
```

Session API 保留 debug / runtime 能力：

```text
GET /runtime/sessions/{session_id}
GET /runtime/sessions/{session_id}/events
GET /lifecycle-runs/{run_id}/attempts/{attempt_id}/session
```

## Migration Direction

项目处于预研期，后续实现可以通过 migration 直接收敛模型：

- 新增 AgentPermissionRequest / AgentPermissionGrant，Story scope 管理权限作为其中一种 scope。
- 新增 LifecycleRun subject/link association，表达 Story、RoutineExecution、Task、Project、外部来源与 run 的多对多关系。
- 将 LifecycleRun 与 Story 的关联从 session binding 反查改为显式 run association。
- 将 ActivityAttempt 到 RuntimeSession 的关系从 run/root session 硬绑迁移为 attempt association。
- 将 Task activation facade 改为 run/activity dispatch facade。
- 将 Story session API 调整为 runtime/debug 或 companion 专用能力。
- 前端导航从 session 主路径切到 Story / LifecycleRun / Activity。
- CapabilityResolver 从 owner-type matrix 扩展到 Agent assignment + permission grant + permission cap compiler + lifecycle contract。
- 审计 `WorkflowBindingKind/binding_kinds`，拆分 definition catalog filter、launch scope、subject requirements 与 capability contract。

## Trade-offs

### 显式 Run Association

优点：保留 LifecycleRun 的纯 runtime 边界，能表达 Story 与 LifecycleRun 多对多，也能纳入 RoutineExecution、Task、外部实体等来源。  
代价：查询 Story 相关 run、投影 timeline、权限解释都需要新的 association repository 和服务层。

### StoryRun / WorkRun wrapper

优点：当一个业务轮次聚合多个 LifecycleRun 时，产品概念清楚，前端和 API 可围绕轮次组织。  
代价：当前需求尚未确认必须存在轮次级产品主语，提前引入会增加抽象层。

推荐方向：先落显式 LifecycleRun association 与 Agent permission request / grant skeleton；后续若 UI/API 明确需要“一个业务轮次聚合多个 LifecycleRun”，再引入 WorkRun / StoryRun 作为产品层索引。

## Spec Updates Needed

- `.trellis/spec/backend/story-task-runtime.md`：从 Story-as-durable-session 改为 Story-as-thin-business-scope + LifecycleRun association。
- `.trellis/spec/backend/session/architecture.md`：明确 Session 是 runtime substrate，不是产品控制面主语。
- `.trellis/spec/backend/workflow/architecture.md`：补充 LifecycleRun / Activity / Attempt 与 Story scope 的关系。
- `.trellis/spec/backend/capability/architecture.md`：补充 Agent permission request/grant、permission cap -> tool cap、RuntimeCapabilityTransition -> CapabilityState 的能力来源。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：补充 run-oriented DTO 与 TS 生成要求。
- workflow / shared library contract：审计 `WorkflowBindingKind/binding_kinds` 的目标职责。
- 前端 workflow / story spec：补充 Story run 页面、timeline、activity dispatch 的产品入口。
