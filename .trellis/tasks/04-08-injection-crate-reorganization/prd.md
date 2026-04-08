# agentdash-injection Crate 归并重组

> 状态：planning
> 优先级：P3（不阻塞功能开发，但值得在合适时机清理）

## 问题背景

`agentdash-injection` 当前混合了两类职责：

| 文件 | 职责 | 当前问题 |
|------|------|---------|
| `address_space.rs` | 前端能力发现（Discovery Registry + Provider trait） | trait 应属于 plugin contract，实现应属于 application |
| `resolver.rs` | 声明式上下文来源解析（SourceResolver trait + 内置实现） | 同上 |
| `composer.rs` | ContextFragment 合并 | 纯 application 层实现，不需要独立 crate |

**关键约束**：`agentdash-plugin-api` 已经 re-export 这两个 trait，说明它们在概念上属于 plugin contract 层——应在 plugin-api 里定义，而非在 injection 里定义再被 re-export。

## 目标状态

```
agentdash-plugin-api（定义合同）
  ├── AddressSpaceDiscoveryProvider trait
  ├── AddressSpaceDescriptor、SelectorHint（轻量 DTO）
  ├── SourceResolver trait
  └── ContextFragment、MergeStrategy（轻量 contract 类型）

agentdash-application/src/context/（实现合同）
  ├── address_space_discovery.rs  ← address_space.rs 的实现部分
  │     AddressSpaceDiscoveryRegistry + 内置 providers
  ├── source_resolver_registry.rs ← resolver.rs 的实现部分
  │     SourceResolverRegistry、ManualTextResolver、FileResolver、ProjectSnapshotResolver
  │     resolve_declared_sources()、resolve_declared_sources_with_registry()
  └── composer.rs                 ← composer.rs 原样迁入

agentdash-injection → 删除
```

## 依赖图变化

```
变更前：
  plugin-api → injection（re-export trait）
  application → injection（使用 trait + 实现）
  api → injection（使用 trait + registry）

变更后：
  application → plugin-api（使用 trait 定义）
  api → application（使用 registry 和实现）
  api → plugin-api（使用 trait 定义，用于 plugins 注册）
```

## 迁移步骤

1. **把 trait + 轻量 DTO 移进 plugin-api**
   - `AddressSpaceDiscoveryProvider`、`AddressSpaceDescriptor`、`SelectorHint`、`AddressSpaceContext`
   - `SourceResolver`
   - `ContextFragment`、`MergeStrategy`
   - 删除 `agentdash_injection::*` 的 use，改为 plugin-api 内部定义

2. **把实现代码移进 application**
   - `AddressSpaceDiscoveryRegistry` + 4 个内置 provider → `context/address_space_discovery.rs`
   - `SourceResolverRegistry` + 3 个内置 resolver + `resolve_declared_sources` → `context/source_resolver_registry.rs`
   - `ContextComposer` → `context/composer.rs`

3. **更新所有消费方的 use 路径**
   - `agentdash-api`：`task_agent_context.rs`、`routes/address_spaces.rs`、`app_state.rs`、`plugins.rs`
   - `agentdash-application`：`context/builder.rs`、`context/builtins.rs`、`context/contributor.rs`、`context/workspace_sources.rs`、`session/plan.rs`、`project/context_builder.rs`、`story/context_builder.rs`

4. **删除 `agentdash-injection` crate**
   - 从 `Cargo.toml` workspace 移除
   - 删除 `crates/agentdash-injection/` 目录

5. **更新 `agentdash-executor` Cargo.toml**（我在 skill task 里误加了该依赖，也一并移除）

## 注意事项

- 纯机械重组，**无任何逻辑变更**
- `serde_yaml`、`walkdir` 依赖从 injection 转移到 application（它们的 Cargo.toml 已有或直接追加）
- `agentdash-plugin-api` 追加的类型必须满足"轻量"约束：只允许 `serde`、`serde_json`、`thiserror`、`uuid`、`async-trait`，不引入 `walkdir` 等文件系统库（这些留在 application 的实现里）

## 与 skill task 的关系

`04-08-skill-discovery-injection` 实现时，skill 类型放 spi、实现放 application，不依赖 injection crate。两个 task 可以独立推进，但先完成本 task 能让 skill task 的 import 路径更清晰。
