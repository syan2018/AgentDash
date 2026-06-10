# FIX-014: VFS search/grep 职责拆分

## 模块

`vfs-service`

## 来源

- `reviews/001-vfs-service.md`
- `research/vfs-service-executable-plan.md`
- worker: `019eb2f3-2c57-7be3-a16a-280d25f2ed24`

## 更新

- 新增 `vfs/search.rs` 承接 search/grep 专属逻辑。
- `TextSearchParams`、长行裁剪、命中格式化、inline grep/search helper 从 `VfsService` 移出。
- `VfsService::search_text`、`search_text_extended`、`grep_text_extended` 保持 public facade，只委托 search helper。
- 保留 identity 传递、binary skip、VCS 目录排除、长行裁剪和 grep 输出格式语义。
- `service::is_vcs_path` 保留 crate 内 re-export，避免影响现有 `fs_glob` import。

## 涉及文件

- `crates/agentdash-application/src/vfs/search.rs`
- `crates/agentdash-application/src/vfs/service.rs`
- `crates/agentdash-application/src/vfs/mod.rs`

## 验证

- `cargo test -p agentdash-application search_identity`：3 passed。
- `cargo test -p agentdash-application fs_grep`：12 passed。
- `cargo test -p agentdash-application vfs::`：113 passed。
- `cargo fmt --check -p agentdash-application`：通过。
- `git diff --check`：通过。
- Rust 测试输出存在既有 `session::construction` dead_code warnings，与本次改动无关。

## Commit

`264ec228 refactor(vfs): 拆出 search grep 服务边界`
