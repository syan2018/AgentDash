# Host Integration API 扩展规范

> Host Integration 体系的稳定性分层、装配规则与双仓策略。

---

## 双仓结构

- **开源仓**：核心宿主 + `agentdash-integration-api`（contract）+ `agentdash-spi` + `agentdash-first-party-integrations`
- **企业仓**：企业集成（SSO、KM、内部服务）+ 企业 binary
- 企业版只能"追加集成"，不能维护独立的宿主装配逻辑

---

## 稳定性分层

| 层级 | 含义 | 当前范围 |
|------|------|----------|
| **Stable** | 可对企业仓长期承诺 | `AgentDashIntegration` 入口、`AuthProvider`、冲突检测 |
| **Experimental** | 开源仓内试验，不承诺兼容 | `VfsDiscoveryProvider`、`SourceResolver`、`ExternalServiceClient` |
| **Internal** | 内部实现细节 | `AppState` 字段布局、中间件、临时 wiring |

---

## Contract Crate 依赖约束

`agentdash-integration-api` 允许依赖：`serde`、`async-trait`、`thiserror`、`uuid`、轻量领域类型 crate

禁止透传：`tokio`、`axum`、`sqlx`、`reqwest`、`rmcp`、具体执行器/连接器运行时

---

## 装配顺序

收集集成 → 汇总到 IntegrationRegistry → 冲突检测 → 构建 connector/auth/provider → 构建 AppState → 启动

不允许先构建运行时再塞入集成。

---

## 冲突策略

所有扩展点 **fail fast**，不隐式覆盖：

| 扩展点 | 策略 |
|--------|------|
| `AuthProvider` | 单例，重复注册启动失败 |
| Connector / Executor ID | 冲突启动失败 |
| Descriptor / Provider ID | 冲突启动失败 |
| Integration embedded LibraryAsset | 同一 `asset_type + system scope + key` 冲突启动失败；同一 integration 同一 seed 可幂等更新 |

---

## First-Party Integration 策略

开源仓必须用 first-party integrations 验证集成合同的合理性，防止企业仓成为第一个消费者。First-party baseline 包括默认认证集成和默认连接器集成。

## Memory Discovery Providers

### 1. Scope / Trigger

`AgentDashIntegration::memory_discovery_providers()` 是 native integration 向 session runtime 贡献 memory source inventory 的启动期入口。Memory 发现只消费已经进入本次 runtime VFS 的受控 mount 与 bounded index 文件，原因是 Agent 记忆应服从 VFS 能力边界，并通过 ContextFrame 注入为历史经验线索，而不是新增独立 Memory API、数据库实体或工具通道。

### 2. Signatures

```rust
fn memory_discovery_providers(&self) -> Vec<Arc<dyn MemoryDiscoveryProvider>> {
    vec![]
}

#[async_trait]
pub trait MemoryDiscoveryProvider: Send + Sync {
    fn provider_key(&self) -> &str;
    fn vfs_discovery_rules(&self) -> Vec<MemoryDiscoveryVfsRule>;
    async fn discover_from_vfs(
        &self,
        context: MemoryDiscoveryContext,
        mounts: Vec<MemoryDiscoveryMount>,
        files: Vec<MemoryDiscoveryVfsFile>,
    ) -> Result<MemoryDiscoveryOutput, MemoryDiscoveryError>;
}
```

Provider 返回 `MemoryDiscoveryOutput { clusters, diagnostics }`，其中 source 使用受控 VFS URI，例如 `agent://` 与 `agent://MEMORY.md`。`MemoryDiscoveryMount` 是 runtime mount summary，只包含 mount id、provider、display name、capabilities、purpose、owner kind 和脱敏 metadata summary。

### 3. Contracts

- `MemoryDiscoveryVfsRule` 只声明宿主可从当前 active VFS 读取的候选文件，字段包括 `exact_paths`、`file_names`、`scan_prefixes`、`recursive`、`max_depth`、`max_files`、`max_size_bytes`。
- `MemoryDiscoveryVfsFile` 只携带 bounded text content、mount id、path、rule key 与 size；宿主按 rule 读取文件，不扩大 mount capability。
- `DiscoveredMemorySource` 必须携带 `source_key`、`source_uri`、`index_uri`、`mount_id`、`scope`、`capabilities`、`format`、`index_status`、`trust_level`，可携带 `bounded_index_content`。
- `source_uri` / `index_uri` 必须是受控 VFS URI；本机绝对路径、`file://`、drive path、反斜杠路径和 `..` path segment 不进入 discovery output。
- first-party ProjectAgent provider 默认发现 runtime `agent://` source；`MEMORY.md` 缺失时仍返回 source，并以 `index_status=missing` 表达索引状态。

