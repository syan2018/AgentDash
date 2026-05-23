# Persistence Ports 分层下沉 Design

## Boundary

本任务处理 Rust crate 依赖方向，不改变数据库 schema、HTTP API 或业务行为。

## Target Dependency

```text
domain / agent-protocol / relay
        ↓
spi / persistence-contract
        ↓
application

adapters:
  infrastructure
  executor
  first-party-plugins

composition roots:
  api
  local
  local-tauri
```

## First Candidates

- `SessionPersistence`
- session terminal effect persistence
- runtime command persistence
- context/audit persistence
- repository-facing record DTO

## Dependency Inventory

迁移前 `agentdash-infrastructure` 对 `agentdash-application` 的依赖集中在两类：

| 使用位置 | 类型 / 函数 | 归类 |
| --- | --- | --- |
| `postgres/session_repository.rs`、`sqlite/session_repository.rs` | `SessionPersistence`、`PersistedSessionEvent`、`SessionEventBacklog`、`SessionEventPage`、`SessionMeta` | session persistence port / record |
| `postgres/session_repository.rs`、`sqlite/session_repository.rs` | `TerminalEffectRecord`、`NewTerminalEffectRecord`、`TerminalEffectStatus`、`TerminalEffectType` | terminal effect outbox contract |
| `postgres/session_repository.rs`、`sqlite/session_repository.rs` | `RuntimeCommandRecord`、`RuntimeCommandStatus`、`PendingCapabilityStateTransition` | runtime command persistence contract |
| `postgres/shared_library_repository.rs` | `shared_library::seed_digest` | shared library payload digest helper |

这些依赖不需要 application orchestration 能力，因此第一批下沉到 `agentdash-spi::session_persistence` 与 `agentdash-domain::shared_library`。

## Migration Strategy

先做 inventory，再选择最小端口下沉。优先移动 trait 与 record 类型，不移动 orchestration logic。迁移后 application 依赖 contract，infrastructure 实现 contract，api/local 负责组装。

## Spec Update

更新 backend architecture 与 repository pattern，说明 `RepositorySet` 当前保留的原因和后续收窄方向。
