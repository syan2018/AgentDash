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

## Migration Strategy

先做 inventory，再选择最小端口下沉。优先移动 trait 与 record 类型，不移动 orchestration logic。迁移后 application 依赖 contract，infrastructure 实现 contract，api/local 负责组装。

## Spec Update

更新 backend architecture 与 repository pattern，说明 `RepositorySet` 当前保留的原因和后续收窄方向。
