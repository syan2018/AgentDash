# Design · Marketplace 后端 MVP 收束

## 1. 设计立场

后端 MVP 的完成标准不是“有外部列表 API”，而是外部资产能稳定进入平台内部事实源：

```text
MarketplaceSourceProvider
  -> external listing/detail/fetch
  -> LibraryAsset(source=remote_imported)
  -> Shared Library install
  -> Project MCP Preset / Project SkillAsset
  -> source-status / explicit refresh
```

外部来源只负责发现和拉取候选资产。导入、安装、版本、digest、Project 资源来源状态仍由 Shared Library 和 Project Asset 维护，原因是运行时只能消费安装后的 Project 资源。

## 2. 当前缺口

已落地能力：

- Source SPI 和 registry 能收集 `MarketplaceSourceProvider`。
- 外部 Marketplace API 能 list/detail/import/refresh。
- import service 能把 provider fetched payload 写成 `LibraryAsset(source=remote_imported)`。
- Skill URL Import 已通过 `materialize_remote_skill_template` 收束到 Shared Library install。

未闭环点：

- `mcp_server_template` payload 当前保存的是具体 `McpTransportConfig`，`parameter_schema` 尚未进入安装语义。
- `install_library_asset_to_project` 对 MCP 模板只复制 payload.transport，无法接收用户安装参数。
- generic install request 没有资产类型专属 options。
- 外部 MCP import 缺少对“公共模板不得携带私密/本机绑定连接材料”的显式 typed validator。
- 后续前端需要一个稳定的 wire contract 来提交 MCP 参数，而不是直接理解 `payload` 内部 JSON。

## 3. MCP Template Contract

后端应把 `mcp_server_template` 收束为“公共模板 + 安装输入”的组合。

建议领域模型：

```rust
pub struct McpServerTemplatePayload {
    pub transport_template: McpTransportTemplate,
    pub route_policy: Option<McpRoutePolicy>,
    pub parameter_schema: Option<serde_json::Value>,
    pub capabilities: Vec<String>,
}
```

`McpTransportTemplate` 使用受限模板字符串，不引入任意表达式语言：

```rust
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportTemplate {
    Http {
        url_template: String,
    },
    Sse {
        url_template: String,
    },
}
```

MVP 不把外部 remote-imported MCP 模板扩展到本机 `stdio`，原因是外部 catalog 的 stdio 模板会把本机进程执行、包下载、路径和 env 管理带入来源治理面。已有 builtin / user-authored MCP Preset 可以继续使用 stdio；外部来源 MVP 先覆盖远端 HTTP/SSE MCP 服务。

模板字符串只支持 `${parameter_key}` 占位符。解析规则：

- `parameter_schema` 使用 JSON Schema object 表达字段、类型、required、description、enum/default 等信息。
- install input 提供 `parameters: serde_json::Value`，必须是 object。
- resolver 只接受 schema 声明过的参数。
- schema `required` 字段必须全部提供。
- MVP 参数值只接受 string / number / bool，并在插值时转成字符串。
- 解析后不得残留 `${...}`。
- 解析后的 HTTP/SSE URL 必须通过现有 MCP transport validation，并额外拒绝 localhost/private network URL。

## 4. Install Contract

generic Shared Library install 入口应保留一个入口，但增加类型化安装 options。

建议 contract：

```rust
pub struct InstallLibraryAssetRequest {
    pub library_asset_id: String,
    pub target_key: Option<String>,
    pub overwrite: bool,
    pub install_options: Option<InstallLibraryAssetOptions>,
}

#[serde(tag = "asset_type", rename_all = "snake_case")]
pub enum InstallLibraryAssetOptions {
    McpServerTemplate {
        parameters: serde_json::Value,
    },
}
```

Application 层对应：

```rust
pub struct InstallLibraryAssetInput {
    pub project_id: Uuid,
    pub library_asset_id: Uuid,
    pub target_key: Option<String>,
    pub overwrite: bool,
    pub install_options: Option<InstallLibraryAssetOptions>,
}
```

安装规则：

- `asset_type != mcp_server_template` 且传入 MCP options 时返回 `BadRequest`。
- `mcp_server_template` 且 payload 有 required 参数时，缺少 options 返回 `BadRequest`。
- 无参数模板可以不传 options。
- resolver 生成具体 `McpTransportConfig` 后，复用 `McpPresetService` / Project MCP Preset repository 的 key、transport、覆盖策略和 `InstalledAssetSource` 写入语义。

这样前端只需要在安装外部 MCP 模板时提交参数 object；后端负责解析、校验和落成 Project 资源。

## 5. Import Contract

`POST /api/marketplace/external-assets/import` 不承担 Project 安装职责，仍只返回 `LibraryAssetDto`。原因是 import 是公共资产写入，install 是 Project 资源写入，两者权限和错误域不同。

