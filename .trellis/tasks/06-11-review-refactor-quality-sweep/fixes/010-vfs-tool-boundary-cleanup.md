# FIX-010: VFS tool 边界收敛

## 模块

`vfs-service`

## 来源

- `reviews/001-vfs-service.md`
- `research/vfs-service-executable-plan.md`
- worker Batch D: `019eb2ce-2ec1-75d2-a4b5-03e0407017a7`
- worker Batch E: `019eb2ce-73b5-75f3-a246-c1115539b10d`

## 更新

- `resolve_uri_path` 对 default mount / single mount 的无前缀路径统一走 typed mount URI/path normalization。
- `.` 在 tool 边界 normalize 为 mount root 空路径。
- 新增 `VfsToolFactory`，只负责构建 VFS-owned tools：`mounts_list`、`fs_read`、`fs_glob`、`fs_grep`、`fs_apply_patch`、`shell_exec`。
- `RelayRuntimeToolProvider` 委托 VFS factory 构建 VFS tools；workflow、companion、canvas、workspace module 装配仍保留原处。
- 保持 capability checks、shared inputs、`SessionToolServices` / `SharedSessionToolServicesHandle`、API bootstrap 与 session startup 行为不变。

## 涉及文件

- `crates/agentdash-application/src/vfs/tools/common.rs`
- `crates/agentdash-application/src/vfs/tools/factory.rs`
- `crates/agentdash-application/src/vfs/tools/provider.rs`
- `crates/agentdash-application/src/vfs/tools/mod.rs`

## 验证

- `cargo test -p agentdash-application resolve_uri_path`：5 passed。
- `cargo test -p agentdash-application fs_glob`：8 passed。
- `cargo test -p agentdash-application fs_grep`：12 passed。
- `cargo test -p agentdash-application fs_read`：11 passed。
- `cargo check -p agentdash-application`：通过。
- `cargo fmt --check -p agentdash-application`：通过。
- Rust 测试输出存在既有 `session::construction` dead_code warnings，与本次改动无关。

## Commit

`a08122a3 refactor(vfs): 收敛工具边界装配`
