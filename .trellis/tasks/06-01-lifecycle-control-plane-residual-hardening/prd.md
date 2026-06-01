# Lifecycle 控制面残存问题硬收口

## Goal

把 Lifecycle 控制面硬切后的残存问题收敛到一个可执行修复任务中。

本任务不继续维护旧 `Session` / binding / owner tree / route-local runtime shape 的兼容路径。相反，它采用更强硬的修法：凡是会让系统重新走回 session-first、Task runtime owner、route-local binding response、permission apply 空转、hook standalone runtime 的入口，都应删除、封闭或改成快速失败。快速失败产生的编译错误、测试失败、运行时报错就是本任务用来定位散落旧路径的主要反馈机制。

目标状态是：项目里所有业务入口、hook/runtime surface、permission/capability 变更、前端主导航与测试，都只能通过 `LifecycleRun -> LifecycleAgent -> AgentFrame -> RuntimeSession` 以及 `SubjectRef -> LifecycleSubjectAssociation -> AgentAssignment -> ActivityAttemptState` 这两条目标链路表达。

## Background

`06-01-lifecycle-control-plane-concept-alignment` 已完成目标模型讨论，并经过一次只读 subagent review。review 结论是：控制面主骨架已经落地，但并未彻底妥善。

已经完成或基本完成的方向：

- `LifecycleAgent` / `AgentFrame` / `AgentAssignment` / `LifecycleSubjectAssociation` / `LifecycleGate` 等事实源与 schema 已存在。
- `LifecycleRun.session_id`、`list_by_session`、`lifecycle_step_key` 等核心旧路由根基本删除。
- `ExecutionIntent` 已不以 `session_id` 为核心输入，ProjectAgent 和 Task 启动已走 `LifecycleDispatchService`。
- Scheduler / Agent launcher 已基本按 agent/frame/assignment 优先的顺序写入执行证据。
- 前端已有 `lifecycleStore`、`SubjectExecutionView`、Agent frame 页面、Task/Story Subject execution 面板。

残存风险集中在：hook/runtime surface、permission apply、Story open、Routine reuse、Companion association、session trace UI、migration backfill、测试覆盖。

## Current Review Result

2026-06-01 的第二轮 subagent 复核结论是：硬切工作推进明显，但不能宣称彻底完成。

已经硬收口的部分：

- `/sessions/{id}/bindings`、`project_sessions` 路由/DTO、前端 `fetchProjectSessions` / `ProjectSessionEntry` / `SessionBindingEntity` 等显式旧入口已删除。
- `WorkflowDefinitionSource` 已收敛为 `DefinitionSource`，contracts 与 generated frontend types 已同步。
- `find_running_by_executor_session` 已从 claim repository trait、Postgres 实现与测试 mock 中删除，active projection 不再依赖 executor session claim 反查。
- `POST /sessions`、`/sessions/{id}/prompt`、`/sessions/{id}/context`、`/sessions/{id}/hook-runtime` 已从 HTTP router 移除；仍调用这些旧业务入口的前端路径会快速失败。
- `sessions` route module 取代旧 `acp_sessions` 命名；session stream 只暴露为 `/sessions/{id}/stream/ndjson`。
- session route 权限检查必须经 `Session -> AgentFrame -> LifecycleAgent -> Project`，不再用 `SessionMeta.project_id` 当业务权限事实源。
- capability 热更在缺少 AgentFrame 或 frame revision 写入失败时直接失败，不再只更新内存缓存。

仍未完成的部分：

- `/session/:id` 前端仍使用 `SessionPage` / `SessionChatView` 体验模型，虽然旧写 API 会失败，但 UI 还没有拆成纯 trace drill-down。
- `fetchSessionContext` / `fetchSessionHookRuntime` 等前端服务函数仍存在，当前作为旧路径断路器暴露残留调用点并会抛出后端 404/405，后续应删除或改接 frame/trace view。
- `GET /sessions` 列表仍用 `SessionMeta.project_id` 做可见性过滤；如果继续保留 session list，也应改为 AgentFrame/LifecycleAgent 派生视图。
- `RoutineExecution.session_id` 与 routine history 仍把 session id 作为可见执行出口，尚未通过 run/agent/frame/subject view 表达。
- permission route-local DTO 与前端手写 permission type 仍未收敛到 contracts。
- Story freeform/manual open、Routine reuse、Companion gate/adoption、Task artifact truth、migration 数据命运仍需要继续实做与测试。

