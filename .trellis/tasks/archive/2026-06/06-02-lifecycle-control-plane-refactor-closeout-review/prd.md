# Lifecycle 控制面重构收口复核

## Goal

确认 `codex/refactor-lifecycle-control-plane` 当前重构不能只停留在“主体结构已迁移”，而要收束到可以被验证声明为完成的状态。

本任务追踪本次 Codex review 发现的剩余阻断项：测试编译失败、runtime delivery anchor 证据链不完整、旧 construction 大对象仍在 production crate 暴露、前端 session-first 派生残留、port output 仍使用 run 级查询，以及 `SessionMeta` 仍承担过宽 launch facts。

父任务：`.trellis/tasks/06-02-lifecycle-control-plane-final-convergence/`

## Confirmed Facts

- `RuntimeLaunchRequest` 已基本退出启动主链路，`FrameLaunchEnvelope` 是当前 launch handoff 类型。
- `FrameConstructionService` 已下沉到 application 层，API provider 已退化为 thin wrapper。
- `AgentFrameSurfaceExt` typed accessor 已落地，frame surface 可直接投影 VFS / MCP / capability / executor profile。
- `delivery_runtime_ref` 已出现在 dispatch result 与部分前端消费路径。
- `RuntimeSessionExecutionAnchor` 表、domain 类型、repository 均已落地。
- `cargo check --workspace` 通过，但 `agentdash-application` 产生 48 个 deprecated warning，集中在 `RuntimeContextInspectionPlan` / `ResolvedSessionOwner` 旧 construction 链路。
- `cargo test -p agentdash-domain --lib -- --format terse` 失败，dispatch DTO 测试仍使用已删除的 `runtime_session_ref` / `trace_ref` 字段。
- `cargo test -p agentdash-application --lib -- --format terse` 失败，`present_canvas` 后 frame visible canvas mount projection 未更新到断言期望。
- `pnpm --filter app-web run typecheck` 通过。

## Requirements

- 修复当前分支的测试编译阻断，使 domain dispatch DTO 的测试、序列化断言与 `delivery_runtime_ref` 合并后的 contract 一致。
- 让 `RuntimeSessionExecutionAnchor` 成为 runtime delivery session 回到 lifecycle control plane 的可信证据，而不是只作为 `find_by_runtime_session` 的辅助索引。
- 取缔或测试隔离 `RuntimeContextInspectionPlan` / `ResolvedSessionOwner` / `construction_use_case` 的 production 暴露面，使 Phase 5 不再只是 deprecated 降级。
- 前端 sidebar / active list / lifecycle store 以 agent/frame delivery ref 派生运行时会话，不再把 `runtime_trace_refs[0]` 当 primary session。
- Hook runtime 前端命名与 SPI 语义对齐：adapter session id 只表示 provenance，不再作为 hook/runtime 主体概念。
- 将 workflow compose 阶段的 port output 查询从 run 级 `load_port_output_map` 收束到 activity attempt scoped artifact ref，或者明确把 attempt 生成前移到可 scoped 查询的位置。
- 将 `SessionMeta` launch facts 中仍被消费的 executor/runtime 字段缩窄为明确的 runtime trace state，避免 session meta 再次成为跨层传参袋。
- 完成后必须能用验证命令证明：Rust test 编译通过、frontend typecheck 通过、残留扫描符合目标词表。

## Acceptance Criteria

- [ ] `cargo test -p agentdash-domain --lib -- --format terse` 通过。
- [ ] `cargo test -p agentdash-application --lib -- --format terse` 通过。
- [ ] `cargo check --workspace` 通过，且 application crate 不再因旧 construction 类型产生 deprecated warning 洪流。
- [ ] `RuntimeSessionExecutionAnchor` 写入使用真实 `frame.activity_key` / graph entry activity key，不再硬编码 `"entry"`。
- [ ] runtime session -> lifecycle association resolver 优先消费 anchor `assignment_id` / `launch_frame_id` 证据，并有 custom entry activity 的回归测试。
- [ ] `rg "RuntimeContextInspectionPlan|ResolvedSessionOwner" crates/agentdash-application/src --type rust` 只命中 test-only fixture 或为 0。
- [ ] `construction_use_case` 不再作为 production public module 暴露。
- [ ] `rg "runtime_trace_refs\\[0\\]|primarySessionId" packages/app-web/src` 不再命中 session-first primary 派生路径。
- [ ] `rg "HookSessionRuntimeInfo" packages/app-web/src` 为 0，或只剩明确命名为 adapter provenance 的 trace DTO。
- [ ] `rg "load_port_output_map" crates/agentdash-application/src/session crates/agentdash-application/src/workflow` 不再命中 compose 阶段 run 级查询。
- [ ] `pnpm --filter app-web run typecheck` 通过。
- [ ] 完成一次针对 launch pipeline / terminal callback / frontend active session list 的最小手动验证记录。

## Scope

本任务聚焦当前 refactor branch 是否可以宣布“重构完成”的收口缺口。它不重新讨论 Lifecycle / AgentFrame 的目标概念，也不替代父任务中更大范围的 Companion、Routine、WorkflowContract 全量收束。

## Reference

- `research/current-state-review.md`：本次 review 的证据、命令结果和残留定位。
- `design.md`：建议的薄架构边界和剩余收束设计。
- `implement.md`：按风险排序的执行清单与验证命令。
