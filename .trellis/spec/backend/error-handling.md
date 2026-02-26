# Error Handling

> How errors are handled in this project.

---

## Overview

使用自定义 `ConnectorError` 枚举处理执行器相关错误，配合 `thiserror` 实现错误转换。

- 使用 `Result<T, ConnectorError>` 作为返回类型
- 错误自动转换为 HTTP 状态码
- 关键错误使用 `tracing::error!` 记录

---

## Error Types

参考 `executor/connector.rs`：

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("Execution failed: {0}")]
    Execution(String),
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
```

---

## Error Handling Patterns

```rust
// SSE 流中的错误处理
while let Some(next) = stream.next().await {
    match next {
        Ok(notification) => {
            let _ = store.append(&session_id, &notification).await;
            let _ = tx.send(notification);
        }
        Err(e) => {
            tracing::error!("执行流错误 session_id={}: {}", session_id, e);
            break;
        }
    }
}
```

---

## API Error Responses

错误以纯文本返回，客户端通过 HTTP 状态码判断：

```rust
if !res.ok {
    let text = await res.text().catch(() => "");
    throw new Error(text || `promptSession failed: HTTP ${res.status}`);
}
```

---

## Common Mistakes

| 错误 | 正确 |
|------|------|
| `unwrap()` 直接 panic | 使用 `?` 或 `match` 处理错误 |
| 吞掉错误（空的 match arm） | 至少记录错误信息 |
| 返回 String 作为错误 | 定义具体的错误枚举 |
