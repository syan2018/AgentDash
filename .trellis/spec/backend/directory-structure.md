# Directory Structure

> 后端代码的整洁架构分层组织方式。

---

## Crate 一览

```
crates/
├── agentdash-api/               # Interface Layer — HTTP 路由、DTO、中间件
├── agentdash-application/       # Application Layer — 用例编排
├── agentdash-application-ports/ # Application Boundary Ports — API/local 实现、application 消费的纯端口
├── agentdash-canvas/            # Canvas Boundary — identity、module ref、URI 与 key helper
├── agentdash-domain/            # Domain Layer — 实体、值对象、Repository 接口
├── agentdash-infrastructure/    # Infrastructure Layer — PostgreSQL/SQLite 持久化
├── agentdash-executor/          # Infrastructure Layer — 连接器、LLM Bridge
├── agentdash-spi/               # SPI — Connector/Hook trait + 能力协议
├── agentdash-agent/             # Agent Loop 引擎（纯 loop + bridge trait）
├── agentdash-agent-types/       # Agent 领域通用类型
├── agentdash-agent-protocol/    # Backbone Protocol + 外部协议 adapter
├── agentdash-mcp/               # MCP Server 实现
├── agentdash-relay/             # WebSocket Relay 协议
├── agentdash-local/             # 本机后端
└── agentdash-local-tauri/       # Tauri 桌面端封装
```

> 具体的文件级目录结构请直接查看代码库，不在 spec 中维护逐文件列表。

---

## 整洁架构分层

**核心原则**：依赖方向始终向内，外层依赖内层。

```
Interface Layer (agentdash-api)
    ↓ depends on
Application Layer (agentdash-application)
    ↓ depends on
Domain Layer (agentdash-domain)
    ↑ implemented by
Infrastructure Layer (agentdash-infrastructure, agentdash-executor)

Agent 子系统（独立于主分层）：
agentdash-agent-types → agentdash-agent → agentdash-spi → agentdash-executor
```

### 分层职责

| 分层 | Crate | 职责 | 允许依赖 |
|------|-------|------|----------|
| **Interface** | `agentdash-api` | HTTP 路由、DTO、中间件、错误映射 | application, domain |
| **Application** | `agentdash-application` | 用例编排：session / context / task / VFS / story | domain, spi, executor |
| **Application Ports** | `agentdash-application-ports` | application 边界 port、transport trait、轻量 DTO/error | domain, relay, agent-protocol |
| **Canvas Boundary** | `agentdash-canvas` | Canvas identity、Workspace Module ref、presentation/VFS/provider root URI 与 operation/view key 常量，供多个 application crate 共享同一业务引用 | 轻量通用库 |
| **Domain** | `agentdash-domain` | 实体、值对象、Repository 接口、领域事件 | 无外部业务库 |
| **Infrastructure** | `agentdash-infrastructure`, `agentdash-executor` | 持久化实现、连接器、WebSocket 中继 | domain |
| **Agent Types** | `agentdash-agent-types` | 跨层共享类型（Message/Tool/Context/Delegate） | serde, async-trait |
| **Agent Engine** | `agentdash-agent` | Agent Loop 引擎、LlmBridge trait | agent-types, domain |

---

## 添加新模块的步骤

1. **Domain Layer**：`agentdash-domain/src/<module>/` 下创建 `entity.rs` + `repository.rs` + `value_objects.rs`
2. **Infrastructure Layer**：`agentdash-infrastructure/src/persistence/postgres/` 下创建 `<module>_repository.rs`
3. **Interface Layer**：在 `app_state.rs` 添加 trait 对象，在 `routes/` 添加路由
4. **Application Layer**（复杂业务时）：创建用例模块，路由改为调用用例

> **禁止跨层依赖**：API 层不能直接访问 Repository 的具体实现。

`agentdash-application-ports` 只承载 API/local 实现、application 消费的纯端口，原因是 transport trait 需要被 interface/runtime composition root 实现，同时又不能让 API 反向依赖 application 内部编排模块。Domain 仍不依赖 contracts、protocol DTO 或 application ports。

---

## 命名规范

| 类型 | 规范 | 示例 |
|------|------|------|
| Crate | `agentdash-<layer>` | `agentdash-domain` |
| 实体 struct | PascalCase | `Story`, `Task` |
| Repository trait | `<Entity>Repository` | `StoryRepository` |
| Repository 实现 | `<Tech><Entity>Repository` | `PostgresStoryRepository` |
| 值对象 | PascalCase 描述性 | `StoryStatus`, `TaskStatus` |
| 目录 | 小写单数 | `agentdash-domain/src/story/` |
