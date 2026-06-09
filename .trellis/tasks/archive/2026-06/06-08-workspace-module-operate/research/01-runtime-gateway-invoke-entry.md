# Research: RuntimeGateway 调用入口 (invoke API + 现有 adapter)

- **Query**: RuntimeGateway 公共 invoke API；RuntimeInvocationRequest/Context/Target/Error 定义；RuntimeActionToolAdapter 怎么构造 request 拿 backend；ExtensionRuntimeActionProvider 期望的 payload 形状
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### 入口函数：`RuntimeGateway::invoke`

`crates/agentdash-application/src/runtime_gateway/gateway.rs` L82-126：

```rust
pub async fn invoke(
    &self,
    request: RuntimeInvocationRequest,
) -> Result<RuntimeInvocationResult, RuntimeInvocationError>
```

派发逻辑（这是 invoke 分支派发的现有锚点）：
1. 先按 `request.action_key` 精确匹配 `providers: HashMap<RuntimeActionKey, _>`（L89-92）。
2. 未命中再走 `dynamic_providers`，找第一个 `provider.supports(&action_key, &context)` 为 true 的（L93-98）。
3. 都没有 → `ProviderUnavailable`（L99-102）。
4. `validate_request`（L104）→ actor/context 一致性校验（见下）。
5. 再次 `provider.supports(...)` 复核，否则 `capability_denied`（L106-114）。
6. `provider.invoke(request).await`，错误经 `with_trace_if_missing` 补 trace（L116-119）。

注意：**ExtensionRuntimeActionProvider 是 dynamic provider**——它的 `marker_key` 是 `"extension.runtime_action"`，但 `supports()` 对任何"含 `.` 且 SessionRuntime"的 action_key 都返回 true（见 02 文档）。所以 agent 传的 extension action_key（如 `demo.profile`）走 dynamic 分支命中它。注册处见下。

`validate_actor_context`（gateway.rs L147-214）对 `SessionRuntime` 要求：context 必须是 `Session{session_id}` 且非空，actor 的 `session_id()` 必须等于 context 的（L161-193）。`Setup` 要求 setup context + setup actor。

### RuntimeInvocationRequest（types.rs L245-280）

```rust
pub struct RuntimeInvocationRequest {
    pub action_key: RuntimeActionKey,         // 见下，受 validate_action_key 约束
    pub actor: RuntimeActor,
    pub context: RuntimeContext,
    pub target: Option<RuntimeTarget>,        // extension 必须 Some(Backend{..})
    pub input: Value,
    pub policy: RuntimePolicy,                // default timeout 30s
    pub trace: RuntimeTrace,                  // 自动生成 trace_id/invocation_id
    pub metadata: BTreeMap<String, Value>,    // extension workspace 经此传
}
// 构造：RuntimeInvocationRequest::new(action_key, actor, context, input) → target=None, default policy/trace/metadata
```

`RuntimeActionKey`（types.rs L10-83）：newtype，`parse()` 校验**只允许小写字母/数字/`_`/`-`/`.` 分段**（`validate_action_key` L66-83）。⚠️ 这对 channel operation_key `channel.method` 没问题（点分段合法），但若 module operation_key 含大写（如 channel method `readProfile`）会**解析失败**——Child 1 投影出的 `demo.api.readProfile` 含大写 `P`，直接 `RuntimeActionKey::parse` 会报 `InvalidFormat`。invoke 分支处理 channel 时不应把整个 operation_key 塞进 `action_key`（见 02 文档：channel 走独立 `ExtensionRuntimeChannelInvoker`，不经 action_key）。

### RuntimeContext（types.rs L146-182）

```rust
pub enum RuntimeContext {
    Session { session_id: String, project_id: Option<Uuid>, workspace_id: Option<Uuid> },
    Setup    { project_id, workspace_id, backend_id: Option<String>, root_ref: Option<String> },
}
```
- extension/canvas invoke 都用 `Session{..}`，**project_id 必须 Some**（ExtensionRuntimeActionProvider 强制，见 02 文档 `session_project`）。

