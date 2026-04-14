# 错误处理

> AgentDashboard 后端错误处理规范。

---

## 分层错误体系

| 层级 | 错误类型 | 位置 | 用途 |
|------|---------|------|------|
| Domain | `DomainError` | `agentdash-domain/src/common/error.rs` | 领域层统一错误 |
| Connector | `ConnectorError` | `agentdash-spi/src/connector.rs` | 执行器相关错误 |
| API | `ApiError` | `agentdash-api/src/rpc.rs` | HTTP 语义映射 |

使用 `thiserror` 实现错误枚举，配合 `?` 操作符自动转换。

---

## 错误类型

### DomainError

```rust
pub enum DomainError {
    NotFound { entity: &'static str, id: String },
    InvalidTransition { from: String, to: String },
    Serialization(#[from] serde_json::Error),
    InvalidConfig(String),
}
```

### ConnectorError

```rust
pub enum ConnectorError {
    InvalidConfig(String),
    SpawnFailed(String),
    Runtime(String),
    ConnectionFailed(String),
    Io(#[from] std::io::Error),
    Json(#[from] serde_json::Error),
}
```

### ApiError HTTP 映射

- `DomainError::NotFound` → 404
- `DomainError::InvalidTransition` → 400
- `DomainError::InvalidConfig` → 400
- `DomainError::Serialization` → 400
- `ConnectorError::InvalidConfig` → 400
- `ConnectorError::*` (其他) → 502
- 其他 → 500

---

## 错误边界规则

| 层级 | 允许的错误类型 | 禁止 |
|------|---------------|------|
| Domain | `DomainError` | 裸 `String`、`anyhow::Error` 直传 |
| Application | `DomainError` + `std::io::Error` | 裸 `.to_string()` 后 wrap |
| Connector | `ConnectorError` | `Box<dyn Error>` |
| API | `ApiError` | 领域枚举直接序列化给前端 |

---

## 常见错误

| 错误 | 正确 |
|------|------|
| `unwrap()` 直接 panic | 使用 `?` 或 `match` 处理 |
| 吞掉错误（空 match arm） | 至少记录错误信息 |
| 返回 `String` 作为错误 | 定义具体的错误枚举 |
| 在领域层引用 `sqlx::Error` | 转换为 `DomainError::InvalidConfig` |

---

*更新：2026-04-14 — 对齐实际 DomainError/ConnectorError 变体和文件位置*
