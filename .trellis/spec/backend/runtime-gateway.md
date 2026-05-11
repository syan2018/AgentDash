# Runtime Gateway

Runtime Gateway 是 application 层的统一运行时能力调用入口。它承载 Session Runtime Action 与 Setup Action 的 Invocation / Provider / Result / Trace 协议，避免 Canvas、Agent、Workflow、环境准备 UI 各自维护一套能力调用道路。

## Scenario: Session Runtime Action 通过 Runtime Gateway 调用 MCP 工具

### 1. Scope / Trigger

- Trigger: 新增或迁移普通运行时能力调用，例如 `mcp.list_tools`、`mcp.call_tool`、后续 Agent tool adapter / Canvas bridge / Workflow node 运行期调用。
- Scope: Runtime Gateway 负责 actor/context 校验、provider 路由、trace 回填与 provider 错误归一化；SessionHub 负责读取 session 当前或最近的 `CapabilityState` 并复用既有 MCP direct/relay discovery。
- Boundary: Session Runtime Action 必须绑定 `RuntimeContext::Session { session_id, ... }`，调用 actor 必须携带同一个 `session_id`。不得使用 `PlatformUser` / `EnvironmentSetup` 绕过 session surface 调用普通 runtime action。

### 2. Current Actions

```rust
pub const MCP_LIST_TOOLS_ACTION: &str = "mcp.list_tools";
pub const MCP_CALL_TOOL_ACTION: &str = "mcp.call_tool";
```

`mcp.list_tools` 输入：

```rust
pub struct McpListToolsInput {
    pub server_names: Option<Vec<String>>,
}
```

`mcp.list_tools` 输出：

```rust
pub struct McpListToolsOutput {
    pub tools: Vec<RuntimeMcpToolDescriptor>,
}

pub struct RuntimeMcpToolDescriptor {
    pub runtime_name: String,
    pub server_name: String,
    pub tool_name: String,
    pub uses_relay: bool,
    pub description: String,
    pub parameters_schema: serde_json::Value,
}
```

`mcp.call_tool` 输入：

```rust
pub struct McpCallToolInput {
    pub runtime_name: Option<String>,
    pub server_name: Option<String>,
    pub tool_name: Option<String>,
    pub arguments: Option<serde_json::Value>,
}
```

`mcp.call_tool` 输出：

```rust
AgentToolResult
```

调用方应优先使用 `runtime_name`；若希望显式路由，也可以同时提供 `server_name` + `tool_name`。`arguments` 只允许 JSON object 或 null。

### 3. Capability Contract

- Session MCP surface 的唯一能力来源是 `CapabilityState`。
- Provider 不直接读取 MCP preset、agent config 或 relay 裸命令；它通过 `RuntimeSessionMcpAccess` 进入 SessionHub。
- SessionHub 使用 `get_latest_capability_state(session_id)` 读取 active turn 或 `session_profile` 中的 canonical state。
- MCP server 列表必须来自 `CapabilityState.tool.mcp_servers`。
- direct MCP discovery 与 relay MCP discovery 必须复用 `agentdash_executor::mcp` 中的 capability-aware 入口；所有工具暴露都必须经过：

```rust
capability_state.is_capability_tool_enabled(
    capability_key,
    tool_name,
    None,
)
```

- 空 `CapabilityState` 不得暴露任何 MCP 工具，即使 MCP server 已挂载。
- `RuntimeGateway::surface_for(Session)` 目前只表达 action 粒度可用性；具体 MCP tool surface 由 `mcp.list_tools` 输出并应用 session capability policy。

### 4. Error Matrix

| Condition | Runtime error |
| --- | --- |
| Session Runtime Action 使用 Setup context | `InvalidRequest` |
| actor 未绑定 session 或 session_id 不一致 | `CapabilityDenied` |
| session 没有可用 `CapabilityState` | `Conflict` |
| `mcp.call_tool` 未提供 tool target | `InvalidRequest` |
| `arguments` 不是 object/null | `InvalidRequest` |
| 目标工具不在 capability-filtered surface 中 | `CapabilityDenied` |
| MCP discovery 连接失败 | `ProviderFailed` |
| MCP tool execute 失败 | `ProviderFailed` |

