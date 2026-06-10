# FIX-013: VFS patch path target 解析收敛

## 模块

`vfs-service`

## 来源

- `reviews/001-vfs-service.md`
- `research/vfs-service-executable-plan.md`
- worker: `019eb2eb-2f0d-75b3-af10-a08f850c3d90`

## 更新

- 在 `vfs/apply_patch.rs` 引入共享 `PatchPathTarget`、`NormalizedPatchEntryTargets`、`parse_patch_path_target`、`normalize_patch_entry_targets`。
- `VfsService::apply_patch_multi` 分组使用共享 helper，不再维护本地 `split_mount_prefix` / `normalize_patch_entry_paths`。
- `fs_apply_patch` mutation key collection 使用同一 helper，不再维护本地 `mutation_key_parts`。
- service 执行分组和 tool lock key 对 explicit mount path、bare path、move target 使用同一 normalize 语义。
- cross-mount move 在 lock-key collection 阶段同样报错，不再与执行语义分叉。
- `FsApplyPatchTool::execute` 的 lock-key 解析失败返回 `AgentToolError::ExecutionFailed`，避免无锁运行。

## 涉及文件

- `crates/agentdash-application/src/vfs/apply_patch.rs`
- `crates/agentdash-application/src/vfs/service.rs`
- `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs`
- `crates/agentdash-application/src/vfs/mod.rs`

## 验证

- `cargo test -p agentdash-application apply_patch_mutation_keys`：4 passed。
- `cargo test -p agentdash-application patch_entry`：3 passed。
- `cargo test -p agentdash-application fs_apply_patch`：4 passed。
- `cargo fmt --check -p agentdash-application`：通过。
- `git diff --check`：通过。
- Rust 测试输出存在既有 `session::construction` dead_code warnings，与本次改动无关。

## Commit

`c2390ff7 refactor(vfs): 统一 patch 路径目标解析`
