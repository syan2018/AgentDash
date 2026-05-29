# Session 外露与 Story/Lifecycle 控制面架构 Review

## Goal

审计并规划 AgentDashboard 的 Story / Lifecycle / Session / Agent 控制面边界，使产品层围绕 Story 与 LifecycleRun 组织，Session 收敛为 runtime 内部日志与恢复索引，Story 保持薄业务层，Agent 通过显式控制权获得 Story 范围内的编排能力。

本任务是 architecture review / planning 任务，不直接实施大规模改造。它的产出应能管理后续拆分任务，并为后续代码与 spec 更新提供共同语言。

## Background

当前讨论聚焦三个架构判断：

- Story 与 Session 的绑定过强，影响巡检 Agent 这类自动化工作流。目标链路应是 Routine 触发一次 LifecycleRun；巡检 Agent 基于运行结果，通过受控工具或应用服务请求 Story 新建、控制权申请、Task 创建与后续派发。
- Session 原则上应降级为 runtime substrate：保存 turn、event、connector state、compaction projection 与审计日志；产品入口应通过 LifecycleRun 与显式关联索引封装，Story 作为业务薄层和转发入口。
- 当前项目存在多类古早冗余关联模型：业务归属、运行事实、权限事实和 runtime 日志常被压进同一个字段或 binding。除 Story-Session 外，`WorkflowBindingKind/binding_kinds`、`LifecycleRun.session_id`、`SessionOwnerType` 能力矩阵、Task-step 直连也需要纳入同一轮审计。
- 解耦是本任务的核心锚点。后续任何 Story/Routine/Workflow feature 都应建立在“业务对象、运行实例、Agent 权限事实、runtime 日志”四类事实分离的基础上。
- Story 权限不应孤立建模；目标方向是 Agent permission system：权限跟随 Agent/ProjectAgent assignment，可由 Agent 主动申请，经 policy/审批形成 grant，再由 permission cap 解析成 tool cap、MCP、VFS 与 runtime CapabilityState。

已确认的代码与 spec 事实：

- `.trellis/spec/backend/story-task-runtime.md` 当前定义 `Story-as-durable-session`，规定 Story 与 Story session 1:1，LifecycleRun 1:1 挂在 Story session 上。
- `POST /stories` 当前只创建 Story，不创建 Story session 或 LifecycleRun，说明 Story 已经能独立于 Session 存在。
- `StoryStepActivationService::activate_story_step` 当前通过 `Story -> SessionBinding(Story, "companion") -> active LifecycleRun -> Task` 启动任务执行。
- `story_sessions` API 当前把 Story root session label 收敛为 `companion`。
- `SessionOwnerResolver` 当前把多重归属压成 `Task > Story > Project` 的 primary owner。
- capability visibility 当前按 `SessionOwnerType` 做硬边界，Story、Task、Project owner 各自拿到不同能力面。
- Routine 已有 `RoutineExecution`、`system_routine` 身份与 ProjectAgent session 派发路径；目标模型中 Routine 负责触发 LifecycleRun，Story 操作由 run 内 Agent 通过授权路径请求。
- workflow / lifecycle definition 仍以 `binding_kinds` 表达 project/story 挂载范围；该模型无法表达更细的启动上下文、subject requirements、scope、capability contract 与 run-time association。

## Requirements

1. 明确目标架构原则：
   - 产品层主语是 Story、LifecycleRun、Activity / Step、Attempt。
   - LifecycleRun 是 lifecycle definition 的运行实例，只承载 runtime state、activity/step state、execution log 和 timestamps。
   - Story 与 Lifecycle / LifecycleRun 原则上是多对多，通过显式 run association / subject index 表达。
   - Session 是 runtime 内部资源，用于事件日志、connector 恢复、上下文压缩、tool call 审计和调试回放。
   - Story 是薄业务壳，承载业务语义、上下文引用、可见状态投影和转发入口。
   - Story 控制权由显式 Actor / Grant / Role 模型表达，而不是由 SessionBinding 隐式决定。

2. 审计当前张力：
   - Story 创建、Story session 创建、Task 启动、LifecycleRun 创建、Routine 派发、Companion 派发、CapabilityResolver、workflow binding、前端导航之间的事实源是否一致。
   - 找出依赖 `session_id`、`SessionBinding`、`WorkflowBindingKind`、`SessionOwnerType` 或 `Task.lifecycle_step_key` 作为业务主语或权限事实的 API、service、前端页面和 spec。
   - 标注哪些依赖属于 runtime 必需，哪些属于产品层外露，哪些属于权限事实源混用。
   - 列出所有把业务关联、运行事实、权限事实压进同一字段或同一 binding 的冗余关联。
   - 输出 Story 相关整体耦合评估，区分必须迁出的 core 字段、可保留的 projection/cache、需要后续设计决策的冗余字段。