### 5. Good/Base/Bad Cases

- Good: Gateway 注册 `McpListToolsProvider` / `McpCallToolProvider`，provider 只依赖 `RuntimeSessionMcpAccess`，测试可用 fake access 验证协议行为。
- Good: SessionHub 复用既有 direct/relay MCP adapter，新增 metadata entry 只用于 Runtime Gateway descriptor 与按 `runtime_name` 调用，不复制 MCP 协议执行逻辑。
- Base: `mcp.list_tools` 可选按 `server_names` 过滤，但过滤发生在 canonical capability surface 之后。
- Bad: Gateway provider 自己解析 MCP transport、直接调用 relay command，或把 relay 返回的裸工具列表绕过 `CapabilityState` 暴露给 Canvas/Agent/Workflow。

### 6. Tests Required

- Provider 单测：`mcp.list_tools` 返回 `RuntimeMcpToolDescriptor` payload。
- Provider 单测：`mcp.call_tool` 缺少 tool target 返回 `InvalidRequest`。
- Provider 单测：`mcp.call_tool` 目标工具不可见返回 `CapabilityDenied`。
- Gateway 单测：Session Runtime Action 拒绝 setup actor、拒绝不一致 session actor。
- Executor 单测：direct/relay discovery 继续遵守 capability-aware filter。
- Check：至少运行 `cargo test -p agentdash-application runtime_gateway`、`cargo test -p agentdash-executor mcp`、`cargo check -p agentdash-application`、`cargo check -p agentdash-api`。

## Scenario: Agent 通过 RuntimeActionToolAdapter 调用 Runtime Action

### 1. Scope / Trigger

- Trigger: 需要把某个 Session Runtime Action 暴露为 Agent 可调用工具，例如后续将 `mcp.call_tool`、`context.read` 或 workflow runtime action 包装为 `AgentTool`。
- Scope: `RuntimeActionToolAdapter` 只做 AgentTool → RuntimeInvocationRequest 的协议转换；真实授权、provider 路由、trace 和错误归一仍必须由 Runtime Gateway 完成。
- Boundary: Adapter 不得直接持有或调用底层 provider，也不得绕过 Gateway 构造 executor/MCP/relay 请求。

### 2. Signatures

```rust
pub struct RuntimeActionToolSpec {
    pub tool_name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub action_key: RuntimeActionKey,
    pub actor: RuntimeActor,
    pub context: RuntimeContext,
    pub target: Option<RuntimeTarget>,
    pub metadata: BTreeMap<String, serde_json::Value>,
}

pub struct RuntimeActionToolAdapter;

impl AgentTool for RuntimeActionToolAdapter {
    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError>;
}
```

Agent session 便捷构造必须使用：

```rust
RuntimeActor::AgentSession { session_id, agent_id }
RuntimeContext::Session { session_id, .. }
```

### 3. Contracts

- Adapter 的 `args` 作为 provider `input` 原样进入 `RuntimeInvocationRequest`，具体 input schema 由 `RuntimeActionToolSpec.parameters_schema` 描述。
- Adapter 不自行做 capability 裁决；Gateway 与 provider 负责拒绝不合法 context、actor、action 或目标工具。
- 若 provider 输出已经是 `AgentToolResult`，Adapter 保留其 content/is_error，并将 runtime action/trace 与 provider details 合并到 `details`。
- 若 provider 输出是普通 JSON，Adapter 将 JSON pretty-print 为 text content，并把 runtime action/trace 写入 `details`。
- 当前实现只提供可复用 adapter 基础件；是否把某个 Runtime Action 注入 session tool surface，必须由后续 capability/surface 策略显式决定，不能默认全量注入。

