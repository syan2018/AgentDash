# Story / Lifecycle / Session / Agent 权限目标状态

## Intent

本轮架构收敛的目标不是修补某条 Story session 链路，而是把 Story 周边的事实源彻底归位：

- 业务事实归 Story / Task spec。
- 运行事实归 LifecycleRun / Activity / Attempt。
- 权限事实归 AgentPermissionRequest / AgentPermissionGrant。
- 工具面归 CapabilityState / ToolCapability runtime projection。
- runtime 日志归 RuntimeSession。
- 跨对象关系归显式 association，并通过 role / scope / subject 类型解释关系语义。

核心判断：后续任何 Story、Routine、Workflow、Companion、Agent 自动化 feature 都按事实类型选择模型：业务对象用业务模型，运行对象用 run/activity/attempt，权限状态用 AgentPermission，工具可见性用 CapabilityResolver。

## Current Model

当前实现大致是：

```text
Story
  可以独立创建，POST /stories 不创建 session 或 LifecycleRun

Story runtime
  spec 仍按 Story-as-durable-session 描述
  Story root session 通过 SessionBinding(owner_type=Story, label=companion) 表达

LifecycleRun
  core 持有 session_id
  历史 migration 已从 binding_kind + binding_id 迁到 session_id
  run 当前跟 session 走，不直接跟 Story / Task 走

Task start
  task_id -> story_id
  -> Story companion SessionBinding
  -> lifecycle_run_repo.list_by_session(session_id)
  -> Task.lifecycle_step_key
  -> launch session

Workflow binding
  workflow/lifecycle definition 用 binding_kinds = project | story 做挂载范围
  Task owner 映射成 Story binding

Capability
  tool visibility 由 SessionOwnerType + allowed_owner_types 硬切

Routine
  RoutineExecution 可记录 session_id
  Routine executor 当前偏 session prompt dispatch，而不是 LifecycleRun-first
```

这个模型的问题不是某一个字段错了，而是多个字段被同时用来表达业务归属、运行关联、权限事实、runtime 日志定位和产品导航。

## Target Fact Boundaries

目标状态按事实类型分层：

```text
Story
  业务工作单元 / 用户可见上下文壳

Task
  Story child spec / 用户可见工作项投影

LifecycleRun
  lifecycle definition 的一次运行实例

Activity / Attempt
  运行中的节点与执行尝试

RuntimeSession
  Agent turn / event / tool / connector resume / debug replay 日志

AgentPermissionRequest / AgentPermissionGrant
  Agent 权限申请与授权事实

CapabilityState / ToolCapability
  当前 runtime 可见工具面的投影

LifecycleRunLink / association
  LifecycleRun 与 Story / RoutineExecution / Task / Project / external subject 的显式关系
```

每层只做自己的事。产品层不再以 session 为主语，权限层不再以 session owner type 为事实源，运行层不再以 Story 外键或 root session 表达业务归属。

## Story Target

Story 是薄业务壳：

- 持有标题、描述、上下文、优先级、标签、摘要。
- 持有 Task specs 与 Task view projections。
- 通过 association 查询相关 LifecycleRuns。
- 通过 active AgentPermissionGrant + runtime association 查询有权限的 Agent 会话。
- 对外提供 Story 业务入口，把动作转给权限系统、run association、Activity dispatch 或 Story service。

Story 不负责：

- 创建 RuntimeSession。
- 判断 Agent 是否有权限。
- 持有 LifecycleRun。
- 持有 runtime truth。
- 注入工具。
- 派发 companion session。
- 通过 companion session 证明自己可运行。

Story 页面最终应展示：

- Story 业务信息。
- Task specs / task view。
- related LifecycleRuns。
- active / past Activity attempts。
- pending Agent permission requests。
- authorized Agent sessions。
- RuntimeSession trace drill-down。

## LifecycleRun Target

LifecycleRun 是运行实例：

```text
LifecycleRun
- id
- project_id
- lifecycle_id
- status
- activity_state
- execution_log
- created_at / updated_at / last_activity_at
```

LifecycleRun core 字段聚焦：

```text
definition_id
status
current_activity_key
activity_state
execution_log
timestamps
```

LifecycleRun 与业务对象的关系通过显式 association 表达：

```text
LifecycleRunLink
- run_id
- subject_kind: story | project | routine_execution | task | external | lifecycle_run
- subject_id
- role: source | subject | projection_target | control_scope | spawned_by
- metadata
- created_at
```

典型含义：

- `source`: run 的触发来源，例如 RoutineExecution、manual command、另一个 run。
- `subject`: run 正在处理的对象，例如 Project、Story、外部实体。
- `projection_target`: run 输出投影到哪里，例如 Story view、Task view。
- `control_scope`: run 内 Agent 可申请管理权限的 scope。
- `spawned_by`: 父 run / activity lineage。

这样一个 LifecycleRun 可以影响多个 Story，一个 Story 也可以聚合多个 LifecycleRun。

