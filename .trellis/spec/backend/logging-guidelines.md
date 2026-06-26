# 日志规范

> AgentDashboard 后端日志记录规范。

> **重要**：平台过程诊断（「平台进程在干什么、哪里出问题」）现在**只走** `agentdash_diagnostics::diag!` 宏，
> 裸调用 `tracing::{info,warn,error,debug,trace}!` 已被 clippy `disallowed-macros` 禁止。
> 入口约定、`Subsystem` 取值、关联字段、日志落地与查询端点见
> [Diagnostics Guidelines](./diagnostics-guidelines.md)。本文档保留级别 / 字段 / 脱敏的通用约定，
> 但下方代码示例中的 `tracing::xxx!` 应替换为 `diag!(Level, Subsystem::Xxx, ...)`。

---

## 概览

平台过程诊断使用 `agentdash-diagnostics` 的 `diag!` facade（底层基于 `tracing` + `tracing-subscriber`）进行结构化记录。

```rust
// Cargo.toml
agentdash-diagnostics = { workspace = true }
```

---

## 日志级别

| 级别 | 使用场景 | 示例 |
|------|----------|------|
| `error` | 需要人工干预的错误 | 数据库连接失败、关键业务逻辑异常 |
| `warn` | 可恢复的异常情况 | broadcast `Lagged`、配置降级 |
| `info` | 重要生命周期事件 | 会话启动/完成、连接建立、服务启动 |
| `debug` | 开发调试信息 | 消息内容、状态变化细节、SQL 查询 |
| `trace` | 极细粒度追踪 | 每个 NDJSON 事件、每个 tool call |

---

## 结构化字段

使用 `tracing` 的 span 和结构化字段，不要拼接字符串：

```rust
// ✅ 正确：结构化字段
tracing::info!(
    session_id = %session_id,
    task_id = %task_id,
    status = %new_status,
    "任务状态变更"
);

// ✅ 正确：使用 span 追踪上下文
let _span = tracing::info_span!(
    "session_execution",
    session_id = %session_id,
    connector = %connector_id,
).entered();

// ❌ 错误：字符串拼接
tracing::info!("session {} task {} status changed to {}", session_id, task_id, new_status);
```

---

## 必须记录的事件

| 事件 | 级别 | 必须包含的字段 |
|------|------|---------------|
| 会话启动 | `info` | `session_id`, `connector_id`, `model_id` |
| 会话完成 | `info` | `session_id`, `terminal_kind`, `elapsed_ms` |
| 执行流错误 | `error` | `session_id`, 错误详情 |
| 后端连接建立 | `info` | `backend_id`, `workspace_roots` |
| 后端连接断开 | `warn` | `backend_id`, 断开原因 |
| Hook 触发 | `debug` | `session_id`, `trigger`, `decision` |
| Relay 命令路由 | `debug` | `backend_id`, `command_type` |

---

## 禁止记录的内容

- API 密钥和令牌
- 用户密码
- 完整的环境变量（可能包含 secrets）
- 大段 Agent 输出文本（仅记录长度/摘要）
- 用户输入的完整 prompt 内容

---

## 错误日志标准

错误日志必须包含足够的上下文供排查：

```rust
// ✅ 正确：包含上下文
tracing::error!(
    session_id = %session_id,
    error = %e,
    "执行流错误，会话将终止"
);

// ❌ 错误：信息不足
tracing::error!("error: {}", e);
```
