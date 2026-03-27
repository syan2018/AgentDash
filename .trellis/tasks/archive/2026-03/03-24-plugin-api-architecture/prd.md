# 开源核心 + 原生插件 + 企业扩展：双仓维护友好的插件架构收敛方案

## Goal

为 AgentDash 设计一套**双仓维护友好**的插件架构，使项目可以同时支持：

- 开源核心仓库中的 first-party / 原生插件
- 企业私有仓库中的 private / enterprise 插件
- 未来逐步演进而不轻易打破外部契约

最终目标不是“把当前所有内部抽象都暴露成插件接口”，而是先收敛出一层**足够小、足够稳定、可被开源仓自己先验证**的契约面，再让企业扩展建立在这层契约之上。

## Why This Exists

- AgentDash 需要面向开源社区独立分发，同时支持企业内部定制部署
- 我们原则上也会长期提供一些开源内置/原生插件，不能把“插件模型是否合理”的验证压力全部推给企业仓库
- 当前代码里已经存在多类可插拔点（认证、连接器、Address Space descriptor、SourceResolver 等），但它们的稳定程度并不一致
- 如果把仍处于过渡态的内部抽象直接冻结成外部 SPI，后续统一 Address Space / runtime provider 时会给企业仓库带来破坏性迁移

## Core Principles

### P1: 先让开源仓自己吃自己的狗粮

- first-party / 原生插件与 enterprise 插件必须尽量走同一套宿主装配模型
- 开源仓应先用内置插件持续验证插件合同是否合理，再把这套合同暴露给企业仓

### P2: 稳定外部合同必须尽量小

- `plugin-api` 与其下游合同层 crate（当前为 `agentdash-connector-contract`）只承载真正稳定的外部契约
- 未闭环、未经过 first-party 验证、仍处于内部演进期的扩展点不能直接承诺给企业仓

### P3: 宿主先聚合注册结果，再构建运行时

- 插件注册必须发生在 `AppState` / `CompositeConnector` / 各类 Registry 构建之前
- 不能先构造运行时，再尝试把插件“塞进去”

### P4: 企业仓不是平台试验场

- 企业仓只负责企业私有实现
- 平台抽象是否稳定、冲突策略是否清晰、first-party 插件是否能落地，必须先在开源仓验证

### P5: 原生插件与企业插件的差异只在“来源”，不在“模型”

- 开源版 binary：加载 open-source first-party plugins
- 企业版 binary：在 first-party plugins 之上追加 enterprise plugins
- 两者不应维护两套完全不同的装配路径

## Current State Assessment

### 已具备的积极信号

- `run_server(plugins)` 这种公共启动缝已经出现，说明 binary 入口与宿主运行时开始解耦
- `AgentDashPlugin` 聚合入口 trait 的方向是对的
- 静态链接 + trait object 的总体策略适合 Rust 与企业部署场景

### 当前最主要的问题

- `agentdash-plugin-api` 当前虽然已切出，但连接器合同仍需要进一步下沉到独立轻量 crate
- `agent_connectors()`、`source_resolvers()`、`external_service_clients()` 在宿主里尚未形成可靠闭环
- 当前 `agentdash_injection::AddressSpaceProvider` 只是 descriptor / discovery provider，不是统一 runtime provider，不适合现在就冻结成长期外部 SPI
- 文档里“对外承诺的插件能力”已经超过了“当前真实可用、可长期维护的能力”

## Requirements

### R1: 收敛外部稳定契约

- `plugin-api` 第一阶段只承诺**最小稳定 SPI**
- 稳定 SPI 必须满足：
  - 已经接入真实宿主链路
  - 在开源仓可由 first-party 插件持续验证
  - 后续 1-2 个阶段内大概率不会因内部重构而破坏签名

### R2: 区分 stable / experimental / internal 扩展点

- `stable`：可以给企业仓依赖
- `experimental`：可在开源仓内试用，但不能承诺长期兼容
- `internal`：仅开源仓内部使用，不进入外部契约

### R3: 开源仓必须维护 first-party plugins

