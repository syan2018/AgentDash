# Lifecycle 控制面长链路收敛与 Frame 化 Design

## Architecture Intent

目标架构以 Frame 作为 runtime session 事实解析与投影的主边界。Session 层只处理一次 turn 的运行监督，不继续解析业务 owner、上下文、capability、VFS、MCP、workflow activity 等控制面事实。Activity attempt 的执行事实由 `AgentAssignment` 锚定，runtime session 只作为 delivery / trace container。

## Target Invariants

- `AgentFrame` 是 connector launch 的 surface snapshot；每个 revision 都能独立说明本轮运行的 capability、context、VFS、MCP、execution profile 与 runtime delivery refs。
- `LifecycleAgent.current_frame_id`、frame repository current revision、runtime session delivery frame 三者必须有一个明确的权威规则；调用方不得混用多个 current 概念。
- `AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)` 是 Activity attempt 的执行锚点。
- `RuntimeSession` 只保存运行轨迹和投递引用；业务反查必须立即落到 frame / assignment anchor。
- lifecycle artifacts 必须按 `graph_instance_id + activity_key + attempt + port_key` 定位；run 级 artifact 聚合只是 read model。
- `WorkflowGraphInstance.activity_state` 是 Activity runtime state 的事实源；run-level active projection 只能是派生视图。
- `FrameLaunchEnvelope` 必须是 launch-ready 的非半成品结构，Session planner 不再负责补齐 owner/context/capability 基础事实。

## Proposed Boundary Split

### Frame Construction Layer

Frame construction 接收 subject / agent / workflow / command intent / pending frame transition，输出新的 `AgentFrame` revision 与 `FrameLaunchEnvelope`。

职责：

- 解析 owner scope 与 subject association。
- 解析 context bundle / context slice。
- 解析 capability state、VFS、MCP servers、skill baseline、execution profile。
- replay pending `AgentFrameTransitionRecord`，生成 effective runtime surface。
- 选择 runtime delivery policy，并生成 runtime session refs 或 delivery command。
- 产出 launch-ready envelope：working directory、executor config、typed capability/VFS/MCP、prompt blocks、identity、terminal effect binding、hook runtime source facts。

推荐类型拆分：

```text
FrameLaunchIntent
  -> AgentFrameConstructionPlan / FrameConstructionResult
  -> FrameLaunchEnvelope
  -> ConnectorLaunchInput / ExecutionContextProjection
```

其中 `FrameLaunchIntent` 只表达 source intent；`FrameConstructionResult` 负责持久化 frame revision；`FrameLaunchEnvelope` 是 Session launch 的唯一输入；`ConnectorLaunchInput` 只面向 connector projection。

### Session Runtime Layer

Session runtime 消费 `FrameLaunchEnvelope`，只管理 turn lifecycle。

职责：

- claim / activate / cancel / cleanup active turn。
- 组装 connector-facing `ExecutionContext`。
- 处理 connector accepted boundary。
- commit turn events 与 bootstrap meta。
- attach stream processor / adapter。
- persist terminal event，释放 active turn，写入 terminal effect outbox。
- 管理 backend execution lease 的 claim / release / fail。

Session runtime 不再：

- 从 RuntimeSession meta 重新推导 owner / workspace / VFS / MCP。
- 从 session id 遍历 frame / agent / assignment 猜测业务归属。
- 在 planner 中补齐 frame surface 的 missing fields。

## Data Flow

### Agent Activity Launch

```text
WorkflowGraphInstance activity_state
  -> ActivityExecutionClaim
  -> AgentActivityExecutorLauncher
  -> LifecycleAgent
  -> AgentFrame revision
  -> AgentAssignment
  -> RuntimeSession delivery ref
  -> FrameLaunchEnvelope
  -> Session turn runtime
```

### Terminal Callback

```text
RuntimeSession terminal
  -> runtime_session_execution_anchor
  -> AgentAssignment
  -> ActivityEvent
  -> LifecycleEngine
  -> WorkflowGraphInstance.activity_state
```

若无需新增表，也应提供 repository 级 direct query，避免 `list_by_run + select_assignment_for_runtime_frame` 成为业务路径。

### Lifecycle Output Artifact

```text
lifecycle_vfs artifacts/{port_key}
  -> graph_instance_id / activity_key / attempt from active mount context
  -> scoped port artifact
  -> completion policy / hook gate / artifact binding
  -> ActivityOutputArtifact
```

run-level artifact view 从 scoped artifact 聚合生成。

### Frontend Runtime Query

```text
session_id
  -> GET /sessions/{runtime_session_id}/frame-runtime
  -> AgentFrameRuntimeView
  -> lifecycleStore frames/runtimeTraces
  -> WorkspacePanel runtime state
```

前端 store 不承担 session-to-frame 事实推断。

## Migration Notes

- 项目处于预研期，可以通过数据库 migration 直接重塑表结构或清理旧列。
- 旧 `port_outputs/{port_key}` 数据如需保留，仅在 migration 中迁入 scoped path；运行时代码不保留兼容读取。
- `RuntimeLaunchRequest` 可通过新类型替换，而不是长期维持 optional bag。
- API / TS contract 可以同步改动，不需要维护旧字段兼容。

## Validation Strategy

- Domain / application unit tests：assignment anchor、frame current invariant、scoped artifact key、launch envelope required fields。
- Integration tests：activity session terminal 直接推进 activity attempt；多 graph instance 同名 port 不冲突。
- Frontend tests：session runtime hook 不使用本地 fallback；frame runtime view 按后端返回渲染。
- Contract check：Rust contracts 与 generated TS 保持同步。
