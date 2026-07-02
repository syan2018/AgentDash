# Canvas runtime surface 与资源绑定收束

## Goal

让 AgentRun Canvas 会话已有的绑定/状态层成为 Canvas runtime 的统一消费入口，并把 Workspace Module 内部的 runtime/session/backend/VFS/action 投影收束成一个深模块接口：Canvas 预览、图片资源读取、runtime action、数据绑定、后端执行选择都从现有 `CanvasAgentRunContext` / AgentRun Canvas runtime state / current AgentFrame surface / Workspace Module runtime context 闭包读取。

直接用户价值是修复 Canvas 内部 `agentdash.assets.url(...)` 读图时出现的“Canvas 图片资源需要绑定 runtime resource surface”，并让后续 Canvas 工具执行、资源链接、Extension Canvas panel、Workspace Module operate/invoke/present 与 AgentRun workspace 的行为可解释、可测试。

## Background

- 前端报错来自 `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx:373`：图片资源请求要求 `snapshot.resource_surface_ref`，缺失时返回“Canvas 图片资源需要绑定 runtime resource surface”。
- `CanvasRuntimeResourceService` 只在 `session_id` 存在时生成 `session-runtime:{session_id}`，见 `crates/agentdash-workspace-module/src/canvas/runtime_resource.rs:33`。
- AgentRun Canvas snapshot route 已经解析到 `context.runtime_session_id` 和 current runtime VFS，但调用 `build_runtime_snapshot_with_bindings` 时传入 `session_id=None`，见 `crates/agentdash-api/src/routes/canvases.rs:713`。
- AgentRun runtime binding upsert route 同样在返回 snapshot 时传入 `session_id=None`，见 `crates/agentdash-api/src/routes/canvases.rs:762`。
- Workspace module 工具执行会通过 `RuntimeSurfaceUpdateRequest` 更新 current AgentFrame/runtime VFS，见 `crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:185` 与 `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs:99`。
- Workspace Module 内部已有 `WorkspaceModuleResolveContext`、`WorkspaceModuleOperationRuntimeSource`、`ResolvedInvocationBackend`、`WorkspaceModuleOperateCommand`、`WorkspaceModuleInvokeCommand`、`WorkspaceModulePresentCommand`，但 operate/invoke/present/canvas runtime update 仍分别携带 delivery runtime session、backend、bridge、current user、VFS 等片段，见 `crates/agentdash-workspace-module/src/workspace_module/surface.rs:127`、`crates/agentdash-workspace-module/src/workspace_module/surface.rs:274`、`crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:167`。
- VFS surface 已有 `session-runtime:{session_id}` 和 `agent-run:{run_id}:{agent_id}` 两种稳定引用，解析入口在 `crates/agentdash-application-ports/src/vfs_surface_runtime.rs:28` 与 `crates/agentdash-application/src/vfs_surface_resolver.rs:186`。
- 当前 Canvas snapshot 上同时存在 action surface 概念和 resource surface 概念：`runtime_bridge.surface/actions` 表示可调用 action，`resource_surface_ref` 表示资源读取面。
- 已有 AgentRun Canvas 绑定/状态层：`CanvasAgentRunContext` 持有 `run`、`agent`、`canvas`、`runtime_session_id`、`current_agent_frame`、`delivery_trace_ref` 和 `agent_run_canvas_ref`，见 `crates/agentdash-application/src/canvas/diagnostics.rs:20`。
- 已有持久 latest state 表 `agent_run_canvas_runtime_observations` 和 `agent_run_canvas_interaction_snapshots` 以 `(run_id, agent_id, canvas_mount_id)` 为唯一键，并持有 `delivery_trace_ref`、`current_agent_frame_id` 和 `agent_run_canvas_ref`，见 `crates/agentdash-infrastructure/migrations/0026_agent_run_canvas_runtime_state.sql:1`。
- 该 migration 明确写明 RuntimeSession 只作为派生 delivery trace 诊断字段，不作为 ownership key；因此本任务不应新增第二套绑定事实源。