### 4. Validation & Error Matrix

| 条件 | 行为 |
| --- | --- |
| provider key 为空或重复 | integration registration 失败 |
| provider 声明 VFS rule 但 session 缺少 active VFS / VfsService | 生成 `vfs_context_missing` diagnostic，跳过该 provider |
| index 文件超过 `max_size_bytes` | source 标记 `index_status=too_large`，不附带正文 |
| source URI 或 index URI 不是受控 VFS URI | normalization 丢弃该 source 并生成 `invalid_memory_source_uri` diagnostic |
| 同一 provider 返回重复 source key | 保留首个 source，生成 `duplicate_source_key` diagnostic |

### 5. Good / Base / Bad Cases

- Good：ProjectAgent runtime VFS 存在 `agent` mount，provider 返回 `source_uri=agent://`、`index_uri=agent://MEMORY.md`、capabilities 来自该 mount。
- Base：`agent://MEMORY.md` 不存在时，provider 仍返回 source，Agent 可通过普通 VFS 文件工具创建索引和 topic 文件。
- Bad：provider 返回 `C:\workspace\memory` 或 `file:///tmp/MEMORY.md` 作为 source URI，normalization 必须将其从 inventory 移除。

### 6. Tests Required

- SPI 测试覆盖 VFS URI validation、duplicate source normalization、rule default bound。
- Integration registration 测试覆盖 memory provider 收集、空 key 和重复 key。
- First-party provider 测试覆盖 `MEMORY.md` 缺失、bounded index 附加、topic body 不注入。
- Runtime projection 测试覆盖 mount summary 不含 `root_ref` / `backend_id` / workspace root。

### 7. Ownership Pair

#### Provider-owned

```rust
provider.discover_from_vfs(context, mount_summaries, bounded_files).await
```

#### Platform-owned

```text
active VFS -> bounded file scan -> normalized MemoryDiscoveryOutput -> memory_context ContextFrame -> connector system context
```

这样分层的原因是不同 integration 可以贡献 memory inventory 识别逻辑，但读写行为、权限边界和 prompt 注入生命周期必须继续由平台的 VFS、ContextFrame 和 connector runtime 统一控制。

## Integration Embedded Shared Library Assets

`AgentDashIntegration::library_asset_seeds()` 是 native integration 向 Shared Library 贡献内嵌资产的启动期入口。

约束：

- integration 只声明 `IntegrationLibraryAssetSeed`，不直接写数据库，也不修改 Project 运行配置。
- 宿主统一计算 digest、设置 `scope=system`、`source=integration_embedded` 和 `source_ref=integration:{integration_name}:{asset_type}:{key}`。
- seed payload 必须通过 Shared Library typed validator；例如 runtime extension 走 `extension_template` schema。
- `IntegrationLibraryAssetSeed.version` 表达单个 embedded asset 的版本，不表达 integration 包版本；integration 包版本只用于审计或发布节奏。
- 宿主在启动 seed 阶段校验 version/digest 不变量：payload digest 变化时 asset version 必须提升，asset version 提升时 payload digest 也必须变化。
- 该入口继承 native integration 的重启边界：管理员安装/更新 integration 后重启服务，用户再从 Marketplace 显式安装到 Project。

## Marketplace Source Providers

### 1. Scope / Trigger

`AgentDashIntegration::marketplace_source_providers()` 是 native integration 向 Marketplace 贡献外部目录来源的启动期入口。该入口只声明发现能力，原因是企业 Skill / MCP 分发服务需要跟随企业版源码发布节奏装配和回滚，而 Marketplace / Shared Library 仍保持平台内统一的导入、安装、版本和审计事实。

### 2. Signatures

