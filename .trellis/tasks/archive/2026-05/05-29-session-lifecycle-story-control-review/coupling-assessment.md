# Story 相关耦合评估

## Anchor

本轮架构 review 的核心目的不是补一个 Routine case，而是把 Story 从 Session、Lifecycle、Workflow binding、Capability owner type 等历史耦合中彻底解开。目标锚点：

- Story 是业务工作单元和用户可见上下文壳。
- LifecycleRun 是运行实例，不是 Story 的子对象，也不是 Session 的别名。
- Session 是 runtime substrate，只承载 event、turn、tool、connector resume、debug replay。
- Task 是 Story 下的工作项 spec / view，不应把 workflow runtime 或 session identity 固化为自身事实。
- 权限由 Agent assignment、AgentPermissionRequest / AgentPermissionGrant、permission cap 到 tool cap 的解析与 lifecycle contract 决定，不由 session owner type 或 binding label 推断。

判断一个字段或关联是否需要清理，使用同一条规则：如果它同时承担“业务归属、运行关联、权限事实、runtime 日志定位”中的两类以上职责，就应拆分。

## Coupling Map

| Area | Current Coupling | Evidence | Risk | Target Boundary | Priority |
| --- | --- | --- | --- | --- | --- |
| Story session | Story 与 root session 1:1，`companion` label 既像对话入口又像控制入口 | `.trellis/spec/backend/story-task-runtime.md`; `crates/agentdash-api/src/routes/story_sessions.rs`; `crates/agentdash-api/src/routes.rs` | Story 运行能力依赖 session 是否存在，自动化 Agent 难以直接进入 Story scope | Story 独立存在；companion 是 runtime/协作 attempt；Story 管理面走 Agent permission grant | P0 |
| LifecycleRun session | `LifecycleRun.session_id` 注释仍表达 Story root session 模型 | `crates/agentdash-domain/src/workflow/entity.rs`; `crates/agentdash-infrastructure/migrations/0008_lifecycle_run_session_id.sql`; `/lifecycle-runs/by-session/{session_id}` | run 查询与冲突范围被 session 绑住，Story/Lifecycle 多对多无法表达 | LifecycleRun core 不持业务对象；run association 连接 Story/RoutineExecution/Task/Project；attempt association 连接 RuntimeSession | P0 |
| SessionBinding | `SessionBinding(owner_type, owner_id, label)` 同时承担 runtime association、产品归属、查询入口、能力上下文 | `crates/agentdash-domain/src/session_binding/entity.rs`; `SessionBindingRepository`; context construction / hooks / story_sessions | label 值域扩张后不可治理，业务权限被 runtime 关联污染 | SessionBinding 收敛为 runtime/debug/context association；业务关系和控制权迁出 | P0 |
| Capability owner type | `allowed_owner_types` 以 Project/Story/Task 做能力硬边界 | `crates/agentdash-spi/src/platform/tool_capability.rs` | Agent assignment、主动申请、审批、临时授权、撤销、过期无法被解释 | Agent permission grant + permission cap compiler + lifecycle contract + agent config 共同解析能力 | P0 |
| Workflow binding | `WorkflowBindingKind/binding_kinds` 用 project/story 表达 workflow/lifecycle 可挂载范围 | `crates/agentdash-domain/src/workflow/value_objects/binding.rs`; `crates/agentdash-infrastructure/migrations/0024_workflow_binding_kinds.sql`; MCP workflow server | 古早 catalog filter 被当成启动上下文、scope 和权限近似值 | 拆成 definition catalog filter、launch scope、subject requirements、capability contract | P1 |
| Task step key | `Task.lifecycle_step_key` 直接指向 workflow step key | `crates/agentdash-domain/src/task/entity.rs`; `StoryStepActivationService::activate_story_step`; `.trellis/spec/backend/story-task-runtime.md` | 用户工作项与迁移中的 Step 概念强耦合，step key 改动影响 Story task | 研究删除，运行时绑定迁到 run/activity association | P1 |
| Task projection fields | `Task.status` / `artifacts` 是 LifecycleRun step state 的只读投影 | `crates/agentdash-domain/src/task/entity.rs`; `task/view_projector.rs` | 若被当成 runtime 真相，会与 LifecycleRun state 分叉 | 保留为 Story view projection；真相源在 run/activity state | P2 |
| Story task embedding | Story 持有 `stories.tasks` JSONB，Task 内仍有 `story_id` / `project_id` | `crates/agentdash-domain/src/story/entity.rs`; `crates/agentdash-domain/src/task/entity.rs`; `story_repository.rs` | Task 既像 Story child 又像全局实体，API `/tasks/{id}` 容易误导边界 | 保持 Task 为 Story child；若需要跨 Story/run 查询，建 view/index，不把 Task 升级为运行主语 | P1 |
| Story task_count | `Story.task_count` 是聚合冗余字段 | `crates/agentdash-domain/src/story/entity.rs` | 低风险；由 aggregate 方法维护时可作为 UI cache | 可保留为 projection/cache，不参与权限或运行决策 | P3 |
| Routine execution session | `RoutineExecution.session_id` 把 Routine 执行落到 session | `crates/agentdash-domain/src/routine/entity.rs`; routine executor | Routine 的目标运行主语被 session 替代，后续巡检结果难以挂到 LifecycleRun | RoutineExecution 关联 LifecycleRun；session 只在 attempt runtime 层出现 | P1 |
| API surface | `/stories/{id}/sessions`、`/tasks/{id}/session`、`/lifecycle-runs/by-session/{session_id}` 把 session 暴露成业务导航 | `crates/agentdash-api/src/routes.rs` | 前端和外部调用继续围绕 session 建模 | 产品 API 围绕 Story、LifecycleRun、Activity、Attempt；session API 移到 runtime/debug | P0 |
| Story source in capability/VFS | `source_story_id` 出现在 mount/capability projection 中 | `crates/agentdash-domain/src/common/mount.rs`; VFS/session construction | 如果作为权限来源会重复 Story scope；作为 UI/runtime source ref 则可接受 | 仅作 projection source/debug metadata，不作 control scope 或 capability source | P2 |

