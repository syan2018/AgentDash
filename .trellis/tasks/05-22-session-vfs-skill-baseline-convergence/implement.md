# Session Capability Projection Pipeline 收束执行计划

## Checklist

- [x] 以 `pipeline-reference.md` 作为链路目标，不做脱离整体重构的局部 hotfix。
- [x] 对照 `pipeline-reference.md` 的完整重构覆盖矩阵，确认本轮改动属于 Phase 1，且不会阻断 Phase 2-4。
- [x] 读取相关规范与代码入口：
  - `.trellis/spec/backend/vfs/vfs-access.md`
  - `.trellis/spec/backend/session/execution-context-frames.md`
  - `.trellis/spec/backend/session/session-startup-pipeline.md`
  - `.trellis/spec/backend/capability/tool-capability-pipeline.md`
  - `crates/agentdash-api/src/bootstrap/session_context_query.rs`
  - `crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs`
  - `crates/agentdash-application/src/capability/resolver.rs`
  - `crates/agentdash-application/src/session/construction.rs`
  - `crates/agentdash-application/src/session/assembler.rs`
  - `crates/agentdash-application/src/session/launch_planner.rs`
  - `crates/agentdash-application/src/session/capability_service.rs`
  - `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
  - `crates/agentdash-application/src/session/hub/tool_builder.rs`
  - `crates/agentdash-application/src/canvas/tools.rs`

## Phase Goals

### Phase 1: Projection Normalizer

目标：先消灭当前补丁散落，让所有现存 full-state projection 在入库、查询、live apply 前都经过同一条依赖闭包。

本阶段执行：

- [x] 调整 session context query：先 finalize，再基于最终 VFS attach `runtime_surface`。
- [x] 增加测试覆盖 pending VFS overlay 后 `/sessions/{id}/context` 的 `runtime_surface` 与最终 VFS 一致；若现有测试层级不便，至少补 application/API 层最接近的单测。
- [x] 抽出 capability projection normalizer 的第一阶段实现，至少包含：
  - effective VFS 写回 `CapabilityState.vfs.active`
  - effective MCP 写回 `CapabilityState.tool.mcp_servers`
  - Skill baseline 派生并写回 `CapabilityState.skill.skills`
  - live transition 保留非 VFS/local skill 的合并语义
- [x] 抽出 Skill baseline resolver，复用现有 `load_skills_from_vfs`、`load_skills_from_local_dirs`、`build_session_baseline_capabilities`。
- [x] 替换 bootstrap 与 live transition 中重复的 Skill discovery 主流程。
- [x] 审查 `SessionConstructionPlanner::build_session_capabilities` 是否应切到同一 resolver，避免 inspect 展示路径继续漂移。
- [x] 确认 runtime command 入队前的 state 已经经过 normalizer；本轮不改变 repository 持久化结构。
- [x] 在 construction finalize 后执行现有 invariant 或等价断言，确保 `surface.vfs` / `capability_state.vfs.active` / `projections.mcp_servers` 同步。
- [x] 删除或替换因 normalizer 引入后变成冗余的局部派生路径，避免保留两套事实生成逻辑。
- [x] 补 Canvas 工具级测试，直接执行工具并断言 meta、VFS、Skill 与事件。
- [x] 运行聚焦验证命令。

完成标准：

- construction finalize、context inspect、live transition、pending transition 不再各自补 Skill/VFS 派生。
- `runtime_surface` 只从 final VFS 生成。
- pending transition 继续保存完整 state，但该 state 已由 normalizer 闭包完成。

### Phase 2: Runtime Command Intent 化

目标：让 pending runtime command 不再把完整 after-state projection 当事实源，而是持久化 typed intent / patch。

本阶段执行：

- [ ] 设计 `RuntimeContextPatch` 或等价结构，覆盖 tool directives、mount directives、VFS overlay、MCP delta、phase metadata。
- [ ] 将 pending command repository payload 从完整 `CapabilityState` 快照迁移为 patch / intent。
- [ ] next-turn launch、context query、live apply 都 replay patch，经 `CapabilityProjectionPipeline` 得到 effective projection。
- [ ] 增加 repository 与恢复测试，覆盖 requested / applied / failed 状态下 patch replay 的幂等行为。

完成标准：

- `PendingCapabilityStateTransition.state` 不再是持久化事实源。
- runtime command store 保存的是可解释 intent。
- 旧的 full-state pending 快照路径被删除，而不是和 patch 路径并存。

### Phase 3: Fact Source 边界收口

目标：清理 cached runtime state 兜底构建事实的灰区，让 construction 的 durable inputs 可解释、可审计。

本阶段执行：

- [ ] 明确 `SessionProfile` / `TurnExecution` 只作为 projection cache 和 connector hot-update cache。
- [ ] construction 默认不再从 cached capability state 补 VFS/MCP；只在明确 resume/recovery 场景读取。
- [ ] 所有读取 cached runtime state 的路径写入 resolution trace，说明 recovery 来源。
- [ ] 扩展 `validate_for_launch` 或新增 pipeline gate，覆盖 VFS、MCP、Skill、runtime surface 的一致性。

完成标准：

- owner/session meta/workflow/runtime command 成为主要 durable inputs。
- cached runtime state 不再作为普通构建兜底来源。
- 恢复场景的 cached state 使用可追踪、可测试。

### Phase 4: Surface 与 Derived Dimensions 收束

目标：所有派生 DTO 和 VFS 依赖维度都从 final projection 生成，前端不自行推断能力可见性。

本阶段执行：

- [ ] `runtime_surface`、guidelines、session_capabilities、tool schema delta 都从 final capability projection 派生。
- [ ] 审查 Project / Story / Task preview surface：复用 Session runtime 语义的迁入同一 projection 边界，语义不同的显式分层。
- [ ] 前端 VFS browser / workspace panel / session context 只消费 projection DTO，不自行拼 mount 或推断 capability visibility。
- [ ] 增加前后端 contract 测试，覆盖 final projection DTO 的一致性。

完成标准：

- 后端只有一条 final projection -> derived DTO 链路。
- 前端没有隐藏的 mount / capability 推断路径。
- preview surface 与 session runtime surface 的关系被显式表达。

## Validation Commands

```bash
cargo test -p agentdash-application runtime_context_transition_derives_skill_dimension_from_active_vfs
cargo test -p agentdash-application canvas --lib
cargo test -p agentdash-application session::construction
cargo test -p agentdash-api session_context
pnpm --filter app-web test -- SessionPage.hook-runtime.test.tsx
```

如 API 聚焦测试名称不匹配，改用相关 crate 的可发现测试名或 `cargo test -p agentdash-api` 的较窄过滤。

当前已运行：

```bash
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application runtime_context_transition_derives_skill_dimension_from_active_vfs
cargo test -p agentdash-application live_vfs_skill_merge_replaces_uri_skills_and_preserves_local_skills
cargo test -p agentdash-application canvas --lib
cargo test -p agentdash-application session::construction
cargo test -p agentdash-api session_context
pnpm --filter app-web test -- SessionPage.hook-runtime.test.tsx
python ./.trellis/scripts/task.py validate .trellis/tasks/05-22-session-vfs-skill-baseline-convergence
```

## Review Gates

- 确认没有新增直接手写 `CapabilityState.skill.skills = ...` 的业务路径。
- 确认 `runtime_surface` 不再早于 finalize 生成。
- 确认 `CapabilityResolver` 仍保持 tool/MCP/companion 纯解析职责，VFS/Skill 派生由 projection normalizer 负责。
- 确认 `PendingCapabilityStateTransition.state` 入库前是闭包完整 state，而不是缺 Skill / VFS 的中间状态。
- 确认 Canvas 工具仍在 `canvas_presented` 之前完成 VFS / meta / capability 同步。
- 确认工作区没有无关格式化或文档 churn。

## Risky Files

- `crates/agentdash-api/src/bootstrap/session_context_query.rs`
- `crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs`
- `crates/agentdash-application/src/session/capability_service.rs`
- `crates/agentdash-application/src/session/capability_state.rs`
- `crates/agentdash-application/src/session/construction.rs`
- `crates/agentdash-application/src/session/construction_planner.rs`
- `crates/agentdash-application/src/canvas/tools.rs`
- `crates/agentdash-application/src/session/hub/tests.rs`

## Implementation Notes

- 保持中文注释与错误信息。
- 不引入兼容性分支或 fallback。
- 不修改数据库 schema。
- 如果抽取 resolver 后发现 API crate 依赖方向不适合直接调用，resolver 应放在 application crate 并通过 public/internal API 暴露给 API bootstrap。
- Runtime command typed patch/event 是后续任务；本轮只保证现有 full-state transition 在入队前被 normalizer 关闭依赖闭包。
