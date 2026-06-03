# Design: Lifecycle 控制面重构收口

## Target Thin Architecture

当前项目的核心薄架构应该让业务事实锚定在 lifecycle control plane，而 runtime session 只保留 delivery / adapter trace 语义：

```text
Write side:
  ExecutionIntent
    -> LifecycleDispatchService
    -> LifecycleRun + LifecycleAgent + AgentFrame + AgentAssignment + LifecycleGate
    -> FrameConstructionService
    -> FrameLaunchEnvelope
    -> RuntimeSessionTurnRuntime

Return side:
  RuntimeSession delivery id
    -> RuntimeSessionExecutionAnchor
    -> AgentAssignment / launch AgentFrame evidence
    -> WorkflowGraphInstance activity state

Read side:
  LifecycleRunView + LifecycleAgentView + AgentFrameRuntimeView + SubjectExecutionView
    -> frontend agent/frame-first store
```

RuntimeSession 的职责是承载 connector delivery identity、terminal adapter provenance、trace metadata。它不再负责推导 owner、workspace、capability、activity、assignment 或 frontend primary subject。

## Current Completion Assessment

这次 review 认为当前状态是“主体结构已迁移，但收口未完成”：

- `RuntimeLaunchRequest` 删除和 `FrameLaunchEnvelope` 主链路已经是正确方向。
- `FrameConstructionService` 下沉后，API 层 compose 逻辑的厚度明显降低。
- `RuntimeSessionExecutionAnchor` 的表和 repository 已有雏形，但 resolver 还没有以它作为直接证据。
- `RuntimeContextInspectionPlan` / `ResolvedSessionOwner` 被 deprecated，而不是被删除或测试隔离。
- 前端已引入 `delivery_runtime_ref`，但仍保留 `primarySessionId` 与 `runtime_trace_refs[0]` fallback。

因此重构不能被视为妥善完成，只能视为进入 final closeout 阶段。

## Boundary Decisions

### RuntimeSessionExecutionAnchor

Anchor 是 runtime delivery session 到 lifecycle control plane 的唯一窄桥。

它应该保存足够稳定的事实：

- `runtime_session_id`
- `run_id`
- `agent_id`
- `launch_frame_id`
- `assignment_id` when activity-bound
- `graph_instance_id`
- `activity_key`
- `attempt`

Resolver 的优先级应是：

1. 按 `runtime_session_id` 查 anchor。
2. 若有 `assignment_id`，直接取 assignment 并校验 run / agent / frame 关系。
3. 若无 assignment，但有 `launch_frame_id`，按 frame 判断这是 non-activity runtime 还是待绑定 runtime。
4. JSON contains 只作为迁移前数据查询能力；当前预研项目没有兼容压力时，可以删除或压到明确的 legacy audit path。

### FrameConstructionService

`FrameConstructionService` 是 frame facts 到 launch envelope 的唯一 compose 入口。它可以接收 launch command 与 runtime trace state，但不应接收完整 `SessionMeta` 作为事实袋。

建议引入更窄的输入：

```text
RuntimeTraceLaunchState:
  title / timestamps / existing runtime marker / connector resume id
```

executor config、capability、workspace、context slice 继续来自 `AgentFrame` typed surface 或 subject/lifecycle association。

### Deprecated Construction Types

`RuntimeContextInspectionPlan` 和 `ResolvedSessionOwner` 当前仍 public 暴露，因此会继续吸引新代码误用。

收口方式：

- production module 不导出旧类型。
- test fixture 需要的构造器迁到 `#[cfg(test)]` test support。
- `construction_use_case` 若只用于 audit trace，应从 production module tree 删除；如果仍有价值，应改名为 test/support fixture。

### Frontend Read Model

前端主语应是 agent/frame/run，而不是 primary session。

建议派生路径：

```text
run_id
  -> agentsByRun(run_id)
  -> primary LifecycleAgentView
  -> delivery_runtime_ref.runtime_session_id
  -> session trace meta only for title/status fallback
```

`runtime_trace_refs` 可以作为 run 的 trace 列表保留，但不承担 primary selection。

### Port Output Scope

activity 输出属于 activity attempt，不属于 run 全局 keyspace。compose 阶段如果缺 attempt，应先调整 assignment/attempt 创建时序，或者在 activation input 中携带 `ActivityPortArtifactRef`，再调用 `load_scoped_port_output_map`。

## Verification Shape

收口完成的定义不是“引用量下降”，而是这些命令和扫描能证明架构边界已经稳定：

```bash
cargo test -p agentdash-domain --lib -- --format terse
cargo check --workspace
pnpm --filter app-web run typecheck
rg "RuntimeContextInspectionPlan|ResolvedSessionOwner" crates/agentdash-application/src --type rust
rg "runtime_trace_refs\\[0\\]|primarySessionId" packages/app-web/src
rg "load_port_output_map" crates/agentdash-application/src/session crates/agentdash-application/src/workflow
```

## Relationship To Existing Tasks

This task is a child of `06-02-lifecycle-control-plane-final-convergence`. It narrows that broad final convergence plan to the concrete blockers found in the current branch review.

Existing sibling/related task tree `06-02-lifecycle-control-plane-frame-convergence` already decomposes several work areas. This task can either absorb those child tasks as execution references or act as the final review gate after they are implemented.
