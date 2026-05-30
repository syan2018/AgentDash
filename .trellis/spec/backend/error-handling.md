# 错误处理

> AgentDashboard 后端错误处理规范。

## 分层错误体系

| 层级 | 错误类型 | 位置 | 用途 |
|------|---------|------|------|
| Domain | `DomainError` | `agentdash-domain/src/common/error.rs` | 领域与 repository port 的统一错误 |
| Application | `ApplicationError` | `agentdash-application/src/error.rs` | application service 到 API 的统一语义边界 |
| Session Persistence | `SessionStoreError` | `agentdash-spi/src/session_persistence.rs` | session runtime 持久化端口的 NotFound / InvalidInput / InvalidData / Database 语义 |
| Connector | `ConnectorError` | `agentdash-spi/src/connector.rs` | 执行器与连接器相关错误 |
| API | `ApiError` | `agentdash-api/src/rpc.rs` | HTTP 状态码与响应体映射 |

使用 `thiserror` 实现错误枚举，配合 `?` 操作符自动转换。错误语义应在层边界保留，不能先 `.to_string()` 抹平后再由上层解析字符串。

## 错误类型

### DomainError

`DomainError` 变体覆盖领域与 repository 可判定语义：

- `NotFound`
- `Conflict`
- `Forbidden`
- `InvalidTransition`
- `Serialization`
- `InvalidConfig`
- `Database`

`InvalidConfig` 表达用户配置或领域配置无效；数据库连接、协议、decode、未知 SQLx 失败使用 `Database`。唯一约束、外键约束、排他约束等可判定约束失败使用 `Conflict`，原因是 API 层需要稳定映射到 409，而不是猜测数据库错误文本。

### ApplicationError

`ApplicationError` 作为 application 到 API 的统一错误边界，变体为：

- `BadRequest`
- `NotFound`
- `Forbidden`
- `Conflict`
- `InvalidConfig`
- `Unavailable`
- `Internal`

Application 层可保留局部 `*ApplicationError`，但跨 use case 的新入口优先返回 `ApplicationError` 或提供到 `ApplicationError` 的结构化转换。IO 与数据库类内部错误在 application / API 边界记录日志，对客户端返回固定内部错误文案。

### SessionStoreError

`SessionStoreError` 是 session persistence SPI 的端口错误，保留持久化边界可判定语义：

- `NotFound`
- `InvalidInput`
- `InvalidData`
- `Database`
- `Internal`

Session runtime 需要区分缺失记录、调用参数非法、持久化数据损坏和数据库失败，原因是 session event、projection、lineage 与 terminal effect 都参与可恢复事务链路。上层可以在边缘把它映射为 `ApplicationError`、`ConnectorError` 或临时 `std::io::Error`，但不能依赖错误文本解析语义。

### ApiError HTTP 映射

- `DomainError::NotFound` / `ApplicationError::NotFound` -> 404
- `DomainError::Conflict` / `ApplicationError::Conflict` -> 409
- `DomainError::Forbidden` / `ApplicationError::Forbidden` -> 403
- `DomainError::InvalidTransition`、`DomainError::InvalidConfig`、`DomainError::Serialization`、`ApplicationError::BadRequest`、`ApplicationError::InvalidConfig` -> 400
- `ApplicationError::Unavailable`、`ConnectorError::ConnectionFailed` -> 503
- `DomainError::Database`、内部 IO、connector runtime/spawn 失败 -> 500，响应体使用固定内部错误文案

## 错误边界规则

| 层级 | 允许的错误类型 | 禁止 |
|------|---------------|------|
| Domain | `DomainError` | 裸 `String`、`anyhow::Error`、`sqlx::Error` 直传 |
| Application | `ApplicationError`、局部 `*ApplicationError`、下层结构化错误转换 | 裸 `Result<_, String>` 作为 service 边界 |
| Session Persistence | `SessionStoreError` | `std::io::ErrorKind` + 字符串作为端口语义 |
| Connector | `ConnectorError` | `Box<dyn Error>` 直传到 API |
| API | `ApiError` | 领域/基础设施错误原文直接序列化给前端 |

## Repository 错误映射

PostgreSQL repository 通过 `agentdash-infrastructure::persistence::postgres::db_err` / `sql_err_for` 把 `sqlx::Error` 转换为 `DomainError`：

- `RowNotFound` -> `DomainError::NotFound`
- SQLSTATE `23505` / `23503` / `23P01` -> `DomainError::Conflict`
- 其他数据库错误 -> `DomainError::Database`

这样做的原因是 repository port 属于 domain 边界，上层只应消费业务可映射语义；数据库原始错误文本只进入日志或内部 message，不作为 HTTP 响应事实源。
