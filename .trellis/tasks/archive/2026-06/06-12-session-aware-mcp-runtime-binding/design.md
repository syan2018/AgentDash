# 会话感知 MCP 运行时绑定设计

## Architecture Overview

本功能新增一条运行时投影链路：

```text
McpPreset(runtime_binding + static transport)
  + SessionRuntimeMcpContext(final VFS main mount)
  -> Resolved SessionMcpServer
  -> CapabilityState.tool.mcp_servers
  -> direct / relay MCP discovery and call
```

设计重点是保持三层事实分离：

- Project asset：`McpPreset` 保存可复用配置与绑定声明。
- Session surface：VFS `main` mount 保存本次会话最终 workspace / binding 事实。
- Runtime result：`SessionMcpServer` 保存本次会话已解析的连接参数，只存在于 session/capability/runtime 投影中。

## Domain Model

在 `agentdash-domain::mcp_preset` 增加结构化值对象：

```rust
pub struct McpRuntimeBindingConfig {
    pub mount_id: Option<String>, // MVP 默认 main
    pub bindings: Vec<McpRuntimeBindingRule>,
}

pub struct McpRuntimeBindingRule {
    pub source: McpRuntimeBindingSource,
    pub target: McpRuntimeBindingTarget,
    pub required: bool,
}

pub enum McpRuntimeBindingSource {
    VfsRootRef,
    VfsBackendId,
    WorkspaceId,
    WorkspaceBindingId,
    WorkspaceIdentity { path: Vec<String> },
    WorkspaceDetectedFact { path: Vec<String> },
}

pub enum McpRuntimeBindingTarget {
    HttpQuery { name: String },
    HttpHeader { name: String },
    StdioEnv { name: String },
    StdioCwd,
}
```

序列化建议使用 `snake_case` discriminated union，前端可生成稳定 TS：

```json
{
  "mount_id": "main",
  "bindings": [
    {
      "source": { "kind": "workspace_detected_fact", "path": ["p4", "client_name"] },
      "target": { "kind": "http_query", "name": "p4_client" },
      "required": true
    }
  ]
}
```

`McpPreset` 增加：

```rust
pub runtime_binding: Option<McpRuntimeBindingConfig>
```

空值表示静态 preset，不走任何运行时绑定逻辑。

## Persistence

新增 migration：

```sql
ALTER TABLE mcp_presets
    ADD COLUMN IF NOT EXISTS runtime_binding text;
```

原因：

- 复杂值对象按当前数据库规范以 JSON text 存储。
- 列名表达业务语义，不加 `_json`。
- 不修改既有 migration 历史。

Repository 更新点：

- `COLS` 增加 `runtime_binding`。
- create/update 绑定 `Option<String>`。
- row mapping 解析 `Option<McpRuntimeBindingConfig>`，错误上下文使用 `mcp_presets.runtime_binding`。
- builtin/user clone 保留 runtime binding。

## Contracts And Frontend

`agentdash-contracts::mcp_preset` 增加 DTO：

- `McpRuntimeBindingConfigDto`
- `McpRuntimeBindingRuleDto`
- `McpRuntimeBindingSourceDto`
- `McpRuntimeBindingTargetDto`

`McpPresetResponse`、`CreateMcpPresetRequest`、`UpdateMcpPresetRequest` 携带 `runtime_binding`。

前端更新：

- `features/mcp-shared/helpers.ts` 扩展 form state、create/update patch、validation。
- `McpPresetCategoryPanel` 增加高级绑定配置 UI。
- `ProjectAgent` 内的 quick-create 表单可先不暴露高级编辑，但必须通过 generated type 保留字段，不做逐字段丢弃。

UI MVP 控件建议：

- source：select + path segmented preset，先提供常用 P4 source。
- target：select + name input。
- required：checkbox。
- mount_id：默认 `main`，高级输入可留但默认不显示。

## Session Runtime Context

新增 application 层类型：

```rust
pub struct SessionRuntimeMcpContext<'a> {
    pub vfs: Option<&'a Vfs>,
}
```

解析规则：

1. 找到 `runtime_binding.mount_id.unwrap_or("main")`。
2. 从 final session VFS 中读取该 mount。
3. 从 mount 字段读取：
   - `root_ref`
   - `backend_id`
