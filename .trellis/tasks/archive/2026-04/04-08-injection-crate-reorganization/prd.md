# agentdash-injection Crate 归并重组

> 状态：planning
> 优先级：P3（不阻塞功能开发，但值得在合适时机清理）

## 问题背景

`agentdash-injection` 当前混合了两类职责：

| 文件 | 职责 | 当前问题 |
|------|------|---------|
| `address_space.rs` | 前端能力发现（Discovery Registry + Provider trait） | trait 是 SPI，实现应属于 application |
| `resolver.rs` | 声明式上下文来源解析（SourceResolver trait + 内置实现） | 同上 |
| `composer.rs` | ContextFragment 合并 | 纯 application 层实现，不需要独立 crate |

**核心判断**：`SourceResolver`、`AddressSpaceDiscoveryProvider` 本质是 Service Provider Interface（服务提供者接口），与 `AgentConnector`、`HookTrigger` 同属 SPI 层。`agentdash-spi` 已是此类 trait 的家，injection 作为独立 crate 不必要。

`agentdash-plugin-api` 当前 re-export 这两个 trait 给外部插件实现者。plugin-api 是扩展入口、不是合同定义处——它应从 spi re-export，而非从 injection re-export。

## 目标状态

```
agentdash-spi（SPI 层，定义 provider 接口）
  ├── AddressSpaceDiscoveryProvider trait
  ├── AddressSpaceDescriptor、SelectorHint、AddressSpaceContext（轻量 DTO）
  ├── SourceResolver trait
  └── ContextFragment、MergeStrategy（轻量 contract 类型）

agentdash-application/src/context/（实现 SPI）
  ├── address_space_discovery.rs  ← address_space.rs 的 Registry + 内置 providers
  ├── source_resolver_registry.rs ← resolver.rs 的 Registry + 内置 resolvers + resolve_declared_sources()
  └── composer.rs                 ← composer.rs 原样迁入

agentdash-plugin-api（扩展入口，re-export 给插件实现者）
  └── 从 agentdash-spi re-export AddressSpaceDiscoveryProvider、SourceResolver 等

agentdash-injection → 删除
```

## 依赖图变化

```
变更前：
  plugin-api → injection（re-export trait）
  application → injection（使用 trait + 实现）
  api → injection（使用 trait + registry）

变更后：
  spi 定义 trait（已是 AgentConnector 等 trait 的家）
  application → spi（使用 trait 定义 + 提供实现）
  api → application（使用 registry 和实现）
  plugin-api → spi（re-export 给外部插件实现者）
```

## 迁移步骤

1. **把 trait + 轻量 DTO 移进 agentdash-spi**
   - `AddressSpaceDiscoveryProvider`、`AddressSpaceDescriptor`、`SelectorHint`、`AddressSpaceContext`
   - `SourceResolver`
   - `ContextFragment`、`MergeStrategy`

2. **把实现代码移进 agentdash-application/src/context/**
   - `AddressSpaceDiscoveryRegistry` + 4 个内置 provider → `context/address_space_discovery.rs`
   - `SourceResolverRegistry` + 3 个内置 resolver + `resolve_declared_sources` → `context/source_resolver_registry.rs`
   - `ContextComposer` → `context/composer.rs`

3. **更新 agentdash-plugin-api**
   - 从 `agentdash_spi` re-export（替换当前从 `agentdash_injection` 的 re-export）

4. **更新所有消费方的 use 路径**
   - `agentdash-api`：`task_agent_context.rs`、`routes/address_spaces.rs`、`app_state.rs`、`plugins.rs`
   - `agentdash-application`：`context/builder.rs`、`context/builtins.rs`、`context/contributor.rs`、`context/workspace_sources.rs`、`session/plan.rs`、`project/context_builder.rs`、`story/context_builder.rs`

5. **删除 `agentdash-injection` crate**
   - 从 `Cargo.toml` workspace 移除
   - 删除 `crates/agentdash-injection/` 目录

6. **清理 `agentdash-executor` Cargo.toml**（skill task 误加的 injection 依赖也一并移除）

## 注意事项

- 纯机械重组，**无任何逻辑变更**
- `serde_yaml`、`walkdir` 依赖从 injection 转移到 application（Cargo.toml 追加）
- `agentdash-spi` 追加的类型必须满足轻量约束：只允许 `serde`、`serde_json`、`thiserror`、`std::path` 等基础类型，不引入 `walkdir` 等文件系统库（留在 application 实现侧）

## 与 skill task 的关系

`04-08-skill-discovery-injection` 实现时，`SkillRef` 放 spi、解析/扫描放 application，与本 task 同一设计方向。两个 task 可独立推进，但先完成本 task 能让 skill 的 import 路径更清晰。