3. 设计后续目标模型：
   - LifecycleRun 作为运行实例，不直接持有 Story 外键、permission grant 外键或 Story 业务语义。
   - `lifecycle_run_subjects` / `lifecycle_run_links` / `story_run_links` 或等价模型作为显式关联层，role 表达 `source`、`subject`、`projection_target`、`spawned_by`、`control_scope`。
   - Activity / Step / Attempt 作为执行与派发单元。
   - RuntimeSession 作为 Attempt 的日志与恢复资源。
   - AgentPermissionRequest / AgentPermissionGrant 作为 Agent 对 Project / Story / Lifecycle / Backend 等 scope 的统一权限事实。
   - Story 层按 active Agent permission grants + runtime associations 捞取有权限的 Agent 会话，不引入 OwnedAgent core 概念。
   - CapabilityResolver 基于 Agent assignment、permission grant、permission cap compiler、tool mapping、lifecycle contract 解析能力。
   - `WorkflowBindingKind/binding_kinds` 的职责应被拆分为 launch scope、subject requirements、capability contract 等更精确概念。

4. 规划迁移路径：
   - 给出从现状到目标架构的阶段拆分。
   - 每个阶段应有独立验收标准、风险说明、建议验证方式。
   - 识别需要迁移的数据库表、DTO、API、前端路由和 spec。
   - 保留项目预研期原则：以正确模型为目标，不设计长期兼容层。

5. 输出后续任务管理建议：
   - 判断是否需要拆为父任务与多个子任务。
   - 给出建议子任务清单、依赖顺序、主要涉及文件和验收口径。
   - 明确哪些内容只需 review / spec 更新，哪些进入实现任务。

## Acceptance Criteria

- [ ] `design.md` 明确 Story / LifecycleRun / ActivityAttempt / RuntimeSession / Agent permission system 的目标职责边界。
- [ ] `design.md` 明确 Story 与 LifecycleRun 的多对多关系，并通过显式 association/index 表达。
- [ ] `design.md` 列出当前实现与目标模型之间的主要张力，并用代码或 spec 位置作为证据。
- [ ] `coupling-assessment.md` 给出 Story 相关整体耦合地图，覆盖 Lifecycle、Session、Task、Routine、Capability、Workflow binding 中的冗余字段与关联。
- [ ] `coupling-assessment.md` 将冗余字段分类为 must move、projection/cache、needs decision，并说明目标职责。
- [ ] `agent-permission-system.md` 明确 Agent permission request / grant / permission cap / tool cap / CapabilityState 的分层关系。
- [ ] `design.md` 给出目标数据流：Routine 触发 LifecycleRun；巡检 Agent 凭 Project scope 工具完成 Story 创建、Story 管理权限申请、Task 创建与 companion/task 派发。
- [ ] `design.md` 列出可清理的冗余关联清单，并给出目标替代职责。
- [ ] `implement.md` 给出可执行的后续拆分路线，至少覆盖后端 domain/application/API、capability、frontend、spec/migration、workflow binding review 五类工作。
- [ ] `implement.md` 标注每个阶段的建议验证方式和风险点。
- [ ] 本任务在进入实现前，由开发者确认优先级和首个落地阶段。

## Scope Notes

- 本任务不直接重写 Story / Session / Lifecycle 代码。
- 本任务不引入新的 agent run 抽象；当前讨论中的 agent run 语义按 LifecycleRun 处理。
- WorkRun / StoryRun 暂不作为必选落地模型；只有当“业务轮次聚合多个 LifecycleRun”成为明确 UI/API 主语时再引入产品层索引。
- 本任务不要求保持既有 API / 数据库字段长期兼容；项目处于预研期，后续实现可通过 migration 直接收敛到正确模型。
- 本任务产出应避免记录一次性修补细节，重点记录目标边界、事实源和为什么这样组织。

## Open Questions

- 首个落地阶段应优先从“隐藏 Session 外露 API / 前端入口”开始，还是从“建立 run association + Agent permission request / grant 应用模型”开始。
- Story 下的 Task 是否继续作为 Story aggregate 内 child spec，还是逐步投影为 run activity view，需要在后续实现设计中结合查询与并发写入风险确定。
- `WorkflowBindingKind/binding_kinds` 是整体替换为 launch scope / subject requirements / capability contract，还是先保留 definition 级 catalog filter 并移除其权限和 runtime association 语义。
