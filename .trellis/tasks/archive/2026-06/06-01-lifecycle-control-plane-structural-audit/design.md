# 结构性修复设计

## Goal

本设计把 `structural-analysis.md` 中逐项问题收束为少数稳定封装。目标不是再造更多转发层，而是让每个新增边界拥有明确事实源、不变量、查询边界或事务边界。

## Core Architecture Invariants

1. RuntimeSession 是 trace / transport substrate，不是 control-plane command target。
2. LifecycleRun 是 tracked life process，不直接 owns graph activity_state。
3. WorkflowGraphInstance owns Activity execution state。
4. LifecycleAgent owns agent runtime identity。
5. AgentFrame owns effective runtime surface revision。
6. AgentAssignment owns Agent -> ActivityAttempt execution evidence bridge。
7. LifecycleSubjectAssociation owns SubjectRef -> run / agent relation。
8. Read models 只能聚合事实；commands 不能回传 read model 再写事实源。
9. 所有 business execution ingress 都必须通过 typed intent，不能 route-local 构造 run/session/frame。
10. 所有 generated contract 覆盖 lifecycle / subject / agent / frame 关键 command result。

## Boundary 1: ActivityRuntimeAssociationResolver

### Responsibility

把 runtime terminal / tool advance / callback provenance 解析成 lifecycle execution evidence。

### Owns

- runtime session ref 到 assignment / graph instance / attempt 的解析规则。
- frame revision fallback 规则。
- terminal event 无法定位时的 domain error。

### Does Not Own

- capability updates。
- hook execution。
- RuntimeSession event stream。

### Required API Shape

```text
resolve_terminal(runtime_session_ref, turn_ref?)
  -> ActivityRuntimeAssociation {
       run_ref,
       graph_instance_ref,
       lifecycle_agent_ref,
       assignment_ref,
       activity_key,
       attempt,
       launch_frame_ref,
       current_frame_ref
     }
```

## Boundary 2: WorkflowGraphInstanceRuntime

### Responsibility

执行 Workflow graph instance 的 Activity state transitions。

### Owns

- activity_state。
- active activity cursor。
- ActivityEvent application。
- graph-scoped scheduler view。

### Does Not Own

- run-level subject associations。
- agent runtime surface。
- runtime session launch。

## Boundary 3: Execution Dispatch Taxonomy

### Responsibility

把业务入口变成清楚的 typed execution intent。

### Intent Families

- `LifecycleRunStartIntent`: 创建 tracked life process + graph instance。
- `AgentLaunchIntent`: 创建 / 复用 LifecycleAgent + AgentFrame + optional RuntimeSession。
- `SubjectExecutionIntent`: 指定 SubjectRef 并解析到 Activity/Agent/Assignment。
- `InteractionDispatchIntent`: 创建 CompanionChannel / LifecycleGate / optional child agent。

### Result Families

- `RunStarted`
- `AgentLaunched`
- `SubjectExecutionScheduled`
- `SubjectExecutionAssigned`
- `InteractionGateOpened`

不再允许一个全 optional `ExecutionDispatchResult` 假装覆盖所有状态。

## Boundary 4: WorkflowGraphResolver

### Responsibility

解析 graph definition identity。它是 catalog/config boundary，不属于 runtime creation。

### Rules

- `ByKey` 必须解析到 existing workflow graph。
- `ById` 必须校验 project / scope。
- Freeform 必须显式使用 `InlineFreeform` 或 equivalent。
- runtime 层不允许自己生成 definition id。

## Boundary 5: AgentFrameSurfaceService

### Responsibility

生成和更新 AgentFrame revision。

### Owns

- effective capability。
- context projection。
- VFS / MCP / canvas surface。
- procedure binding。
- runtime refs。
- hook policy snapshot。

### Does Not Own

- connector event stream。
- business subject truth。
- ActivityAttempt terminal result。

### Important Split

```text
AgentFrameTransition     -> changes frame truth
RuntimeDeliveryCommand   -> delivers a frame revision to RuntimeSession
```

## Boundary 6: ProjectActiveAgentsViewBuilder

### Responsibility

提供 project-scoped runtime overview。

### Owns

- project filter。
- active lifecycle agents。
- subject associations。
- frames and gates。
- runtime trace refs。

### Does Not Own

- frontend route state。
- direct session tree ownership。

## Boundary 7: SubjectExecutionContract

### Responsibility

稳定表达 Task / Story / RoutineExecution / External subject 的 execution command 和 read projection。

### Required Shapes

- `SubjectRefDto`
- `SubjectExecutionRequest`
- `SubjectExecutionDispatchResult`
- `SubjectExecutionView`

Task-specific routes may remain as convenience wrappers, but they must use generated DTOs and common dispatch.

## Boundary 8: RuntimeTraceView

### Responsibility

保留 RuntimeSession 调试价值，但防止它重新成为业务入口。

### Rules

- `/session/:id` only shows trace, transcript, turn/tool events, lineage。
- It can link back to AgentFrame / SubjectExecution。
- It cannot assemble LifecycleRunView independently。

## Rollout Principle

先引入边界和不变量，再删除旧路径：

1. Resolver / graph state / dispatch taxonomy。
2. Frame transition split。
3. Business ingress migration。
4. Read model consolidation。
5. Naming cleanup。
6. Schema and E2E invariant checks。

这能避免“旧字段删了，但旧耦合换了个名字继续存在”。
