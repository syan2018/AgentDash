# 开源核心 + 企业插件架构设计：plugin-api crate 与仓库分离方案

## Goal

设计并落地 AgentDash 的可插拔架构，使项目能够以**开源核心 + 企业扩展私有仓库**的模式分发。核心仓库不包含任何企业内部 API、凭证逻辑或专有协议实现，但通过 `agentdash-plugin-api` crate 定义的 trait 契约，企业侧可以无缝接入认证、KM、外部服务等能力。

## Why This Exists

- AgentDash 要面向开源社区独立分发，同时支持企业内部定制化部署
- 企业场景需要接入内部 SSO/LDAP 登录、KM 知识库、文档中心等非标准服务
- 这些企业定制代码不应出现在开源仓库中，但需要有明确的扩展点
- 当前项目已有良好的 trait 抽象基础（`AgentConnector`、`AddressSpaceProvider`、`SourceResolver` 等），需要进一步收拢为统一的插件契约

## Background：当前已有的可插拔设计

| 已有抽象 | 位置 | 扩展机制 |
|---|---|---|
| `AgentConnector` trait | `agentdash-executor/connector.rs` | `CompositeConnector` 组合路由 |
| `AddressSpaceProvider` trait | `agentdash-injection/address_space.rs` | `Registry.register()` 动态注册 |
| `SourceResolver` trait | `agentdash-injection/resolver.rs` | 按 `ContextSourceKind` 注册 |
| `LlmBridge` trait | `agentdash-agent/bridge.rs` | `BridgeKind` enum 切换 |
| Repository 全家桶 | `agentdash-domain/*/repository.rs` | `Arc<dyn XxxRepository>` DI |
| `pi-agent` feature flag | `agentdash-executor/Cargo.toml` | 整个 LLM 运行时可选编译 |
| `RuntimeToolProvider` trait | `agentdash-executor/connector.rs` | 运行时工具动态注入 |
| `ExecutionHookProvider` trait | `agentdash-executor/hooks.rs` | Hook 策略可替换 |

## Requirements

### R1: 新增 `agentdash-plugin-api` crate

- 作为开源仓库与企业仓库之间的**唯一契约面**
- 只包含 trait 定义、类型定义、错误枚举
- **零业务实现、零外部重依赖**（仅允许 serde / async-trait / uuid 等基础依赖）
- 必须定义以下核心 SPI：
  - `AgentDashPlugin`：插件入口 trait，聚合所有扩展点注册
  - `AuthProvider`：认证与授权抽象
  - `ExternalServiceClient`：外部服务客户端抽象（对齐 03-19 PRD）
  - 复用已有的 `AddressSpaceProvider`、`SourceResolver`、`AgentConnector` trait

### R2: 改造 DI 组合根（AppState）

- `AppState::new()` 接受 `Vec<Box<dyn AgentDashPlugin>>` 参数
- 遍历所有插件，将扩展注入到对应的 Registry
- 内置实现保持不变，作为默认行为
- 开源版 `main.rs` 传入空插件列表即可正常运行

### R3: 仓库分离方案设计

- 明确开源仓库（仓库 A）包含的内容边界
- 明确企业私有仓库（仓库 B）的结构和依赖方式
- 企业 binary 通过 Cargo 依赖引入开源核心 + 私有扩展 crate
- CI/CD 层面的构建策略

### R4: 与现有 feature flag 体系配合

- 保持 `pi-agent` feature 不变
- 可选的开源内置扩展（如 OAuth）走 feature flag
- 企业扩展不通过 feature flag，而是通过独立 binary 入口

### R5: 不引入动态加载

- 采用 trait + 静态链接方案
- 不使用 dylib / WASM 插件
- 理由：ABI 稳定性、类型安全、编译期检查，企业场景重编译成本可接受

## Proposed Design

### 1. 仓库结构

```
# 仓库 A：agentdash（开源）
crates/
  agentdash-domain/          # 领域模型 + Repository trait
  agentdash-plugin-api/      # ← 新增：插件 SPI（只有 trait + 类型）
  agentdash-injection/       # AddressSpace / SourceResolver + 内置实现
  agentdash-agent/           # AgentTool / LlmBridge + 内置实现
  agentdash-executor/        # AgentConnector + 内置 connector
  agentdash-infrastructure/  # SQLite 实现
  agentdash-application/     # 业务编排
  agentdash-api/             # HTTP/WS 入口 + DI 组合根
  agentdash-local/           # 本机后端

# 仓库 B：agentdash-enterprise（私有）
crates/
  agentdash-auth-corp/       # 企业 SSO / LDAP
  agentdash-km-corp/         # 企业 KM → AddressSpaceProvider
  agentdash-provider-corp/   # 企业 external_service 实现
  agentdash-bin-enterprise/  # 企业版 binary 入口
```