## Requirements

### R1. 收掉错误 API 与旧入口

- 删除或禁用 `/acp/sessions/{id}/bindings` 这类已无真实语义的 session binding API，不保留空数组 stub。
- 删除前端和 E2E 对 `SessionBindingEntity`、`sessionId ?? session_id`、`task.session_id` 等旧字段兼容读取。
- 业务入口不得通过通用 session create 隐式表达 Story / Task / Project / Routine ownership。
- 旧 API 被调用时应明确失败，而不是返回空结果让调用方误以为仍可用。

### R2. Hook / active workflow / runtime surface 硬切到 AgentFrame

- active workflow projection 不再通过 executor session claim 反查，而应使用 `RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState`。
- hook snapshot 与 hook runtime 以 `AgentFrame` / `LifecycleAgent` 为事实源；session 只作为 runtime trace lookup。
- 生产路径不得创建脱离持久化 frame 的 standalone hook runtime，除非是明确标记的测试 fixture。
- `StepActivation` / session construction 中仍然独立存在的 capability/context/VFS/MCP surface 应收敛为 `AgentFrame` revision 或 frame builder 内部细节。
- pending capability transition 与 runtime command 不得只以 `session_id` 作为控制面主键。

### R3. Permission approve/revoke 必须真正改变 AgentFrame

- approve / revoke API 不能只更新 grant 状态。
- 批准 grant 后必须产生可追溯的 AgentFrame revision 或等价 frame delta，并让 RuntimeSession 消费新的 delivery snapshot。
- ControlScope escalation 必须写入 `LifecycleSubjectAssociation`，并能解释 grant 来源、effect frame、source runtime trace。
- 删除 route-local permission DTO 或把它们收敛到 `agentdash-contracts`，避免前后端各自维护权限 shape。

### R4. Story / Routine / Companion 入口收敛

- Story root/freeform/manual open 必须通过 `LifecycleDispatchService` 创建或选择 Story `LifecycleAgent`，并建立 Story subject association。
- Story context injection 以 AgentFrame context slice 为事实源，legacy story context contributor 只能作为 frame builder 输入。
- Routine `Fresh` / `Reuse` / `PerEntity` policy 必须映射到明确的 agent reuse / run boundary 规则；`Reuse` 不应意外创建新 run。
- Routine dispatch 应携带足够的 ProjectAgent / AgentProfile 信息，让 RuntimeSession launch 能恢复目标 agent surface。
- Companion child dispatch 必须建立 child LifecycleAgent、AgentFrame inherited slice、durable gate、lineage、subject/control association；不能只靠 parent/child session context。

### R5. Task projection 与 artifact truth 收敛

- Task start/continue 只提交 `SubjectRef(kind=Task)` 与 execution intent。
- Task view 不能硬编码 active 状态，也不能依赖空 trace ref。
- Task artifacts/status 如果保留缓存，必须带 source run / agent / assignment / attempt / revision；否则应从 lifecycle facts 派生。
- 命令型 artifact 写入路径需要明确写入 Lifecycle artifact 或 projection cache，不得让 Task 重新成为 runtime truth owner。

### R6. 前端主模型从 session tree 彻底迁走

- 侧边栏主导航改用 lifecycle run / agent / subject indexes，不再按 `parent_session_id` 构建 session tree。
- `/session/:id` 只能是 `RuntimeSessionTraceView` drill-down，不再承载业务 runtime 主体验。
- `StorySessionInfo` / `ProjectSessionInfo` / `fetchProjectSessions` 迁移到 SubjectExecution / ProjectActiveAgents / AgentFrame runtime views。
- 前端命令路径不得回传 read view；write command 只使用 stable refs 与 intent。

### R7. Migration 与数据收敛

- 对当前开发库中可能存在的旧 lifecycle run/session/link 数据给出明确迁移方案。
- 如果确认当前阶段可清库或旧数据可丢弃，应在任务中明确记录原因，并用 migration / seed 策略体现。
- 不能留下 no-op backfill 与直接 drop 的组合而不解释数据命运。

### R8. 测试补齐

