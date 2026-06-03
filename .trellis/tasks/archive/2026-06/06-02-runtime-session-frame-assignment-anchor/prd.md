# RuntimeSession 到 Frame 与 Assignment 的直接锚定

## Goal

把 RuntimeSession terminal、trace、activity advance 等入口从启发式反查收敛到直接的 Frame / Assignment anchor。目标状态是：给定 `runtime_session_id` 可以直接得到 `AgentFrameRef` 与当前执行的 `AgentAssignmentRef`，并由 assignment 精确定位 `run_id + graph_instance_id + activity_key + attempt`。

## User Value

- Activity terminal 推进更可靠，不受 reused agent、多 frame revision、多 active assignment 场景影响。
- RuntimeSession 保持 trace / delivery container 角色，不重新承担业务 ownership。
- 后续前端 runtime query 和 Session launch 收敛可以复用同一套 frame anchor。

## Confirmed Facts

- 当前 `resolve_activity_session_association` 通过 `find_by_runtime_session -> lifecycle_agent -> list_by_run -> select_assignment_for_runtime_frame -> lifecycle_run` 反查 Activity attempt。
- `select_assignment_for_runtime_frame` 存在多级 fallback：优先 frame id，其次 frame 的 graph/activity，最后 agent 下唯一 active assignment。
- `AgentAssignment` 已经保存 `run_id + graph_instance_id + activity_key + attempt + agent_id + frame_id`。
- `AgentFrame.runtime_session_refs_json` 当前用于 delivery / provenance refs，并可通过 repository 查询 runtime session 所属 frame。
- ContinueRoot / reused runtime session 路径会创建新的 assignment，但同一 runtime session 可能连续承接多个 activity attempt；若不写 per-turn / per-assignment anchor，仍会回到猜测当前 assignment。

## Requirements

- 建立 runtime session 到 frame / assignment 的直接锚定查询，返回结构化 anchor，而不是返回 frame 后再扫描 run 级 assignments。
- 优先评估新增 `runtime_session_execution_anchors` 或等价 repository，而不是继续依赖 `runtime_session_refs_json` 作为 Activity terminal 权威锚点。
- terminal callback 与 `complete_lifecycle_node` 必须消费该 anchor 并直接构造 `ActivityEvent`。
- direct anchor 必须拒绝含混场景；不能用“某 agent 只有一个 active assignment”这种业务 fallback。
- anchor 查询应暴露 frame ref、assignment ref、run ref、graph instance ref、activity key、attempt、runtime session ref。
- ContinueRoot / reused runtime session 的 anchor key 必须明确是否包含 `turn_id` 或 active/terminal 状态。
- 普通 freeform session 或非 activity runtime session 查询时可以返回 no activity anchor，但应仍能返回 frame runtime anchor。
- 相关 error message 应说明缺的是 frame anchor、assignment anchor，还是 runtime session 不属于 activity attempt。

## Acceptance Criteria

- [ ] `runtime_session_id -> frame` 与 `runtime_session_id -> active assignment` 有直接 service/repository 入口。
- [ ] `LifecycleOrchestrator.on_session_terminal` 不再调用 run 级 `list_by_run` 来选择 assignment。
- [ ] `advance_current_activity` / `complete_lifecycle_node` 不再依赖启发式 assignment fallback。
- [ ] 多 assignment、多 frame revision、agent reuse 场景下，anchor 查询要么精确命中，要么给出明确拒绝。
- [ ] ContinueRoot / reused runtime session 场景不会通过 single-active fallback 选择 assignment。
- [ ] API trace / runtime-frame 查询能够复用同一 anchor 结果。
- [ ] 后端测试覆盖 activity session、freeform session、missing assignment、ambiguous delivery frame。

## Out Of Scope

- 不在本任务中重构 `RuntimeLaunchRequest`。
- 不在本任务中处理 lifecycle artifact scope。
- 不把 RuntimeSession 提升为业务 owner。

## Dependency Notes

- 本任务应优先于 `frontend-session-runtime-frame-query`，因为前端 endpoint 应复用这里建立的后端 anchor。
- 本任务可以独立于 scoped artifacts 实施。