### 2. plugin-api crate 核心定义

```rust
// agentdash-plugin-api/src/lib.rs

pub trait AgentDashPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn address_space_providers(&self) -> Vec<Box<dyn AddressSpaceProvider>> { vec![] }
    fn source_resolvers(&self) -> Vec<(ContextSourceKind, Box<dyn SourceResolver>)> { vec![] }
    fn agent_connectors(&self) -> Vec<Arc<dyn AgentConnector>> { vec![] }
    fn auth_provider(&self) -> Option<Box<dyn AuthProvider>> { None }
    fn external_service_clients(&self) -> Vec<Box<dyn ExternalServiceClient>> { vec![] }
}

pub trait AuthProvider: Send + Sync { ... }
pub trait ExternalServiceClient: Send + Sync { ... }
```

### 3. DI 组合根改造

```rust
// app_state.rs
pub fn new(plugins: Vec<Box<dyn AgentDashPlugin>>) -> Self {
    let mut addr_registry = builtin_address_space_registry();
    let mut resolver_registry = builtin_source_resolver_registry();
    let mut connectors = builtin_connectors();

    for plugin in &plugins {
        for provider in plugin.address_space_providers() {
            addr_registry.register(provider);
        }
        // ... 其他扩展点同理
    }
}
```

### 4. 企业版入口

```rust
// agentdash-enterprise/src/main.rs
fn load_plugins() -> Vec<Box<dyn AgentDashPlugin>> {
    vec![
        Box::new(agentdash_auth_corp::CorpAuthPlugin::new()),
        Box::new(agentdash_km_corp::CorpKmPlugin::new()),
    ]
}
```

## Acceptance Criteria

- [ ] `agentdash-plugin-api` crate 定义完成，包含 `AgentDashPlugin` 主 trait 和所有扩展点 trait
- [ ] plugin-api 零重依赖（只依赖 serde / async-trait / 基础类型 crate）
- [ ] `AppState::new()` 改造为接受插件列表，内置行为不变
- [ ] 开源版 binary 不传入任何企业插件时，行为与现有版本完全一致
- [ ] 设计文档明确仓库分离边界、CI 构建策略、版本同步规则
- [ ] 企业版 binary 示例可编译通过（skeleton 级别即可）

## Phased Delivery

| 阶段 | 内容 | 依赖 |
|---|---|---|
| Phase 0 | 新建 `agentdash-plugin-api` crate，定义核心 trait | 无 |
| Phase 1 | 改造 `AppState::new()` 接受插件列表 | Phase 0 |
| Phase 2 | 创建企业私有仓库骨架，实现第一个企业插件（如 SSO） | Phase 1 |
| Phase 3 | 03-19 external_service provider 基于 `ExternalServiceClient` trait 落地 | Phase 0 |

## Out of Scope

- 立即实现所有企业插件
- 动态加载（dylib / WASM）
- 插件市场 / 插件管理 UI
- 插件间通信协议
- 插件版本兼容性矩阵的自动化检测

## Design Decisions Log

| 决策 | 选择 | 理由 |
|---|---|---|
| 插件加载方式 | 静态链接 | ABI 稳定、类型安全、企业场景重编译可接受 |
| 契约面位置 | 独立 crate | 最小依赖、清晰边界、企业仓库只需依赖此 crate |
| 认证扩展方式 | `AuthProvider` trait | 企业 SSO 千差万别，trait 抽象最灵活 |
| 外部服务接入 | `ExternalServiceClient` trait | 对齐 03-19 PRD，统一资源视图模型 |

## Related Tasks

- `03-19-external-service-provider-client`：External Service Provider 设计（plugin-api 的 `ExternalServiceClient` 直接对齐此 PRD）
- `03-10-extend-address-space-entries`：扩展寻址空间条目
- `03-10-extend-source-resolvers`：扩展上下文来源解析器

## Related Files

- `crates/agentdash-api/src/app_state.rs` — DI 组合根
- `crates/agentdash-injection/src/address_space.rs` — AddressSpaceProvider trait
- `crates/agentdash-injection/src/resolver.rs` — SourceResolver trait
- `crates/agentdash-executor/src/connector.rs` — AgentConnector trait
- `crates/agentdash-agent/src/bridge.rs` — LlmBridge trait
- `crates/agentdash-executor/src/connectors/pi_agent_provider_registry.rs` — Provider 注册表
- `.trellis/spec/backend/address-space-access.md` — 寻址空间契约文档
