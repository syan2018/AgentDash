# session 装配流水线收敛设计

## 目标

本轮 wave2 reopen 只处理可验证的 session construction slop：

- 重新核验 `assembler` 与 `construction_planner` 是否真的重复实现同一条 resolver 链。
- 将 `SessionAssemblyBuilder` 从 `assembler.rs` 拆出，降低 god module 体量。
- 将 `compose_owner_bootstrap` / `compose_story_step` 拆成同模块私有阶段 helper，让 compose 函数成为编排入口。
- 消除 `apply_session_assembly` / `finalize_session_construction_projection` 中 `surface.vfs` 与 `context_projection.vfs` 的手工成对赋值，改为 `SessionConstructionPlan` 集中同步 helper。

## 边界

- 主要修改 `crates/agentdash-application/src/session/`。
- 必要时更新 API 侧只读投影调用方，但不改变 route 协议。
- 不把 protocol/contracts DTO 引入 domain 或 session domain 语义层。
- 不引入兼容路径；当前项目未上线，重构目标是单一正确状态。

## Resolver 争议判定

本轮先做证据复核，再决定是否抽 `SessionSurfaceResolver`：

- 如果 launch/query 已共享 `build_bootstrap_plan`、`derive_session_context_snapshot`、`finalize_session_construction_projection` 等收敛点，且剩余差异是 launch bundle 与 query snapshot 的真实契约差异，则不抽全局 resolver，只记录证据。
- 如果仍存在跨路径重复解析同一 `{vfs, capability_state, mcp_servers}` 的实现，则抽局部 resolver，并让两条路径委托它后各自保留后处理。

## VFS 投影集中同步

本批采用集中同步，而不是直接删除 `surface.vfs` / `context_projection.vfs`。原因是 `SessionConstructionPlan` 同时承担装配中间态、launch surface 和 query projection，`surface.vfs` 在 capability state 最终归一化之前仍是装配输入。

设计落点：

- `SessionConstructionPlan` 提供 `active_vfs()` / `active_vfs_cloned()` / `set_active_vfs()` / `sync_vfs_projection_from_capability()`。
- `apply_session_assembly` 与 `finalize_session_construction_projection` 不再手工成对写 `surface.vfs` / `context_projection.vfs`，统一调用 helper。
- 保留 `validate_for_launch` 的真实一致性断言，作为 construction 事实漂移 gate。

## Builder 与 compose 拆分

- 新建 `session/assembly_builder.rs`，迁出 `SessionAssemblyBuilder` 及其 builder 方法。
- `assembler.rs` 保留业务编排与 source-specific compose 入口。
- `compose_owner_bootstrap` 拆为 VFS 准备、capability 输入、context bundle/prompt 输出等私有 helper。
- `compose_story_step` 拆为 executor 解析、VFS 准备、context binding、capability/context 输出等私有 helper。

## 验证

- grep 验证 `apply_session_assembly` 与 finalize 中不再出现 `surface.vfs` / `context_projection.vfs` 成对赋值。
- 行数验证 `compose_owner_bootstrap` / `compose_story_step` 均小于 80 行，`assembler.rs` 明显下降。
- `cargo check --workspace`。
- `cargo test -p agentdash-application --lib`，若遇到已知 test-only persistence mock 债务，记录具体失败并运行覆盖改动面的替代测试。
