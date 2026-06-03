# Session Runtime 控制面事实源收敛

## Goal

将用户面向的 Session 定义为可理解的消息流壳，将业务执行事实完整收敛到 Lifecycle 控制面。标准链路是：

```text
Session shell / runtime_session_id
  -> RuntimeSessionExecutionAnchor
  -> LifecycleRun
  -> LifecycleAgent
  -> current AgentFrame runtime surface
  -> SessionRuntimeControlView
  -> Session 页面 / WorkspacePanel / 会话列表
```

这项重构的用户价值是让 Session 列表、Session 页面、继续发送消息、运行详情入口与 WorkspacePanel context 共享同一份后端控制面投影。用户仍以 Session 作为一级概念；Lifecycle、AgentFrame、assignment、attempt 是内部控制面与详情页概念。

## Requirements

- 当前异常路径一律视为大重构残余，不能作为设计约束；本任务以目标控制面事实源为准。
- `RuntimeSessionExecutionAnchor` 是 runtime session 到业务控制面的唯一权威索引。
- `sessions` 只保存用户消息壳状态：project、title、title source、事件游标、turn、delivery/stream 状态、tab layout 与时间字段。
- `sessions` 不保存 run、agent、frame、assignment、activity 或 lifecycle status 事实。
- `LifecycleRun` 表达业务执行账本；`LifecycleAgent` 表达 run-scoped agent 身份；`AgentFrame` 表达当前 agent runtime surface revision。
- `AgentFrame` runtime refs 只能作为 runtime surface 投影的一部分；业务归属与会话列表不得从 frame JSON refs 推断。
- `LifecycleSubjectAssociation` 只表达 subject 与 run/agent 的业务归属，不通过 Session title、Session meta 或 trace 内容反推 subject。
- 后端必须提供标准 Session 控制面入口 `GET /sessions/{runtime_session_id}/runtime-control`。
- 后端必须提供项目会话列表入口 `GET /projects/{project_id}/sessions`，让前端列表直接消费用户会话投影。
- Session 页面继续发送用户消息必须经 LifecycleAgent 控制面入口解析 anchor、run、agent、frame 后投递到 runtime session。
- 前端 `/session/:runtime_session_id` 主入口必须消费 `SessionRuntimeControlView`，不再从 lifecycle store、route state 或 session meta 自行拼控制面事实。
- WorkspacePanel 接收单一 control projection，不再接收 `LifecycleRunView[]` 表达当前 Session 的运行状态。
- 会话列表标题只来自后端 session title；缺失标题时显示稳定占位文案。
- `/run/:runId` 与 `/agent/:agentId` 保持详情页职责；Session 页面只提供入口和 compact context。

## Acceptance Criteria

- [ ] 数据库 schema 与 repository 查询以 `runtime_session_execution_anchors` 支撑 runtime session、run、agent、frame 的控制面解析。
- [ ] Project Agent launch、Story/Task execution、LifecycleAgent message continuation、freeform project session 创建都会写入或更新 anchor。
- [ ] assignment 创建后会回填 anchor 的 assignment / graph instance / activity / attempt 证据。
- [ ] `LifecycleRunView.runtime_trace_refs` 与 `LifecycleAgentView.delivery_runtime_ref` 由 anchor read model 投影。
- [ ] `GET /sessions/{runtime_session_id}/runtime-control` 返回 session shell、anchor、run、agent、frame runtime、subject associations、send readiness。
- [ ] `GET /projects/{project_id}/sessions` 返回会话列表所需 title、delivery status、run status、run/agent/frame refs、subject label 与更新时间。
- [ ] Session 页面使用 runtime-control 作为标准请求入口。
- [ ] WorkspacePanel runtime data 删除 `lifecycleRuns: LifecycleRunView[]`，改为单一 lifecycle/control target。
- [ ] Context overview 使用单个 `LifecycleRunView` 展示 run status、attempt 与 progress。
- [ ] 侧边栏会话列表和 Agent 页活跃会话列表消费同一个项目会话列表投影。
- [ ] 会话列表不会把 agent role、agent kind 或 route state 当作会话标题。
- [ ] 无 anchor 的 runtime session 显示不可发送/不可解析状态，不能伪装成业务会话。
- [ ] 相关后端 targeted tests、前端 targeted tests、前端 type-check 通过。

## Out Of Scope

- 不新增 Lifecycle 作为用户一级导航概念。
- 不重做完整运行详情页信息架构。
- 不追求旧 API、旧数据库字段或旧前端 fallback 的兼容。