MCP fetched payload 需要在 import 时完成 typed validator：

```jsonc
{
  "transport_template": {
    "type": "http",
    "url_template": "https://mcp.example.com/${workspace}/mcp"
  },
  "route_policy": "direct",
  "parameter_schema": {
    "type": "object",
    "required": ["workspace"],
    "properties": {
      "workspace": { "type": "string", "description": "Workspace slug" }
    },
    "additionalProperties": false
  },
  "capabilities": ["search", "read"]
}
```

Import validation:

- fetched `source_key`、`external_id`、`asset_type` 必须与请求一致。
- `version` 必须非空。
- payload 必须能解析为 `McpServerTemplatePayload`。
- `transport_template` 只能是 HTTP/SSE。
- `url_template` 必须是 absolute URL template，静态部分不得指向 localhost/private network。
- payload 不得携带 header/env/credential 值。
- `source_ref` 继续使用 `market:{source_key}:mcp_server_template:{external_id}`。
- `payload_digest` 继续由平台 canonical JSON 计算。

## 6. Skill Closure

Skill 后端 MVP 已由 URL Import 收束完成，但本任务需要做集成验收：

- 外部 Marketplace generic import 对 `skill_template` 仍能通过 provider fetched payload 写入 `remote_imported` LibraryAsset。
- GitHub / ClawHub / skills.sh URL Import 继续先写入 `LibraryAsset(asset_type=skill_template, source=remote_imported)`，再安装 Project SkillAsset。
- Project `SkillAsset.source` 不承载外部市场版本事实；`InstalledAssetSource` 与 `LibraryAsset.source_ref` 是来源事实。

本任务不新增真实 Skill catalog provider，原因是现有 GitHub / ClawHub / skills.sh 入口是单项 URL 定位，不是目录服务。未来如接入真实 Skill catalog，只需实现 `MarketplaceSourceProvider`，再复用现有 import/materializer。

## 7. Backend Fixture Source

为了让后端和后续前端任务能在没有企业分发服务时验证合同，建议在 first-party integration 中提供一个小型 fixture source：

- `source_key = "agentdash.dev.marketplace"` 或等价稳定 key。
- listing 返回一个 HTTP/SSE MCP template 和可选一个 SkillTemplate fixture。
- fixture 使用公开不可达但格式合法的 URL，例如 `https://mcp.example.com/${workspace}/mcp`，安装后 probe 可以返回明确连接错误，不影响 import/install 合同验证。
- fixture 必须走同一 SPI、API、import、install 链路，不绕过 provider registry。

如果实现阶段认为 first-party fixture 会影响产品默认展示，可把它做成测试 provider，只在 route/application tests 中存在；验收重点是后端金线有稳定覆盖。

## 8. Error Mapping

后端错误需要让前端能够定位安装失败原因：

- provider 请求非法、source/listing/fetch 身份不一致：`400 BadRequest`。
- 外部来源不存在或 external_id 不存在：`404 NotFound`。
- LibraryAsset identity 被其它来源占用：`409 Conflict`。
- MCP required parameter 缺失、未知参数、类型不匹配、模板未解析：`400 BadRequest`，message 包含字段路径。
- Project MCP key 冲突且 `overwrite=false`：`409 Conflict` 或现有 domain conflict 映射。
- provider unavailable：`502/503` 等现有 API error mapping 保持一致。

## 9. Frontend Handoff Boundary

后续前端任务只需要消费：

- `GET /api/marketplace/sources`
- `GET /api/marketplace/external-assets`
- `GET /api/marketplace/external-assets/{source_key}/{external_id}`
- `POST /api/marketplace/external-assets/import`
- `POST /api/projects/{project_id}/shared-library/install`
- `GET /api/projects/{project_id}/shared-library/source-status`

前端无需直接拼装 Project MCP Preset；它只提交 `library_asset_id`、`target_key`、`overwrite` 和 MCP install parameters。Project 资源来源状态继续来自 source-status，不在外部 listing 卡片上形成第二套状态模型。

## 10. 数据与迁移

本任务预计不需要新增数据库表。需要确认：

- 如果 `mcp_server_template` payload 字段从 `transport` 改为 `transport_template`，内置 seed、测试 fixture 和已有 JSON payload 样例必须同步更新。
- 预研期不做旧 payload 兼容分支；数据库 baseline / seed 数据保持当前正确形态。
- 若发现迁移 baseline 中保存了旧 enum 或 source check，按项目 migration 规则更新并运行 `pnpm run migration:guard`。

## 11. 规格更新

实现完成后至少更新：

- `.trellis/spec/backend/shared-library.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/spec/backend/capability/integration-api.md`

规格应说明为什么外部 MCP 模板分为公共模板与安装输入，为什么运行时只读取 Project MCP Preset，以及为什么外部 Marketplace refresh 不直接改 Project 资源。
