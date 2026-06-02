# Lifecycle 控制面残存问题硬收口设计

## Purpose

把残留的 session-first 路径转换为快速失败和目标链路实现。本设计倾向破坏性收口，不保留兼容双轨。

## Strategy

### 1. 先拆旧出口

优先删除或显式禁用这些旧出口：

- session binding API。
- 前端 session tree 主导航。
- E2E 对旧 session field 的兼容读取。
- route-local binding / permission DTO。
- hook standalone runtime 的生产入口。

这样可以让编译、类型检查、E2E 直接暴露仍依赖旧路径的调用点。

### 2. 再修目标链路

每个失败点必须迁到目标谓词：

```text
RuntimeSession
  -> AgentFrame
  -> LifecycleAgent
  -> AgentAssignment
  -> ActivityAttemptState

SubjectRef
  -> LifecycleSubjectAssociation
  -> LifecycleAgent
  -> AgentAssignment
  -> ActivityAttemptState / LifecycleArtifact
```

旧路径不能用 adapter 重新包起来；如果旧路径还被调用，说明调用方本身要改。

### 3. 最小允许保留的 Session 语义

`RuntimeSession` 只允许表达：

- turn / event log / tool call / resume / debug trace。
- connector transport / cancellation / replay。
- drill-down trace UI。

它不允许表达：

- Story / Task / Project owner。
- permission effect surface。
- active lifecycle ownership。
- Task execution truth。
- hook/capability 当前事实源。

## Target Chain By Area

| Area | Required chain |
| --- | --- |
| terminal callback | `RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState` |
| active workflow projection | `AgentFrame/Assignment -> Activity state` |
| permission grant | `Grant source -> effect AgentFrame revision -> RuntimeSession delivery snapshot` |
| Story open | `SubjectRef(story) -> dispatch -> LifecycleAgent + AgentFrame + association` |
| Task execution | `SubjectRef(task) -> association -> assignment -> attempt/artifacts` |
| Companion | `parent LifecycleAgent -> LifecycleGate + lineage + child LifecycleAgent + AgentFrame` |
| Routine | `RoutineExecution SubjectRef -> dispatch policy -> run/agent projection` |
| Frontend nav | `ProjectActiveAgents / SubjectExecution / LifecycleRunView` |

## Fast-Failure Moves

These changes are intentionally forceful:

- Remove API routes whose only behavior is legacy shape preservation.
- Remove generated or hand-written frontend types that name old session ownership.
- Replace silent fallback with explicit error when an old path has no target-chain equivalent.
- Make tests fail loudly when they rely on `session_id` as business runtime root.
- Collapse thin compatibility services after callers are migrated.

## Implementation Guardrails

- Do not add `Legacy*`, `Compat*`, or fallback modules.
- Do not preserve old response shapes with empty data.
- Do not make read views command inputs.
- Do not let `ActivityAttemptState` become subject anchor.
- Do not let `RuntimeSession` resolve business ownership directly.
- Do not introduce a new abstraction unless it owns truth, invariants, lifecycle, query boundary, or external dependency isolation.

## Verification Shape

The task is complete only when old calls fail at compile/test time or have been migrated. A green run is meaningful only after the old API/type surfaces have been removed, not while compatibility stubs still hide usage.