4. 从 mount metadata 读取：
   - `workspace_id`
   - `workspace_binding_id`
   - `workspace_identity_payload`
   - `workspace_detected_facts`

需要修改 workspace mount metadata，把 selected binding facts 加进去：

```json
{
  "workspace_id": "...",
  "workspace_identity_kind": "p4_workspace",
  "workspace_identity_payload": {},
  "workspace_binding_id": "...",
  "workspace_detected_facts": {}
}
```

该 metadata 只表达本次 selected binding；如果未来 workspace resolution 从 default binding 升级为 online binding，resolver 仍只消费最终 VFS，不需要关心选择算法。

## Runtime Binding Resolver

在 `agentdash-application::mcp_preset` 增加 resolver：

```rust
pub fn resolve_preset_mcp_server(
    preset: &McpPreset,
    context: Option<&SessionRuntimeMcpContext<'_>>,
) -> Result<SessionMcpServer, McpRuntimeBindingError>
```

行为：

- 无 `runtime_binding`：返回当前静态转换结果。
- 有 `runtime_binding` 且无 context：返回 `MissingSessionContext`。
- source 不存在：
  - required = true：错误。
  - required = false：跳过该 rule。
- target 与 transport 不匹配：
  - HTTP query/header 只能作用于 HTTP/SSE。
  - stdio env/cwd 只能作用于 stdio。
  - 不匹配返回配置错误。

HTTP query 注入要求：

- 使用 URL parser，不做字符串拼接。
- 同名 query 覆盖或追加需要固定策略。推荐覆盖，原因是 runtime binding 是会话事实，应优先于静态默认值。

Header 注入要求：

- header name/value 在 resolver 阶段做基本非空校验。
- rmcp reserved header 冲突由 HTTP client custom_headers 保持最终校验。

stdio cwd 要求：

- `McpTransportConfig::Stdio` 当前没有 cwd 字段，需要扩展 transport 模型：

```rust
Stdio {
    command: String,
    args: Vec<String>,
    env: Vec<McpEnvVar>,
    cwd: Option<String>,
}
```

这会触及 domain、contracts、relay protocol、本机 parser、前端 editor。由于项目未上线且不做兼容分支，直接把 stdio cwd 纳入目标形态。

## Construction Flow

当前 `build_project_agent_context` 过早把 `mcp_preset_keys` 解析成 `SessionMcpServer`，此时还没有 final VFS。需要调整为：

1. ProjectAgent context 只解析/校验 preset keys，并保留 preset 或 key 列表。
2. Owner bootstrap 先构建 final VFS。
3. 基于 final VFS 创建 `SessionRuntimeMcpContext`。
4. 将 agent-level presets 和 capability resolver available presets 都通过 runtime binding resolver 转换。
5. `normalize_owner_bootstrap_mcp_projection` 去重并写入 capability state。

为减少改动，可先让 `AgentLevelMcp` 从 `preset_mcp_servers` 改成 `preset_mcp_presets: Vec<McpPreset>`，在 assembler 内统一 resolve。

`CapabilityResolverInput` 增加：

```rust
pub mcp_runtime_context: Option<SessionRuntimeMcpContextOwned>
```

或更小改法：在 assembler 调用 `CapabilityResolver` 前先把 `AvailableMcpPresets` 解析成 `AvailableMcpServers`。推荐前者，因为 resolver 当前职责就是从 `mcp:<preset>` 解析 MCP 候选，保持能力解析入口集中。

## Direct MCP

`agentdash-executor::mcp::direct` 更新：

- `McpHttpServerSpec` 携带 headers。
- `parse_http_session_server` 解析 HTTP/SSE URL + headers。
- `connect_http_server` 接收 headers 并构造 `StreamableHttpClientTransportConfig::custom_headers`。
- Sse transport 当前 direct parser 可按现有能力决定是否支持；如果 direct 仍只支持 HTTP，需要明确跳过 Sse 并保留现状。

## Relay MCP Protocol

当前 `CommandMcpListToolsPayload` / `CommandMcpCallToolPayload` 只有 `server_name`。升级为：

