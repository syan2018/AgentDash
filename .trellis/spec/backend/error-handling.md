# 错误处理

> AgentDashboard 后端错误处理规范。

---

## 概览

后端使用分层错误体系：

| 层级 | 错误类型 | 用途 |
|------|---------|------|
| Domain | `DomainError` | 领域层统一错误（NotFound、Storage、Validation 等） |
| Connector | `ConnectorError` | 执行器相关错误 |
| API | `ApiError` | HTTP 语义映射（自动转 HTTP 状态码） |

使用 `thiserror` 实现错误枚举，配合 `?` 操作符自动转换。

---

## 错误类型

### DomainError（领域层）

```rust
// agentdash-domain/src/common/error.rs
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("storage error: {0}")]
    Storage(String),
}
```

### ConnectorError（执行器层）

```rust
// agentdash-executor/src/connector.rs
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("connection failed: {0}")]
    Connection(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}
```

### ApiError（接口层）

```rust
// agentdash-api/src/rpc.rs
pub struct ApiError { ... }
```

ApiError 自动映射 HTTP 状态码：
- `DomainError::NotFound` → 404
- `DomainError::AlreadyExists` → 409
- `DomainError::InvalidData` / `InvalidConfig` → 400
- `ConnectorError` → 500 / 502
- 其他 → 500

---

## 错误处理模式

### 路由层：使用 `?` 自动转换

```rust
pub async fn get_story(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<StoryResponse>, ApiError> {
    let story = state.repos.story_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| DomainError::NotFound(format!("story {id}")))?;
    Ok(Json(StoryResponse::from(story)))
}
```

### 流处理：记录并继续

```rust
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
| 在领域层引用 `sqlx::Error` | 转换为 `DomainError::Storage` |

---

## 相关规范

- [流式协议](./streaming-protocol.md) — SSE/NDJSON 流式推送契约
- [领域类型化标准](./domain-payload-typing.md) — 结构化错误边界标准
- [Quality Guidelines](./quality-guidelines.md) — Session 执行状态持久化

---

*更新：2026-03-29 — 充实错误体系，流式协议已拆分到独立文件*
