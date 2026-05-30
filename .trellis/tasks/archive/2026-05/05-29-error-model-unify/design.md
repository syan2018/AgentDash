# 错误模型统一设计

## 目标边界

本任务先建立跨 domain / application / infrastructure / api 的错误语义骨架，再逐步替换 stringly error。目标不是让所有模块立刻共享同一个大枚举，而是让每一层保留自己的边界，同时保证错误语义不会在下层被 `.to_string()` 抹平。

## 分层模型

### DomainError

`DomainError` 继续是领域层唯一错误类型。新增语义变体：

- `Conflict { entity, constraint, message }`：唯一约束、状态冲突、重复资源。
- `Forbidden { action, reason }`：领域规则禁止的操作。
- `Database { operation, message }`：基础设施把数据库错误转换到领域可理解的持久化失败；message 仅供日志/内部，不作为 API 原文返回。

`InvalidConfig` 只表达用户配置或领域配置无效，不再承载 sqlx 原文。

### ApplicationError

新增 `agentdash_application::error::ApplicationError`，作为 application service 与 API 之间的统一边界错误。它承载 HTTP 可映射语义，但不依赖 axum：

- `BadRequest(String)`
- `NotFound(String)`
- `Forbidden(String)`
- `Conflict(String)`
- `InvalidConfig(String)`
- `Unavailable(String)`
- `Internal(String)`

`From<DomainError>` 保留语义映射，`From<ConnectorError>` 把 connector 的配置/连接/运行时失败映射到 `BadRequest` / `Unavailable` / `Internal`。现有局部错误（`WorkflowApplicationError`、`McpPresetApplicationError`、`SkillAssetApplicationError` 等）先通过 `From<ApplicationError>` 或 `Into<ApplicationError>` 兼容迁移，后续再逐模块删除局部重复。

### Infrastructure mapping

Postgres repository 统一通过 `persistence/postgres/mod.rs::db_err` / `sql_err_for` 映射 `sqlx::Error`：

- `RowNotFound` → `DomainError::NotFound`
- Postgres unique violation / foreign key violation / exclusion violation → `DomainError::Conflict`
- 连接池、协议、迁移、decode 等数据库失败 → `DomainError::Database`

repository 不再把 sqlx 原文包进 `InvalidConfig`。

### API mapping

`ApiError` 继续负责 axum response，但所有 `From` impl 走结构化错误：

- `DomainError::NotFound` → 404
- `DomainError::Conflict` → 409
- `DomainError::Forbidden` → 403
- `DomainError::InvalidConfig` / `InvalidTransition` / `Serialization` → 400
- `DomainError::Database` → 500，响应体使用通用内部错误文案
- `ApplicationError::*` → 对应 HTTP status

`looks_like_unique_violation` / `looks_like_skill_asset_unique_violation` 删除，唯一冲突由 infra/application 错误类型携带。

## 迁移策略

1. 先新增错误类型与 `From` 映射，不改业务行为。
2. 替换 Postgres `db_err` 语义，优先让唯一约束错误成为 `Conflict`。
3. API `From` 层改用结构化语义，并加测试证明 DB 原文不进入响应体。
4. 批量替换 handler 的 `ApiError::Internal(e.to_string())` 为 `?` 或显式结构化 mapping。
5. 逐模块迁移 `Result<_, String>` 到 `ApplicationError`，优先迁移 routine / project / mcp_preset definition。
6. session SPI port 的 `io::Result` 迁移交给 `infra-residual`，但本任务定义 `SessionStoreError` 与 `ApplicationError` 的衔接方式，避免后续重新设计。

## 风险与回滚

- 最大风险是把真实内部错误误映射为 400/409。处理原则：只有明确语义的错误映射到客户端语义，无法分类的数据库/内部失败统一返回 500。
- 批量替换 handler 时按 route 文件分批，每批保持 `cargo check --workspace` 可恢复。
- 如果某个局部 `*ApplicationError` 迁移面过大，保留局部枚举但新增 `From<ApplicationError>` 桥接，并在 implement 里记录 follow-up，不阻塞主语义骨架。

## 验证证据

- grep 计数证明 `InvalidConfig(...to_string)`、`ApiError::Internal(...to_string)`、unique violation sniffing 清零或有逐项豁免。
- API 测试覆盖唯一约束冲突返回 409，数据库内部错误不泄漏 sqlx/Postgres 原文。
- `cargo check --workspace` 通过。