## RuntimeSession Target

Session 降级为 runtime substrate：

```text
RuntimeSession
- event log
- turn / tool call stream
- connector resume state
- compaction projection
- runtime command / effect
- debug replay
- trace drill-down
```

RuntimeSession 与业务对象的关系服务 context/debug/timeline；权限和产品导航分别来自 AgentPermission 与业务对象索引。

`SessionBinding` 最终只保留 runtime association 语义：

```text
SessionBinding
  RuntimeSession -> context/debug metadata
```

业务控制面的事实源迁移到：

- Story runtime/readiness service。
- LifecycleRunLink / run association。
- AgentPermissionGrant。
- Story / Project / Task 业务索引。

ActivityAttempt 与 RuntimeSession 的关系应下沉为 attempt runtime association。只有某次 Agent/tool 执行需要日志、resume 或 replay 时，才创建或关联 RuntimeSession。

## Agent Permission System Target

Story 权限不单独建一套小系统。目标是一套 Agent permission system。

核心链路：

```text
Agent / ProjectAgent assignment
  -> base permission caps / can_request caps
  -> Agent 自己申请权限
  -> policy / platform broker / human approval
  -> AgentPermissionRequest
  -> AgentPermissionGrant
  -> permission cap compiler
  -> ToolCapability / tool filters / MCP servers / VFS access
  -> RuntimeCapabilityTransition
  -> CapabilityState replay
  -> tool schema delta / runtime tool hot update
```

### AgentPermissionRequest

Request 表达“Agent 想要什么”：

```text
AgentPermissionRequest
- id
- project_id
- requester_agent_id
- requester_session_id / lifecycle_run_id
- source_kind: tool_request | lifecycle_policy | human_action | platform_broker | routine_run
- target_scope_kind: project | story | task | lifecycle_run | backend | workspace | mcp_server
- target_scope_id
- requested_permission_caps
- requested_tool_paths
- reason
- risk_level
- requested_ttl / expires_at
- status: created | pending_policy | pending_approval | approved | rejected | cancelled | expired
- policy_decision
- approval_ref
- created_at / updated_at
```

### AgentPermissionGrant

Grant 表达“Agent 实际拥有什么权限”：

```text
AgentPermissionGrant
- id
- request_id
- project_id
- agent_id
- scope_kind
- scope_id
- permission_caps
- tool_capabilities
- tool_filters
- mcp_servers
- status: active | revoked | expired | superseded
- granted_by_kind: policy | user | system | platform_broker
- granted_by_id
- effective_at
- expires_at
- revoked_at
- audit_reason
```

Grant 是权限事实源。`CapabilityState` 只是运行时工具投影。

### Permission Cap To Tool Cap

Permission cap 是高层授权语义，例如：

- `project.story.create`
- `story.manage`
- `story.task.dispatch`
- `workflow.lifecycle.modify`
- `backend.workspace.prepare`
- `mcp.server.use:{server}`

Permission cap 经过 compiler 解析为：

- `ToolCapability` key，例如 `story_management`、`task_management`、`workflow_management`。
- tool path allow / deny，例如 `story_management::create_story`。
- MCP server visibility。
- VFS / mount access。
- runtime policy / approval requirements。

因此权限系统回答“谁为什么有权限”，Capability runtime 回答“当前有哪些工具可用”。

## Agent Permission Request Flow

Agent 可以主动申请权限：

```text
Agent wants more capability
  -> companion capability_grant_request or permission request tool
  -> platform broker
  -> AgentPermissionRequest
  -> policy decision
  -> human approval if needed
  -> AgentPermissionGrant
  -> RuntimeCapabilityTransition
  -> CapabilityState replay
  -> tool schema delta
```

审批/策略需要支持：

- 自动批准低风险权限。
- 人工审批高风险权限。
- platform broker 自动拒绝不可能的权限。
- 部分批准 requested paths。
- TTL / lease。
- revoke / expire 后反向收缩 CapabilityState。

这个方向复用既有设计：

- `05-26-companion-interaction-capability-grant` 的 capability grant request 链路。
- `05-26-companion-interaction-persistence-model` 对 durable interaction / approval / permission grant 事实源的讨论。
- `05-17-backend-capability-expansion-governance` 中 capability request、policy decision、TTL、revoke、ack 的治理经验。

## Routine Inspection Story Flow

巡检 Agent case 的目标链路：

```text
Routine trigger
  -> create / start LifecycleRun for inspection workflow
  -> inspection Agent runs inside LifecycleRun
  -> Agent finds issue
  -> Agent uses story management tool
       tool availability comes from AgentPermissionGrant or base permission
  -> create Story
  -> create LifecycleRunLink records
       source = RoutineExecution
       projection_target = Story
       control_scope = Story
  -> if Agent needs Story-scope management:
       create AgentPermissionRequest(scope_kind=story)
       policy / approval
       AgentPermissionGrant(scope_kind=story)
  -> grant compiles to story/task/companion tool caps
  -> Agent creates Task specs under Story
  -> dispatch task/companion through lifecycle/activity services
  -> Activity attempts create RuntimeSession only for runtime logs
```

