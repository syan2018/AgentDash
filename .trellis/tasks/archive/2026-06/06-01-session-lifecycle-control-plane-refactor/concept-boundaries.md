# 新概念职责边界

## 目的

本文定义本轮重构新增概念的职责、边界和腐化信号。目标是防止重构变成重命名：每个新概念必须拥有明确事实、不可替代的不变量、查询边界或生命周期；如果一个概念只是在转发旧字段，它应被合并、删除或降级为 private helper。

## 边界规则

新增概念必须同时满足至少两项：

- 拥有独立事实源。
- 拥有不可替代的不变量。
- 拥有稳定查询边界。
- 拥有独立生命周期。
- 能隔离外部依赖或 runtime substrate。

不满足这些条件的新增层，不应成为 public domain / contract / repository 概念。

## 概念矩阵

| 概念 | 拥有的事实 | 不拥有的事实 | 必须保持的不变量 | 腐化信号 |
| --- | --- | --- | --- | --- |
| `LifecycleRun` | 被追踪的执行生命过程、lifecycle-level events、gates、artifacts/ports/VFS exchange、agent set、graph instance set | `WorkflowGraph` definition、RuntimeSession event stream、业务对象数据、Agent effective surface | 可以包含多个 `WorkflowGraphInstance`；没有 `session_id`；对外没有唯一 graph 指针 | 代码向 run 查询 session owner，假设一个 run 只有一个 graph，或把业务 subject 存成 run 字段 |
| `WorkflowGraph` | Activity graph definition、activities、transitions、ports、artifact binding、executor slots | Runtime state、subject association、agent identity、session trace | 纯定义；没有 run status；没有 runtime refs | Graph config 开始携带 session id、subject id、active attempt 或 agent status |
| `WorkflowGraphInstance` | 某张 graph 在一个 `LifecycleRun` 内的生效实例、graph role、activity state namespace、attempt namespace | 独立生命周期边界、graph definition 本体、RuntimeSession stream | 状态 key 是 `run_id + graph_instance_id + activity_key`；root graph 只是 `role=root` | 只因为“它是子图”就创建 child run；状态只按 `activity_key` 定位 |
| `Activity` | WorkflowGraph node、executor kind、ports、completion policy、retry/iteration policy | Task business spec、Agent runtime identity、session trace | Activity 身份由 graph definition 限定；运行身份由 graph instance 限定 | Activity 变成 Task、tool call 或用户普通动作的同义词 |
| `ActivityAttemptState` | 某次 activity attempt 的执行证据：status、timestamps、terminal summary、outputs | Subject ownership、Agent identity、capability/context surface | Attempt key 是 `graph_instance_id + activity_key + attempt`；Agent 只能通过 assignment 找到 | Attempt 存 current agent surface、permission scope 或 subject owner |
| `LifecycleAgent` | Run-scoped Agent runtime identity、role、status、lineage、current frame pointer | ProjectAgent profile、RuntimeSession log、业务 subject data、procedure body | 只属于一个 LifecycleRun；可以有多个 frame 和 runtime refs | 代码把 ProjectAgent 配置或 RuntimeSession id 当成 Agent 本体 |
| `AgentFrame` | Effective runtime surface revision：AgentProcedure、capability、context slice、VFS、MCP、runtime refs、revision provenance | Business truth、完整 runtime transcript、LifecycleRun activity state | Frame 有 revision；runtime launch 从 frame 投影；permission/context/capability 变化产生 frame delta/revision | capability/context/VFS/MCP 继续以 SessionMeta、HookSessionRuntime 或 live maps 为权威 |
| `AgentAssignment` | LifecycleAgent/AgentFrame 到 graph activity attempt 的桥接、assignment lease/provenance | Attempt terminal result、subject association、runtime transcript | 引用 `run_id + graph_instance_id + activity_key + attempt`；attempt status 仍归 ActivityAttemptState | Scheduler 或 terminal callback 继续按 session 路由 |
| `AgentProcedure` | 单个 Agent Activity 的 behavior/capability/context/hook/completion contract | 整张 graph topology、run state、agent instance identity | 替代当前 `WorkflowDefinition`；由 AgentFrame / Activity executor 引用 | Procedure 开始承载 activity 间 transition 或 lifecycle run state |
| `LifecycleSubjectAssociation` | SubjectRef 到 whole run 或 LifecycleAgent 的关系，包含 role/source/projection/control/lineage metadata | Activity evidence、attempt status、RuntimeSession trace | Anchor 只能是 run 或 LifecycleAgent；Activity/Attempt 证据来自 assignment/artifact/event | 出现新的 ActivitySubjectLink，或 Task 被保存为 runtime owner |
| `SubjectRef` | runtime 可携带的业务对象 kind/id 稳定引用 | 业务对象内容、runtime state、projection cache | Runtime 携带 subject ref，不携带 Task/Story aggregate body | Task/Story entity 开始拥有 executor/session/runtime state |
| `LifecycleGate` | human/platform/companion/permission 等 durable wait/review/resume 点 | Session metadata、完整 permission grant truth、runtime transcript | Gate 可跨进程重启恢复，并能恢复 agent/frame/run context | wait/adoption 仍停在 in-memory registry 或 SessionMeta companion context |
| `RuntimeSession` | Turn/tool/event stream、connector resume、debug replay、compaction/projection provenance、trace lineage | Business ownership、permission scope、Lifecycle progress、Agent effective surface | 只能用于 trace 和 delivery；反查必须走 frame/agent/run | 产品 API 需要 session id 才能找 owner、active lifecycle、permission scope 或 agent state |
| `RuntimeTraceView` | RuntimeSession 的 UI/debug projection | Command input、business state、lifecycle truth | 只读 projection；不能作为 command payload 回传 | UI 把 trace view 数据写回控制面 command |
| `TaskProjection` | 从 lifecycle facts 派生的用户可见 status/artifact cache | Task spec、runtime truth | 若持久化，必须带 source run/agent/graph/activity/attempt/revision refs | Task status/artifacts 被当成 primary runtime state 更新 |
| `AgentLineage` | Agent spawn/delegation/companion relation | RuntimeSession fork lineage、subject association | 控制树使用 AgentLineage；session lineage 只作为 debug trace | 用 session lineage 推断 companion/agent ownership |
| `PermissionGrant` | Permission decision 与 audit provenance；effect 落到 AgentFrame revision | Runtime command delivery、业务 subject data | Source 可包含 runtime session/turn/tool；effect 必须标识 frame/run/scope | active grant 主要按 session id 查询 |

