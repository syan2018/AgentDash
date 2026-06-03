# Frontend Actor Subject Views 设计

## 目标

将前端从 session-first run grouping（`runsBySessionId`、`SessionGroupNode`、`ProjectSessionEntry`）迁到 subject / agent / lifecycle / runtime trace 视图体系。UI 仍支持进入运行详情，但不把 Session 当作业务控制面主轴。

## 蓝图阶段

推进 `target-state-blueprint.md` B6 Contracts And Frontend Views。

## 存量结构分析

### Frontend 当前结构

| 存量结构 | 当前作用 | 问题 |
| --- | --- | --- |
| `workflow-contracts.ts` → `StoryRunOverviewDto` | run view，包含 `session_id?`、`lifecycle_id` 作为唯一 graph pointer | `session_id` 作为可选但实际依赖的主键；`lifecycle_id` 隐含单 graph |
| `workflow-contracts.ts` → `ExecutorRunRef::AgentSession` | attempt 到 session 的直接引用 | session 作为 executor evidence root |
| `workflow-contracts.ts` → `EffectiveSessionContract.active_step_key` | 表示当前 workflow 节点 | step vocabulary |
| `session-grouping.ts` → `groupSessionsByStory()` | 按 owner_type(story/task/project) + parent_session_id 分组 | session tree 作为业务导航根 |
| `session-grouping.ts` → `SessionGroupNode` | story/task/orphan/project 节点，含 linkedChildren | session 分组节点，session 作为 UI 树 identity |
| `ProjectSessionEntry` type | `session_id`, `owner_type`, `owner_id`, `story_id`, `parent_session_id` | session-first owner model |
| `/session/:id` route | session 详情页 | 业务运行根入口 |
| `runsBySessionId` (implied store pattern) | 以 session_id 为 key 的 run lookup | session-first lifecycle 索引 |
| `SessionBindingResponse` | API 返回的 session-owner binding | binding vocabulary |

### 目标 View 体系

| View | 来源 | 用途 |
| --- | --- | --- |
| `LifecycleRunView` | `LifecycleRun` + `WorkflowGraphInstance[]` + `LifecycleAgent[]` + links | run 详情，展示多 graph instance / agents |
| `SubjectExecutionView` | `LifecycleSubjectAssociation` + projection | 业务对象（Story/Task/Routine）的执行概览 |
| `AgentFrameRuntimeView` | `AgentFrame` + capability + context + runtime refs | agent 运行面详情 |
| `ProjectActiveAgentsView` | project scope 下 active `LifecycleAgent` 聚合 | 项目活跃 agent 面板（替代 session owner tree） |
| `RuntimeTraceView` | `RuntimeSession` + events + projection + lineage | session 降级为 trace/debug 视图 |

### 目标 Store 归一化

```text
stores/
  lifecycleRuns:        Map<run_id, LifecycleRunView>
  graphInstances:       Map<graph_instance_id, WorkflowGraphInstanceView>
  lifecycleAgents:      Map<agent_id, LifecycleAgentView>
  agentFrames:          Map<frame_id, AgentFrameRuntimeView>
  subjectExecutions:    Map<subject_kind+subject_id, SubjectExecutionView>
  runtimeTraces:        Map<runtime_session_id, RuntimeTraceView>
```

## 迁移决策表

| 存量 | 决策 | 目标 |
| --- | --- | --- |
| `StoryRunOverviewDto` | 替换为 `LifecycleRunView`：删除 `session_id?`；`lifecycle_id` → `graph_instances[]`；新增 `agents[]` | generated contract 更新 |
| `ExecutorRunRef::AgentSession.session_id` | 降级为 `AgentAssignment` evidence 下的 trace ref | `AgentAssignmentView.trace_ref` |
| `EffectiveSessionContract.active_step_key` | 替换为 `active_activity_key` + `AgentProcedure` 引用 | activity vocabulary |
| `groupSessionsByStory()` | 替换为 `groupExecutionsBySubject()`：按 SubjectRef 分组 | `SubjectExecutionView` 树 |
| `SessionGroupNode` | 替换为 `SubjectExecutionNode { kind, subject, agents[], children[] }` | 不再以 session 为节点 identity |
| `ProjectSessionEntry` | 替换为 `ProjectActiveAgentEntry { agent_id, run_id, subject_ref, frame_ref, trace_ref? }` | agent-first 列表 |
| `/session/:id` route | 降级为 `/trace/:id`（`RuntimeTraceView`）；业务入口改为 `/run/:id`、`/agent/:id`、`/subject/:kind/:id` | session 不是业务根入口 |
| `runsBySessionId` store | 删除；替换为 `lifecycleRuns` Map | run-first 索引 |
| `SessionBindingResponse` | 替换为 `SubjectExecutionView` / `LifecycleSubjectAssociationDto` | 删除 binding vocabulary |
| Story sessions panel | 替换为 Story subject runs / agents panel | 从 subject view 进入 |
| Task session panel | 替换为 Task execution view（从 `TaskProjection`） | subject view |
| Project sessions panel | 替换为 `ProjectActiveAgentsView` | agent-first |

## 导航层级重建

```text
目标导航结构：

Project page
  └── ProjectActiveAgentsView（活跃 agent 面板）
        ├── LifecycleAgentView → AgentFrameRuntimeView → RuntimeTraceView
        └── SubjectExecutionView(Story/Task)

Story page
  └── SubjectExecutionView(kind=Story)
        ├── LifecycleRunView (含多个 graph instances)
        │     ├── WorkflowGraphInstanceView
        │     └── LifecycleAgentView[]
        └── Task SubjectExecutionView[] (child tasks)

Task page
  └── SubjectExecutionView(kind=Task)
        ├── TaskProjection (status, artifacts, current agent)
        └── RuntimeTraceView (drill-down)

/trace/:session_id (旧 /session/:id)
  └── RuntimeTraceView
        ├── Events / turns / tools
        ├── Projection / compaction
        └── Lineage (trace-only)
```

## 不变量

- `runsBySessionId` 不再是 workflow run 主 store。
- 前端 state 支持 `run → workflowGraphInstances → activities/attempts`。
- `/session/:id` 是 `RuntimeTraceView`，不是业务运行根入口。
- generated contracts 中 nullable `session_id` 不被前端当作必填业务主键。
- read view 数据不作为 command input 回传。

## 断裂点

- Project / Story / Task 页面在迁移期间可能丢失导航（session 树被移除，subject view 尚未完全接通）。
- `workflow-contracts.ts` 的 generated types 会有大量变化，需要同步 backend contracts generator。
- 前端 store 归一化需要一次性迁移，中间状态可能导致 UI 闪烁或数据不一致。
