# Research: 工具如何拿到 backend target / session_id（invoke 内聚需要的内部路由）

- **Query**: Child 1 工具用 project_id_from_context；invoke 还需 backend_id + session_id；ExecutionContext 能拿到吗；RuntimeActionToolAdapter 怎么解析 backend
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### Child 1 工具的装配点与可拿到的上下文

`crates/agentdash-application/src/vfs/tools/provider.rs`（`RelayRuntimeToolProvider::build_tools`，L116-368）。Workspace Module 簇在 L333-365：

```rust
if clusters.contains(&ToolCluster::WorkspaceModule) {
    if let Some(project_id) = project_id_from_context(context) {  // L334
        let visibility = flow.workspace_module.clone();
        ... WorkspaceModuleListTool::new(installation_repo, canvas_repo, project_id, visibility)
    }
}
```

`project_id_from_context`（L371-386）：先 `hook_runtime.snapshot().run_context.project_id`，否则 `context.session.vfs.source_project_id`。

### invoke 需要的三件套，分别从哪拿（全部在 `ExecutionContext` 可达）

`ExecutionContext = { session: ExecutionSessionFrame, turn: ExecutionTurnFrame }`（`crates/agentdash-spi/src/connector/mod.rs` L121-125）。

| invoke 所需 | 来源 | file:line |
|---|---|---|
| `project_id` | `project_id_from_context(context)` | provider.rs L371-386 |
| `session_id` | `context.turn.hook_runtime.session_id()`，回退 `context.session.turn_id` | provider.rs L131-136 |
| `backend_id` | `context.session.vfs` 的 mount（见下） 或 `context.session.backend_execution.backend_id` | connector/mod.rs L75 / L80-90 |
| workspace（mount_id + root_ref） | `context.session.vfs` 选 mount | extension_runtime 路由 `select_extension_invocation_workspace` 同款逻辑 |

`ExecutionSessionFrame`（connector/mod.rs L63-83）：
- `vfs: Option<Vfs>`（L75）——含 `default_mount_id`、`mounts: Vec<Mount>`，每个 `Mount { id, backend_id, root_ref, .. }`（`Mount.backend_id` 见 mount.rs L1126）。
- `backend_execution: Option<ExecutionBackendPlacement>`（L80）——`ExecutionBackendPlacement { backend_id, lease_id, selection_mode }`（L85-90）。**仅 remote backend execution 时填**，是已 claim 的 backend lease 投影。

### 现有 extension invoke 怎么解析 backend + workspace（HTTP 侧权威样板）

API 路由 `invoke_project_extension_runtime_action`（`crates/agentdash-api/src/routes/extension_runtime.rs` L112-166）：
- backend_id 来自**请求体 `req.backend_id`**（L133）——HTTP 侧由前端显式传，不是自动解析。
- `ensure_project_backend_access(state, project_id, backend_id)`（L139）做 backend 归属校验。
- workspace 经 `resolve_extension_invocation_workspace` → `resolve_session_frame_vfs` → `select_extension_invocation_workspace(vfs, backend_id)`（L315-357）：
  - 优先 `default_mount_id` 且 `mount.backend_id == backend_id && root_ref 非空` 的 mount（L336-347）；
  - 否则第一个 `backend_id` 匹配且 root_ref 非空的 mount（L348-356）。
- 然后 `request.target = Some(RuntimeTarget::Backend { backend_id })`（L159-161），`attach_extension_invocation_workspace(&mut request, workspace)`（L162）把 workspace 塞进 `metadata["extension_invocation_workspace"]`（extension_actions.rs L54-74）。

> **对 Child 2 的含义**：agent 工具侧没有 HTTP 请求体提供 backend_id，必须从 `ExecutionContext` 自推。两条候选（design 选一并明确优先级）：
> 1. `context.session.backend_execution.backend_id`（remote 时直接是已 claim 的 backend）。
> 2. 从 `context.session.vfs` 复用 `select_extension_invocation_workspace` 同款逻辑——但它需要先有 backend_id 才能选 mount。更合适：取 default mount 的 `backend_id`（`vfs.default_mount().backend_id`），再用同 mount 的 root_ref 作 workspace。
>
> `select_extension_invocation_workspace` 在 API crate（routes/extension_runtime.rs），是私有 fn。Child 2 在 application 层需要等价逻辑，建议把 backend+workspace 解析抽到 application 层共享 helper（避免在 connector 里重复 hack，见 MEMORY「connector 不应重复同一类 hack」）。`ExtensionInvocationWorkspaceContext` 与 `attach_extension_invocation_workspace` 已在 application 层公开（runtime_gateway 导出，mod.rs L15-20）。

### RuntimeActionToolAdapter 怎么拿 backend（对比）

`RuntimeActionToolAdapter` 的 `spec.target` 是**装配时固定**的（tool_adapter.rs L24/103），`runtime_session()` 构造器默认 `target: None`。即现有静态 adapter **没有运行时解析 backend** 的逻辑——backend 由装配方预置或留空。所以 Child 2 的元工具是首个需要「在 agent 侧运行时自推 backend」的场景，没有现成可直接抄的 application 层实现，只有 HTTP 侧样板。

### RelayRuntimeToolProvider 当前不持 RuntimeGateway

`RelayRuntimeToolProvider`（provider.rs L33-41）字段：`service / repos / session_services_handle / inline_persister / function_runner / shell_output_registry / materialization`——**无 `runtime_gateway` 句柄**。Child 2 若把 `workspace_module_invoke` 做成 runtime tool，需要给该 provider 注入 `Arc<RuntimeGateway>`（以及 channel 用的 `installation_repo` 已有 = `repos.project_extension_installation_repo`，见 tools.rs L52）。

## Caveats / Not Found

- 03 文档（Child 1 research）已确认 `ExecutionContext` 拿不到 AgentFrame，但 project/session/capability/vfs/backend_execution 都可达——invoke 所需内部路由齐全。
- `backend_execution` 仅 remote 时填；纯本地 desktop runtime 场景下 backend_id 应取自 vfs default mount。design 需明确两种 backend 来源的优先级与「缺 backend」时的报错（acceptance 要求缺 backend 明确报错，不裸 panic）。