### 4. Validation & Error Matrix

| Condition | Agent tool error |
| --- | --- |
| `CancellationToken` 已取消 | `ExecutionFailed` |
| Gateway 返回 `InvalidRequest` | `InvalidArguments` |
| Gateway 返回 `CapabilityDenied` | `ExecutionFailed` |
| Gateway 返回 `Conflict` | `ExecutionFailed` |
| Gateway 返回 `ProviderUnavailable` | `ExecutionFailed` |
| Gateway 返回 `ProviderFailed` | `ExecutionFailed` |
| Gateway 返回 `Timeout` | `ExecutionFailed` |

### 5. Good/Base/Bad Cases

- Good: Agent tool call 进入 `RuntimeGateway::invoke(request)`，再由 Gateway 找 provider。
- Good: Adapter test 使用 fake `RuntimeProvider`，验证 provider 捕获到的 input 与 Agent tool args 一致。
- Base: Adapter 可包装 `mcp.call_tool`，但若同一 session 已暴露 per-MCP-tool schema，是否额外暴露 generic action tool 需要单独策略。
- Bad: Adapter 直接调用 `McpRelayProvider::call_relay_tool` 或 direct MCP client；这会绕过 Runtime Gateway 的 actor/context/trace 语义。

### 6. Tests Required

- Adapter 单测：AgentTool execute 调用 Gateway provider，并返回 provider 的 `AgentToolResult`。
- Adapter 单测：provider details 与 runtime trace 写入 `AgentToolResult.details`。
- Adapter 单测：未注册 action / provider error 不被吞掉，映射为 `AgentToolError`。
- Check：至少运行 `cargo test -p agentdash-application runtime_gateway`。

### 7. Wrong vs Correct

#### Wrong

```rust
impl AgentTool for RuntimeActionToolAdapter {
    async fn execute(&self, _: &str, args: Value, _: CancellationToken, _: Option<ToolUpdateCallback>) -> Result<AgentToolResult, AgentToolError> {
        self.mcp_relay.call_relay_tool("server", "tool", args.as_object().cloned()).await?;
        // ...
    }
}
```

问题：Adapter 重新拥有底层 provider 调用链，会绕过 Runtime Gateway 的 actor/context 校验、trace 回填和错误模型。

#### Correct

```rust
impl AgentTool for RuntimeActionToolAdapter {
    async fn execute(&self, _: &str, args: Value, _: CancellationToken, _: Option<ToolUpdateCallback>) -> Result<AgentToolResult, AgentToolError> {
        let request = RuntimeInvocationRequest::new(
            self.spec.action_key.clone(),
            self.spec.actor.clone(),
            self.spec.context.clone(),
            args,
        );
        let result = self.gateway.invoke(request).await?;
        // ...
    }
}
```

这样 Agent tool adapter 只是消费端桥接层，真实 runtime policy 仍集中在 Gateway/provider。

## Scenario: Setup Action 通过 Runtime Gateway 调用

### 1. Scope / Trigger

- Trigger: 新增或迁移创建 Session 前的能力调用，例如 `mcp.probe_transport`、`workspace.detect`、`workspace.browse_directory`、`environment.prepare`。
- Scope: API route 只做鉴权、请求解析、组装 `RuntimeInvocationRequest`；业务动作必须进入 application 层 `RuntimeProvider`。
- Boundary: Setup Action 不进入普通 Session Runtime Surface，也不能由 Agent/Canvas/Workflow 这类 session actor 直接调用。

### 2. Signatures

Application 层入口：

```rust
pub struct RuntimeGateway;

impl RuntimeGateway {
    pub fn register(&mut self, provider: Arc<dyn RuntimeProvider>);

    pub async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationResult, RuntimeInvocationError>;
}

#[async_trait]
pub trait RuntimeProvider: Send + Sync {
    fn action_key(&self) -> &RuntimeActionKey;
    fn action_kind(&self) -> RuntimeActionKind;

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError>;
}
```

