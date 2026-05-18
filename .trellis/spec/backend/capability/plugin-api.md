# Plugin API 扩展规范

> 插件体系的稳定性分层、装配规则与双仓策略。

---

## 双仓结构

- **开源仓**：核心宿主 + `agentdash-plugin-api`（contract）+ `agentdash-spi` + `agentdash-first-party-plugins`
- **企业仓**：企业插件（SSO、KM、内部服务）+ 企业 binary
- 企业版只能"追加插件"，不能维护独立的宿主装配逻辑

---

## 稳定性分层

| 层级 | 含义 | 当前范围 |
|------|------|----------|
| **Stable** | 可对企业仓长期承诺 | `AgentDashPlugin` 入口、`AuthProvider`、冲突检测 |
| **Experimental** | 开源仓内试验，不承诺兼容 | `VfsDiscoveryProvider`、`SourceResolver`、`ExternalServiceClient` |
| **Internal** | 内部实现细节 | `AppState` 字段布局、中间件、临时 wiring |

---

## Contract Crate 依赖约束

`agentdash-plugin-api` 允许依赖：`serde`、`async-trait`、`thiserror`、`uuid`、轻量领域类型 crate

禁止透传：`tokio`、`axum`、`sqlx`、`reqwest`、`rmcp`、具体执行器/连接器运行时

---

## 装配顺序

收集插件 → 汇总到 PluginRegistry → 冲突检测 → 构建 connector/auth/provider → 构建 AppState → 启动

不允许先构建运行时再塞入插件。

---

## 冲突策略

所有扩展点 **fail fast**，不隐式覆盖：

| 扩展点 | 策略 |
|--------|------|
| `AuthProvider` | 单例，重复注册启动失败 |
| Connector / Executor ID | 冲突启动失败 |
| Descriptor / Provider ID | 冲突启动失败 |
| Plugin embedded LibraryAsset | 同一 `asset_type + system scope + key` 冲突启动失败；同一 plugin 同一 seed 可幂等更新 |

---

## First-Party Plugin 策略

开源仓必须用 first-party plugins 先验证插件合同的合理性，防止企业仓成为第一个消费者。当前 first-party 包括默认认证插件和默认连接器插件。

## Plugin Embedded Shared Library Assets

`AgentDashPlugin::library_asset_seeds()` 是 native plugin 向 Shared Library 贡献内嵌资产的启动期入口。

约束：

- plugin 只声明 `PluginLibraryAssetSeed`，不直接写数据库，也不修改 Project 运行配置。
- 宿主统一计算 digest、设置 `scope=system`、`source=plugin_embedded` 和 `source_ref=plugin:{plugin_name}:{asset_type}:{key}`。
- seed payload 必须通过 Shared Library typed validator；例如 runtime extension 走 `extension_template` schema。
- 该入口继承 native plugin 的重启边界：管理员安装/更新 plugin 后重启服务，用户再从 Marketplace 显式安装到 Project。

---

## 双仓版本管理

- 企业仓生产依赖只跟开源仓 tag/release
- stable SPI 的 breaking change 必须升版本 + 迁移说明
- experimental SPI 可演进但必须标注"不承诺兼容"

---

## 禁止模式

- 把过渡态内部抽象直接承诺为企业稳定 SPI
- 让企业仓直接依赖重运行时 crate 来实现轻量扩展
- 为开源版和企业版维护两套宿主装配逻辑
- 插件冲突时隐式覆盖