- 原生认证、原生连接器、未来一部分原生内容扩展应以插件形式存在
- first-party plugins 用于持续验证 host registration、冲突策略和版本兼容策略

### R4: 企业版 binary 仅在原生插件之上扩展

- 企业版 binary 不复制宿主装配逻辑
- 企业版通过 `builtin_plugins() + enterprise_plugins()` 组合加载

### R5: 宿主需采用“先注册、后构建”的启动模型

- 插件先汇总到统一 `PluginRegistry` / `HostRegistration`
- 再基于注册结果统一构建：
  - connector 集合
  - auth 中间件配置
  - descriptor registry
  - 未来的 runtime provider / mount registry

### R6: 冲突必须 fail fast

- 不能依赖隐式 `last-write-wins`
- 对单例扩展点（如 auth）重复注册时应直接启动失败
- 对具名扩展点（如 connector / provider / descriptor）出现 ID 冲突时应直接启动失败

### R7: 不引入动态加载

- 保持 trait + 静态链接方案
- 不使用 dylib / WASM 插件

## Contract Stratification

### 1. Stable SPI（第一阶段对企业仓承诺）

建议第一阶段只稳定以下能力：

- `AgentDashPlugin`：插件入口与元信息
- `AuthProvider`：认证/授权
- `AgentConnectorRegistration` 或等价的连接器注册抽象
- 宿主插件注册与冲突策略

说明：

- 这里的“连接器注册抽象”不一定沿用当前 `agentdash-executor` 整包依赖的形式
- 当前代码已先切出 `agentdash-connector-contract`，后续可以继续收敛其依赖与暴露面

### 2. Experimental SPI（开源仓可试验，不对企业仓长期承诺）

这些能力短期可保留在 `plugin-api` 草案或开源仓内部试验，但**不能作为稳定企业合同宣传**：

- 当前 `agentdash_injection::AddressSpaceProvider`
- `SourceResolver`
- `ExternalServiceClient`

原因：

- 三者分别位于 descriptor、context injection、external provider client 三个不同层级
- 当前它们还没有被统一到同一个 runtime provider / mount model 下
- 一旦 Address Space runtime 抽象收敛，很可能需要合并或重命名

### 3. Internal Only（只在开源仓内部使用）

- 宿主具体装配细节
- `AppState` 内部字段组织方式
- 具体中间件、桥接器、缓存实现
- 为兼容当前实现而存在的桥接代码

## Proposed Repository Layout

### 仓库 A：`agentdash`（开源）

```text
crates/
  agentdash-domain/              # 领域模型
  agentdash-connector-contract/  # 连接器/执行上下文合同层
  agentdash-plugin-api/          # 现阶段 contract crate（后续可按需要更名）
  agentdash-api/                 # 宿主服务 + 组合根
  agentdash-executor/            # 执行器实现
  agentdash-injection/           # 注入与 descriptor 发现实现
  agentdash-first-party-plugins/ # first-party plugin 骨架与默认集合
```

### 仓库 B：`agentdash-enterprise`（私有）

```text
crates/
  corp-auth-sso/
  corp-km/
  corp-docs/
  agentdash-enterprise-bin/
```

## Loading Model

### 开源版

```rust
let plugins = builtin_plugins();
run_server(plugins).await
```

### 企业版

```rust
let mut plugins = builtin_plugins();
plugins.extend(enterprise_plugins());
run_server(plugins).await
```

### 关键要求

- `builtin_plugins()` 与 `enterprise_plugins()` 的返回类型一致
- 宿主只负责加载统一 `Vec<Box<dyn AgentDashPlugin>>`
- 企业版不复制 host wiring，只追加插件集合

## Host Composition Model

### 目标流程

```text
加载插件列表
  -> 统一注册到 PluginRegistry / HostRegistration
  -> 校验冲突
  -> 构建 connector/auth/descriptor/runtime provider 等宿主能力
  -> 构建 AppState
  -> 启动服务
```

### 关键约束

- 插件 connector 必须在构建 `CompositeConnector` 之前收集完成
- Auth provider 必须在路由构建前确定
- 未来 descriptor / source / runtime provider 的桥接也必须在组合根阶段一次性组装，而不是运行后热插

