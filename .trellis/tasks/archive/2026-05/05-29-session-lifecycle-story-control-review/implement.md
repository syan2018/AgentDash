# Session 外露与 Story/Lifecycle 控制面架构 Review Implement Plan

## Phase 0: Evidence Audit

- [ ] 审计后端 API 中所有直接暴露 `session_id` 作为业务主语的 route。
- [ ] 审计 application service 中通过 `SessionBinding` 反查 Story / Task / LifecycleRun 的路径。
- [ ] 审计前端路由、store 和组件中 `/session/{id}` 的产品入口用途。
- [ ] 审计 capability resolver 中 `SessionOwnerType` 硬边界对 Story scope Agent 的影响。
- [ ] 审计 Routine / ProjectAgent / Companion 当前如何派发 session 或 LifecycleRun。
- [ ] 审计 `WorkflowBindingKind/binding_kinds` 在 domain、MCP、repository、contracts、frontend 中承担的职责。
- [ ] 审计 `Task.lifecycle_step_key` 是否只是 task spec 到 activity role 的投影，还是承担了运行关联职责。
- [ ] 审计 `RoutineExecution.session_id` 是否仍把 Routine 执行落到 Session 主语。
- [ ] 维护 `coupling-assessment.md`，把每个 Story 周边耦合点归类为 must move / projection-cache / needs decision。

建议命令：

```powershell
rg -n "session_id|SessionBinding|SessionOwnerType|/session|story_sessions|activate_story_step|RoutineExecution" crates packages .trellis/spec tests
rg -n "WorkflowBindingKind|binding_kinds|binding_kind|Task.lifecycle_step_key|lifecycle_step_key|allowed_owner_types" crates packages .trellis/spec tests
rg -n "allowed_owner_types|CAP_|workflow_management|collaboration|story_management|task_management" crates/agentdash-spi crates/agentdash-application
```

验收：

- 形成当前依赖清单，按 runtime 必需 / 产品外露 / 权限事实源 / 古早 catalog filter 四类标注。
- 标出首批必须调整的 API、service、repository 和 spec。
- 列出可清理的冗余关联，并为每一项给出目标替代职责。
- 完成 Story 周边整体耦合评估，作为后续解耦任务拆分依据。

## Phase 1: Target Model Spec

- [ ] 更新 Story / Task runtime spec，明确 Story 薄层、LifecycleRun association、Session runtime substrate。
- [ ] 更新 Session architecture，明确 Session 的内部职责与 debug/view 用途。
- [ ] 更新 Workflow architecture，明确 LifecycleRun、Activity、Attempt、RuntimeSession 与 Story scope 的关系。
- [ ] 写入 Agent permission request / grant system 的领域定义和能力解析原则。
- [ ] 明确不引入 OwnedAgent core 概念；Story 层通过 active permission grants + runtime associations 查询有权限的 Agent 会话。
- [ ] 关联 `05-26-companion-interaction-capability-grant`，复用 Agent 主动申请、平台 broker、permission/grant record、RuntimeCapabilityTransition、CapabilityState replay 的链路。
- [ ] 写入 `WorkflowBindingKind/binding_kinds` 的目标拆分方向：launch scope、subject requirements、capability contract。

验收：

- spec 中的事实源不再把 Story 存在性或控制权绑定到 companion session。
- spec 中能解释目标链路：Routine 触发 LifecycleRun；巡检 Agent 凭 Agent permission grant 解析出的工具完成 Story 创建、管理权限申请、Task 创建与后续派发。
- spec 中明确 LifecycleRun 不直接持有 Story 外键、permission grant 外键或 Story 业务语义。
- spec 只记录目标边界和原因，不记录一次性修补细节。

## Phase 2: Domain / Database Planning

- [ ] 设计 `agent_permission_requests` / `agent_permission_grants` schema 或等价领域实体。
- [ ] 设计 permission cap value objects 与 permission cap -> tool capability compiler。
- [ ] 设计 `lifecycle_run_subjects` / `lifecycle_run_links` / `story_run_links` 或等价 run association 表。
- [ ] 设计 ActivityAttempt 到 RuntimeSession 的 association。
- [ ] 设计清理或替换 `lifecycle_runs.session_id` 业务用途的路径。
- [ ] 审计 `workflow_definitions.binding_kinds` / `lifecycle_definitions.binding_kinds` 是否应被 launch scope / contract / capability requirements 替代。
- [ ] 审计 `routine_executions.session_id`，将 Routine 执行索引收敛到 LifecycleRun 或 run association。
- [ ] 设计 migration 路径，包含历史数据如何映射为 runtime association、run subject 或 permission grant。

