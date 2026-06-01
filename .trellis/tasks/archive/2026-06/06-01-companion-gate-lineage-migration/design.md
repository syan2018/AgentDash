# Companion Gate Lineage 迁移设计

## 目标

把 companion wait/adoption/parent-child lineage 从 `SessionMeta.companion_context`、in-memory wait registry 迁到 durable `LifecycleGate`、`LifecycleAgent`、`AgentFrame`、`AgentLineage`。companion 默认作为 same-run agent/graph，只有独立生命周期边界成立时才创建 linked run。

## 蓝图阶段

推进 `target-state-blueprint.md` B5 Business Subject Migration。

## 存量结构分析

### Companion 域当前结构

| 存量结构 | 当前作用 | 问题 |
| --- | --- | --- |
| `CompanionRequestTool` | companion_request tool 实现 | 通过 `current_session_id` 路由，依赖 `ExecutionContext.session` |
| `CompanionSessionRef.session_id` | companion child session 引用 | session 作为 companion 业务 identity |
| `CompanionSessionContext` (session module) | companion parent-child context slice | 存储在 `SessionMeta` / session construction 中 |
| `CompanionLaunchSource` / `CompanionLaunchWorkflowSource` | companion session 启动来源标记 | 通过 `LaunchCommand` 传递，session-first |
| `SessionMeta.companion_context` | companion wait/slice/adoption/parent 引用 | session metadata 作为 companion 控制面事实源 |
| `CompanionAdoptionMode` | suggestion / follow_up_required / blocking_review | 决定 companion 结果采纳方式 |
| `HookPendingAction` (companion_result trigger) | companion 结果到 hook pending action 桥接 | 在 session hook runtime 中，依赖 session scope |
| `SessionLineageRecord` (companion kind) | companion parent-child session 关系 | session lineage 作为 agent ownership 推断源 |
| In-memory companion wait (hook runtime) | companion wait → resume 的同步桥接 | 进程重启丢失；不可靠 |

### 目标链路

```text
Companion dispatch:
  Parent LifecycleAgent(in parent run)
    -> ExecutionIntent(parent_agent_id, gate_policy=companion_wait, agent_policy=spawn_child)
    -> LifecycleDispatchService
    -> same-run: 新建 WorkflowGraphInstance(role=companion_review) + LifecycleAgent(child)
    -> AgentLineage(parent_agent_id → child_agent_id)
    -> LifecycleGate(kind=companion_wait, correlation=gate_id, agent_ref=parent)
    -> Child AgentFrame + RuntimeSession

Companion resume:
  Child LifecycleAgent terminal / adoption
    -> LifecycleGate.resolve(payload=adoption_result)
    -> Parent LifecycleAgent frame resumes
    -> HookPendingAction / AgentFrame event

Companion ownership query:
  LifecycleAgent(child)
    -> AgentLineage(parent=parent_agent_id)
    -> LifecycleSubjectAssociation(anchor_agent_id=child)
    -> SubjectRef
```

## 迁移决策表

| 存量 | 决策 | 目标 |
| --- | --- | --- |
| `SessionMeta.companion_context` | 删除；companion wait/slice 迁入 `LifecycleGate` + `AgentFrame.context_slice` | `LifecycleGate(kind=companion_wait)` |
| `CompanionSessionRef.session_id` | 替换为 `CompanionAgentRef { agent_id, frame_id, run_id }` | companion child 以 agent 身份存在 |
| `CompanionLaunchSource` | 替换为 `ExecutionIntent.parent_agent_id` + `gate_policy` | dispatch service 处理 companion 启动 |
| `CompanionSessionContext` | 拆为 `AgentFrame.context_slice`（child inherits parent slice）+ `AgentLineage` | context 归 frame；关系归 lineage |
| In-memory companion wait | 替换为 durable `LifecycleGate`；gate resolution 写入持久化 | 进程重启可恢复 |
| `HookPendingAction`(companion_result) | gate resolution → `AgentFrameEvent` → hook pending action bridge | hook 从 gate/frame event 读取 |
| `SessionLineageRecord(companion)` | 保留为 `RuntimeSessionLineage`（trace）；agent 关系写入 `AgentLineage` | session lineage 不推断 ownership |
| `CompanionAdoptionMode` | 保留枚举；adoption 语义映射到 gate resolution policy | suggestion → auto_resolve；blocking → manual_resolve |
| `CompanionRequestTool.current_session_id` | 替换为 agent_id / frame_id 路由 | 通过 frame 获取 runtime session ref |
| companion child session 创建 | 由 dispatch service 创建 child `LifecycleAgent` + `AgentFrame` + `RuntimeSession` | session 只是 trace |

## Same-run vs Linked-run 判定

Companion 默认作为 same-run `WorkflowGraphInstance(role=companion_review)` + `LifecycleAgent`(child)：

- 共享 parent run 的 lifecycle-level artifact/event/port exchange。
- 共享 parent run 的 gate surface。
- 不需要独立的生命周期管理。

只有以下情况升级为 linked run：

- companion 需要独立权限/控制边界（不同 project scope）。
- companion 需要独立导航管理（用户可独立查看/取消/恢复）。
- companion 跨越不同 lifecycle context（不同业务对象的长期投影）。

## 不变量

- `SessionMeta.companion_context` 不是 companion 控制面事实源。
- Companion wait/resume 在进程重启后可从 `LifecycleGate` 恢复。
- Companion child 的业务归属通过 `AgentLineage` + `LifecycleSubjectAssociation` 查询，不从 session lineage 推断。
- Companion graph 不因"是子图"而自动创建 child `LifecycleRun`。
- `RuntimeSessionLineage` 只用于 trace/debug/fork 视图。

## 断裂点

- `CompanionRequestTool` 在迁移期间可能无法正确发起 companion request，直到 dispatch 和 gate 路径接通。
- `companion_result` hook trigger 在 gate 路径接通前可能无法触发。
- 前端 companion 面板在 `frontend-actor-subject-views` 任务中迁移。
