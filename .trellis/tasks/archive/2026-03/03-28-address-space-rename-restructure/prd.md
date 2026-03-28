# Address Space 概念重命名与组装权下沉

## Goal

1. 解决 `AddressSpaceProvider`（injection 层，能力发现）与 `MountProvider`（application 层，I/O 操作）之间的命名混淆
2. 将 `MountProviderRegistry` 的组装从 API 层下沉到 application 层，消除 `app_state.rs` 中的越层拼装

## 背景

### 问题 A1 — 命名混淆

| 当前名称 | 位置 | 职责 |
|---------|------|------|
| `AddressSpaceProvider` | `agentdash-injection` | 能力发现 — 告诉前端有哪些可引用的地址空间类型 |
| `MountProvider` | `agentdash-application` | I/O 操作 — `read_text`/`write_text`/`list`/`search_text`/`exec` |

两个 trait 都带有 "Provider" 后缀，且都与 "address space" 概念相关，但职责完全不同。新成员容易误认为 `AddressSpaceProvider` 是做 I/O 的。

### 问题 A2 — MountProviderRegistry 组装被迫上推到 API 层

`MountProviderRegistry` 在 `agentdash-api/src/app_state.rs` 中组装，因为：
- `RelayFsMountProvider` 依赖 `BackendRegistry`（API 层）
- `InlineFsMountProvider` 和 `LifecycleVfsMountProvider` 在 application 层

这导致注册逻辑散落在 API 层。

### 问题 A3 — `MountOperationContext.extra` 使用 `HashMap<String, Box<dyn Any>>`

这是一个类型安全隐患，但影响范围较小，可在本 task 中一并改进。

## Requirements

### Part 1: 重命名

| 当前 | 重命名为 | 理由 |
|------|---------|------|
| `AddressSpaceProvider` (injection) | `AddressSpaceDiscoveryProvider` | 强调"发现/描述"职责 |
| `AddressSpaceRegistry` (injection) | `AddressSpaceDiscoveryRegistry` | 与上对齐 |
| `MountProvider` (application) | 保持不变 | 已经足够清晰 |
| `MountProviderRegistry` (application) | 保持不变 | 已经足够清晰 |

重命名范围：
- `crates/agentdash-injection/src/address_space.rs` 中的 trait 和 struct
- 所有 `use` 导入和实现处
- `agentdash-api` 中的使用处

### Part 2: MountProviderRegistry 组装权下沉

**策略**：在 application 层定义 `MountProviderRegistryBuilder`，API 层只需注入 API 层特有的 provider：

```rust
// application 层
pub struct MountProviderRegistryBuilder {
    registry: MountProviderRegistry,
}

impl MountProviderRegistryBuilder {
    /// 注册 application 层的内建 provider
    pub fn with_builtins(self) -> Self {
        // InlineFsMountProvider, LifecycleVfsMountProvider 等
        self
    }

    /// 允许上层追加 provider（如 RelayFsMountProvider）
    pub fn register(mut self, provider: Arc<dyn MountProvider>) -> Self {
        self.registry.register(provider);
        self
    }

    pub fn build(self) -> MountProviderRegistry { self.registry }
}
```

API 层只需：
```rust
let registry = MountProviderRegistryBuilder::new()
    .with_builtins()
    .register(Arc::new(relay_fs_provider))
    .build();
```

### Part 3: MountOperationContext 改进

将 `extra: HashMap<String, Box<dyn Any>>` 替换为显式字段：

```rust
pub struct MountOperationContext {
    pub backend_registry: Option<Arc<dyn BackendAvailability>>,
    pub overlay: Option<Arc<InlineOverlay>>,
    // 如有其他需求再追加具体字段
}
```

这需要：
- 确认所有现有 `ctx.get::<T>("key")` 调用使用了哪些 key
- 将每个 key 替换为显式的 `Option<Arc<...>>` 字段

## Acceptance Criteria

- [ ] `AddressSpaceProvider` → `AddressSpaceDiscoveryProvider` 全局重命名完成
- [ ] `AddressSpaceRegistry` → `AddressSpaceDiscoveryRegistry` 全局重命名完成
- [ ] `MountProviderRegistryBuilder` 在 application 层，内建 provider 在 builder 中注册
- [ ] API 层 `app_state.rs` 只追加 API 层特有的 provider
- [ ] `MountOperationContext` 不再使用 `Box<dyn Any>`
- [ ] 所有 test 通过，`cargo check --workspace` 无错误

## Technical Notes

- Part 1 是纯重命名，可用 IDE rename + 全局搜索完成，无行为变更
- Part 2 的关键挑战是确认 `RelayFsMountProvider` 是否可以通过 trait 抽象其对 `BackendRegistry` 的依赖
- Part 3 需要先 grep 所有 `ctx.get::<` 和 `ctx.insert(` 调用，梳理实际使用的 key-type 对

## 依赖

- 无前置依赖，可独立执行

## 优先级

P2 — 低优先级，改善开发体验但不影响功能正确性