- 增加 `LifecycleAgent` / `AgentFrame` / `AgentAssignment` / `LifecycleSubjectAssociation` invariant 测试。
- 增加 terminal callback 到 `AgentFrame -> AgentAssignment -> ActivityAttemptState` 的端到端测试。
- 增加 permission grant approve/revoke 影响 AgentFrame revision 的测试。
- 增加 Story dispatch、Routine reuse、Companion durable gate resume/adoption 的测试。
- 增加前端 lifecycle store / AgentFrame panel / SubjectExecution panel / session trace drill-down 的测试。
- 更新 E2E，移除所有旧 session binding / task session 字段兼容断言。

## Hard Cutover Policy

本任务优先采用快速失败策略：

- 能删除的旧 API 直接删除。
- 必须暂留的底层 runtime/session API 需要改名或注释成 trace/debug substrate，不得表达 business owner。
- 对旧 command path 加 `unimplemented` / explicit error / compile break，比静默 fallback 更好。
- 禁止新增 compatibility adapter、legacy fallback、dual-write 双轨。
- 若旧路径仍被调用，让调用方失败，再沿失败栈修到目标链路。
- 删除 route-local DTO 优先于继续桥接字段。
- 允许 breaking migration；但必须处理开发数据库数据命运。

## Residual Issue Inventory

### P0

- Story open/freeform/manual session 仍未走 dispatch + Story Agent association。
- `/session/:id` 前端仍承载旧 SessionPage/SessionChatView 体验；旧写 API 已断开，但 UI 尚未成为纯 trace drill-down。
- RoutineExecution terminal view 仍以 session id 为主要可见执行出口。
- permission DTO/type 仍未收敛到 contracts。

### P1

- `StepActivation` 仍是独立 activation surface，尚未成为 AgentFrame activation input/delta。
- Companion dispatch 缺 subject/control association，inherited slice 写入点偏后。
- Routine reuse / ProjectAgent launch surface 未完全接稳。
- Task execution view / artifacts 仍有硬编码和命令型写入残留。
- migration/backfill 对旧 session/run/link 数据命运尚未形成可执行策略。

### P2

- Contract 命名存在 `LifecycleAgentView` / `AgentFrameRuntimeView` 与目标 DTO 清单不完全一致的问题，需要确认是否是有意命名。
- `SessionHookSnapshot` 作为 trace adapter 仍存在大量 deprecated usage，需要后续迁到 frame-native hook view。
- route-local permission DTO 与前端手写 permission type 未收敛。
- 新 lifecycle store/pages 的测试覆盖不足。

## Acceptance Criteria

- [x] 删除或封闭 session binding API；旧调用方失败并全部迁移。
- [x] `rg "list_by_session"`、`rg "lifecycle_step_key"`、`rg "SessionBinding"`、`rg "task.session_id"` 在源码/前端/E2E 中无控制面残留。
- [ ] Hook snapshot、active workflow projection、terminal callback 都能从 Session trace 解析到 AgentFrame / LifecycleAgent / AgentAssignment / ActivityAttemptState。
- [x] Permission approve/revoke 会产生 AgentFrame revision 或等价 frame delta，并有测试覆盖。
- [ ] Story root/freeform/manual open 通过 dispatch 建立 Story subject association。
- [ ] Routine reuse 不意外创建新 run，RoutineExecution terminal view 从 lifecycle/agent projection 派生。
- [ ] Companion wait/resume/adoption 使用 durable LifecycleGate，并建立 lineage + subject/control association。
- [ ] Task status/artifacts 来源可追溯到 SubjectRef / association / assignment / attempt / artifact。
- [ ] 前端主导航不再以 session tree 作为 runtime 模型；`/session/:id` 仅为 trace drill-down。
- [ ] Migration 明确处理旧数据：要么 backfill，要么清楚记录当前预研阶段的数据重置策略。
- [ ] Phase 8 关键测试补齐，并通过 `pnpm run contracts:check`、后端相关测试、前端相关测试。

## Out Of Scope

- 不重新讨论 Lifecycle / Workflow / AgentFrame 的概念命名，除非发现命名残留正在阻碍硬切。
- 不引入兼容旧 API 的桥接层。
- 不新增独立抽象来包住旧路径；新增对象必须拥有事实源、不变量、查询边界、生命周期或外部依赖隔离。

## Source Review

本任务来自 2026-06-01 对 `06-01-lifecycle-control-plane-concept-alignment` 的 subagent review 汇总。父任务只作为目标状态和证据来源；本任务负责实际硬收口。
