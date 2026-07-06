# Design

## Final Direction

`backendService` 是 extension package 内声明的本机服务能力。云端保存 manifest、artifact 与 Project installation 事实；本机 local runtime 负责解包、启动、健康检查和转发。Panel fetch route 与 Agent operation 都通过 bridge 调用同一个 service instance。

主线保持三层分工：

- Manifest/domain: 描述 `backend_services[]`、`fetch_routes[]`、`operation_catalog[]`。
- Cloud/API/Workspace Module: 解析 Project installation、校验 operation visibility、选择 target backend，并发起 bridge 调用。
- Local runtime: materialize package artifact，管理 service lifecycle，执行 HTTP/IPC 转发并返回 response/diagnostic。

## Existing Baseline

- `@agentdash/extension` 已支持 `backendService()` recipe。
- app-pipeline 已生成 `backend_services`、`fetch_routes`、`operation_catalog`。
- domain `ExtensionTemplatePayload` 已持久化这些字段。
- extension runtime projection 已投出 backend services。
- Workspace Module 目前识别 backendService operation，但 fail-closed。
- Relay protocol 已有 backendService dispatch 形状，但没有完整 handler/lifecycle。

## Runtime Identity

Service instance identity 使用：

```text
project_id + backend_id + extension_key + service_key + package_artifact.artifact_id + archive_digest
```

原因是同一个 extension 可安装在多个 Project，同一 Project 也可能在不同 backend 上运行；artifact digest 变化时必须重新 materialize。

## Package And Materialization

`backend_services[].entry` 指向 package 内文件。toolchain pack 阶段必须保证 entry 被包含在 archive 中；local runtime 解包后用 normalized path 定位 entry。

本机 cache 结构应归属 local runtime data root，例如：

```text
extension-artifacts/{artifact_id}/{archive_digest}/
backend-services/{project_id}/{extension_key}/{service_key}/
```

Materialization 输出：

```rust
struct ExtensionBackendServiceMaterialization {
    artifact_id: Uuid,
    archive_digest: String,
    extension_key: String,
    service_key: String,
    service_root: PathBuf,
    entry_path: PathBuf,
}
```

## Lifecycle

MVP lifecycle 支持 Node service：

- `start`: materialize 后启动 service process。
- `health`: 请求 `health_path`，缺省可用进程存活作为基础 readiness。
- `stop`: 停止 service process。
- `restart`: stop + start。
- `logs`: 读取 stdout/stderr ring buffer 或 bounded log file。

Service readiness：

```text
missing_artifact -> unavailable
materialize_failed -> unavailable
starting -> unavailable with retry hint
health_failed -> unavailable with diagnostic
ready -> routable
process_exited -> unavailable
```

## Bridge And Routing

Panel fetch route flow：

```text
iframe fetch helper
-> parent ExtensionWebviewPanel bridge
-> AgentRun/Project scoped extension runtime API
-> cloud relay command to selected local backend
-> local backend service manager
-> service endpoint
-> response back through same chain
```

Agent operation flow：

```text
workspace_module_invoke
-> operation_catalog lookup
-> backend_service dispatch
-> service readiness check
-> bridge invoke
-> local service manager
```

Cloud/API 不访问 `localhost`。Cloud 只发送 service invoke intent；local runtime 执行本机转发。

## Contracts

Backend service invoke request:

```rust
struct ExtensionBackendServiceInvokeRequest {
    project_id: Uuid,
    extension_key: String,
    extension_id: String,
    service_key: String,
    route: String,
    method: String,
    headers: BTreeMap<String, String>,
    body: Option<Vec<u8>>,
    trace_id: String,
}
```

Response:

```rust
struct ExtensionBackendServiceInvokeResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Option<Vec<u8>>,
    metadata: ExtensionBackendServiceInvokeMetadata,
}
```

Diagnostic:

```rust
enum ExtensionBackendServiceReadiness {
    Ready,
    MissingArtifact,
    MaterializeFailed,
    Starting,
    HealthFailed,
    ProcessExited,
    UnsupportedRuntime,
}
```

## Security And Boundary

- Panel 和 app code 只提交 route/method/body，不提交 project/backend/session 事实。
- Cloud 按 Project installation 与 runtime target 选择 backend。
- Local runtime 只运行 package artifact 中声明的 service entry。
- Route 必须匹配 manifest `backend_services[].routes` 或 `fetch_routes[]` 的 explicit target。
- Logs 和 diagnostics 必须 bounded，避免泄漏无限输出。

## Test Strategy

- Toolchain tests：backend service entry 打包、manifest validation、bad entry/bad route。
- Domain/projection tests：`backend_services` 保留、package requirement、runtime projection。
- Local runtime tests：materialize、start/health/stop、process exit diagnostic。
- Relay/API tests：backend service invoke payload roundtrip、permission/project/backend metadata。
- Workspace Module tests：ready dispatch、unavailable diagnostic、panel-only blocked。
- Browser tests：fetch route backendService target、headers/body/no-body response。

## Rollout Shape

第一阶段只支持 Node service 和 local backend。协议字段保持当前名称，新增 handler/lifecycle 不改变 `defineApp()` authoring API。service unavailable 由 local runtime readiness 统一产出结构化 diagnostic。
