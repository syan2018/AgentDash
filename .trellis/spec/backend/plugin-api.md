# Plugin API 扩展规范（收敛版）

> **定位**：本文档说明 AgentDash 当前阶段如何设计、维护和使用插件体系。
> 重点不是“把所有内部扩展点都暴露给外部”，而是明确：
>
> - 哪些能力已经可以作为稳定外部合同
> - 哪些能力仍在开源仓内部演进
> - 开源 first-party plugins 与企业 private plugins 如何长期共存

---

## 1. 设计目标

插件体系需要同时服务两类场景：

- **first-party / 原生插件**：由开源仓维护，用来提供默认认证、默认连接器、未来的部分默认内容扩展
- **enterprise / 私有插件**：由企业仓维护，用来接入企业 SSO、KM、文档中心、内部服务等

核心原则：

- 开源仓先用 first-party plugins 持续验证插件合同是否合理
- 企业仓只做最后一层私有适配，不承担平台抽象试错职责
- 外部稳定合同必须尽量小，不能把仍在演进的内部抽象提前冻结

---

## 2. 总体结构

```text
agentdash（开源仓）
  ├─ 核心宿主（agentdash-api / app_state / routes）
  ├─ plugin SPI（agentdash-plugin-api）
  ├─ connector contract（agentdash-spi）
  └─ first-party plugins（agentdash-first-party-plugins）

agentdash-enterprise（私有仓）
  ├─ enterprise plugins
  └─ enterprise binary
```

### 加载方式

开源版：

```rust
let plugins = builtin_plugins();
run_server(plugins).await
```

企业版：

```rust
let mut plugins = builtin_plugins();
plugins.extend(enterprise_plugins());
run_server(plugins).await
```

**要求**：

- 开源版与企业版必须共用同一套宿主装配路径
- 企业版只能“追加插件集合”，不能复制一套不同的宿主 wiring

---

## 3. 稳定性分层

### 3.1 Stable

可以对企业仓长期承诺的能力，必须同时满足：

- 已接入真实宿主链路
- 已被 first-party plugins 验证
- 后续 1-2 个阶段内大概率不会因内部重构而破坏签名

当前建议纳入 stable 的范围：

- `AgentDashPlugin` 入口模型
- `AuthProvider`
- 连接器注册抽象（前提是已从运行时实现中拆轻，避免整包透传依赖）
- 插件冲突检测与宿主装配规则

### 3.2 Experimental

可以在开源仓内试验，但**不应向企业仓承诺长期兼容性**。

当前属于 experimental / incubating 的范围：

- 当前 `agentdash_injection::AddressSpaceProvider`
- `SourceResolver`
- `ExternalServiceClient`

原因：

- 这些点分别位于 descriptor discovery、声明式来源解析、外部内容读取三个不同层级
- 它们尚未被统一到一个稳定的 runtime provider / mount model 中
- 如果现在冻结为外部合同，未来统一 Address Space 时几乎必然带来破坏性迁移

### 3.3 Internal

只属于开源仓内部实现细节，不进入外部合同：

- `AppState` 内部字段布局
- 各类桥接器、缓存、宿主中间件
- 为兼容当前实现而存在的临时 wiring

---

## 4. 当前阶段的关键判断

### 4.1 当前 `agentdash-plugin-api` 还不是最终形态的“全功能稳定合同”

虽然名字叫 plugin api，但当前更准确的定位是：

- 一个**第一版 contract 草案**
- 用于开始切出宿主与插件之间的边界
- 还需要经过 first-party 验证与依赖收敛

### 4.2 不要把过渡态 Address Space 抽象直接冻结给企业仓

根据 [address-space-access.md](./address-space-access.md)，当前
`agentdash_injection::AddressSpaceProvider` 只是 descriptor / discovery provider，
并不是统一的 runtime `read / write / list / search / exec` provider。

因此：

- 不能默认把它视为长期稳定的企业扩展点
- 文档和实现都应避免暗示“企业 KM 插件现在就应该围绕它建立长期合同”
- 在统一 runtime provider 收敛之前，它最多只能算 experimental

### 4.3 宿主必须先聚合注册结果，再构建运行时

插件装配的正确顺序是：

```text
收集插件列表
  -> 汇总到统一 HostRegistration / PluginRegistry
  -> 校验冲突
  -> 基于注册结果构建 connector / auth / descriptor / 未来 runtime provider
  -> 构建 AppState
  -> 启动服务
```

错误顺序是：

```text
先构建运行时
  -> 再尝试把插件塞进去
```

对静态链接插件而言，后者会导致“文档支持、运行时不生效”的假扩展点。

---

## 5. Contract Crate 设计约束

### 5.1 目标

contract crate 的职责是：

- 承载稳定外部合同
- 提供必要的 trait、类型和错误定义
- 尽量不感知宿主运行时、HTTP 框架、数据库、具体执行器实现

### 5.2 依赖约束

**允许**：

- 轻量领域类型 crate
- `serde` / `serde_json`
- `async-trait`
- `thiserror`
- `uuid`

**不应直接透传**：

- `tokio`
- `axum` / `tower`
- `sqlx`
- `reqwest`
- `rmcp`
- `executors`
- 任何具体 LLM / relay / connector 运行时实现

### 5.3 关于连接器抽象

如果外部稳定 SPI 需要暴露连接器能力，应优先采用以下方向之一：