```rust
fn marketplace_source_providers(&self) -> Vec<Arc<dyn MarketplaceSourceProvider>> {
    vec![]
}

#[async_trait]
pub trait MarketplaceSourceProvider: Send + Sync {
    fn descriptor(&self) -> MarketplaceSourceDescriptor;
    async fn list_assets(&self, query: MarketplaceAssetQuery)
        -> Result<MarketplaceAssetPage, MarketplaceSourceError>;
    async fn get_asset_detail(&self, external_id: &str)
        -> Result<MarketplaceAssetDetail, MarketplaceSourceError>;
    async fn fetch_asset_payload(&self, external_id: &str)
        -> Result<MarketplaceFetchedAsset, MarketplaceSourceError>;
}
```

`MarketplaceAssetQuery` 的分页字段为 `cursor`、`limit`，返回页使用 `next_cursor`。分页能力在 SPI 层固定，原因是企业目录通常已有自己的搜索索引，宿主不能要求 provider 一次返回完整目录。

### 3. Contracts

`MarketplaceSourceDescriptor` 必须提供：

- `source_key`：全局唯一来源键。
- `display_name` / `description`：UI 展示文本。
- `provider_kind`：`integration` 或 `builtin`。
- `supported_asset_types`：首期只允许 `skill_template` 与 `mcp_server_template`。
- `trust_level`：`curated` / `organization` / `public_index`。
- `enabled`：来源当前是否启用。

`MarketplaceAssetListing` 必须携带 `source_key`、`external_id`、`asset_type`、`key`、`display_name`、`version`，并可携带 `digest`、`updated_at`、`tags`、`author` 与安装需求摘要。`version` 和可选 `digest` 属于远端来源身份的一部分，后续 import / refresh 用它们判断上游是否有新版本；Project 运行事实仍由安装后的 Project Asset 持有。

`mcp_server_template` 的 fetched payload 必须表达为 HTTP/SSE `transport_template`、`parameter_schema` 与公开 `capabilities`。Provider 不传递 header/env/credential 值、本机路径或 stdio 进程模板，原因是 native integration 只贡献目录发现，用户连接输入必须在 Shared Library install 阶段解析成 Project MCP Preset。

### 4. Validation & Error Matrix

| 条件 | 行为 |
| --- | --- |
| `source_key` 为空 | `collect_integration_registration` 返回 `InvalidMarketplaceSourceDescriptor` |
| `source_key` 重复 | `collect_integration_registration` 返回 `DuplicateMarketplaceSourceKey`，错误包含 first owner 与 second owner |
| `supported_asset_types` 为空 | `InvalidMarketplaceSourceDescriptor` |
| `supported_asset_types` 包含非 Skill/MCP 类型 | `InvalidMarketplaceSourceDescriptor` |
| provider 请求参数不合法 | provider 返回 `MarketplaceSourceError::BadRequest` |
| 外部 asset 不存在 | provider 返回 `MarketplaceSourceError::NotFound { source_key, external_id }` |
| 来源暂不可用 | provider 返回 `MarketplaceSourceError::Unavailable` |

### 5. Good / Base / Bad Cases

- Good：企业 integration 注册 `corp-skill-hub`，支持 `skill_template`，宿主启动收集成功，后续 API 从 registry 读取来源。
- Base：first-party fixture marketplace source 提供一个 HTTP MCP template，用于证明 source/list/detail/fetch/import 合同可实现且不会改变 Project 运行事实。
- Bad：两个 integration 同时声明 `corp-skill-hub`，宿主启动失败并指出两个 owner。

### 6. Tests Required

- `collect_integration_registration` 成功收集 marketplace source provider。
- 重复 `source_key` 触发 `DuplicateMarketplaceSourceKey`。
- 空 `source_key`、空 `supported_asset_types`、非 Skill/MCP 类型触发 descriptor 错误。
- first-party integration 至少有一个示例 source，避免企业仓成为第一个 contract 消费者。
- contract crates 编译不需要 HTTP client、database、web framework 或 MCP runtime。

### 7. Ownership Pair

#### Provider-owned

```rust
// 外部目录发现、分页、详情和拉取候选 payload
provider.list_assets(MarketplaceAssetQuery { cursor, limit, .. }).await
```

#### Platform-owned

```rust
// 导入、安装、版本状态和 Project 运行事实
External listing -> typed import -> LibraryAsset -> Shared Library install -> Project Asset
```

这样分层的原因是企业服务协议差异应收束在 provider，平台内资产版本、digest、source-status 和运行事实必须继续由 Shared Library / Project Asset 统一维护。

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
- 集成冲突时隐式覆盖
