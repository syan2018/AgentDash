# FIX-012: VFS runtime file metadata 访问器收敛

## 模块

`vfs-service`

## 来源

- `reviews/001-vfs-service.md`
- `research/vfs-service-executable-plan.md`
- worker: `019eb2e2-260d-7420-af5d-73957f220791`

## 更新

- 在 application VFS owning module 增加 runtime file metadata 常量与 accessor。
- `VfsService` 的 inline overlay stat metadata 构造改用统一 helper。
- inline grep binary 跳过改用 `runtime_entry_is_binary`。
- `fs_read` binary 路由和 MIME 读取改用统一 accessor，删除本地 metadata 解析函数。
- API surface helper 保留 DTO 边界 wrapper，内部复用 application VFS accessor。
- 保持 `RuntimeFileEntry.attributes` wire shape 不变，不做 SPI/public DTO migration。

## 涉及文件

- `crates/agentdash-application/src/vfs/types.rs`
- `crates/agentdash-application/src/vfs/service.rs`
- `crates/agentdash-application/src/vfs/tools/fs/read.rs`
- `crates/agentdash-api/src/routes/vfs_surfaces/helpers.rs`

## 验证

- `cargo test -p agentdash-application fs_read`：11 passed。
- `cargo test -p agentdash-application vfs::`：111 passed。
- `cargo test -p agentdash-api vfs_access`：9 passed。
- `cargo fmt --check -p agentdash-application`：通过。
- `cargo fmt --check -p agentdash-api`：通过。
- `git diff --check`：通过。
- Rust 测试输出存在既有 `session::construction` dead_code warnings，与本次改动无关。

## Commit

`46cbdbfb refactor(vfs): 收敛 runtime file metadata 访问器`
