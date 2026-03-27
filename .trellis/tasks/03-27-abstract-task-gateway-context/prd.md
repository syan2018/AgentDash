# task_execution_gateway 与 task_agent_context 接口抽象与迁移

## Goal

将 `api/src/bootstrap/task_execution_gateway.rs`（1498 行）和
`api/src/task_agent_context.rs`（残留 api 层逻辑）从 api 层解耦，
通过 RepositorySet 下沉和 runtime_bridge 迁移，将核心业务逻辑迁出 api crate。

## Background

这两个模块是 api crate 中**耦合最深**的 God module 残余。

### task_execution_gateway.rs (1498 行)

**`AppStateTaskExecutionGateway`** 实现了 `agentdash_application::task_execution::TaskExecutionGateway` trait，
但内部通过 `AppState` 访问所有服务。

| api 层依赖 | 用途 | 解耦方案 |
|-----------|------|---------|
| `AppState.repos.*` | 访问所有 repository | **RepositorySet 下沉到 application** |
| `AppState.services.*` | executor_hub, address_space_service 等 | trait 抽象或直接传参 |
| `crate::runtime_bridge::*` | 类型转换 | **runtime_bridge 迁入 executor** |
| `crate::task_agent_context::*` | context contributor | 已部分迁入 application::context |
| `crate::workspace_resolution::*` | workspace binding 解析 | **迁入 application** |
| `crate::address_space_access::SessionMountTarget` | 已在 application 中定义 | 改 import 路径即可 |

### task_agent_context.rs

大部分逻辑已迁入 `agentdash_application::context`，当前文件是薄 re-export +
`resolve_workspace_declared_sources()` 等依赖 `AppState` 的残留函数。

### workspace_resolution.rs (191 行)

Workspace binding 解析逻辑。依赖 `AppState.services.backend_registry.is_online()`。
**决策：迁入 application 层**，通过抽象 `BackendRegistry` trait 解耦。

## 重构策略

### Phase 1：RepositorySet 下沉

将 `RepositorySet` 从 `api/src/app_state.rs` 移至 `application` 层（或 domain 层），
使 application 层的 service 可以直接持有 repo 集合。

```rust
// application/src/repository_set.rs (或 domain 层)
pub struct RepositorySet {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    // ... 其他 repos
}
```

api 层的 `AppState` 改为持有 `application::RepositorySet`。

### Phase 2：runtime_bridge 迁入 executor

`runtime_bridge.rs` 是纯转换函数层（connector-contract 类型 ↔ application runtime 类型）。
**决策：迁入 executor 层**，executor 已依赖 connector-contract + domain。

需要确认 executor 是否已依赖 application（可能需要新增依赖，
或将转换函数放在 connector-contract 层）。

### Phase 3：workspace_resolution 迁入 application

抽象 `BackendRegistry` trait：

```rust
// application/src/workspace/resolution.rs
#[async_trait]
pub trait BackendAvailability: Send + Sync {
    async fn is_online(&self, backend_id: &str) -> bool;
}

pub async fn resolve_workspace_binding(
    availability: &dyn BackendAvailability,
    workspace: &Workspace,
) -> Result<ResolvedWorkspaceBinding, WorkspaceResolutionError> { ... }
```

api 层的 `BackendRegistry` 实现 `BackendAvailability` trait。

### Phase 4：迁移 gateway 核心逻辑

- Gateway 核心任务启动/继续/artifact 逻辑迁入 application
- api 层的 `AppStateTaskExecutionGateway` 变为薄适配器

### Phase 5：清理 task_agent_context 残留

- `resolve_workspace_declared_sources()` 随 workspace_resolution 一起迁移
- api 层仅保留 re-export

## Acceptance Criteria

- [ ] `RepositorySet` 定义在 application（或 domain）层
- [ ] `runtime_bridge` 转换函数在 executor 层
- [ ] `workspace_resolution` 核心逻辑在 application 层
- [ ] `task_execution_gateway.rs` 在 api 层缩减到 < 300 行（适配+胶水）
- [ ] `task_agent_context.rs` 在 api 层缩减到 < 10 行（纯 re-export）
- [ ] gateway 核心逻辑可在 application 层独立测试
- [ ] `cargo check` 全 crate 通过
- [ ] `cargo test` 全 crate 通过

## Risk & Complexity

**高复杂度**——这是所有迁移任务中最难的一个：
- `AppState` 是 api 层的组合根，gateway 通过它访问十几个服务
- `runtime_bridge` 的转换逻辑涉及 executor、relay、ACP 三种协议
- RepositorySet 下沉影响 AppState 的构造方式

建议在 SPI 下沉、hooks 迁移和 tool 迁移**全部完成后**再启动本任务。

## Dependency Chain

```
前置：03-27-agent-tool-dependency-decoupling (SPI 下沉)
前置：03-27-migrate-execution-hooks-to-application
前置：03-27-migrate-address-space-services-to-application (含 tool 迁移)
前置：03-26-app-boundary-domain-decomposition (runtime bridge 基础)
```

## Affected Files

| 文件 | 当前行数 | 迁移后预期 |
|------|---------|-----------|
| `api/src/app_state.rs` | ~300 | RepositorySet 改为引用 application 层定义 |
| `api/src/bootstrap/task_execution_gateway.rs` | 1498 | < 300（api 适配器） |
| `api/src/task_agent_context.rs` | ~434 | < 10（re-export） |
| `api/src/workspace_resolution.rs` | 191 | < 10（re-export） |
| `api/src/runtime_bridge.rs` | 226 | 删除或 < 10（re-export） |
| `application/src/hooks/` | 新建 | ~2800（从 api 迁入） |
| `application/src/workspace/resolution.rs` | 新建 | ~200 |
| `executor/src/bridge.rs` | 新建 | ~226（从 api 迁入） |
