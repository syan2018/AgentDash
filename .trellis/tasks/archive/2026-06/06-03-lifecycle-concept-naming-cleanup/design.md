# Lifecycle 概念与命名清理设计

## Architecture Boundary

本任务不改变运行行为，只把当前已确认的控制面语义沉淀为后续开发的不变量。

核心分层：

| 概念 | 角色 |
| --- | --- |
| `AgentProcedure` | 单个 Agent Activity 的行为、能力、上下文、hook 与 port 契约 |
| `WorkflowGraph` | 可执行 Activity DAG 定义 |
| `LifecycleRun` | 业务执行控制账本 |
| `WorkflowGraphInstance` | 一个 run 内的 graph 生效实例与 activity state namespace |
| `ActivityAttemptState` | `graph_instance_id + activity_key + attempt` 定位的一次 activity attempt |
| `ActivityExecutionClaim` | scheduler 对 attempt 的 durable claim |
| `LifecycleAgent` | run-scoped Agent 身份 |
| `AgentAssignment` | activity attempt 到 agent/frame 的执行绑定 |
| `AgentFrame` | Agent runtime surface revision |
| `RuntimeSession` | connector delivery / trace evidence |
| `RuntimeSessionExecutionAnchor` | runtime session 反查 lifecycle attempt 的权威索引 |
| `LifecycleSubjectAssociation` | subject 到 run/agent 的业务归属关联 |

## Frontend Boundary

前端需要维持三条清晰边界：

| 层 | 当前入口 | 边界 |
| --- | --- | --- |
| 定义态 | `workflowStore`, `services/workflow.ts`, workflow editor | 管理 `WorkflowGraph` definition；编辑 Agent Activity 时配套维护关联 `AgentProcedure` draft；不表达运行状态 |
| 运行态 | `lifecycleStore`, `services/lifecycle.ts`, lifecycle view API | 缓存 `LifecycleRunView`、`SubjectExecutionView`、`AgentFrameRuntimeView`、trace view |
| 会话视角 | `SessionPage`, `ActiveSessionList`, session services | 展示 runtime trace 和用户会话入口；不作为 lifecycle control state 的事实源 |

## Naming Cleanup Strategy

命名清理分三档：

1. **立即清理**：局部变量、注释、task/spec 文档中明显把 `RuntimeSession` 说成业务 owner 的表达；不会改变导出 API。
2. **计划性清理**：保留 `workflowStore` 作为 `WorkflowGraph` 定义态 store；只清理局部变量、注释或 selector 中把运行态 `LifecycleRun` / `RuntimeSession` 混入 WorkflowGraph 定义语义的表达。
3. **暂缓清理**：generated DTO、HTTP route、用户路由、数据库字段。原因是它们牵动跨层合同，需要单独任务和 contracts check。

## Frontend Cleanup Plan

### Immediate Low-Risk Cleanup

这些项只影响局部命名、注释或 UI label，不改变跨层合同：

| 区域 | 清理方向 |
| --- | --- |
| Workflow editor activity inspector | `procedure_key` 相关列表、draft、label 使用 Procedure / AgentProcedure 语义 |
| Lifecycle port sync helper | 从 `workflow*` 局部命名改为 `procedure*` / `activity*` 语义 |
| AgentProcedure injection panel | guidance 文案表达为 Agent / Procedure 指引，不把它归属于 RuntimeSession |
| Hook trigger labels | wire enum 不变，UI label 区分 runtime terminal 与 Lifecycle terminal |
| Lifecycle run/detail pages | 展示 `WorkflowGraphInstance` 时使用 graph instance / instance 语义 |
| Task / Story subject projection | latest attempt 文案补齐 Activity Attempt 与 graph instance namespace |

### Planned Cleanup

这些项需要额外任务，因为会触碰导出类型、跨 feature props 或合同生成：

| 区域 | 计划方向 |
| --- | --- |
| `WorkflowRun` / `WorkflowRunStatus` 前端别名 | 收敛到 `LifecycleRunView` / `LifecycleRunStatus` |
| `submitHumanDecision` 返回值 | 后端 route 与 generated contract 确认后返回完整 `LifecycleRunView` |
| `WorkflowTemplateWorkflow` / template `workflows` | 确认 Shared Library payload 后收敛为 procedure 语义 |
| `workflowDraftsByActivityKey` 公共 editor API | 计划性改为 `procedureDraftsByActivityKey` 并同步 editor tests |
| `sessionMetas` / `ActiveSessionList` / `SessionShortcutList` | 先确定产品层“用户会话”命名，再区分 RuntimeSession trace 与 conversation entry |
| `fetchSessionFrameRuntime` / workspace panel runtime hook | 收敛为 trace adapter 命名，避免 RuntimeSession 变成 frame owner |

## Spec Update Targets

- `backend/workflow/architecture.md`：补齐核心词汇表与 control-plane identity chain。
- `backend/workflow/activity-lifecycle.md`：强化 state source 和 runtime trace 边界。
- `backend/session/architecture.md`：强化 RuntimeSession 只做 trace/delivery，以及 anchor 反查路径。
- `frontend/architecture.md` / `frontend/state-management.md` / `frontend/workflow-activity-lifecycle.md`：补齐 definition/runtime/session UI 的边界。

## Trade-Offs

- 不在本任务做大规模 rename，原因是概念清理首先要稳定不变量；跨层 rename 需要合同生成、路由、UI 文案和测试一起收敛。
- 保留 `workflow` 作为前端 WorkflowGraph 定义态资产类目，原因是它准确表达 graph definition 入口；AgentProcedure draft 是 Agent Activity contract 的配套编辑数据，不改变 store 的主语义。