API 层 wiring：

```rust
pub struct ServiceSet {
    pub runtime_gateway: Arc<RuntimeGateway>,
}
```

当前已注册 Setup Action：

```rust
pub const MCP_PROBE_TRANSPORT_ACTION: &str = "mcp.probe_transport";
pub const WORKSPACE_BROWSE_DIRECTORY_ACTION: &str = "workspace.browse_directory";
pub const WORKSPACE_DETECT_ACTION: &str = "workspace.detect";
pub const WORKSPACE_DETECT_GIT_ACTION: &str = "workspace.detect_git";
```

### 3. Contracts

`RuntimeInvocationRequest` 的关键字段：

- `action_key`: 点分段小写 action key，例如 `mcp.probe_transport`；反序列化必须校验格式。
- `actor`: 调用身份。Setup Action 只能使用 `PlatformUser` 或 `EnvironmentSetup`。
- `context`: 调用上下文。Setup Action 必须使用 `RuntimeContext::Setup`。
- `input`: provider 自有输入 JSON；provider 内部负责反序列化成 application/domain 类型。
- `trace`: invocation trace。provider error 不带 trace 时，Gateway 必须补回本次 invocation trace。

`mcp.probe_transport` 输入：

```rust
McpTransportConfig
```

`mcp.probe_transport` 输出：

```rust
ProbeResult
```

`workspace.detect` 输入：

```rust
pub struct WorkspaceDetectInput {
    pub backend_id: String,
    pub root_ref: String,
}
```

`workspace.detect` 输出：

```rust
WorkspaceDetectionResult
```

`workspace.detect_git` 输入：

```rust
pub struct WorkspaceDetectGitInput {
    pub backend_id: String,
    pub root_ref: String,
}
```

`workspace.detect_git` 输出：

```rust
pub struct WorkspaceDetectGitOutput {
    pub resolved_root_ref: String,
    pub is_git_repo: bool,
    pub source_repo: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
}
```

`workspace.browse_directory` 输入：

```rust
pub struct WorkspaceBrowseDirectoryInput {
    pub backend_id: String,
    pub path: Option<String>,
}
```

`workspace.browse_directory` 输出：

```rust
pub struct WorkspaceBrowseDirectoryOutput {
    pub current_path: String,
    pub entries: Vec<WorkspaceBrowseDirectoryEntry>,
}

pub struct WorkspaceBrowseDirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}
```

HTTP route 保持原响应契约，不能让前端看到 Gateway 内部 envelope：

```rust
POST /api/projects/{project_id}/mcp-presets/probe
Request: McpTransportConfig
Response: ProbeResult

POST /api/projects/{project_id}/workspaces/detect
Request: DetectWorkspaceRequest
Response: DetectWorkspaceResponse

POST /api/workspaces/detect-git
Request: DetectGitRequest
Response: DetectGitResponse

POST /api/backends/{backend_id}/browse
Request: BrowseDirectoryRequest
Response: BrowseDirectoryResponse
```

### 4. Validation & Error Matrix

| Condition | Runtime error | API mapping |
| --- | --- | --- |
| action key 未注册 | `ProviderUnavailable` | `503 Service Unavailable` |
| Setup Action 使用 Session context | `InvalidRequest` | `400 Bad Request` |
| Setup Action 使用 Agent/Canvas/Workflow actor | `CapabilityDenied` | `403 Forbidden` |
| provider 输入 JSON 无法反序列化 | `InvalidRequest` | `400 Bad Request` |
| action 目标当前状态不可用，例如 backend 离线 | `Conflict` | `409 Conflict` |
| provider 内部执行失败 | `ProviderFailed` | `500 Internal Server Error` |
| provider 超时 | `Timeout` | `503 Service Unavailable` |

`mcp.probe_transport` 的连接失败、relay 离线、目标 MCP 不可达属于 probe 业务结果，保持 `ProbeResult::Error { error }`，不升级为 HTTP error。

