# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

- **Linting**: Clippy (Rust)
- **格式化**: rustfmt
- **检查命令**: `cargo check`, `cargo clippy`

代码提交前必须通过 clippy 检查。

---

## Forbidden Patterns

| 禁止模式 | 原因 | 替代方案 |
|----------|------|----------|
| `unwrap()` | 可能导致 panic | 使用 `?` 或 `match` |
| `panic!()` | 不可恢复错误 | 返回 `Result` |
| 裸 `std::sync::Mutex` | 可能死锁 | 使用 `tokio::sync::Mutex` (异步) |

---

## Required Patterns

- 异步函数使用 `async/await`
- 共享状态使用 `Arc<Mutex<T>>`
- 错误类型实现 `thiserror::Error`
- 序列化使用 `serde`，字段名 camelCase

---

## Testing Requirements

当前以集成测试为主。关键 API 需要：

1. 正常流程测试
2. 错误处理测试
3. 并发安全测试

---

## Code Review Checklist

- [ ] 无 `unwrap()` 或已标记为安全
- [ ] 错误处理完善
- [ ] 异步函数正确使用 `.await`
- [ ] 共享状态使用 `Arc`
- [ ] 日志记录适当