## Redundant Field Assessment

### Must Move Out Of Core

- `LifecycleRun.session_id`: 从 core run 字段迁出。run 与 runtime session 的关系应由 ActivityAttempt runtime session association 表达。
- `RoutineExecution.session_id`: 从 Routine 的主要运行索引迁出。RoutineExecution 应关联 LifecycleRun，session 只作为 Agent attempt 的日志资源。
- `SessionBinding.owner_type/owner_id/label` 的业务事实用途：保留 runtime association，移除 Story 控制权、run 查询、产品导航职责。
- `WorkflowBindingKind/binding_kinds` 的权限和 runtime association 用途：保留或替换为 definition catalog 语义，启动和能力判断进入更精确 contract。

### Can Stay As Projection Or Cache

- `Story.task_count`: 由 Story aggregate 维护，可作为 UI cache。
- `Task.status` / `Task.artifacts`: 可作为 Story task view 投影，但不能作为 runtime truth。
- `Mount.source_story_id`: 可作为 runtime projection source ref，但不能作为 Story control scope 或 capability 来源。

### Needs Design Decision During Cleanup

- `Task.story_id` / `Task.project_id` inside embedded Story tasks: 作为 child entity 冗余可以保留以便 DTO 和投影，但需要避免让 Task 重新变成独立 aggregate。
- `Task.lifecycle_step_key`: 目标上研究删除。Lifecycle/Step 还会继续界定，Task 不应继续持有 Step 绑定语义；运行查找迁到 run/activity association。
- `LifecycleRun.step_states` 与 `activity_state`: 当前存在新旧运行态并行，解耦时应确认 Activity lifecycle 是唯一对外 contract，旧 step state 只保留内部过渡或直接清除。

## Desired Relationship Model

```text
Story
  owns Task specs / task view projections
  references current or related LifecycleRuns through run association

LifecycleRun
  owns runtime state for one lifecycle definition execution
  links to Story / RoutineExecution / Task / Project through association roles

ActivityAttempt
  owns executor attempt state
  links to RuntimeSession when an agent/tool execution needs logs or resume

AgentPermissionRequest / AgentPermissionGrant
  owns actor authority state for Story scope
  starts from Agent assignment, self-request, policy and approval
  references actor + story + context, where context can be LifecycleRun

SessionBinding
  records RuntimeSession context/debug association only
```

Run association roles:

- `source`: who triggered or spawned the run, such as RoutineExecution or another LifecycleRun.
- `subject`: what the run is working on, such as Story, Project, external entity.
- `projection_target`: where outputs should update visible state, such as Story or Task view.
- `control_scope`: which scope an actor inside the run may request control over.
- `spawned_by`: parent run/activity lineage.

## Decoupling Order

1. Establish vocabulary and evidence.
   - Update specs so Story, LifecycleRun, Session, Task, Grant, Association have one responsibility each.
   - Mark every current coupled field as core, association, projection, cache, or debug metadata.

2. Add explicit association and grant models.
   - Introduce run association for Story / RoutineExecution / Task / Project / external subjects.
   - Introduce Agent permission request / grant system.
   - Let Story query authorized Agent runtime sessions through active grants and runtime associations, without introducing OwnedAgent as a core concept.
   - Introduce ActivityAttempt runtime session association.

3. Move application flows to new facts.
   - Task start/continue uses Story + Task + run/activity services, not Story companion session.
   - Routine triggers LifecycleRun; LifecycleRun Agent requests Story operations through controlled service.
   - Capability resolution uses Agent permission grant, permission cap compiler, lifecycle contract and runtime association.

4. Retire product-facing session paths.
   - Replace Story/session and Task/session navigation with Story/LifecycleRun/Activity/Attempt APIs.
   - Keep runtime session APIs only for debug, replay, trace and connector runtime.

5. Clean residual redundant fields.
   - Remove or demote `LifecycleRun.session_id`, `RoutineExecution.session_id`, and session-derived run queries.
   - Rework `WorkflowBindingKind/binding_kinds` into catalog/launch/capability contracts.
   - Re-evaluate Task embedded redundancy after run association is stable.

## Feature Enablement After Decoupling

Once these boundaries are stable, the following features become straightforward:

- 巡检 Agent 基于 LifecycleRun 结果，凭 Agent permission grant 解析出的 story management tool 创建 Story，并在需要 Story scope 管理时申请权限。
- 一个 LifecycleRun 可影响多个 Story；一个 Story 可聚合多个 LifecycleRun。
- Task/companion dispatch 不需要预先创建 Story companion session。
- Workflow definition 可以按 subject requirements 和 capability contract 选择，而不是按 project/story 二分。
- 前端可以围绕 Story timeline / LifecycleRun timeline / Activity attempt trace 组织，而不是围绕 session 页面绕路。