- 将 `AgentConnector` trait 与其必要 DTO 拆到真正的 trait-only crate
- 或在 contract crate 中定义更轻的 connector registration 抽象，再由宿主适配到运行时实现

当前代码已经先切出 `agentdash-spi` 作为第一步，但它仍可继续减重；
不推荐再让企业仓为了实现一个连接器而直接依赖整个运行时 crate。

---

## 6. `AgentDashPlugin` 的职责边界

插件入口 trait 的长期职责应是：

- 提供插件标识与元信息
- 向宿主注册自己支持的稳定扩展能力
- 在宿主允许的生命周期点执行初始化/关闭逻辑

### 当前建议

- `name()`：稳定
- `auth_provider()`：可作为稳定能力推进
- `agent_connectors()`：只有在宿主完成真实闭环、且依赖收敛后，才能视为 stable
- `source_resolvers()` / `external_service_clients()` / 当前 `address_space_providers()`：先视为 experimental

换句话说：

**“trait 里有方法”不等于“已经是稳定对外合同”。**

---

## 7. First-Party Plugin 策略

开源仓必须提供并维护一批 first-party plugins，用于验证插件合同。
当前代码中，这层骨架已经收敛到 `agentdash-first-party-plugins` crate。

推荐优先从以下类型开始：

- 默认认证插件：如 `auth-none`、`auth-basic`
- 默认连接器插件：如 `connector-codex`、`connector-claude`
- 一小部分简单内容扩展插件：仅在 runtime provider 收敛后再推进

### 作用

- 让开源仓先验证插件装配、冲突检测、版本管理是否合理
- 防止企业仓成为第一个使用插件合同的人
- 让“插件体系”成为平台自身长期维护的一部分，而不是文档概念

---

## 8. Enterprise Plugin 策略

企业仓负责的内容应尽量收敛到：

- 企业认证：SSO / LDAP / OAuth / 权限系统
- 企业内容源：KM / 文档中心 / 内部门户 / 知识网关
- 企业版 binary

不应放在企业仓的内容：

- 宿主运行时 wiring
- 通用冲突策略
- 平台级插件合同定义
- 开源仓已经能提供的默认插件

---

## 9. 冲突策略

插件冲突必须 **fail fast**，不能靠隐式覆盖。

| 扩展点 | 策略 |
|---|---|
| `AuthProvider` | 单例；重复注册直接启动失败 |
| Connector / Executor ID | 冲突直接启动失败 |
| Descriptor / Provider ID | 冲突直接启动失败 |
| Experimental 能力 | 默认不允许企业仓依赖为长期合同 |

说明：

- 除非某个扩展点明确设计为可 override，否则不要使用 `last-write-wins`
- 对企业部署来说，“启动时失败并报清楚原因”远好于“默默覆盖成功”

---

## 10. 与 Address Space 长期方向的关系

插件体系必须服从统一 Address Space 的长期方向，而不是绕开它。

根据 [address-space-access.md](./address-space-access.md)：

- 长期目标是统一到 `mount + relative path`
- context injection、runtime tool、frontend browse 应共享同一 provider 底座

因此对企业内容扩展的正确节奏是：

1. 先把外部稳定合同收缩到最小
2. 再在开源仓内部推进 Address Space runtime provider 收敛
3. 最后再决定 descriptor / source resolver / external content client 的稳定外部形态

**不要倒过来做**，否则会把过渡态模型冻结到企业仓。

---

## 11. 双仓版本管理

### 推荐方式

- 企业仓生产依赖只跟开源仓 tag / release
- 本地联调时使用 path override / patch 指向本地开源仓
- `plugin-api` 需要单独维护 changelog

### 不推荐方式

- 企业仓长期直接跟开源仓 `main`
- 让企业仓自行判断某个 experimental SPI 是否可长期依赖

### 变更规则

- `stable` SPI 的 breaking change 必须显式升级版本并写清迁移说明
- `experimental` SPI 可以演进，但必须在文档里明确标注“不承诺兼容”

---

## 12. 当前实现状态说明

截至当前阶段，下面这些点应被视为**正在演进中**：

- 插件 connector 的真实宿主闭环尚未完成
- `source_resolvers()` 与 `external_service_clients()` 尚未进入稳定运行时链路
- `AuthProvider` 已出现宿主承载位，但路由中间件闭环仍待补齐

因此，本规范的重点不是宣告“所有扩展点都已经成熟”，而是：

- 明确哪些点可以稳定推进
- 明确哪些点只应在开源仓内继续收敛
- 为后续双仓长期维护建立清晰边界

---

## 13. 禁止模式

```text
❌ 把仍处于过渡态的内部抽象直接承诺为企业稳定 SPI
❌ 让企业仓直接依赖重运行时 crate 来实现轻量扩展
❌ 为开源版和企业版维护两套不同的宿主装配逻辑
❌ 在插件冲突时依赖隐式覆盖
❌ 把平台 contract 是否合理的验证责任推给企业仓
```

---

## 14. 相关文件

- [plugin-api PRD](../../tasks/03-24-plugin-api-architecture/prd.md)
- [Address Space Access](./address-space-access.md)
- [agentdash-spi](../../../crates/agentdash-spi/src/lib.rs)
- [agentdash-first-party-plugins](../../../crates/agentdash-first-party-plugins/src/lib.rs)
- [app_state.rs](../../../crates/agentdash-api/src/app_state.rs)
- [lib.rs](../../../crates/agentdash-api/src/lib.rs)