## Conflict Policy

| 扩展点 | 冲突策略 |
|---|---|
| `AuthProvider` | 单例；重复注册直接启动失败 |
| Connector / Executor ID | ID 冲突直接启动失败 |
| Descriptor / Provider ID | ID 冲突直接启动失败 |
| Experimental 扩展点 | 默认不允许 enterprise 仓依赖；如启用需显式标注实验性 |

## Versioning & Dual-Repo Maintenance

### 开发节奏

- 开源仓先在 `main` 上迭代 first-party plugins 与 contract
- 企业仓仅在需要联调时通过 path override / patch 指向本地开源仓

### 发布策略

- 企业仓生产依赖只跟开源仓 tag / release，不直接跟 `main`
- `plugin-api` 需要单独维护 changelog
- 文档中必须标注每个扩展点的稳定性等级

### 变更约束

- `stable` SPI 的 breaking change 必须显式升级版本
- `experimental` SPI 可以调整，但必须在文档里明确“不承诺兼容”

## Phased Delivery

| 阶段 | 内容 | 结果 |
|---|---|---|
| Phase 0 | 收敛文档：明确 stable / experimental / internal 边界 | 企业仓不再被误导为“所有方法都可用且稳定” |
| Phase 1 | 抽轻 contract 依赖，避免 `plugin-api` 透传 executor 运行时栈 | 契约面真正变轻 |
| Phase 2 | 在开源仓新增 first-party plugins，并改为统一装配 | 开源仓先完成自验证 |
| Phase 3 | 重构组合根为“先注册、后构建”模型，connector/auth 真正闭环 | 插件机制从文档能力变成真实能力 |
| Phase 4 | Address Space runtime provider 收敛完成后，再决定 descriptor/source/external client 的稳定外部形态 | 避免冻结过渡态抽象 |
| Phase 5 | 企业仓按稳定 SPI 接入私有认证、KM、文档中心等插件 | 双仓长期维护进入稳态 |

## Acceptance Criteria

- [ ] PRD 明确区分 stable / experimental / internal 扩展点
- [ ] PRD 明确 first-party plugin 与 enterprise plugin 的统一装配模型
- [ ] PRD 明确企业版 binary 仅在 builtin plugins 之上扩展
- [ ] PRD 明确插件注册顺序必须早于宿主运行时构建
- [ ] PRD 明确冲突策略采用 fail fast，而不是隐式覆盖
- [ ] PRD 明确双仓版本管理与发布策略

## Out of Scope

- 立即把所有当前 trait 都改造成稳定外部 SPI
- 立即实现所有企业插件
- 动态加载（dylib / WASM）
- 插件市场 / 插件管理 UI
- 自动化插件兼容矩阵平台

## Design Decisions Log

| 决策 | 选择 | 理由 |
|---|---|---|
| 插件加载方式 | 静态链接 | ABI 稳定、类型安全、企业场景可接受 |
| 平台自验证方式 | 开源仓 first-party plugins | 先由自己验证 contract 是否合理 |
| 外部合同策略 | 最小稳定 SPI | 降低双仓长期维护成本 |
| 冲突处理 | fail fast | 比隐式覆盖更易诊断、更可控 |
| Address Space 对外暴露节奏 | 延后稳定化 | 避免冻结过渡态抽象 |

## Related Tasks

- `03-19-external-service-provider-client`：External Service Provider 设计
- `03-10-extend-address-space-entries`：扩展寻址空间条目
- `03-10-extend-source-resolvers`：扩展上下文来源解析器

## Related Files

- `crates/agentdash-api/src/lib.rs` — `run_server()` 公共入口
- `crates/agentdash-api/src/app_state.rs` — 当前组合根与插件注入位置
- `crates/agentdash-plugin-api/` — 插件 SPI 聚合入口
- `crates/agentdash-connector-contract/` — 连接器合同层
- `crates/agentdash-first-party-plugins/` — first-party plugin 骨架
- `.trellis/spec/backend/plugin-api.md` — 本次收敛后的插件规范
- `.trellis/spec/backend/address-space-access.md` — Address Space 长期方向
