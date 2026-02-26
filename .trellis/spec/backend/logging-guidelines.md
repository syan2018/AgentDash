# Logging Guidelines

> How logging is done in this project.

---

## Overview

使用 `tracing` crate 进行结构化日志记录。

- `tracing::info!` - 重要生命周期事件
- `tracing::error!` - 错误和异常
- `tracing::debug!` - 调试信息（开发环境）

---

## Log Levels

| 级别 | 使用场景 | 示例 |
|------|----------|------|
| error | 需要人工干预的错误 | `tracing::error!("执行流错误: {}", e)` |
| info | 重要生命周期事件 | 会话启动、连接建立 |
| debug | 调试信息 | 消息内容、状态变化 |

---

## Structured Logging

<!-- Log format, required fields -->

(To be filled by the team)

---

## What to Log

- 会话生命周期（启动、完成、错误）
- 连接状态变化
- 执行错误（包含 session_id 用于追踪）

---

## What NOT to Log

- API 密钥和令牌
- 用户密码
- 完整的环境变量（可能包含 secrets）
