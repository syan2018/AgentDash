# VFS 搜索工具默认忽略策略收束实施计划

## Checklist

- [x] 在 implementation 前加载 `trellis-before-dev` 并读取 backend / VFS 相关 spec。
- [x] 在 `agentdash-local/src/tool_executor.rs` 抽出文件发现策略 helper，定义普通 ignore、内置噪音目录和 VCS hard exclude。
- [x] 调整 `file_list` 递归遍历，使默认 root 扫描应用普通 ignore 与内置噪音目录，显式 subtree 扫描允许进入普通 ignored subtree。
- [x] 调整 fallback search 复用同一文件发现策略。
- [x] 调整 ripgrep search 参数，确保默认扫描与显式 ignored subtree 搜索语义一致，并显式排除 VCS 元数据目录。
- [x] 更新 `fs_glob` / `fs_grep` 工具描述。
- [x] 补充 local backend 单测：默认跳过 ignored subtree、显式 path 可进入 ignored subtree、默认跳过内置噪音目录、VCS hard exclude 生效。
- [x] 根据修改范围运行窄测试，至少覆盖 `agentdash-local` 相关测试；必要时运行相关 crate check。
- [x] 若实现形成可复用 VFS 不变量，更新 `.trellis/spec/backend/vfs/` 相关文档。

## Likely Files

- `crates/agentdash-local/src/tool_executor.rs`
- `crates/agentdash-application/src/vfs/tools/fs/glob.rs`
- `crates/agentdash-application/src/vfs/tools/fs/grep.rs`
- `.trellis/spec/backend/vfs/architecture.md` 或 `.trellis/spec/backend/vfs/vfs-access.md`（仅当需要固化长期契约）

## Validation Commands

```powershell
cargo test -p agentdash-local tool_executor
cargo check -p agentdash-local
```

如工具描述或 application crate 行为有实质代码修改，再补：

```powershell
cargo test -p agentdash-application fs_glob fs_grep
cargo check -p agentdash-application
```

## Review Gates

- PRD 里的 open question 得到用户确认后，再执行实现。
- `task.py start` 只能在用户批准规划后运行。
- 实现时保持 relay 协议不扩展，除非代码证据证明 local backend 无法自行推导 intent。
