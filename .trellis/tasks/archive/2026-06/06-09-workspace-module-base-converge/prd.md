# Child A — 能力原语 + workspace_module 收口（含前端，修 carry-forward bug）

> Parent: [.trellis/tasks/06-09-capability-update-primitives](../06-09-capability-update-primitives/prd.md) · 设计见本目录 design.md · 模型见 parent design.md §2-7。
>
> 注：slug 沿用 `workspace-module-base-converge`，但本 child 范围已扩展为「定义原语 + 端到端用在 workspace_module 维度上」的完整纵切（原 4-child 的 Child1+Child2+Child4 合并）。

## Goal

一次性交付：(1) 显式的能力原语 `AccumulationPolicy` + 6 维度归类；(2) 把 workspace_module 声明式可见性从旁路字段收口到 base `CapabilityState.workspace_module`（经 `effective_capability_json`），**修复"清空 allowlist 无法回到全部可见"的 bug**；(3) 前端清空语义与 picker 三态对齐 + 契约/文档收尾。取代 PR #45 Child 4 临时语义。

## Requirements

### A. 能力原语（奠定词汇，纯加性）
1. `agentdash-spi/src/session_persistence.rs` 新增 `AccumulationPolicy { Replace, Accumulate, Ephemeral }`（决策 DD），含 doc 注释。
2. 补齐标识：`CAPABILITY_DIMENSION_SKILL`、`CAPABILITY_DIMENSION_WORKSPACE_MODULE` 常量；`CapabilityArtifactSource::preset()`（kind="preset"）。
3. `CapabilityDimensionModule` trait 加 `fn policy(&self) -> AccumulationPolicy`；现有 4 module 标注：tool=Replace、mcp=Replace、vfs=Accumulate、companion=Replace（parent §4）。

### B. workspace_module 收口 base（修 bug）
4. workspace_module 声明式真值改投影进 base `CapabilityState.workspace_module`：`None`/`Some([])` → `mode=All`；`Some([..])` → `mode=Allowlist`。经 `capability_state_to_frame_surfaces` → `effective_capability_json` → `project_capability_state_from_frame`。
5. 删除三处旁路：frame_construction/mod.rs L363-374 直接赋值；frame_builder 混合 match 臂 + `with_visible_workspace_module_refs` + builder 字段；composer workspace_module 路径 `unwrap_or_default`（改三态直达，决策 DE）。
6. `visible_workspace_module_refs_json` DB 列保留不删（零迁移），写入逻辑删除（恒 NULL），doc 标注为"运行时 Accumulate grant 预留"。
7. 三态保真：`Unspecified` 继承 / `Cleared` 回 All / `Allowlist` 受限。

### C. 前端 + 契约 + 文档收尾
8. preset-editor workspace_module 维度：清空 → 落到 `mode=All`（与后端一致，不再"清空被忽略"）；picker 反映三态；辅助文字遵循 [[feedback_no_ui_helper_text]]。
9. 契约：`AccumulationPolicy`/三态 DTO 若上前端则同步 ts_rs 导出，`pnpm contracts:check` 通过；否则确认无漂移。
10. 文档：docs 写「能力更新原语」一节 + 6 维度归类表；docs/extension-system.md 的 Workspace Module 一节更新到目标态。

## Acceptance Criteria

- [ ] `AccumulationPolicy` + 标识常量落地；4 个 module `policy()` 与 parent §4 一致，单测覆盖。
- [ ] **回归场景全过**：(a) 新 agent → 全部可见；(b) set allowlist → 受限；(c) **set 后 clear → 全部可见（bug 修复点）**；(d) set 后保持 → 仍受限。单测/集成固化。
- [ ] 三处旁路代码删除，grep 无残留；workspace_module 运行时仍由 `WorkspaceModuleDimension.allows()` 过滤，四工具行为一致。
- [ ] UI 端到端：set→保存→受限；clear→保存→全部可见；picker 三态展示正确。
- [ ] `cargo build --workspace` + `cargo test --workspace` + `pnpm contracts:check` + `pnpm --filter app-web typecheck` 通过；涉 DB 注释则 `node scripts/check-migration-history.js` 通过。
- [ ] tool/vfs/mcp 零回归；文档与代码语义一致。

## Notes

- 全任务**核心交付与修 bug 落点**，可独立于 Child B（skill/companion）交付。
- 收口后 effective_capability_json 成为 workspace_module 唯一 upstream，符合 [[feedback_converge_full_chain]]。
