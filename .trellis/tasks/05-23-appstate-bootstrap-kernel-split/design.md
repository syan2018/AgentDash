# AppState Bootstrap 拆分 Design

## Boundary

本任务只整理 API composition root，不改变 route handler、service 行为或数据库 schema。目标是让 `AppState::new_with_plugins` 从“知道每个对象如何创建”变成“表达 bootstrap 顺序和组合结果”。

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

## Architecture Guard

增加轻量边界检查，确保 bootstrap 可依赖 application/domain/infrastructure/executor/plugin，但 application/bootstrap 不反向依赖 route helper。实现方式可以是单元测试扫描 import，也可以先在 spec 中记录并由 review gate 执行 `rg`。

## Spec Update

更新 `.trellis/spec/backend/architecture.md` 和 capability/plugin API appendix 中 AppState/PluginHost 的当前基线。