## Requirements

- R1：AgentRun Canvas runtime snapshot 必须携带可用的 `resource_surface_ref`，并且该 ref 与当前 delivery runtime session / current runtime surface 同源。
- R2：Canvas runtime binding upsert 返回的新 snapshot 必须保留同一个资源 surface 绑定，确保绑定更新后预览资源读取仍可用。
- R3：Canvas iframe 的图片资源读取、runtime action 调用、interaction/observation 上传、Agent submit 必须从现有 AgentRun Canvas 绑定层和 Workspace Module runtime context 取得身份、Project、runtime session、current frame/surface revision 与后端选择。
- R4：普通 Canvas 预览、AgentRun Canvas 预览、promoted Extension Canvas panel 必须明确区分静态预览与 AgentRun 运行期预览；运行期资源读取只能在绑定了 runtime resource surface 时可用。
- R5：后端应扩展并收束现有 `CanvasAgentRunContext` / Canvas runtime state 读模型，集中表达 `run_id`、`agent_id`、`canvas_mount_id`、`runtime_session_id`、Project、current surface frame/revision、resource surface ref、VFS resource surface、runtime action catalog 和 backend anchor。
- R6：Workspace Module 内部应形成一个 runtime context 模块接口，覆盖 operate/invoke/present/canvas runtime update 共享的 delivery runtime session、active VFS、current user、AgentRun bridge、backend readiness、RuntimeGateway actor/context、Canvas latest state ownership。
- R7：API route、workspace module tools 和前端组件消费同一个后端上下文投影或由该投影生成的 snapshot，资源 surface 与 runtime bridge 的含义由投影显式表达。
- R8：保持当前预研阶段的正确模型优先，不引入兼容性分支或历史字段回退；若 wire contract 需要调整，按合同生成和迁移要求更新。

## Acceptance Criteria

- [ ] AgentRun Canvas runtime snapshot route 返回 `resource_surface_ref=session-runtime:{runtime_session_id}` 或等价的 canonical runtime resource surface ref。
- [ ] AgentRun Canvas runtime binding upsert route 返回的 snapshot 同样包含资源 surface ref，且前端 `agentdash.assets.url(...)` 可通过 `/vfs-surfaces/read-file-blob` 读取当前 runtime VFS 中的图片。
- [ ] Canvas runtime action invocation 与 image asset resolution 都经过同一个 AgentRun Canvas runtime context 校验 Project、Canvas mount、runtime session 与当前 surface。
- [ ] Workspace Module 的 operate/invoke/present/canvas runtime update 使用同一个 runtime context 接口完成 runtime session、VFS、backend、AgentRun bridge、current user、runtime action catalog 投影。
- [ ] promoted Extension Canvas panel 在 AgentRun workspace 中有清晰的 runtime resource binding 策略；无法绑定时展示明确不可用状态，而不是让 iframe 内部报缺失 surface。
- [ ] Canvas snapshot / bridge DTO 的 action surface 与 resource surface 语义清楚，前端不需要通过 shape probing 判断 `runtime_bridge` 是 `surface` 还是 `actions`。
- [ ] 后端测试覆盖 AgentRun runtime snapshot、binding upsert、Project mismatch、缺失 delivery runtime anchor、Workspace Module context 缺失 backend/bridge/session 的诊断。
- [ ] 前端测试覆盖没有 `session_id` 但有 `resource_surface_ref` 的 AgentRun snapshot，以及缺失 runtime binding 时的可解释错误。

## Out Of Scope

- 不重做 Canvas 源文件编辑器。
- 不重做 VFS provider 或 RuntimeGateway 基础架构。
- 不增加历史兼容层；必要的合同变更直接收束到当前正确形态。
- 不新增独立 AgentRun Canvas binding 表；已有 runtime observation / interaction snapshot latest state 与 `CanvasAgentRunContext` 是本任务必须复用和补齐的绑定层。
