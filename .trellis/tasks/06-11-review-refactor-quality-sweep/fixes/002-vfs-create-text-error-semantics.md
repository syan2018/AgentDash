# FIX-002: vfs create_text 错误语义收敛

## 模块

`vfs-service`

## 来源

- `reviews/001-vfs-service.md`

## 更新

- `VfsService::create_text` 只把 `MountError::NotFound` 视为可创建条件。
- 其它 `read_text` 错误直接返回，不再把 provider 不支持、backend 离线、权限错误或内部错误吞掉后继续尝试写入。

## 涉及文件

- `crates/agentdash-application/src/vfs/service.rs`

## 验证

- `cargo test -p agentdash-application vfs::`：105 passed，0 failed。
- 测试输出存在既有 `session::construction` dead_code warnings，与本次改动无关。

## Commit

待提交。
