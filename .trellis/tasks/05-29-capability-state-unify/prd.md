# 能力状态机统一

> 病灶 4（capability 散落）。事实源：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`。
> **高风险深逻辑重构**，依赖：`session-assembly-converge` 之后（同改 session/，串行）。完成后标注"建议人工 review"。

## Scope
统一 capability 的"解析/演化/投影"三处散落，合并两套并行 dimension trait。

## 证据
- 散 4 处：`capability/resolver.rs`(1141, 静态归约)、`session/capability_state.rs`(1285, 运行时 transition/delta/replay)、`session/capability_projection.rs`(188, 派生)、`session/dimension/`。
- 两套 dimension trait 并行：`capability_state.rs:248` `CapabilityDimensionModule`（validate+replay）vs `dimension/mod.rs:19` `DimensionDelta`（delta+render），覆盖几乎相同维度（vfs/mcp/tool/skill）。
- 纯数据 delta 类型（`CapabilityStateDelta`/`VfsSurfaceDelta`）在 application，应在 spi/domain。

## Approach
1. 合并两套 dimension trait 为单一 `CapabilityDimension`（validate/replay/delta/render 同 trait）。
2. transition 应用收敛为单一 `CapabilityTransitionService`（live/pending/next-turn 统一入口，原 `hub/runtime_context_transition.rs` 并入）。
3. delta 纯数据类型上移 spi 或 domain。
4. capability_state.rs 瘦身为"调 transition + 存 persistence"的胶水。

## Acceptance
- [ ] 单一 dimension trait
- [ ] 单一 `CapabilityTransitionService` 作为能力切换唯一入口
- [ ] delta 类型不在 application
- [ ] `cargo check --workspace` 通过；capability/session 测试通过

## Constraints
- 仅改 `crates/`（application/spi/domain）。**不要 git commit**，orchestrator gate 后提交。
- **高风险**：能力切换行为须等价。完成后 commit message + journal 标注建议人工 review。
- 在 `session-assembly-converge` 之后做。
