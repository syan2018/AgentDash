# Lifecycle 控制面残存问题硬收口实施计划

## Phase 1: Break Legacy Session Ownership

- 删除 `/acp/sessions/{id}/bindings` stub 和对应前端/E2E 类型。
- 删除 Task/session 兼容字段读取。
- 将 `/session/:id` 明确降级为 runtime trace route，业务入口改走 agent/subject routes。
- 首轮运行 contracts/frontend type check，记录所有爆出的旧路径。

## Phase 2: Runtime Surface Hard Cutover

- 将 active workflow projection 改为 frame/assignment lookup。
- 将 hook snapshot/runtime production path 改为 frame-backed。
- 将 pending capability transition / runtime command 迁到 frame-aware key。
- 确保 frame revision 更新保留 runtime session refs。

## Phase 3: Permission And Capability Revision

- 让 approve/revoke API 调用 application service，而不是只改状态。
- grant approve 产出 AgentFrame revision / frame delta。
- revoke 同步产生可追溯 effect。
- 补 permission frame revision 测试。

## Phase 4: Business Entrypoints

- Story open/freeform/manual session 改 dispatch。
- Routine reuse policy 修到明确 run/agent boundary。
- Companion dispatch 补 subject/control association 与 inherited slice。
- Task projection 补 trace/status/artifact source revision。

## Phase 5: Frontend And Tests

- 侧边栏切到 lifecycle/agent/subject indexes。
- Story/Project session info 查询替换为 SubjectExecution / ActiveAgents。
- 补 lifecycle store、AgentFrame panel、SubjectExecution panel、RuntimeTrace drill-down 测试。
- 更新 E2E，删除旧 session binding / task session 断言。

## Phase 6: Verification

- `pnpm run contracts:check`
- 后端 targeted tests：workflow dispatch/scheduler/orchestrator/permission/task/companion/routine。
- 前端 targeted tests：lifecycle store/pages/components。
- 关键 E2E：ProjectAgent open、Story dispatch、Task subject execution、Companion gate、Permission grant。

## 2026-06-01 收口记录

- Session 业务入口硬删除：`POST /sessions`、`GET /sessions`、`/sessions/{id}/prompt`、旧 context/hook runtime service 与前端 session history/runtime state 已移除；`/session/:id` 只作为 runtime trace drill-down。
- Permission 查询与 DTO 统一进入 `agentdash-contracts`，前端使用 generated permission contracts；查询锚点为 `effect_frame_id` / `run_id`。
- Routine 控制面统一使用 dispatch vocabulary：API/frontend/domain/repository/schema 都使用 `dispatch_strategy` 与 `dispatch_refs`，`0085_routine_dispatch_strategy.sql` 负责旧开发库列名向前迁移。
- Companion sub dispatch 缺 lifecycle anchors 时快速失败，provider 创建工具时先解析 project/run/agent/frame/session anchor。
- Task artifact/status effect 以 `RuntimeSession -> AgentFrame -> LifecycleAgent -> task subject association` 验证写入来源；verified artifact context 与 persist helpers 限制在 gateway 内部。
- 当前阶段项目未上线，旧 session-first records 不作为稳定业务事实源保留；migration 采用 breaking hard cutover，开发库可通过向前 rename/drop 或重置到目标 schema。
