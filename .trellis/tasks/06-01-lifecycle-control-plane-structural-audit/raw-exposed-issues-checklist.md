# 原始暴露问题 Checklist

## Purpose

本文只记录本轮 review 暴露出的原始问题，不提前合并、不淡化、不把它们改写成机械 TODO。后续 `structural-analysis.md` 会按这里的顺序逐项分析。

## Checklist

- [ ] P0-01 Terminal event 通过 runtime session 反查 assignment / attempt 时，只匹配最新 `AgentFrame` 的 `frame_id`，frame revision 变化后可能找不到原 `AgentAssignment`。
- [ ] P0-02 `LifecycleRun` 仍直接持有并推进单个 `activity_state`；`WorkflowGraphInstance` 虽存在，但还不是 engine / scheduler / orchestrator 的主状态事实源。
- [ ] P0-03 `LifecycleDispatchService` 主路径不创建 `AgentAssignment`，`ExecutionDispatchResult.assignment_ref` 固定为空，SubjectRef 到 ActivityAttemptState 的证据链没有闭合。
- [ ] P0-04 `WorkflowGraphRef::ByKey` 在 dispatch 内没有真正解析，ProjectAgent / default lifecycle 可能传入 key 后仍创建随机 graph / lifecycle id。
- [ ] P0-05 manual lifecycle run 仍通过 `ActivityLifecycleRunService::start_run` 直接创建 run，没有进入统一 dispatch / intent / association / frame 入口。
- [ ] P0-06 Story root / freeform launch 未发现统一通过 `ExecutionIntent` 创建 Story subject association 和 root LifecycleAgent 的路径。
- [ ] P1-07 runtime commands 仍是 `session_runtime_commands(session_id, phase_node)`，没有 frame / agent / assignment 化。
- [ ] P1-08 Hook runtime 已改为 `AgentFrameHookRuntime`，但入口仍是 `session_id -> find frame`；capability 热更新服务 API 仍以 session 为命令目标。
- [ ] P1-09 `StepActivation` 仍是独立 activation DTO，并且存在 apply-to-running-session 路径，没有完全收束进 `AgentFrameBuilder` / frame revision delta。
- [ ] P1-10 `ContinueRoot` 仍以 root RuntimeSession 为控制条件，而不是 Agent / Assignment / Frame 复用策略。
- [ ] P1-11 `RuntimeLaunchRequest::from_frame` 面对多 RuntimeSession refs 时只取第一个，runtime ref selection 没有一等策略。
- [ ] P1-12 Project active agents 没有后端 `ProjectActiveAgentsView` / project-scoped projection，前端从全局 lifecycle store 拼装。
- [ ] P1-13 `ActiveLifecycleList` 接收 `projectId` 但未用于过滤，可能跨项目展示 runs / agents。
- [ ] P1-14 `/session/:id` 虽标注为 RuntimeTraceView，但实现仍混用 session feed / meta / projection / lineage，trace 页面仍可能回扩成控制面入口。
- [ ] P1-15 Task execution API response 仍是 route-local 手写 DTO，未进入 generated contracts；前端 start/continue 后丢弃 dispatch response 再 fetch task。
- [ ] P1-16 `story_runs` route 自建 `LifecycleRunView` 时 `runtime_trace_refs` 为空，而通用 lifecycle view 会从 AgentFrame 收集 trace refs。
- [ ] P1-17 `ExecutorRunRef::RuntimeSession { session_id }` 仍裸露在 workflow contracts / generated TS 中，尚未完全转为 `RuntimeSessionRefDto` 语义。
- [ ] P1-18 Task 仍保留 `agent_binding` 作为 spec / execution preference，并参与 executor config 决策。
- [ ] P1-19 Task cancel 仍通过 current frame 找 runtime session 后调用 `cancel_session`，取消语义没有明确落在 Agent / Gate / Assignment。
- [ ] P1-20 Companion sub dispatch 已接入 dispatch / gate，但 parent notification、human wait 和轮询仍依赖 runtime session notification / hook runtime。
- [ ] P1-21 Routine `Reuse` 策略名义上映射为 agent reuse，但缺少 anchor lookup；无 parent_run_id 时仍会新建 run。
- [ ] P1-22 PermissionGrant 已有 run provenance + effect frame anchor，但仍携带 source runtime session；需要明确 provenance 与 effect owner 的边界，避免 session 再次成为 permission 查询根。
- [ ] P2-23 shared-library 仍接受 `entry_step_key / steps / edges` legacy template normalization，和预研阶段硬切目标冲突。
- [ ] P2-24 `WorkflowContract` 仍作为单 Agent `AgentProcedure.contract` 的类型和 UI 文案，graph Workflow 与 AgentProcedure contract 命名仍有残留混淆。
- [ ] P2-25 schema readiness 只检查目标表存在，不检查关键列、旧列删除、索引/约束完整性。
- [ ] P2-26 E2E 仍大量用 `/session/:id` 和 `/sessions/:id/stream/ndjson` 验证 Task / Story runtime，Subject / Agent / Frame view 的验证不足。
- [ ] P2-27 `ProjectActiveAgentsView` 没有 contract / API / service / test，Project active runtime 不是稳定边界对象。
- [ ] P2-28 同一 DTO 家族在不同 route 的填充完整度不一致：通用 lifecycle view 与 story-specific view 对 trace refs / agents / subject associations 的聚合不一致。
- [ ] P2-29 `WorkflowContract`、shared-library legacy conversion、session trace E2E 等残留说明“旧词消失”不等于“新模型高内聚”，需要重新定义命名与契约 owner。
- [ ] P2-30 当前 check 结果未实际运行 `pnpm run check`，无法证明 runtime chain、contracts、frontend projection 和 E2E 都通过。