候选 migration / 模型：

```text
agent_permission_requests
agent_permission_grants
permission_cap_to_tool_cap compiler
lifecycle_run_subjects 或 lifecycle_run_links
activity_attempt_runtime_sessions
workflow launch scope / subject requirements / capability contract columns or specs
routine_execution_run_links 或 lifecycle_run_subjects(subject_kind = routine_execution)
```

验收：

- 数据模型能表达目标链路：Routine 触发 LifecycleRun；巡检 Agent 凭 Agent permission grant 解析出的 story management tool 完成 Story 创建，并在需要 Story scope 管理时申请权限。
- Story 与 LifecycleRun 是多对多关系，不通过 `LifecycleRun.story_id` 或 root session 表达。
- Run 查询不需要通过 Story companion session 反查 Story。
- RuntimeSession 仍可被 attempt timeline 和 debug 工具定位。

## Phase 3: Application Service Refactor Plan

- [ ] 设计 `LifecycleRunAssociationService` 或等价服务：
  - attach_subject
  - list_runs_for_subject
  - list_subjects_for_run
  - record_projection_target
  - record_spawned_run
- [ ] 设计 `AgentPermissionService`：
  - request_permission
  - approve_or_reject_permission
  - grant_permission
  - revoke_or_expire_permission
  - compile_permission_caps
  - list_authorized_agent_sessions
- [ ] 设计受控 Story tool / application service，使 LifecycleRun 内 Agent 凭已解析出的 story management tool capability 创建 Story，并在 Story scope 内请求管理权限。
- [ ] 将 `StoryStepActivationService` 的 Task facade 收敛到 run/activity dispatch。
- [ ] 将 Routine executor 从 session prompt 派发路径调整为 LifecycleRun 启动/关联路径。
- [ ] 将 Companion dispatch 与 activity attempt / run timeline 对齐。
- [ ] 将 projector 从 `run.session_id -> StoryBinding` 改为 `run association -> Story / Task projection`。

验收：

- Task 启动不再要求 Story 预先存在 `label=companion` 的 session binding。
- Routine 负责触发 LifecycleRun；Story 创建由 LifecycleRun 内 Agent 凭 Agent permission system 解析出的 tool capability 请求，Story scope 管理由 AgentPermissionService 管理。
- Run / Activity service 成为后续业务操作入口。
- Projection 能解释输出来自哪个 LifecycleRun、哪个 actor、哪个 permission grant 或 association。

## Phase 4: Capability / Authorization Plan

- [ ] 设计 capability / permission 输入：
  - actor identity
  - agent assignment
  - base permission caps
  - active AgentPermissionGrant
  - lifecycle run context
  - story/project scope
  - permission request / grant role
  - lifecycle contract
  - runtime session association
- [ ] 调整 capability resolver 的思路，从 `SessionOwnerType` 单轴扩展为 Agent permission grant + permission cap compiler + lifecycle contract。
- [ ] 定义 Story controller 能力组合：
  - story management
  - task/work item management
  - lifecycle/workflow dispatch
  - companion/collaboration
  - VFS / MCP 根据 lifecycle contract 和 ProjectAgent 配置授予
- [ ] 对 MCP server / tool 注入路径补权限来源 trace。
- [ ] 审计 `WorkflowBindingKind/binding_kinds` 与 capability visibility 的重叠职责，拆出 workflow 可启动范围与工具可见性来源。

验收：

- 有权限的 Agent 会话能在 Story scope 内派发 task / companion / lifecycle activity。
- LifecycleRun 内 Agent 的 Story 操作能被 AgentPermissionGrant 与 run context 审计追踪。
- Capability trace 能解释能力来自 Agent assignment、permission grant、permission cap compiler、lifecycle contract、agent config 还是 runtime association。
- `SessionOwnerType` 不再是 Story scope 工具可见性的唯一硬边界。

## Phase 5: API / Frontend Plan

- [ ] 新增或规划 LifecycleRun-oriented API。
- [ ] 新增或规划 run association 查询 API。
- [ ] 迁移 Story 页面主要操作到 Story / LifecycleRun / Activity 入口。
- [ ] 将 session 页面定位为 runtime detail / debug / event replay。
- [ ] 前端 Store 从 session-first 查询转向 story/run-first 查询。
- [ ] 测试 LifecycleRun-created Story 的 run timeline 展示。

验收：