```rust
pub struct CommandMcpListToolsPayload {
    pub server: McpServerDeclarationRelay,
}

pub struct CommandMcpCallToolPayload {
    pub server: McpServerDeclarationRelay,
    pub tool_name: String,
    pub arguments: Option<Map<String, Value>>,
}

pub struct McpServerDeclarationRelay {
    pub name: String,
    pub transport: McpTransportConfigRelay,
}
```

云端 `McpRelayProvider`：

- `list_relay_tools` 应接收 resolved relay servers，而非 names。
- backend 选择仍可先按 `server.name` 找 capability provider；但发送命令时必须带 resolved transport。
- 后续可进一步按 `context.session.backend_execution.backend_id` 精确路由；本任务至少避免 transport 丢失。

本机 `McpClientManager`：

- 支持 transient/session-scoped resolved server declaration。
- 连接池 key 不能只用 `server_name`。建议 key = `server_name + stable hash(transport)`。
- capability_entries 仍来自本机静态 config，用于发现“该 backend 可以承载哪些 server name”。
- list/call 接收到 resolved server 后，用 resolved transport 建连；静态 config 仅作为该 backend 声明能力和默认 transport 的来源。

本机 prompt 路径已有 `mcp_servers` 明细，parser 也需要支持新增 stdio `cwd`。

## Local MCP Client

`agentdash-local::mcp_connect::mcp_http_worker` 改为接收 headers。

`McpClientManager`：

- `ensure_connected` 改为按 resolved declaration 获取 transport。
- stdio spawn 时设置 `current_dir(cwd)`。
- HTTP/SSE worker 传入 headers。
- `close` 可支持按 server name 关闭所有 hash 连接，或新增 internal close key；MVP 保持按 server name 关闭匹配前缀。

## Probe Semantics

普通 preset probe 没有 session context：

- 无 runtime binding：按当前逻辑 probe。
- 有 runtime binding：
  - 如果所有 required=false 且静态 transport 可用，可 probe 静态部分。
  - 如果存在 required=true，返回 `Unsupported { reason }`。

未来可以新增带 workspace context 的 probe endpoint，但本任务不要求。

## Error Semantics

运行时绑定错误必须包含：

- preset key
- rule index
- source path
- target kind/name
- 缺失变量或 transport mismatch 说明

示例：

```text
MCP preset `p4-local` runtime_binding[0] 缺少 required source workspace.detected_facts.p4.client_name
```

## Compatibility And Migration

项目未上线，不需要旧客户端兼容方案。仍需保持：

- DB migration 只新增目标字段，不改历史 migration。
- `runtime_binding = null` 的既有 preset 行为不变。
- generated TS drift 由 `contracts:check` 管理。

## Validation Strategy

Backend:

- domain serialization tests for runtime binding DTO/value object。
- repository roundtrip test for `runtime_binding`。
- resolver tests:
  - static preset unchanged
  - P4 client name to HTTP query
  - workspace root to header
  - P4 client to stdio env
  - root_ref to stdio cwd
  - missing required variable fails
  - target/transport mismatch fails
- assembler/capability tests for `mcp_preset_keys` and `mcp:<preset>` equality。
- relay protocol serialization tests for resolved server declaration。
- local MCP manager tests for hash key isolation and stdio cwd/env mapping。

Frontend:

- helper tests for form create/update/validation.
- focused component test for adding/removing binding rules.

Commands:

```powershell
pnpm run migration:guard
pnpm run contracts:check
cargo test -p agentdash-domain mcp_preset
cargo test -p agentdash-application mcp_preset
cargo test -p agentdash-application capability
cargo test -p agentdash-relay mcp
cargo test -p agentdash-local mcp
pnpm run frontend:check
```

## Risks And Tradeoffs

- `CapabilityResolver` 当前是接近纯函数的 resolver；引入 runtime context 会扩大输入面。接受该 tradeoff 的原因是 `mcp:<preset>` 本身就是 session capability projection，必须与 final VFS 一致。
- relay backend 选择仍按 server name 找 provider，无法保证同名 server 在多 backend 多 workspace 时完全精确。更完整的方案是让 `RelayMcpCallContext` 携带 backend execution placement 并优先路由到 session backend；可作为同任务实现项或后续强化项。
- stdio `cwd` 会扩大 transport schema 改动面，但用户明确提到 cwd，且项目未上线，直接建模比用 env 曲线表达更正确。
