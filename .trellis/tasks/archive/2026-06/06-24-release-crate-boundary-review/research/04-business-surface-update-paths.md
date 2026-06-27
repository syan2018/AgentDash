# Research: business surface update paths

- Query: business surface update path 调研；追踪 Canvas / WorkspaceModule / Permission / Capability / Hooks / VFS / MCP preset / runtime tools / extension runtime 对 AgentRun runtime surface 的声明、读取、更新与 active runtime adoption 路径。
- Scope: internal
- Date: 2026-06-24

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-application/src/agent_run/frame/surface_service.rs` | AgentRun frame/surface command facade；定义 typed `RuntimeSurfaceUpdateRequest` 和 `AgentRunFrameSurfaceService`。 |
| `crates/agentdash-application/src/agent_run/frame/builder.rs` | `AgentFrameBuilder` revision writer primitive；把 `CapabilityState` 拆成 capability/VFS/MCP frame surface。 |
| `crates/agentdash-application/src/agent_run/runtime_surface_update.rs` | AgentRun runtime surface update service；写 Canvas revision、查询 effective view、委托 active runtime adopter。 |
| `crates/agentdash-application/src/agent_run/effective_capability.rs` | AgentRun effective capability/admission view；包含 PermissionGrant effect classification。 |
| `crates/agentdash-application/src/session/hub/tool_builder.rs` | `AgentRunActiveRuntimeSurfaceAdopter` 的 session hub 实现；只同步 persisted frame revision 到 live runtime。 |
| `crates/agentdash-application/src/permission/{service.rs,compiler.rs,runtime_surface_update.rs}` | Grant 生命周期、`RuntimeCapabilityTransition` 编译、AgentFrame effect revision 写入和 active runtime adoption。 |
| `crates/agentdash-application/src/canvas/{tools.rs,runtime_surface.rs,runtime.rs,runtime_resource.rs,management.rs}` | Canvas repository mutation、runtime snapshot/read-only projection、Canvas surface update helper。 |
| `crates/agentdash-application/src/workspace_module/{mod.rs,tools.rs,runtime_tool_provider.rs,visibility.rs,skill_projection.rs}` | WorkspaceModule declaration/projection/tool invocation；HostCanvas 分支触发 Canvas surface update。 |
| `crates/agentdash-application/src/capability/{mod.rs,resolver.rs}` | Capability declaration resolver；产出 `CapabilityState`，不直接写 AgentFrame。 |
| `crates/agentdash-application/src/vfs/{surface.rs,surface_query.rs,tools/factory.rs}` | VFS surface DTO/read-only summary 与 runtime tool declaration gating。 |
| `crates/agentdash-application/src/mcp_preset/{service.rs,runtime.rs}` | MCP preset CRUD 与 runtime server projection；无 runtime surface update caller。 |
| `crates/agentdash-application/src/hooks/{provider.rs,active_workflow_snapshot.rs,script_engine.rs,active_workflow_contribution.rs}` | Hook snapshot/evaluation/logging；读取 workflow/frame context，不改 AgentFrame runtime surface。 |
| `crates/agentdash-application/src/runtime_tools/provider.rs` | Runtime tool composer handles；把 `AgentRunRuntimeSurfaceUpdateService` 暴露给 tools。 |
| `crates/agentdash-application/src/extension_runtime.rs` | Project extension installation 的 runtime projection；供 WorkspaceModule / Gateway 读取。 |

### Related Specs

- `.trellis/spec/backend/architecture.md`: API 只做入口/DTO/错误映射；application 负责 use case 和 query/update facade。
- `.trellis/spec/backend/capability/architecture.md`: AgentRun effective capability/admission 是 runtime 能力读取入口；Grant tool-internal effect 不写 surface，toolset expansion 写 AgentFrame revision。
- `.trellis/spec/backend/permission/architecture.md`: Surface-changing Grant 写入新的 AgentFrame revision；active-runtime adoption 采纳已持久化 revision。
- `.trellis/spec/backend/vfs/architecture.md`: AgentRun resource surface 从当前 AgentFrame typed VFS 派生；runtime tool composer 只装配工具声明。
- `.trellis/spec/backend/runtime-gateway.md`: RuntimeGateway / API current-surface consumer 不直接解析 AgentFrame，应消费 AgentRun runtime surface query DTO。
- `.trellis/spec/backend/hooks/architecture.md`: Hook provider 读取 AgentFrame/workflow snapshot 并输出 hook resolution/effects。

### Core AgentRun Surface Boundary

`AgentRunFrameSurfaceService` 已经是当前最接近目标的 write facade。源码说明业务域应提交 typed construction/update intent，不应拥有 `AgentFrameBuilder`、完整 `CapabilityState` projection 或 live-runtime adoption timing（`crates/agentdash-application/src/agent_run/frame/surface_service.rs:1-6`）。同文件把写命令分成 `Construct` 和 `Update(RuntimeSurfaceUpdateRequest)`（`.../surface_service.rs:18-24`），并把 `RuntimeSurfaceUpdateRequest` 限定为稳定 changed-resource identity（`.../surface_service.rs:85-121`）。`surface_kind()` 已覆盖 Canvas、Permission、MCP、VFS、WorkspaceModule、SkillInventory、AgentProcedure（`.../surface_service.rs:123-137`）。

`AgentFrameBuilder` 是 AgentRun 内部 revision writer。文件头明确它是内部 primitive，业务模块外部变化应先进入 typed command/update boundary（`crates/agentdash-application/src/agent_run/frame/builder.rs:1-10`）；builder 本身负责把 runtime surface 输入收束为单次 immutable revision（`.../builder.rs:79-83`）。`with_capability_state` 一次性写 capability / VFS / MCP 三列，保证和 frame 投影读取对称（`.../builder.rs:136-145`），`build()` 通过 repository 持久化新 revision（`.../builder.rs:225-230`）。

`AgentRunRuntimeSurfaceUpdateService` 当前承担 query/update/adoption 的混合 facade：它持有 `AgentRunRuntimeSurfaceQueryPort`、`AgentFrameRepository`、`VfsService`、`AgentRunActiveRuntimeSurfaceAdopter` 和 skill discovery deps（`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:31-38`）；`adopt_persisted_frame_revision_into_active_runtime` 只是把 target 传给 injected adopter（`.../runtime_surface_update.rs:62-68`）。

`AgentRunActiveRuntimeSurfaceAdopter` 是 live-runtime adoption port，签名只接受 `AgentFrameRuntimeTarget` 并返回新工具 surface（`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:22-28`）。Session hub 实现时先校验 delivery anchor/current frame，再从 adopted frame 投影 capability/VFS/MCP（`crates/agentdash-application/src/session/hub/tool_builder.rs:74-83`, `.../tool_builder.rs:131-144`）；随后重建 execution context、重新 assemble tools、调用 connector `update_session_tools`，并更新 runtime registry 的 `session_profile` / active turn cache（`.../tool_builder.rs:173-204`）。hook runtime notification 是 adoption 之后的 runtime context transition 通知（`.../tool_builder.rs:210-239`）。该实现没有写新的 AgentFrame；trait impl 只是把 session hub 暴露为 adopter port（`.../tool_builder.rs:317-325`）。

### Surface-Changing Update Paths

#### Canvas

Canvas 的业务记录 CRUD 在 `canvas/management.rs` 只操作 `CanvasRepository`：create/update/delete 分别在 `create_project_canvas`、`update_canvas_record`、`delete_canvas_record` 中完成（`crates/agentdash-application/src/canvas/management.rs:45-70`, `.../management.rs:102-124`）。这些 repository mutation 本身不是 AgentRun runtime surface update。

真正改变 runtime surface 的 Canvas path 统一通过 `canvas/runtime_surface.rs`。`submit_canvas_runtime_surface_update` 校验 request 与 Canvas mount 匹配，要求已有 session services 和 RuntimeSession id，然后调用 `session_services.runtime_surface_update.expose_canvas_mount(session_id, canvas)`（`crates/agentdash-application/src/canvas/runtime_surface.rs:10-36`）；如果调用方传入 live `SharedRuntimeVfs`，再用返回的 active VFS 替换本地 runtime VFS cache（`.../runtime_surface.rs:37-40`）。`submit_existing_canvas_visibility_request` 只为已有 Canvas 构造 `RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested`（`.../runtime_surface.rs:43-63`）。

`expose_canvas_mount` 在 AgentRun update service 中读取当前 runtime surface 和 current frame，基于 current frame 投影出 `CapabilityState`，向 active VFS 追加 Canvas mount，必要时刷新 binding files，然后用 `AgentFrameBuilder::with_capability_state` 写新 frame revision（`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:71-118`）。写入后它追加 `visible_canvas_mount` 和 `visible_workspace_module_ref("canvas:<mount_id>")`，创建 frame，并立即通过 active adopter 采纳 persisted revision（`.../runtime_surface_update.rs:116-128`）。

Canvas 工具 path 中，`create_or_attach_canvas_for_session` 创建或附加 Canvas 后提交 `CanvasVisibilityRequested { reason: Created }`（`crates/agentdash-application/src/canvas/tools.rs:65-79`, `.../tools.rs:154-164`）。`bind_canvas_data_for_project` 只更新 Canvas binding repository fact（`.../tools.rs:181-210`）；实际 surface update 由 WorkspaceModule invoke 的 HostCanvas 分支在绑定完成后提交 `CanvasBindingChanged`（`crates/agentdash-application/src/workspace_module/tools.rs:845-880`）。Canvas runtime snapshot/resource path 只构建 DTO 和解析 binding 文件：`build_runtime_snapshot` 初始禁用 bridge surface（`crates/agentdash-application/src/canvas/runtime.rs:82-137`），`CanvasRuntimeResourceService` 只读 VFS 并填充 resolved binding content（`crates/agentdash-application/src/canvas/runtime_resource.rs:20-83`）。

Verdict: Canvas 是 surface-changing path。目标归属应继续是 AgentRun surface update facade；Canvas 模块只提交 `CanvasVisibilityRequested` / `CanvasBindingChanged` intent，不能直接写 `AgentFrameBuilder`。当前 `submit_canvas_runtime_surface_update` 仍通过 `SharedSessionToolServicesHandle` 间接拿 `runtime_surface_update`，拆 crate 时应替换为显式 `AgentRunRuntimeSurfaceUpdatePort` 注入。

#### WorkspaceModule

WorkspaceModule declaration/projection 是 read-only。`build_workspace_modules` 只是把 enabled extension projection 和 canvases 合成为 descriptor list（`crates/agentdash-application/src/workspace_module/mod.rs:174-185`）。Extension operations 转成 `RuntimeAction` / `ProtocolChannel` dispatch descriptor（`.../mod.rs:281-324`）；Canvas module 暴露 `canvas.bind_data` HostCanvas operation（`.../mod.rs:387-425`）。

可见性读取通过 AgentRun effective capability view。`WorkspaceModuleVisibilitySource::effective_view` 从 `session_services.runtime_surface_update.effective_capability_view_for_delivery_runtime(session_id)` 获取 view（`crates/agentdash-application/src/workspace_module/tools.rs:71-96`），`resolve_workspace_module_visibility` 再读取 `view.capability_state.workspace_module` 和 `view.visible_workspace_module_refs` 做过滤（`crates/agentdash-application/src/workspace_module/visibility.rs:28-59`）。这属于 current surface read-only consumer。

`WorkspaceModuleRuntimeToolProvider` 只在 `CapabilityState` 允许 `ToolCluster::WorkspaceModule` 且具体 tool 被 capability gate 开启时装配 list/describe/create/invoke/present（`crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:142-238`）。`workspace_module_invoke` 需要 RuntimeGateway、extension channel transport 和 runtime backend anchor；缺依赖时装配诊断工具，不执行 update（`.../runtime_tool_provider.rs:242-303`）。

Invoke flow 三分支：

- RuntimeAction：构造 `RuntimeInvocationRequest`，actor/context/target 由宿主侧补齐后 `gateway.invoke(request)`（`crates/agentdash-application/src/workspace_module/tools.rs:752-795`）。
- ProtocolChannel：构造 `ExtensionRuntimeChannelInvokeRequest` 并由 channel invoker 调用（`.../tools.rs:796-844`）。
- HostCanvas BindData：更新 Canvas binding，然后提交 `RuntimeSurfaceUpdateRequest::CanvasBindingChanged`（`.../tools.rs:845-880`）。

`workspace_module_create` 对 kind=`canvas` 调 `create_or_attach_canvas_for_session`，该 Canvas helper 会提交 `CanvasVisibilityRequested` 并暴露 Canvas VFS mount（`crates/agentdash-application/src/workspace_module/tools.rs:396-477`；Canvas helper 证据见 `crates/agentdash-application/src/canvas/tools.rs:154-164`）。`workspace_module_present` 对 `renderer_kind == "canvas"` 调 `request_existing_canvas_visibility_for_runtime`，然后只注入 `SessionMetaUpdate` 通知给前端（`crates/agentdash-application/src/workspace_module/tools.rs:1021-1066`）。`SessionMetaUpdate` 是 presentation/eventing，不是 AgentFrame surface fact。

Verdict: WorkspaceModule 是 mixed path。list/describe/visibility/presentation DTO 是 read-only consumer；create/present/HostCanvas invoke 通过 Canvas adapter 间接改变 AgentRun runtime surface；RuntimeAction/ProtocolChannel invoke 是 RuntimeGateway execution path，不改 AgentFrame surface。目标应把 `effective_capability_view_for_delivery_runtime` 拆为 `AgentRunSurfaceReadFacade`，把 Canvas exposure 拆为 `AgentRunSurfaceUpdateFacade.submit(RuntimeSurfaceUpdateRequest)`；WorkspaceModule 不应依赖完整 `SessionToolServices`。

#### Permission

PermissionGrant 的 effect classification 已在 AgentRun effective capability 中实现。`AgentRunGrantEffectClass` 区分 `AdmissionProjection` 和 `AgentFrameSurfaceRevision`（`crates/agentdash-application/src/agent_run/effective_capability.rs:25-29`）；`classify_path` 规则是 `ToolCapabilityPath.tool.is_some()` 时只进 admission projection，否则写 AgentFrame surface revision（`.../effective_capability.rs:57-63`）。`partition_paths` 输出 admission paths 和 surface paths（`.../effective_capability.rs:65-83`）。`AgentRunGrantProjection::from_active_grants` 只把 active grant 的 tool-level paths 放进 admitted tools（`.../effective_capability.rs:49-55`, `.../effective_capability.rs:85-100`）。

AgentRun effective view/admission 读取 current frame surface 和 grant projection。`AgentRunEffectiveCapabilityView` 包含 target、`CapabilityState`、visible capabilities、VFS、MCP、visible workspace module refs 和 grant projection（`crates/agentdash-application/src/agent_run/effective_capability.rs:142-151`）。`admit_tool` 先检查 `CapabilityState.is_capability_tool_enabled`，再检查 grant projection（`.../effective_capability.rs:254-274`）。Runtime execution capability projection 从 runtime session anchor 找 run，再加载 active grants 并投影到 execution `CapabilityState`（`.../effective_capability.rs:276-293`）。

`PermissionGrantCompiler` 总是把 grant requested paths 编译为 `RuntimeCapabilityTransition` declarations（dimension=`tool`, declaration_type=`capability_directive`, source=`permission_grant`），不在 compiler 中决定 frame 写入（`crates/agentdash-application/src/permission/compiler.rs:27-48`）。`RuntimeCapabilityTransition` 本身只是 declarations + effects 容器（`crates/agentdash-spi/src/session_persistence.rs:188-195`）。

`PermissionRuntimeSurfaceUpdateService::project_update_request` 是 surface-changing branch：先把 request 解析为 applied/revoked，再 compile transition；如果 `AgentRunGrantProjection::partition_paths` 的 surface paths 为空，返回 `no_surface`，不写 frame（`crates/agentdash-application/src/permission/runtime_surface_update.rs:121-144`）。有 surface paths 时，它读取 effect frame/current frame，基于 current frame 投影 `CapabilityState`，应用 requested paths，向 transition push `set_tool_access` effect，然后用 `AgentFrameBuilder::with_capability_state` 写 effect frame（`.../runtime_surface_update.rs:146-182`, `.../runtime_surface_update.rs:207-228`, `.../runtime_surface_update.rs:297-309`）。`adopt_update_outcome` 只在有 adoption target 和 adopter 时执行，adoption 失败会返回 visible `ApplicationError::Internal`（`.../runtime_surface_update.rs:185-205`）。

`PermissionGrantService` 在 request auto-approved、approve、revoke、expire 时分别提交 `PermissionGrantApplied` / `PermissionGrantRevoked` typed requests，并在 grant 状态持久化后执行 active-runtime adoption（`crates/agentdash-application/src/permission/service.rs:133-155`, `.../service.rs:163-203`, `.../service.rs:225-260`, `.../service.rs:263-309`）。

Verdict: Permission 是最完整的 AgentRun surface update/admission path。目标 facade 应保留两层：Grant lifecycle service 只拥有 grant 状态机和 policy；AgentRun facade 负责 classify effect、写 frame revision、admission projection、adoption。当前 `PermissionRuntimeSurfaceUpdateService` 在 permission 模块内实现 AgentRun update adapter，拆 crate 时应上移为 AgentRun-owned `PermissionGrantSurfaceEffectApplier` 或通过 trait port 注入，避免 permission crate 直接依赖 `AgentFrameBuilder`。

### Declaration / Read-Only Consumers

#### Capability

Capability resolver 是声明/计算路径。模块说明它统一计算 session 的 `CapabilityState`，包含工具、MCP server 和 VFS 投影（`crates/agentdash-application/src/capability/mod.rs:1-10`）。`CapabilityResolverOutput = CapabilityState`，注释要求 resolver 产出的 state 应通过 `AgentFrameBuilder::with_capability_state` 写入 AgentFrame revision，运行时读取应从 frame 投影（`crates/agentdash-application/src/capability/resolver.rs:232-244`）。`resolve_checked` 合并 contributions、计算 directives/MCP preset、tool clusters 和 policy，最后返回 `CapabilityState`（`.../resolver.rs:269-380`）。因此 capability 模块不是 surface-changing update path；它是 frame construction/update 的 input producer。

`CapabilityState` 的 SPI 定义说明它是 AgentFrame revision 的只读投影，写入流向是 `AgentFrameBuilder::with_capability_state -> AgentFrame revision -> 内存缓存同步`，读取流向是 runtime cache / frame projection（`crates/agentdash-spi/src/connector/mod.rs:348-360`）。Workspace module dimension 的声明式上游是 ProjectAgent preset 的 visible refs，并经 `CapabilityState.workspace_module` 流转（`.../connector/mod.rs:308-313`）。

#### VFS

VFS tools 是 read/write 文件系统操作工具声明，不是 AgentFrame surface update。`VfsToolFactory::build_tools` 根据 `input.flow` 的 enabled clusters 和 `is_capability_tool_enabled` 装配 mounts/read/glob/grep/apply_patch/shell tools（`crates/agentdash-application/src/vfs/tools/factory.rs:46-150`）。shell tool 会携带 `CapabilityState` 做执行期裁决（`.../factory.rs:116-132`），但不写 AgentFrame。

`vfs/surface.rs` 定义统一 `ResolvedVfsSurface` DTO 和 `ResolvedVfsSurfaceSource`，包含 `SessionRuntime`、`AgentRun`、Project preview、Project VFS mount 等 source（`crates/agentdash-application/src/vfs/surface.rs:4-50`），`surface_ref()` 只生成稳定引用（`.../surface.rs:52-83`）。`build_surface_summary` 读取 VFS mounts 和 runtime edit/backend status 生成 summary DTO（`crates/agentdash-application/src/vfs/surface_query.rs:17-74`）。因此 VFS 指定范围内没有直接 `RuntimeSurfaceUpdateRequest::ProjectVfsMountChanged` caller；该 variant 目前只是 AgentRun typed request contract 中的预留/目标入口。

#### MCP Preset

MCP preset service 是 repository CRUD/builtin bootstrap。`McpPresetService` 只围绕 `McpPresetRepository` 封装 create/update/delete/clone/bootstrap（`crates/agentdash-application/src/mcp_preset/service.rs:10-24`, `.../service.rs:70-149`, `.../service.rs:152-180`）。`mcp_preset/runtime.rs` 把 `McpPreset` 和 runtime binding context 解析为 `RuntimeMcpServer`（`crates/agentdash-application/src/mcp_preset/runtime.rs:13-24`, `.../runtime.rs:80-93`），runtime binding 只读取 VFS mount/backend anchor/workspace metadata 并写 transport config（`.../runtime.rs:99-157`）。没有生产 caller 提交 `RuntimeSurfaceUpdateRequest::McpPresetChanged`。

#### Hooks

Hook provider 是 read/evaluate/logging path。`AppExecutionHookProvider` 组合 owner resolver、active workflow snapshot builder 和 script engine（`crates/agentdash-application/src/hooks/provider.rs:32-39`）。`build_snapshot_from_workflow` 构造 `AgentFrameHookSnapshot`，填充 sources/tags/injections/run context/metadata（`.../provider.rs:99-260`）。`load_frame_snapshot` / `refresh_frame_snapshot` / `evaluate_frame_hook` 读取 snapshot 并评估 rules（`.../provider.rs:263-308`）。唯一写路径是 `append_execution_log`，它 flush lifecycle execution log entries（`.../provider.rs:310-315`; builder 实现在 `crates/agentdash-application/src/hooks/active_workflow_snapshot.rs:87-94`），不写 AgentFrame runtime surface。`HookScriptEngine` 只把 evaluation context 交给 evaluator，再 parse decision（`crates/agentdash-application/src/hooks/script_engine.rs:41-95`）；`active_workflow_contribution` 只生成 `HookInjection`（`crates/agentdash-application/src/hooks/active_workflow_contribution.rs:22-45`）。

#### Extension Runtime

`extension_runtime.rs` 是 Project enabled extension installations 的 runtime projection。`ExtensionRuntimeProjection` 包含 commands、flags、runtime_actions、protocol_channels、workspace_tabs、permissions、bundles 等 read model（`crates/agentdash-application/src/extension_runtime.rs:17-29`）。`extension_runtime_projection_from_installations` 遍历 installation manifest，校验 runtime action/channel/tab keys 唯一性，并生成 projection rows（`.../extension_runtime.rs:132-183`, `.../extension_runtime.rs:184-220`）。该 projection 被 WorkspaceModule descriptor 和 RuntimeGateway provider 消费，不直接更新 AgentFrame surface。

### Runtime Tools / Service Handle Boundary

`SessionToolServices` 当前把 session core/eventing/control/launch/hooks/runtime_transition 与 `AgentRunRuntimeSurfaceUpdateService` 放在同一个 shared handle 中（`crates/agentdash-application/src/runtime_tools/provider.rs:37-46`），WorkspaceModule/Canvas tools 通过这个 handle 获取 update/read service。`SessionRuntimeToolComposer` 只是遍历 providers 构建 runtime tools（`.../provider.rs:64-91`），`project_id_from_context` 和 `runtime_session_id_from_context` 从 `ExecutionContext` / hook runtime / VFS 解析当前工具上下文（`.../provider.rs:103-127`）。

This is a release split risk: business tools currently depend on "session services" as service locator even when they only need AgentRun surface read/update and eventing. Target should split handles into narrower ports:

- `AgentRunRuntimeSurfaceReadPort`: `effective_capability_view_for_delivery_runtime(session_id)` / current surface query DTO.
- `AgentRunRuntimeSurfaceUpdatePort`: `submit_runtime_surface_update(session_id, RuntimeSurfaceUpdateRequest)` and Canvas helper `expose_canvas_mount` as an AgentRun-owned adapter, not a session-services method.
- `AgentRunRuntimeAdmissionPort`: tool admission/effective view APIs, including PermissionGrant admission projection.
- `RuntimePresentationEventPort`: `inject_notification` / panel presentation events, separate from AgentFrame surface writes.

### Target Facade Recommendation

Target AgentRun surface update/admission facade should be the public application boundary for all surface-changing business paths:

1. `AgentRunSurfaceFacade::current_surface(runtime_session_id, purpose) -> AgentRunRuntimeSurfaceView`
   - Hides `AgentFrame` and frame repository from Canvas/WorkspaceModule/VFS/RuntimeGateway/API consumers.
   - Backed by existing `AgentRunRuntimeSurfaceQueryPort` and `AgentRunEffectiveCapabilityService`.

2. `AgentRunSurfaceFacade::submit_update(runtime_session_id, RuntimeSurfaceUpdateRequest) -> AgentRunFrameSurfaceCommandOutcome`
   - Accepts typed changed-resource identity only.
   - Owns projection context resolution, `AgentFrameBuilder`, frame repository write, and optional `AgentRunActiveRuntimeSurfaceAdopter` call.
   - Business modules submit `CanvasVisibilityRequested`, `CanvasBindingChanged`, `PermissionGrantApplied/Revoked`, `McpPresetChanged`, `ProjectVfsMountChanged`, `WorkspaceModuleVisibilityChanged`, `SkillInventoryChanged`, `AgentProcedureContractChanged`.

3. `AgentRunAdmissionFacade::effective_view/admit_tool`
   - Returns final visible capability view and admission decisions.
   - Keeps PermissionGrant tool-internal effects out of `CapabilityResolver`, VFS tool factory and RuntimeGateway provider internals.

4. `AgentRunActiveRuntimeSurfaceAdopter` remains a lower-level port implemented by runtime-session delivery, not exposed to business modules. Business services should observe `adopted_active_runtime` through command outcome/diagnostics, not call session hub directly.

### Current Mismatch / Release Split Notes

- The typed request enum already includes MCP/VFS/WorkspaceModule/Skill/AgentProcedure variants (`crates/agentdash-application/src/agent_run/frame/surface_service.rs:102-120`), but production callers found in the requested scope only submit Canvas and Permission requests. `rg` found `McpPresetChanged`, `ProjectVfsMountChanged`, `WorkspaceModuleVisibilityChanged`, `SkillInventoryChanged`, and `AgentProcedureContractChanged` only in enum/tests, not in business services. These update paths need explicit AgentRun adapters before crate split if they are release-critical.
- `PermissionRuntimeSurfaceUpdateService` currently imports `AgentFrameBuilder` directly from permission module (`crates/agentdash-application/src/permission/runtime_surface_update.rs:15-20`, `.../runtime_surface_update.rs:207-228`). This works inside a single crate but should move behind AgentRun-owned adapter/port before physical crate split.
- Canvas/WorkspaceModule surface update helpers depend on `SharedSessionToolServicesHandle` (`crates/agentdash-application/src/canvas/runtime_surface.rs:10-30`; `crates/agentdash-application/src/workspace_module/tools.rs:669-679`). This couples business tools to session service aggregation; target should inject narrow AgentRun surface read/update ports plus eventing port.
- `AgentRunRuntimeSurfaceUpdateService` currently contains Canvas-specific `expose_canvas_mount` logic (`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:71-133`). For a future facade, this can stay as AgentRun-owned Canvas adapter, but public callers should submit typed request rather than know the helper name.
- No `SessionCapabilityService` symbol exists in `crates/agentdash-application/src`, `crates/agentdash-domain/src`, `crates/agentdash-spi/src`, or `crates/agentdash-api/src`. Current equivalents are `AgentRunEffectiveCapabilityService` for final view/admission and `SessionRuntimeInner::{get_current_capability_state,get_latest_capability_state}` for runtime cache reads (`crates/agentdash-application/src/agent_run/effective_capability.rs:196-294`; `crates/agentdash-application/src/session/hub/tool_builder.rs:22-72`).

## Caveats / Not Found

- `task.py current --source` returned no active task in this Codex session; the output path used here is the explicit task path supplied by the user.
- `SessionCapabilityService` was not found by repository search; report treats it as a stale/renamed concept and maps it to `AgentRunEffectiveCapabilityService` plus session hub capability cache readers.
- This report did not use external web references; all evidence is local code/spec.
- Search scope was the user-requested application directories plus AgentRun/session/SPI files required to trace the actual facade/adoption boundary. API consumer paths are covered by the sibling API/RuntimeGateway review, not repeated here.