### RuntimeTarget（types.rs L184-193）

```rust
pub enum RuntimeTarget {
    CurrentSession,
    Backend { backend_id: String },   // extension action 唯一接受的形态
    Workspace { workspace_id: Uuid },
    McpServer { name: String },
    Http { url: String },
    Custom { kind: String, id: String },
}
```

### RuntimeInvocationError（error.rs L13-47）

变体：`InvalidRequest` / `CapabilityDenied` / `Conflict` / `ProviderUnavailable{action_key}` / `ProviderFailed` / `Timeout`，全带 `trace: Option<Box<RuntimeTrace>>`。构造器 `invalid_request` / `capability_denied` / `provider_failed` / `conflict` / `timeout`（L53-100）。`kind()` L102 映射到 `RuntimeInvocationErrorKind`。

`RuntimeInvocationResult`（types.rs L299-304）：`{ action_key, trace, output: RuntimeInvocationOutput }`，`output` 含 `{ output: Value, metadata: BTreeMap }`。

### 工具侧如何发起一次调用：`RuntimeActionToolAdapter`

`crates/agentdash-application/src/runtime_gateway/tool_adapter.rs`：

- 持 `gateway: Arc<RuntimeGateway>` + `spec: RuntimeActionToolSpec`（L58-62）。
- `RuntimeActionToolSpec`（L16-26）携带 `action_key / actor / context / target / metadata` —— **这些在装配工具时就固定**（per-action 一个工具）。`runtime_session(..)` 构造器（L28-56）默认 `target: None`、`context: Session{project_id: None}`。
- `execute()`（L84-112）：把工具入参 `args` 当作 `input`，`RuntimeInvocationRequest::new(spec.action_key, spec.actor, spec.context, args)`，再 `request.target = spec.target.clone()` / `request.metadata = spec.metadata.clone()`，然后 `gateway.invoke(request).await`。
- 错误映射 `runtime_error_to_tool_error`（L140-151）：`InvalidRequest → AgentToolError::InvalidArguments`，其余 → `ExecutionFailed`。
- 结果整形 `invocation_result_to_tool_result`（L115-138）：若 provider 输出本身是序列化的 `AgentToolResult` 就解包，否则 pretty-print；并把 `runtime_action / runtime_trace / provider_details` 塞进 `details`（L132-138，**trace 审计的现有落点**）。

> 对 Child 2 的含义：`RuntimeActionToolAdapter` 是 **per-action 静态工具**（action_key/target 编译期定）。`workspace_module_invoke` 是 **单个元工具**，要在 `execute()` 运行时根据 `module_id + operation_key` 动态决定 action_key/target/分支，不能复用 `RuntimeActionToolSpec`。可复用的是「构造 RuntimeInvocationRequest → gateway.invoke → 整形结果/错误」这套链路。

### ExtensionRuntimeActionProvider 注册位置（dynamic provider）

`grep RuntimeGateway::new / register_dynamic`：provider 装配在 `crates/agentdash-application/src/workflow/frame_construction/mod.rs`（出现在 workspace_module grep 命中里）与 API bootstrap。`state.services.runtime_gateway` 是 `Arc<RuntimeGateway>`，API 路由直接 `state.services.runtime_gateway.invoke(request)`（见 03 文档 canvas 路由 L281、extension 路由 L164）。Child 2 的元工具若在 application 层，需要一个 `Arc<RuntimeGateway>` 句柄注入到 `RelayRuntimeToolProvider`（目前该 provider 不持 gateway，见 04 文档）。

## Caveats / Not Found

- `RuntimeActionKey::parse` 拒绝大写：channel method 名常含驼峰（`readProfile`），**不能**把 operation_key 直接当 action_key 解析。runtime_action 的 action_key（Child 1 投影自 `action.action_key`）按约定是点分小写，可直接解析。
- gateway 没有「按 operation 反查归属 module」的能力；归属校验必须在元工具/服务层先做（R2），gateway 只按 action_key 找 provider。
