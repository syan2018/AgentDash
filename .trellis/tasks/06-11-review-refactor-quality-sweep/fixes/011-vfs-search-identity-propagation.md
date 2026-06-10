# FIX-011: VFS search/grep identity 传递收敛

## 模块

`vfs-service`

## 来源

- `reviews/001-vfs-service.md`
- `research/vfs-service-executable-plan.md`
- worker: `019eb2d7-0739-74d3-a0a4-8b995e60585e`

## 更新

- `TextSearchParams` 增加 `identity`。
- `VfsService::search_text` 新增 identity 参数，并传入 extended search。
- `search_text_extended` / `grep_text_extended` provider dispatch 传递 `params.identity`。
- `grep_inline` 使用从 identity clone 出的 `MountOperationContext`，不再使用 default context。
- `FsGrepTool` 保存 constructor 传入的 identity，并传入 `TextSearchParams`。
- 新增 `search_identity_*` 定向测试，覆盖 provider search、provider grep、inline grep 的 identity 传递。

## 涉及文件

- `crates/agentdash-application/src/vfs/service.rs`
- `crates/agentdash-application/src/vfs/tools/fs/grep.rs`
- `crates/agentdash-api/src/vfs_access/mod.rs`

## 验证

- `cargo test -p agentdash-application search_identity`：3 passed。
- `cargo test -p agentdash-application fs_grep`：12 passed。
- `cargo test -p agentdash-api vfs_access`：9 passed。
- `cargo fmt --check -p agentdash-application`：通过。
- `cargo fmt --check -p agentdash-api`：通过。
- `git diff --check`：通过。
- Rust 测试输出存在既有 `session::construction` dead_code warnings，与本次改动无关。

## Commit

待提交。
