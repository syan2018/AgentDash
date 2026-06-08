# Research: Canvas runtime invoke 现有路径（Child 2 canvas 分支复用目标）

- **Query**: canvases.rs runtime-invoke 端点背后的 application service 签名/参数；canvas binding/bridge 调用路径；Child 2 应复用哪个函数
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### 端点：`invoke_canvas_runtime_action`

`crates/agentdash-api/src/routes/canvases.rs` L249-283。路由 `POST /canvases/{id}/runtime-invoke`（L71-74）。

```rust
pub async fn invoke_canvas_runtime_action(
    State(state), CurrentUser(current_user),
    Path(id): Path<String>,
    Json(req): Json<CanvasRuntimeInvokeRequest>,
) -> Result<Json<RuntimeInvocationResult>, ApiError>
```

关键：**它没有独立的 canvas application service——它直接构造 RuntimeInvocationRequest 调 RuntimeGateway**（与 extension action 同一个 gateway）：

```rust
let action_key = RuntimeActionKey::parse(req.action_key)?;                 // L265-266
let request = RuntimeInvocationRequest::new(
    action_key,
    RuntimeActor::UserCanvas { session_id, canvas_id: Some(canvas.id) },   // L269-272
    RuntimeContext::Session { session_id, project_id: Some(canvas.project_id), workspace_id: None }, // L273-277
    req.input,
);
let result = state.services.runtime_gateway.invoke(request).await?;        // L281
```

即：canvas 的 "runtime invoke" 就是「以 `UserCanvas` actor 身份，对当前 project 的 RuntimeGateway 发一个 runtime action」。它复用的 provider 与 extension/MCP 同池（`mcp.list_tools` / extension actions 等都在 surface 里，见 runtime.rs 测试 L288 用 `mcp.list_tools`）。

### canvas runtime bridge surface

`get_canvas_runtime_snapshot`（canvases.rs L222-247）→ `build_canvas_runtime_bridge_surface`（L285-303）：

```rust
state.services.runtime_gateway.surface_for_actor(
    RuntimeActor::UserCanvas { session_id, canvas_id: Some(canvas.id) },
    RuntimeContext::Session { session_id, project_id: Some(canvas.project_id), workspace_id: None },
)
```
返回 `CanvasRuntimeBridgeSnapshot::enabled(surface)`。`CanvasRuntimeSnapshot` / `CanvasRuntimeBridgeSnapshot` 定义在 `crates/agentdash-application/src/canvas/runtime.rs` L11-66。`surface` 列出 canvas 能调的 runtime actions。

> **结论 / 对 Child 2 的含义**：canvas 没有一个像 `invoke_canvas_runtime_action(...)` 的应用层函数可包——端点本身就是「parse action_key + 构造 UserCanvas request + gateway.invoke」三行。Child 2 的 canvas invoke 分支若要「复用现有 canvas service」，实质是**复用同一个 `runtime_gateway.invoke`，只是 actor 用 `UserCanvas` 而非 `AgentSession`/`SessionUser`**。但 agent 是以 session 身份调用的，actor 应是 `AgentSession`（见 gateway 校验：actor.session_id 必须 == context.session_id）。
>
> PRD/parent design D2 说 canvas invoke「包现有 canvas application service，不另起 authoring」。现实是：canvas 侧没有专门的 invoke service 层，create/update 才有（见下）。Child 2 的 canvas 分支若指 read/present/runtime-invoke，直接 gateway.invoke 即可；若 design 想让 agent 走 binding，则 binding 不是可执行 operation（见 02 文档）。**这个语义错位需要 design 决断。**

### canvas authoring service（create/update/promote，**不在本轮 invoke 范围**）

`crates/agentdash-application/src/canvas/`（mod.rs 导出）：
- `create_project_canvas(repos, CreateCanvasInput)`、`update_canvas_record(repos, canvas, CanvasMutationInput)`、`delete_canvas_record`、`list_project_canvases`、`load_canvas_by_ref`（canvases.rs L7-13 引用）。
- `management.rs` / `promotion.rs` / `visibility.rs` / `tools.rs`（agent canvas 工具：`StartCanvasTool` / `BindCanvasDataTool` / `PresentCanvasTool` / `ListCanvasesTool`）。
- D2 明确：authoring（create/update）作为可选尾段或后续任务，**本轮 invoke 不另起 authoring 路径**。

### CanvasRuntimeInvokeRequest DTO

由 `crate::dto` 导出（canvases.rs L28-31）。字段（据 L258-278 用法）：`session_id: String`、`action_key: String`、`input: Value`。位置 `crates/agentdash-api/src/dto/`（canvas 相关 DTO）。

## Caveats / Not Found

- 没有名为 `invoke_canvas_runtime_action` 的 **application service**；同名只是 API handler。canvas runtime invoke = 通用 `runtime_gateway.invoke` 的一个 actor 变体。
- Child 1 canvas module 投影出的 operation 是 `binding.{alias}`（不可执行），与 runtime-invoke 端点接受的 `action_key`（runtime action）不是同一概念。design 必须明确 canvas invoke 分支接受什么 operation_key、映射到哪个 action_key/actor。
