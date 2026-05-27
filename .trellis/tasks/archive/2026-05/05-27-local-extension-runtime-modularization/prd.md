# Local Extension Runtime 模块目录化

## Goal

把 `agentdash-local` 中 extension runtime 相关根目录文件收敛到 `extensions/` 子系统，先解决 extension host 与 artifact cache 继续增长时的所有权不清问题。

## Requirements

- 移动 `extension_host.rs` 与 `extension_artifact_cache.rs` 到 `crates/agentdash-local/src/extensions/`。
- 新增 `extensions/mod.rs` 作为 extension 子系统入口。
- `lib.rs` 继续 re-export 原有稳定公开类型和函数，crate 外调用不受文件布局影响。
- 不移动 terminal、MCP、workspace、runtime 等无关 local 模块。
- 不改变 extension host/cache 行为。

## Acceptance Criteria

- [ ] `crates/agentdash-local/src` 根目录不再直接平铺 `extension_host.rs` 与 `extension_artifact_cache.rs`。
- [ ] `LocalExtensionHostManager`、`download_and_cache_extension_artifact` 等原有 re-export 仍可从 crate 根访问。
- [ ] `cargo check -p agentdash-local` 通过。
- [ ] extension host/cache 聚焦测试通过。