## 跨概念不变量

- `LifecycleRun` 永远不拥有 `RuntimeSession`；`AgentFrame` 只把 runtime session 当作 trace/delivery refs。
- `LifecycleRun` 对外永远不承诺只拥有一张 graph；root graph 只是 `WorkflowGraphInstance(role=root)`。
- `WorkflowGraph` 永远不拥有 runtime state；`WorkflowGraphInstance` 拥有 graph-local runtime namespace。
- `AgentProcedure` 永远不拥有 graph topology；它只是单个 Agent Activity 的 contract。
- `ActivityAttemptState` 永远不拥有 subject 或 agent identity；这些关系来自 `LifecycleSubjectAssociation` 和 `AgentAssignment`。
- `RuntimeSession` 永远不解释 business ownership；反向 trace lookup 是 `RuntimeSession -> AgentFrame -> LifecycleAgent -> LifecycleRun -> LifecycleSubjectAssociation`。
- Read view 永远不是 command input。Command 使用 `ExecutionIntent`、`SubjectRef`、run/graph/agent/frame/assignment refs。

## 评审清单

接受任何实现切片前都要检查：

- 每个新 public type 是否真的拥有事实，而不是只给旧字段换名？
- 是否有旧 shortcut 变成了新概念上的字段？
- 是否能在不做 session-first lookup 的情况下回答目标谓词句？

```text
LifecycleAgent A in LifecycleRun R
acts on SubjectRef S,
uses AgentFrame F,
is assigned to WorkflowGraphInstance G / Activity X / Attempt N,
can see capabilities/context from F,
and emits RuntimeSession RS only as trace.
```

如果答案需要 `SessionBinding`、`LifecycleRun.session_id`、`Task.lifecycle_step_key`、`owner_type`、`binding_id`、`active_step_key` 或 top-level `WorkflowRun.session_id`，实现已经偏移。
