# VFS 去重执行计划

## 阶段 1：准备与基线

1. 记录当前 grep 基线：
   - `rg "PROVIDER_INLINE_FS" crates/agentdash-application/src/vfs/service.rs -n`
   - `rg "map_err\\(\\|e\\| e.to_string\\(\\)\\)" crates/agentdash-application/src/vfs/service.rs -n`
   - `rg "fn apply_patch_to_inline_files|fn apply_patch_to_fs" crates -n`
   - `rg "fn watch|MountEventReceiver" crates/agentdash-spi crates/agentdash-application/src/vfs -n`
   - `rg "unwrap_or_else\\(\\|_\\| serde_json::Value::String" crates/agentdash-application/src/workflow/orchestrator.rs -n`

## 阶段 2：MountProvider 拆分

1. 在 `agentdash-spi/src/platform/mount.rs` 新增 `ProviderDescriptor` / `MountIo` / `MountSearch`。
2. 将 `read_text_range` 默认实现移到 `MountIo`。
3. 将 `suggest_paths`、`search_text`、`grep_text` 移到 `MountSearch`。
4. 将 metadata 与 availability 移到 `ProviderDescriptor`。
5. 删除 provider SPI 的 `MountEvent*` / `MountEventReceiver` / `watch`。
6. 更新内置 provider、relay provider、测试 mock：
   - `provider_inline.rs`
   - `provider_canvas.rs`
   - `provider_lifecycle.rs`
   - `provider_routine.rs`
   - `provider_skill_asset.rs`
   - `agentdash-api/src/mount_providers/relay_fs.rs`
   - `vfs/tools/fs/read.rs` mock
   - `session/hub/tests.rs` mock
7. 将 inline overlay 事件类型迁到 `inline_persistence.rs` 本地命名：`InlineOverlayEvent` / `InlineOverlayEventKind` / `InlineOverlayEventReceiver`。

## 阶段 3：Patch executor 单源

1. 在 `vfs/apply_patch.rs` 新增 `FsPatchTarget`。
2. 删除 `apply_patch_to_fs` 和 `apply_patch_to_inline_files`。
3. `vfs/mod.rs` 不再 re-export 删除的函数。
4. `agentdash-local/src/tool_executor.rs` 改为调用 `apply_patch_to_target(&FsPatchTarget, patch)`。
5. 迁移原 `apply_patch_to_fs_*` 测试到 `FsPatchTarget`。
6. 删除 inline files 专用测试，保留 `MemoryTarget` 测试覆盖 trait executor。

## 阶段 4：VfsService dispatch 与 MountError 贯通

1. 在 `service.rs` 增加统一 provider resolve / operation logging helper。
2. 用 helper 改造：
   - `read_text_range`
   - `suggest_paths`
   - `read_text`
   - `read_binary`
   - `write_text`
   - `delete_text`
   - `rename_text`
   - `stat`
   - `apply_patch`
   - `apply_multi_mount_patch`
   - `list`
   - `exec`
   - `search_text_extended`
   - `grep_text_extended`
3. `PROVIDER_INLINE_FS` 只留在单一 `is_inline_mount()` helper 中。
4. `VfsService` 内部返回 `MountError`，调用边缘再字符串化。
5. 更新 `context/*`、`vfs/materialization.rs`、`vfs/mutation_dispatcher.rs`、`vfs/tools/fs/*`、API VFS routes 的错误映射。

## 阶段 5：Orchestrator JSON strict

1. 在 `workflow/orchestrator.rs` 将 output port content parse 失败改为 `Err`。
2. 增加或更新单测覆盖 invalid JSON output port。

## 阶段 6：验证

1. `cargo fmt --check`
2. `cargo check -p agentdash-spi -p agentdash-application -p agentdash-api -p agentdash-local`
3. `cargo test -p agentdash-application vfs`
4. `cargo test -p agentdash-local`
5. 验收 grep：
   - `rg "PROVIDER_INLINE_FS" crates/agentdash-application/src/vfs/service.rs -n`
   - `rg "trait MountIo|trait MountSearch|trait ProviderDescriptor" crates/agentdash-spi/src/platform/mount.rs -n`
   - `rg "fn watch|MountEventReceiver" crates/agentdash-spi crates/agentdash-application/src/vfs -n`
   - `rg "fn apply_patch_to_inline_files|fn apply_patch_to_fs" crates -n`
   - `rg "map_err\\(\\|e\\| e.to_string\\(\\)\\)" crates/agentdash-application/src/vfs/service.rs -n`
   - `rg "unwrap_or_else\\(\\|_\\| serde_json::Value::String" crates/agentdash-application/src/workflow/orchestrator.rs -n`
6. 最终如耗时可跑 `cargo check --workspace`。

## 回滚点

- Trait 拆分与行为改造分 commit，若 provider 编译面过大，可先保留 `MountProvider` blanket 聚合减少调用面扩散。
- Patch executor 单源独立于 service dispatch，可单独回滚。
- `MountError` 贯通若牵引 API route 太多，先让 service helper 返回 `MountError`，边缘逐步映射，但不再在 provider dispatch 路径写散落的 `.to_string()`。
