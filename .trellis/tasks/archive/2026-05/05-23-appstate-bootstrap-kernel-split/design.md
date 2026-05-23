# AppState Bootstrap 拆分 Design

## Boundary

本任务整理 API composition root，同时明确 `bootstrap`、API route adapter、application use case 的分界。

`AppState::new_with_plugins` 的目标是从“知道每个对象如何创建”变成“表达 bootstrap 顺序和组合结果”。`agentdash-api/src/bootstrap/` 只能承载宿主装配：repository 实例化、插件汇总、relay/VFS/session/runtime gateway/auth/background worker wiring，以及 AppState-aware 的延迟绑定 adapter。

`bootstrap` 不承载 session construction、VFS surface projection、project-agent context resolver、task execution error mapping 这类业务/查询逻辑。发现这类逻辑时，优先判断能否下沉到 `agentdash-application`；只有 HTTP DTO、鉴权、`ApiError` 映射和 route 参数解析留在 `agentdash-api`。

## Proposed Modules

```text
crates/agentdash-api/src/bootstrap/
  mod.rs
  repositories.rs
  plugins.rs
  auth.rs
  vfs.rs
  relay.rs
  session.rs
  routines.rs
  background_workers.rs
```

每个模块返回窄 output struct，例如：

```rust
pub struct RepositoryBootstrapOutput { ... }
pub struct VfsKernelOutput { ... }
pub struct SessionKernelOutput { ... }
```

原先的 `session_construction_bootstrap.rs` / `session_context_query.rs` 名称虽然位于 `bootstrap/`，但职责不是宿主装配。正确方向不是把 route helper 搬进 bootstrap，而是将依赖 `AppState` / `ApiError` / 鉴权的 adapter 先放入 `agentdash-api/src/session_use_cases/`，并把可复用领域逻辑拆回 application：

```text
agentdash-application/src/session/
  construction_launch.rs      # launch/follow-up construction use case
  context_query.rs            # session context projection query

agentdash-application/src/vfs/
  surface_query.rs            # ResolvedVfsSurface projection
```

API 层只保留 thin adapter：

```text
agentdash-api/src/session_use_cases/
  construction.rs             # AppState-aware adapter around application session planner
  context_query.rs            # authenticated query adapter around application session planner

agentdash-api/src/routes/
  acp_sessions.rs             # HTTP input / auth / DTO
  task_execution.rs           # HTTP input / auth / ApiError mapping
  vfs_surfaces.rs             # HTTP input / auth / file response formatting
```

## Dependency Shape

推荐顺序：

```text
config
  -> repositories
  -> plugins
  -> vfs
  -> relay
  -> session
  -> runtime gateway / routines / auth
  -> background workers
  -> AppState
```

延迟注入点用集中结构记录，例如 `DeferredRuntimeBindings`，原因是当前 session、terminal callback、audit bus、runtime gateway 存在初始化环。

## Use Case Placement

### Belongs in `agentdash-application`

- Session launch / follow-up construction：因为它组装 owner、workspace、MCP、VFS、capability、context bundle，是 session runtime 的核心 use case，不应知道 HTTP route。
- Session context query：因为它复用 construction projection，并服务 route / runtime diagnostics / future non-HTTP callers。
- Project agent context resolution：`SessionConstructionPlanner` 已经拥有 `PROJECT_AGENT_SESSION_LABEL_PREFIX`、project agent context、project workspace resolution；route 侧不应维护另一份 resolver。
- VFS surface summary：`ResolvedVfsSurface` 与 mount summary 是 application VFS projection。backend online 与 inline file count 应通过 application port 输入，而不是在 route resolver 中直接拼装。

### Belongs in `agentdash-api`

- Auth permission checks and current user extraction.
- HTTP DTO parse / serialize.
- `ApiError` mapping and HTTP response shaping.
- Binary file response, multipart upload, stream response.
- AppState bootstrap/wiring and AppState-aware adapters.

### Requires a Small Port Before Moving

`VFS surface summary` 当前需要 `BackendRegistry::is_online` 与 `MountProviderRegistry::edit_capabilities`。下沉到 application 前应先定义窄 port，例如：

```rust
#[async_trait]
pub trait BackendAvailabilityReader {
    async fn is_backend_online(&self, backend_id: &str) -> bool;
}

pub trait MountEditCapabilityReader {
    fn edit_capabilities(&self, mount: &agentdash_spi::Mount) -> ResolvedMountEditCapabilities;
}
```

API/relay registry 作为 adapter 实现这些 port。这样 application 可以生成 surface projection，而不依赖 API registry 类型。

## Architecture Guard

增加轻量边界检查，确保 bootstrap 可依赖 application/domain/infrastructure/executor/plugin，但 bootstrap 不反向依赖 route helper。实现方式可以是单元测试扫描 import，也可以先在 spec 中记录并由 review gate 执行：

```powershell
rg -n "crate::routes|super::routes|routes::" crates/agentdash-api/src/bootstrap
```

如果该扫描失败，优先把逻辑下沉到 application；不要把 helper 随手挪进 bootstrap 作为规避。

## Spec Update

更新 `.trellis/spec/backend/architecture.md` 和 capability/plugin API appendix 中 AppState/PluginHost 的当前基线。

## Implementation Consequence

本任务已经完成的 repository / relay / VFS / session / runtime gateway / auth / background worker bootstrap 拆分是正确方向。剩余的 `bootstrap -> routes` 反向依赖通过两步处理：

1. 将 project-agent label/context resolver 的 route 侧重复逻辑改为复用 `SessionConstructionPlanner` public API。
2. 将 task execution error mapping 留在 API `rpc` 层，但 session construction 不再调用 route mapper。
3. 将 session construction/context query 从 `agentdash-api/src/bootstrap` 移到 `agentdash-api/src/session_use_cases`，表达其 adapter 性质。
4. 将 VFS surface summary 移入 `agentdash-application/src/vfs/surface_query.rs`，通过 backend availability / mount edit capability ports 读取 runtime projection。