关键约束：

- Routine 负责触发 LifecycleRun。
- LifecycleRun 与 Story 通过 LifecycleRunLink 建立关联。
- Story 创建能力来自 Agent permission system。
- Story scope 管理能力来自 scoped AgentPermissionGrant。
- Story 页面通过 active grants + runtime associations 查询有权限的 Agent 会话。

## Workflow Binding Target

`WorkflowBindingKind/binding_kinds` 收敛为早期 catalog filter；权限、scope、subject 与 runtime association 进入独立模型。

目标拆分：

```text
definition catalog filter
  workflow/lifecycle 大致适用于哪些入口

launch scope
  本次启动发生在哪个 scope

subject requirements
  lifecycle 需要哪些 subject，例如 Story、Project、ExternalEntity

capability contract
  lifecycle 需要哪些 permission caps / tool caps
```

短期 `binding_kinds` 可以作为 catalog filter；权限、scope、subject 与 runtime association 由上面的目标模型表达。

## Task / Step Target

Task 继续是 Story child，不升级成全局 aggregate。

```text
Task
  Story child spec / user-visible work item

Activity / Attempt
  lifecycle runtime execution unit

Task -> Activity relation
  through run/activity association or projection
```

`Task.lifecycle_step_key` 是待清理耦合点。目标上研究删除或迁移，不继续保留 Step 绑定语义。`Task.status` / `artifacts` 可以作为 Story task view projection，但 runtime truth 在 LifecycleRun / Activity state。

## API Target

产品 API 围绕业务和运行主语：

```text
GET  /stories/{story_id}
GET  /stories/{story_id}/runs
GET  /lifecycle-runs/{run_id}
GET  /lifecycle-runs/{run_id}/timeline
POST /lifecycle-runs/{run_id}/activities/{activity_key}/dispatch
POST /agent-permission-requests
POST /agent-permission-requests/{id}/approve
POST /agent-permission-requests/{id}/reject
POST /agent-permission-grants/{id}/revoke
```

Runtime/debug API 才暴露 session：

```text
GET /runtime/sessions/{session_id}
GET /runtime/sessions/{session_id}/events
GET /lifecycle-runs/{run_id}/attempts/{attempt_id}/session
```

`/stories/{id}/sessions`、`/tasks/{id}/session`、`/lifecycle-runs/by-session/{session_id}` 这类 session-first API 应迁出产品主路径，最终降级为 debug 或删除。

## Implementation Order

推荐拆解顺序：

1. `story-coupling-inventory`
   - 把 P0/P1 耦合项映射到代码路径、schema、API。

2. `run-association-cleanup`
   - 新增 LifecycleRunLink，替代通过 session 反查 Story / RoutineExecution / Task。

3. `agent-permission-system`
   - 建 AgentPermissionRequest / AgentPermissionGrant、permission cap value objects、permission cap compiler。

4. `capability-permission-resolver`
   - CapabilityResolver 从 SessionOwnerType 迁向 Agent grant / permission cap / lifecycle contract。

5. `story-tool-and-permission-flow`
   - Story create / manage / task dispatch 走 Agent permission system 和受控 service。

6. `session-api-demotion`
   - story/task/lifecycle 的 session-first API 降为 runtime/debug 或删除。

7. `workflow-binding-model-review`
   - binding_kinds 降级为 catalog filter，拆出 launch scope / subject requirements / capability contract。

8. `task-step-decoupling`
   - 研究删除 Task.lifecycle_step_key，把 Task 与 Activity 的关系迁到 association/projection。

## Definition Of Done

达到目标状态时，应满足：

- Story 可独立于 Session 创建、查询、展示和管理。
- LifecycleRun 与 Story 多对多，通过显式 association 表达。
- LifecycleRun core 不持 Story 外键、不持 permission grant 外键、不以 root session 表达业务 identity。
- RuntimeSession 只作为 attempt/runtime/debug 资源出现。
- Agent 权限有统一 request / grant / approval / revoke / expire 事实源。
- Permission cap 可解释地编译成 tool cap、MCP、VFS 与 runtime CapabilityState。
- Agent 可以主动申请权限，审批后工具面可 live update 或 next-turn apply。
- Story 页面能查询有权限的 Agent 会话，核心事实来自 AgentPermissionGrant。
- Task 的运行事实通过 run/activity association 绑定。
- Workflow binding 收敛为 catalog filter，权限和 runtime association 进入独立模型。

完成后，巡检 Agent 自动创建 Story、多 Agent Story 协作、Story timeline、Workflow subject/capability contract、临时工具授权、权限审批与撤销都能在同一套事实源上自然展开。