- 用户从 Story 进入相关 LifecycleRun / Activity，而不是直接进入 session。
- session id 出现在详情或 debug metadata 中，不承担业务导航主语。
- Story 页面能展示 related runs、activity status、attempt/session trace。

## Suggested Child Tasks

建议后续拆成以下子任务：

1. `story-lifecycle-control-spec-update`
   - 更新 Trellis spec，确认目标模型和术语。

2. `story-coupling-inventory`
   - 基于 `coupling-assessment.md` 审计 Story、Lifecycle、Session、Task、Routine、Capability、Workflow binding 的耦合点，锁定 must move 清单。

3. `run-association-cleanup`
   - 增加 LifecycleRun subject/link association，替换 Story / LifecycleRun 通过 session 反查的路径。

4. `agent-permission-system`
   - 增加 Agent permission request / grant state、permission cap 编译器与 migration，并提供有权限 Agent 会话查询投影。

5. `workflow-binding-model-review`
   - 审计 `WorkflowBindingKind/binding_kinds`，规划 launch scope、subject requirements、capability contract 的替代模型。

6. `run-oriented-application-api`
   - 新增 LifecycleRun / Activity dispatch API，收敛 Task facade。

7. `routine-lifecycle-run-entry`
   - Routine executor 支持触发 LifecycleRun，并通过 run association 记录 RoutineExecution 来源。

8. `capability-permission-resolver`
   - CapabilityResolver 支持 Agent permission grant + permission cap compiler + lifecycle contract。

9. `frontend-story-run-navigation`
   - 前端从 session-first 导航转向 Story / LifecycleRun / Activity timeline。

## Validation Strategy

文档更新后的静态验证：

```powershell
rg -n "[R]outine.*创建 Story|lifecycle_runs[.]story_id|controller[_]grant_id" .trellis/tasks/05-29-session-lifecycle-story-control-review
rg -n "lifecycle_run_subjects|run association|WorkflowBindingKind|binding_kinds|AgentPermission|permission cap|tool cap" .trellis/tasks/05-29-session-lifecycle-story-control-review
rg -n "Agent Permission System|RuntimeCapabilityTransition|CapabilityState" .trellis/tasks/05-29-session-lifecycle-story-control-review/agent-permission-system.md
rg -n "Must Move Out Of Core|Can Stay As Projection Or Cache|Needs Design Decision During Cleanup|Decoupling Order" .trellis/tasks/05-29-session-lifecycle-story-control-review/coupling-assessment.md
```

最小后端验证：

```powershell
cargo test -p agentdash-domain story_control
cargo test -p agentdash-application workflow::run
cargo test -p agentdash-application routine
cargo test -p agentdash-api story run
```

最小前端验证：

```powershell
pnpm --filter @agentdash/app-web typecheck
pnpm --filter @agentdash/app-web test
```

端到端验证场景：

- Routine 触发 LifecycleRun。
- 巡检 Agent 在 LifecycleRun 内产出需要 Story 化的结果。
- Agent 通过 Agent permission system 解析出的 story management tool / application service 请求创建 Story。
- Agent 对 Story 获得管理权限 grant。
- Story 下创建 Task specs。
- LifecycleRun 派发一个 Agent activity，后端创建 RuntimeSession。
- Attempt 完成后，Task / Story view 通过 run association projection 更新。

## Risk Points

- `SessionOwnerResolver` 当前被 context construction、hook runtime、frontend context query 多处消费，迁移时需要先保留 runtime association 查询能力。
- `LifecycleRun.session_id` 当前参与 active run 冲突判断，显式 run association 后需要重新定义冲突范围。
- `WorkflowBindingKind/binding_kinds` 已进入 MCP、contracts、frontend 与 shared library，需要先区分 catalog filter、launch scope、capability contract 三类职责。
- `stories.tasks JSONB` 在并发 projection 与用户编辑下有合并逻辑，Run / Activity projector 改造时需要审计写入顺序。
- CapabilityResolver 的 owner-type matrix 是安全边界，迁移到 Agent permission system 时必须保留可解释 trace。
- 前端可能依赖 `/session/{id}` 返回 Story / Task 导航状态，改为 run-first 后需要同步 E2E 场景。

## Planning Gate Before Implementation

- [ ] 开发者确认首个子任务优先级。
- [ ] 确认 run association 的表名和 role 值域。
- [ ] 确认 AgentPermissionRequest / AgentPermissionGrant 的 scope、状态机、审批 owner 与 permission cap 命名。
- [ ] 确认 `WorkflowBindingKind/binding_kinds` 的替代模型拆分。
- [ ] 确认前端 session 页面最终定位为 debug/runtime detail。
