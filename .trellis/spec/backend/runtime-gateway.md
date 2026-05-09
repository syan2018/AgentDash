# Runtime Gateway

Runtime Gateway 是 application 层的统一运行时能力调用入口。它承载 Session Runtime Action 与 Setup Action 的 Invocation / Provider / Result / Trace 协议，避免 Canvas、Agent、Workflow、环境准备 UI 各自维护一套能力调用道路。

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
pub const WORKSPACE_DETECT_ACTION: &str = "workspace.detect";
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

HTTP route 保持原响应契约，不能让前端看到 Gateway 内部 envelope：

```rust
POST /api/projects/{project_id}/mcp-presets/probe
Request: McpTransportConfig
Response: ProbeResult

POST /api/projects/{project_id}/workspaces/detect
Request: DetectWorkspaceRequest
Response: DetectWorkspaceResponse
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

### 6. Tests Required

- Provider 单测：非法 input shape 返回 `InvalidRequest`。
- Provider 单测：stdio probe 在无 relay 时返回 `ProbeResult::Error` payload，而不是 HTTP/Gateway 错误。
- Provider 单测：`workspace.detect` 的空 `root_ref` 返回 `InvalidRequest`，backend 离线返回 `Conflict`。
- Provider 单测：`workspace.detect` 成功返回 `WorkspaceDetectionResult` payload，至少覆盖 Git identity 推断。
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