### 5. Good/Base/Bad Cases

- Good: API route 已完成 project 权限校验后，使用 `RuntimeActor::PlatformUser { user_id }` + `RuntimeContext::Setup { project_id, ... }` 调用 Gateway。
- Base: provider 将 `input` 反序列化为领域类型，调用已有 application 函数或 relay provider，不重写本机 handler。
- Bad: route 直接调用 `probe_transport`、直接拼 relay command，或把 Setup Action 暴露给 Session Runtime Surface。
- Good: `workspace.detect` provider 复用 `detect_workspace_from_backend` 与 `BackendTransport`，API route 只在 Gateway output 后做 matched workspace 计算和响应 DTO 映射。
- Bad: `workspace.detect` route 直接依赖 `BackendRegistry` 拼 `workspace_detect` relay command，或在 route 中复制 identity 推断逻辑。
- Good: `workspace.detect_git` provider 通过 `BackendTransport::detect_git_repo` 复用现有 workspace probe 事实，API route 只保持旧响应 DTO。
- Good: `workspace.browse_directory` provider 通过 `BackendTransport::browse_directory` 复用现有 relay/local 目录浏览实现，API route 不再直接拼 `CommandBrowseDirectory`。
- Bad: 目录浏览属于本机广域枚举能力，只能作为 Setup Action 暴露给 platform/environment actor；不得进入普通 Canvas/Agent/Workflow runtime surface。

### 6. Tests Required

- Provider 单测：非法 input shape 返回 `InvalidRequest`。
- Provider 单测：stdio probe 在无 relay 时返回 `ProbeResult::Error` payload，而不是 HTTP/Gateway 错误。
- Provider 单测：`workspace.detect` 的空 `root_ref` 返回 `InvalidRequest`，backend 离线返回 `Conflict`。
- Provider 单测：`workspace.detect` 成功返回 `WorkspaceDetectionResult` payload，至少覆盖 Git identity 推断。
- Provider 单测：`workspace.detect_git` 的空 `root_ref` 返回 `InvalidRequest`，backend 离线返回 `Conflict`，成功时返回 Git probe payload。
- Provider 单测：`workspace.browse_directory` 的 backend 离线返回 `Conflict`，成功时返回目录 entries payload。
- Gateway 单测：Setup Action 拒绝 session actor。
- API route 相关测试：原 route 响应类型保持 `ProbeResult`，`RuntimeInvocationError` 能映射到 `ApiError`。
- Check：至少运行 `cargo test -p agentdash-application runtime_gateway`、相关 API route/rpc 测试，以及受影响 crate 的 `cargo check`。

### 7. Wrong vs Correct

#### Wrong

```rust
pub async fn probe_mcp_transport_handler(...) -> Result<Json<ProbeResult>, ApiError> {
    let relay: &dyn McpRelayProvider = state.services.backend_registry.as_ref();
    Ok(Json(probe_transport(&transport, Some(relay)).await))
}
```

问题：HTTP route 重新成为业务实现主体，后续 Canvas / Workflow / Setup UI 需要重复维护同一条能力通路。

#### Correct

```rust
pub async fn probe_mcp_transport_handler(...) -> Result<Json<ProbeResult>, ApiError> {
    let request = RuntimeInvocationRequest::new(
        RuntimeActionKey::parse(MCP_PROBE_TRANSPORT_ACTION)?,
        RuntimeActor::PlatformUser { user_id: Some(current_user.user_id.clone()) },
        RuntimeContext::Setup { project_id: Some(project_id), workspace_id: None, backend_id: None, root_ref: None },
        serde_json::to_value(transport)?,
    );

    let invocation = state.services.runtime_gateway.invoke(request).await?;
    Ok(Json(serde_json::from_value(invocation.output.output)?))
}
```

这样 route 只负责 interface 层职责，Setup Action 的 provider、trace、权限分型和错误模型集中在 Runtime Gateway。
