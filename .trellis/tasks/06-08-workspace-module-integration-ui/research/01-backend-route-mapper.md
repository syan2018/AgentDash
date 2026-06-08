# Research: extension-runtime HTTP 路由全链路（GET /projects/{id}/workspace-modules 样板）

- **Query**: 现有 extension-runtime HTTP 路由 handler + mapper + 路由注册 + repos/services 注入
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `crates/agentdash-api/src/routes/extension_runtime.rs` | handler + router() 定义 |
| `crates/agentdash-api/src/dto/extension_runtime.rs` | projection→Response mapper |
| `crates/agentdash-api/src/dto/mod.rs:12,42` | `mod extension_runtime; pub use extension_runtime::*;` |
| `crates/agentdash-api/src/routes.rs:9,91` | `pub mod extension_runtime;` + `.merge(extension_runtime::router())` |
| `crates/agentdash-api/src/routes/canvases.rs:35,55` | `list_project_canvases` GET handler（更贴近"纯只读 list"样板） |
| `crates/agentdash-application/src/canvas/management.rs:32` | `list_project_canvases(repos, project_id) -> Vec<Canvas>` |
| `crates/agentdash-application/src/workspace_module/mod.rs:176` | `build_workspace_modules(ext, canvases) -> Vec<WorkspaceModuleDescriptor>` |

### GET handler 样板（extension_runtime.rs:87-109）

```rust
pub async fn get_project_extension_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectExtensionRuntimePath>,   // { project_id: String }
) -> Result<Json<ExtensionRuntimeProjectionResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;  // Uuid::parse_str → BadRequest
    load_project_with_permission(state.as_ref(), &current_user, project_id, ProjectPermission::View).await?;
    let installations = state.repos.project_extension_installation_repo
        .list_enabled_by_project(project_id).await.map_err(ApiError::from)?;
    let projection = extension_runtime_projection_from_installations(installations)?;
    Ok(Json(extension_runtime_projection_response(projection)))
}
```

要点：
- `State(Arc<AppState>)` + `CurrentUser(current_user): CurrentUser` + `Path<...>` 三段 extractor。
- 权限：`load_project_with_permission(..., ProjectPermission::View)`（imports from `crate::auth`，extension_runtime.rs:14）。Edit 用于 uninstall（行 254）。
- repos 经 `state.repos.<repo>`；services 经 `state.services.<svc>`。

### router() 注册样板（extension_runtime.rs:50-72）

```rust
pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route("/projects/{project_id}/extension-runtime",
            axum::routing::get(get_project_extension_runtime))
        // ... 其余 POST/DELETE 路由
}
```

注册进 router：`routes.rs:91` 在 `secured_api` 链里 `.merge(extension_runtime::router())`。新路由若新建模块 `routes/workspace_modules.rs`，需：
1. `routes.rs:1-36` 区加 `pub mod workspace_modules;`（按字母序）。
2. `routes.rs:65-105` 的 `secured_api` 链加 `.merge(workspace_modules::router())`。
3. 该模块在 `authenticate_request` middleware 之下（secured），自动要求登录。

### mapper 样板（dto/extension_runtime.rs）

- `pub use agentdash_contracts::extension_runtime::{...Response};`（行 9-22）把契约类型 re-export，供 routes.rs `use crate::dto::{...}`（extension_runtime.rs:15）。
- `pub fn extension_runtime_projection_response(projection: ExtensionRuntimeProjection) -> ExtensionRuntimeProjectionResponse`（行 24）做内部投影 → 契约 DTO 的逐字段 map，含枚举 `match` 转换（行 67-71、81-84 等）。

**关键差异**：`WorkspaceModuleDescriptor`（`crates/agentdash-contracts/src/workspace_module.rs`）本身就是 `Serialize/Deserialize/TS` 契约类型，且 `build_workspace_modules` 已直接产出契约类型（`application/workspace_module/mod.rs:176` 返回 `Vec<WorkspaceModuleDescriptor>`，contracts 类型）。因此 **GET /workspace-modules 不需要额外 mapper**——handler 直接 `Ok(Json(modules))` 即可（区别于 extension_runtime 的内部 projection 需 mapper）。响应体可直接是 `Vec<WorkspaceModuleSummary>`（list）或 `Vec<WorkspaceModuleDescriptor>`。

### 新路由数据装配（建议）

GET handler 内复用现成 application 函数：
```rust
let installations = state.repos.project_extension_installation_repo
    .list_enabled_by_project(project_id).await?;
let projection = extension_runtime_projection_from_installations(installations)?;
let canvases = list_project_canvases(&state.repos, project_id).await?; // canvas/management.rs:32
let modules = build_workspace_modules(&projection, &canvases);          // workspace_module/mod.rs:176
Ok(Json(modules))
```
`list_project_canvases` 签名 `(repos: &RepositorySet, project_id: Uuid)`；`build_workspace_modules` 签名 `(ext: &ExtensionRuntimeProjection, canvases: &[Canvas])`。两者均已实现（Child 1/2）。

### 注入需求

- repos：`project_extension_installation_repo`（已用）、`canvas_repo`（经 `RepositorySet`，`canvas/management.rs:37` 已封装）。
- services：list 只读，无需 runtime_gateway / backend_registry（那些只在 invoke 路径用）。

## Caveats / Not Found

- `routes/canvases.rs:55` 的 `list_project_canvases` handler 给出了"纯 GET list + CanvasResponse::from + ProjectPermission::View"的更轻量样板，可与 extension_runtime 对照。
- 若 design 决定 list 只返回 `WorkspaceModuleSummary`（无 schema），需在 contracts 侧补一个仅含 summary 的 list 端点形态，或直接返回 descriptor 的 `summary` 字段切片——这是设计取舍，未在代码中预置。
