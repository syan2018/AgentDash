# Session Capability Projection Pipeline 收束

## Goal

收束 Session context 中 VFS / runtime surface / Skill baseline / capability state 的派生边界，让前端、运行态能力状态和后续 turn 启动投影都基于同一份最终 VFS 事实，并为能力之间的依赖关系建立标准化 projection pipeline，减少类似 Canvas 可见性修复后的二次漂移风险。

## Reference

- [pipeline-reference.md](./pipeline-reference.md)：当前链路、不合理点、目标链路与重构原则的短参考。

## Background

最近三个提交已经把 Canvas 可见后的 live / pending runtime context transition 收束到 `SessionCapabilityService`，并让 Skill 维度从 active VFS 派生。Review 中确认该方向正确，但还存在三类可继续收口的问题：

- `/sessions/{session_id}/context` 在 finalize 前生成 `runtime_surface`，而 finalize 之后可能合并 pending VFS overlay，导致 `vfs` 与 `runtime_surface` 来源不一致。
- Skill baseline discovery 在 session bootstrap / inspect projection 与 live transition 中存在重复实现，后续容易出现诊断、冲突处理、local skill 合并规则漂移。
- Canvas 工具级路径缺少一条直接覆盖 `present_canvas` / `canvas_start` 后 session meta、active VFS、Skill baseline 与事件联动的测试。

进一步 review 后确认：当前问题不是单点 `runtime_surface` 早生成，而是 capability 相关维度存在依赖关系，但这些依赖还没有被标准化表达：

- `CapabilityResolver` 当前负责 tool / MCP / companion 维度，不负责 VFS / Skill / runtime surface；但 `capability/mod.rs` 注释已经把 VFS 投影列入目标职责。
- `SessionConstructionPlan::validate_for_launch` 已要求 `capability_state.vfs.active == surface.vfs`、`capability_state.tool.mcp_servers == projections.mcp_servers`，说明构建阶段已有一致性 gate。
- `SessionRuntime` / `SessionProfile` 缓存的是运行态 projection，不应成为 owner/context/VFS 解析事实源。
- Runtime command 需要从完整 after-state 快照收束为 typed patch / intent，并在 query / launch / live apply 时 replay 到 projection pipeline。

## Confirmed Facts

- `SessionCapabilityService::apply_live_runtime_context_transition` 与 `enqueue_pending_runtime_context_transition` 会在应用前根据 `after_state.vfs.active` 派生 Skill baseline。
- `session_construction_bootstrap::finalize_session_construction_projection` 会合并 pending VFS overlay，并写回 `plan.surface.vfs`、`plan.context_projection.vfs`、`plan.projections.context.vfs`。
- `session_context_query::build_session_context_plan` 当前在 finalize 前调用 `attach_runtime_surface`。
- `session_construction_bootstrap` 与 `SessionCapabilityService` 都各自实现了 VFS skill + extra local skill discovery。
- Canvas 工具通过 `expose_canvas_to_session` 写入 live VFS、`visible_canvas_mount_ids`，再尝试同步 `CapabilityState`。
- `SessionConstructionPlan::validate_for_launch` 已经明确 capability state 与 construction surface 的一致性要求，可作为 pipeline 输出 gate。
- `RuntimeContextTransition` 已经统一生成 capability delta event，但输入 state 的依赖闭包仍由调用方自行补齐。

## Requirements

- `/sessions/{session_id}/context` 返回的 `runtime_surface` 必须基于 finalize 后的最终 `context_projection.vfs` / `surface.vfs` 生成。
- VFS 不应继续作为到处复制的业务事实维护；业务事实应来自 owner/session meta/runtime command 等 durable intents，effective VFS 是 construction / runtime transition 的 projection 输出。
- 必须建立 application 层统一 capability projection normalizer，表达依赖顺序：base capability -> effective VFS -> derived skills -> capability state -> runtime surface / context projection。
- 必须在规划中覆盖完整重构终态，并按阶段区分本轮实现、后续 intent 化迁移、事实源边界收口、derived DTO 收口。
- Skill baseline 派生必须由 normalizer 内的统一入口处理，供 session construction / context inspect / live transition 复用。
- 统一入口必须保留现有行为：扫描 active VFS 中的 skill、合并 `extra_skill_dirs`、记录诊断、按 skill name 处理冲突。
- live / pending runtime transition 必须经过同一 replay + normalizer 链路，使 persisted command 表达 intent，runtime projection 表达闭包状态。
- live / pending transition 的 VFS 派生 Skill 行为必须继续保留，并继续保留非 VFS/local skill 的合理继承语义。
- Canvas 工具级测试必须覆盖可见 Canvas 后的 session meta、active VFS、Skill baseline、`capability_state_changed` 与 `canvas_presented` 事件结果。
- 设计文档必须记录 typed runtime patch/event 模式，说明为什么 runtime command store 保存 intent，而 runtime projection 由 pipeline 生成。
- 不引入兼容性回退；以当前预研阶段的正确模型为准。

## Acceptance Criteria

- [ ] `pipeline-reference.md` 包含完整重构覆盖矩阵，明确 Phase 1-4 的目标和边界。
- [ ] `build_session_context_plan` 或等价路径在 finalize 后生成 `runtime_surface`，pending overlay 后 `runtime_surface.mounts` 与最终 VFS mount 集合一致。
- [ ] construction finalize、context inspect、live transition、pending transition 通过同一个 capability projection normalizer 补齐 VFS 依赖维度。
- [ ] bootstrap / inspect / live transition 不再各自手写一套 Skill baseline discovery 主流程。
- [ ] normalizer 输出满足 `capability_state.vfs.active == surface.vfs`、`capability_state.tool.mcp_servers == projections.mcp_servers` 的现有 construction invariant。
- [ ] 现有 live / pending Skill 派生测试继续通过。
- [ ] 新增 Canvas 工具级测试能证明 `present_canvas` 或 `canvas_start` 完成后，Canvas mount 可见且 `canvas-system` 出现在 runtime capability skill 维度。
- [ ] 前端 `SessionPage` 的 context 刷新逻辑不因后端 surface 生成时机调整发生类型或测试回归。
- [ ] 相关 Rust 单测与前端聚焦测试通过。

## Out of Scope

- 不重写 VFS provider、mount address 模型或 Canvas 资产模型。
- 不调整数据库字段或 migration。
- 不改变 Skill 文件格式、frontmatter 校验规则或 skill loader 的底层发现规则。
- Phase 1 先建立 projection normalizer；Phase 2 在既有 runtime command payload 容器内迁移 typed patch/event。
- 不处理非 Session runtime surface 的 Project / Story / Task preview surface 行为，除非实现中发现它们复用同一 bug 源。

## Open Questions

- 暂无阻塞规划的问题。完整 runtime command typed patch/event 方向已进入覆盖矩阵；Phase 1 先完成 pipeline normalizer 与冗余路径收口，不迁移持久化 schema。

## Notes

- 相关 review 验证已通过：`cargo test -p agentdash-application runtime_context_transition_derives_skill_dimension_from_active_vfs`、`cargo test -p agentdash-application canvas --lib`、`pnpm --filter app-web test -- SessionPage.hook-runtime.test.tsx`。
