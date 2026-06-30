# Research: Extension action availability

- Query: Extension/action availability 设计 research，覆盖 D7 RuntimeGateway dynamic extension action discovery owner 与 D8 runtime action availability 三层 owner 收束。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-design-backlog-review/implement.jsonl` - 当前 design review 任务注入的 spec / research context 清单。
- `.trellis/tasks/06-30-design-backlog-review/prd.md` - D1-D12 设计评估总目标、验收项与 quick convergence residual 要求。
- `.trellis/tasks/06-30-design-backlog-review/design.md` - research 输出模板、决策状态分类与 D7/D8 的建议推进顺序。
- `.trellis/tasks/06-30-design-backlog-review/implement.md` - research worker 调度与“不以新增并行 abstraction 代替收束”的执行约束。
- `.trellis/tasks/06-30-module-adversarial-review/followups/design-backlog.md` - D7/D8 的 canonical backlog source。
- `.trellis/tasks/06-30-module-adversarial-review/research/07-extension-workspace-module-surface.md` - Extension / WorkspaceModule 分叉证据源。
- `.trellis/tasks/06-30-module-adversarial-review/research/08-authority-capability-runtime.md` - Capability / admission / availability 分层证据源。
- `.trellis/tasks/06-30-module-adversarial-review/followups/quick-convergence-task-map.md` - quick convergence 工作项映射。
- `.trellis/tasks/06-30-architecture-quick-convergence/prd.md` - quick convergence 完成范围与 out-of-scope residual。
- `.trellis/tasks/06-30-architecture-quick-convergence/implement.md` - quick convergence 最终结果：schema validator、workspace resolver、renderer-aware loadability 已完成；dynamic action discovery owner 仍为 residual。
- `.trellis/spec/guides/cross-layer-thinking-guide.md` - 跨层事实源、状态推断和 runtime surface 检查清单。
- `.trellis/spec/cross-layer/architecture.md` - Runtime Gateway / Local backend / workspace 物理目录访问跨层边界。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - local extension host、relay extension action/channel payload 与 workspace/process/env host API contract。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - ProjectBackendAccess、workspace binding 与 runtime backend/root_ref owner。
- `.trellis/spec/backend/vfs/architecture.md` - SessionRuntimeToolComposer 与 domain runtime tool provider 边界。
- `.trellis/spec/backend/session/architecture.md` - AgentRun frame/surface command boundary 与 RuntimeSession trace substrate。
- `.trellis/spec/backend/session/session-startup-pipeline.md` - LaunchCommand / FrameLaunchEnvelope / LaunchPlan 分层。
- `.trellis/spec/backend/capability/architecture.md` - AgentRun effective capability/admission 是 runtime 能力读取入口。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` - `workspace_module` capability、tool cluster、schema exposure 与 execution admission 分层。
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md` - extension runtime 属于 projection-only / future extension dimension，不应扩展中心化 transition input。
- `crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs` - RuntimeGateway static provider surface 与 dynamic provider invoke 分叉点。
- `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs` - Extension dynamic runtime action provider、schema/permission/workspace validation 与 transport request。
- `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` - RuntimeGateway 注册 extension dynamic provider。
- `crates/agentdash-api/src/routes/extension_runtime.rs` - Project extension runtime projection、invoke-action、invoke-channel HTTP route。
- `crates/agentdash-api/src/routes/canvases.rs` - Canvas runtime bridge snapshot 读取 RuntimeGateway surface。
- `crates/agentdash-workspace-module/src/extension_runtime.rs` - Project extension installation 到 extension runtime projection 的 flatten owner。
- `crates/agentdash-workspace-module/src/workspace_module/mod.rs` - Extension runtime projection 到 WorkspaceModule descriptor / operation / UI entry 的 projection。
- `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs` - WorkspaceModule runtime tool provider 与 missing runtime dependency diagnostic tool。
- `crates/agentdash-workspace-module/src/workspace_module/tools.rs` - workspace_module list/describe/invoke/present Agent-facing tools 与 effective capability view consumption。
- `crates/agentdash-workspace-module/src/workspace_module/visibility.rs` - AgentRun effective capability view 到 visible workspace module projection。
- `crates/agentdash-application-ports/src/agent_run_surface.rs` - `AgentRunEffectiveCapabilityPort` contract。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` - quick convergence 后的 admission projection 与 visible capability 分离结果。
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts` - Extension webview bridge 对 project extension runtime projection 的 action discovery / availability gate。
- `packages/app-web/src/generated/extension-runtime-contracts.ts` - Extension runtime projection DTO，含 runtime_actions 与 tab loadability。

### Quick Convergence Baseline

本 research 以 quick convergence 后代码为准，不重复讨论已收束问题：

- Schema validator 已收束：quick convergence work item 02 明确把 schema subset validator 移到 runtime gateway 与 workspace module 共同依赖的位置；当前 `extension_actions.rs` 通过 `validate_shared_json_schema_subset` 做 action/channel input 校验，WorkspaceModule 也复用同类 validator。
- Extension invocation workspace resolver 已收束：quick convergence work item 02 要求 API route 与 workspace module runtime bridge 都调用 shared resolver；当前 route 在 `crates/agentdash-api/src/routes/extension_runtime.rs:144` 调用 `resolve_extension_invocation_workspace(...)`。
- Renderer-aware loadability 已收束：`extension_runtime_projection_from_installations()` 先计算 `has_package_artifact` / `has_extension_host_bundle`，再按 renderer kind 生成 `workspace_tab_loadability()`；Canvas panel 为 `UiOnly`，不再被 extension host bundle 缺失误判为 unavailable，见 `crates/agentdash-workspace-module/src/extension_runtime.rs:315`、`:350`、`:386`。
- Dynamic action discovery owner 仍未定：quick convergence PRD 将 `RuntimeGateway dynamic extension action discovery owner` 明确列为 out of scope；work item 02 也写明“不决定 RuntimeGateway dynamic extension action discovery owner”。

### D7 - RuntimeGateway Dynamic Extension Action Discovery Owner

#### Code Evidence

- RuntimeGateway surface 当前只遍历静态 `providers`，不读 `dynamic_providers`：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:65`、`:73`、`:74`、`:77`。
- RuntimeGateway invoke 会先查静态 provider，再查 dynamic provider：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:90`、`:95`、`:97`、`:118`。
- Extension action provider 是 dynamic provider，bootstrap 注册在 `with_dynamic_provider(...)`：`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:57`。
- Extension dynamic provider 的 `describe_action()` 只返回 marker action `extension.runtime_action`，不是具体 Project enabled action：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:104`、`:106`。
- Extension dynamic provider 的 `supports()` 以 dotted action key + SessionRuntime kind 判断，未先查询 Project installation catalog：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:115`、`:116`。
- Extension dynamic provider 的 invoke 路径会按 project enabled installations 查找具体 `runtime_actions`，再校验 kind、artifact、permission、input schema 与 workspace metadata：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:126`、`:143`、`:155`、`:161`、`:170`、`:171`、`:177`。
- Project extension runtime endpoint 另行从 enabled installations 生成 `runtime_actions` projection：`crates/agentdash-api/src/routes/extension_runtime.rs:100`、`:106`；projection flatten 在 `crates/agentdash-workspace-module/src/extension_runtime.rs:275`。
- WorkspaceModule descriptor 也另行枚举 `ext.runtime_actions` 并映射成 `WorkspaceModuleOperationDispatch::RuntimeAction`：`crates/agentdash-workspace-module/src/workspace_module/mod.rs:187`、`:198`。
- Canvas runtime bridge snapshot 从 `runtime_gateway.surface_for_actor(...)` 构造：`crates/agentdash-api/src/routes/canvases.rs:1247`、`:1259`。由于 surface 当前不读 dynamic providers，Canvas bridge 看不到 concrete extension actions。
- Extension webview bridge 用 `workspaceData.extensionRuntime.projection.runtime_actions.find(...)` 判断 action 是否可用，再调用 `/extension-runtime/invoke-action`：`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:91`、`:97`。

#### Wrong Path

当前同一个 extension runtime action 同时存在三套 discovery / availability 入口：

1. RuntimeGateway dynamic provider 能执行 concrete action，但 `surface_for_actor()` 只暴露 static providers。
2. WorkspaceModule descriptor 从 Project extension runtime projection 直接把 manifest runtime action 映射成 module operation。
3. Frontend extension webview bridge 用 project-level extension runtime projection 判断 action 可用，再把请求交给 RuntimeGateway invoke。

这会产生两个错误后果：

- “Runtime action 是 RuntimeGateway action” 与 “Runtime action 是 WorkspaceModule/Extension projection operation” 两个模型并存。
- discovery 与 invoke 不共享同一个 resolved action catalog：surface 可能漏掉可执行 action，WorkspaceModule / frontend 可能看到 project-level manifest action，但无法表达 actor/session/backend/context 下的真实可执行性。

#### Recommended Owner / Contract

决策状态：`self-decided`。

推荐 owner：RuntimeGateway 拥有 runtime action discovery catalog；ExtensionRuntimeProjection 只提供 Project installation / manifest fact source，WorkspaceModule 和 frontend 只消费 RuntimeGateway 给定 actor/context 的 concrete action descriptors。

推荐 contract shape：

```rust
#[async_trait]
pub trait RuntimeProvider {
    fn action_kind(&self) -> RuntimeActionKind;
    fn describe_action(&self) -> RuntimeActionDescriptor; // static provider only
    async fn discover_actions(
        &self,
        actor: &RuntimeActor,
        context: &RuntimeContext,
    ) -> Result<Vec<RuntimeActionDescriptor>, RuntimeInvocationError>;
}
```

或在不改变 static provider trait 的情况下引入窄口：

```rust
#[async_trait]
pub trait DynamicRuntimeActionCatalog {
    async fn discover_actions(
        &self,
        actor: &RuntimeActor,
        context: &RuntimeContext,
    ) -> Result<Vec<RuntimeActionDescriptor>, RuntimeInvocationError>;
}
```

关键 contract 要点：

- `RuntimeGateway::surface_for_actor()` 合并 static provider descriptors 与 dynamic provider concrete descriptors。
- `ExtensionRuntimeActionProvider` 从 `RuntimeContext::Session { project_id, session_id, ... }` 读取 enabled installations，过滤 `SessionRuntime` action，并产出 concrete `RuntimeActionDescriptor { action_key, kind, description, input_schema, output_schema, default_policy }`。
- `ExtensionRuntimeActionProvider::supports()` 不再用 dotted-string heuristic 作为主要判定；invoke 需要复用同一个 resolve/discover helper 找到 concrete action。
- marker descriptor `extension.runtime_action` 不进入 public runtime surface；若仍保留，只能作为内部 provider identity，不作为 actor-visible action。
- `ExtensionRuntimeProjection.runtime_actions` 可继续作为 Project asset/catalog 展示事实，但不能作为 actor/session 下的可执行 action surface。

#### First-Principles Convergence

应归并或删除的路径：

- 删除 `RuntimeGateway::surface_for_actor()` 的 static-only 行为，dynamic provider 必须参与 surface discovery。
- 删除 actor-visible `extension.runtime_action` marker action，改为 concrete extension action descriptors。
- 删除 WorkspaceModule 对 extension runtime action 的 manifest-only operation 枚举；WorkspaceModule operation 应由 RuntimeGateway action catalog 或同源 dynamic action catalog 投影。
- 删除 frontend webview bridge 对 `extensionRuntime.projection.runtime_actions` 的执行可用性 gate；bridge 应读取当前 session/actor runtime action surface，或只发起 invoke 并让 Gateway 返回 typed denial。

### D8 - Runtime Action Availability Three-Layer Owner

#### Code Evidence

- Capability spec 要求 runtime 工具、MCP、VFS、WorkspaceModule、hook runtime 与 extension admission 都从 AgentRun effective capability/admission 服务取值；工具 schema exposure 消费 final visible capability view，tool execution 消费 admission decision。
- `AgentRunEffectiveCapabilityPort` 已定义 `effective_capability()` 和 `admit_tool()` 两个入口：`crates/agentdash-application-ports/src/agent_run_surface.rs:309`、`:315`。
- quick convergence 后，tool-level grant 不再改写 schema-facing `CapabilityState`：`execution_capability_state_for_runtime_session()` 调用 grant projection 但返回 `base_state.clone()`，见 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:266`、`:272`、`:278`。
- quick convergence 后，grant projection 按 `launch_frame_id` 查询 active grants，而不是按整个 run：`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:286`、`:292`、`:293`。
- WorkspaceModule tool provider 先按 `CapabilityState` 的 `ToolCluster::WorkspaceModule` 和 `is_capability_tool_enabled(...)` 决定是否注入 list/describe/operate/invoke/present 工具：`crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:161`、`:188`、`:245`、`:260`。
- WorkspaceModule visible module projection 读取 `AgentRunEffectiveCapabilityView.capability_state.workspace_module` 与 `visible_workspace_module_refs`，再过滤 Project enabled extension/canvas modules：`crates/agentdash-workspace-module/src/workspace_module/visibility.rs:47`、`:48`、`:53`、`:55`。
- WorkspaceModule list/describe 工具通过 AgentRun bridge 读取 effective capability view，而不是自行读 preset：`crates/agentdash-workspace-module/src/workspace_module/tools.rs:101`、`:116`、`:128`。
- `workspace_module_invoke` 在缺少 RuntimeGateway/channel transport 或 runtime backend anchor 时仍装配 diagnostic tool，而不是让 session launch 失败：`crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:292`、`:304`、`:311`、`:323`。
- WorkspaceModule descriptor 当前用 extension host bundle 与 UI-only tab 判断 module status；runtime actions 被直接映射为 operations：`crates/agentdash-workspace-module/src/workspace_module/mod.rs:187`、`:246`、`:260`、`:265`。

#### Wrong Path

D8 的错误不是某一个字段错，而是 availability 被三层重复表达：

- CapabilityState / AgentRun effective view 表达 `workspace_module` 工具 cluster 可见、module allowlist 与 runtime visible refs。
- RuntimeGateway provider support 表达某个 concrete action 在 actor/context 下是否存在、是否可 invoke、schema/permission 是否通过。
- WorkspaceModule / Extension UI 表达 module/tab/operation 是否能展示，以及缺少 backend/runtime dependency 的诊断。

当前重复点是 WorkspaceModule descriptor 和 frontend extension runtime projection 同时把 Project-level manifest action 当成可执行 operation/action；RuntimeGateway 又在 invoke 时重新解析 enabled installation/action。这让 “看到 action” 和 “能执行 action” 没有同源。

#### Recommended Owner / Contract

决策状态：`self-decided`。

推荐三层 owner：

1. **AgentRun effective capability owner**：决定模型/工具面是否有 `workspace_module` cluster、哪些 WorkspaceModule refs 对当前 AgentRun 可见。它不关心 concrete extension action schema、extension host bundle、backend online 或 protocol channel dependency。
2. **RuntimeGateway action catalog owner**：决定 concrete runtime action 在 `(actor, context)` 下是否 discoverable / invokable，拥有 action descriptor、input/output schema、permission declaration、Project enabled installation 过滤、package artifact requirement、runtime target metadata 校验。
3. **WorkspaceModule / Extension presentation owner**：把 visible modules、RuntimeGateway action catalog、extension tab loadability、channel dependency和 backend availability 投影成 UI / Agent-facing diagnostics。它能展示 “module visible but invoke unavailable”，但不制造新的 capability 或 runtime action fact。

Missing dependency 结论：缺失 RuntimeGateway、channel transport、runtime backend anchor 或 backend offline 应是 typed resource diagnostic，不是 launch readiness failure。理由是 list/describe/present 仍可工作，且 module visibility 是 AgentRun capability fact；只有实际 invoke 需要可执行 runtime target。当前 diagnostic tool 方向正确，但 diagnostic 的 action/operation source 应改为 RuntimeGateway catalog 同源。

#### Contract Shape

建议把 availability 名字拆开，避免三层复用同一个 `available`：

```rust
pub struct RuntimeActionCatalogEntry {
    pub descriptor: RuntimeActionDescriptor,
    pub extension_key: Option<String>,
    pub invocation_requirements: RuntimeActionInvocationRequirements,
}

pub enum RuntimeActionReadiness {
    Ready,
    MissingRuntimeGateway,
    MissingChannelTransport,
    MissingRuntimeBackendAnchor,
    BackendOffline,
    ExtensionArtifactMissing,
    PermissionDenied { reason: String },
}

pub struct WorkspaceModuleOperationProjection {
    pub operation_key: String,
    pub dispatch: WorkspaceModuleOperationDispatch,
    pub action: Option<RuntimeActionCatalogEntry>,
    pub readiness: RuntimeActionReadiness,
}
```

Layer rules：

- `CapabilityState.workspace_module` 只回答 “该 module ref 是否 visible”。
- `RuntimeGateway.surface_for_actor()` 只回答 “这些 action descriptors 是当前 actor/context 的 action catalog”。
- `WorkspaceModuleOperationProjection.readiness` 只回答 “当前 UI / Agent 操作能否立即 invoke；不能时用哪类诊断解释”。

#### First-Principles Convergence

应归并或删除的路径：

- WorkspaceModule 不再从 `ExtensionRuntimeProjection.runtime_actions` 直接声明 runtime-action operation；它从 RuntimeGateway action catalog 取得 concrete action descriptors，再按 visible extension/module ref 做 projection。
- Extension webview bridge 不再把 project extension runtime projection 当作 session action availability；如果要支持 `runtime.invoke_action` discovery，应读取 session runtime action surface。
- RuntimeGateway invoke 不再与 surface discovery 使用不同 resolve 逻辑；同一 `resolve_extension_action(context, action_key)` helper 同时服务 catalog 和 invoke。
- `available` 字段只保留在 renderer/tab loadability 语义；action readiness 使用单独 enum / diagnostic，避免和 capability visibility 混淆。

### Implementation Slices

1. **RuntimeGateway dynamic catalog slice**
   - Extend Gateway surface path to include dynamic providers.
   - Add dynamic discover contract or dynamic catalog trait.
   - Make ExtensionRuntimeActionProvider emit concrete descriptors from enabled installations for `RuntimeContext::Session`.
   - Stop exposing marker `extension.runtime_action` in public `RuntimeSurface`.
   - Verification: unit test `surface_for_actor()` includes enabled extension action descriptor; disabled/missing project context produces no extension descriptors; marker descriptor absent.

2. **Shared extension action resolver slice**
   - Extract provider helper that resolves `(project_id, action_key)` to installation + action + artifact/readiness.
   - Reuse helper in `discover_actions()` and `invoke()`.
   - Replace dotted action key heuristic with resolved catalog lookup.
   - Verification: invoke and catalog agree on enabled/disabled/action-kind/artifact-missing cases.

3. **WorkspaceModule operation source slice**
   - Change extension runtime action operations to be produced from RuntimeGateway/dynamic action catalog, not raw manifest projection.
   - Keep protocol channel operations on extension channel catalog, but mark them as channel operations/readiness, not RuntimeGateway actions.
   - Preserve tab loadability from ExtensionRuntimeProjection.
   - Verification: WorkspaceModule describe shows operations only for action catalog entries; UI-only Canvas panel without action remains presentable; missing runtime deps yields typed diagnostic operation state, not module invisibility.

4. **Frontend bridge availability slice**
   - Feed extension webview/canvas runtime bridge with session runtime action surface or remove pre-invoke project-level action gate.
   - Keep `extensionRuntime.projection.workspace_tabs` for tab discovery/loadability.
   - Verification: webview bridge tests assert runtime.invoke_action uses session action surface or backend denial; no test relies on `projection.runtime_actions` as execution availability.

5. **Diagnostics naming slice**
   - Rename or split action/module readiness fields so capability visibility, renderer loadability, and invocation readiness do not share one generic `available` meaning.
   - Verification: tests cover `workspace_module_runtime_dependencies_unavailable`, `runtime_ref_not_found`, artifact missing and backend offline as diagnostics rather than session launch failures.

### Validation Strategy

- Targeted Rust tests only; no broad Rust compile required for this research.
- Suggested tests:
  - `agentdash-application-runtime-gateway` unit tests for dynamic surface catalog + invoke/catalog consistency.
  - `agentdash-workspace-module` tests for descriptor operation source, UI-only tab readiness, missing runtime deps diagnostic, and visibility filtering via AgentRun effective view.
  - `agentdash-api` route tests or handler tests for Canvas runtime bridge snapshot including dynamic extension actions.
  - `app-web` tests for ExtensionWebviewBridge action discovery / invocation behavior and tab loadability consumption.
- Grep checks:
  - `rg "extension.runtime_action"` should only find internal provider identity/tests, not actor-visible surface expectations.
  - `rg "projection.runtime_actions.find" packages/app-web/src/features/extension-runtime` should disappear or be limited to non-execution catalog display.
  - `rg "WorkspaceModuleOperationDispatch::RuntimeAction" crates/agentdash-workspace-module` should point to RuntimeGateway-catalog-backed construction, not raw manifest mapping.

### Decision Summary

- D7 decision: `self-decided` - RuntimeGateway owns dynamic runtime action discovery because it already owns invocation, actor/context validation and action descriptors. Extension runtime projection remains Project installation catalog; WorkspaceModule/frontend consume Gateway action surface for executable actions.
- D8 decision: `self-decided` - AgentRun effective capability owns visibility, RuntimeGateway owns executable action catalog/support, WorkspaceModule/Extension presentation owns readiness diagnostics. Missing dependency is typed resource diagnostic, not launch readiness failure.

### External References

- None. 本次 research 只需要仓库内任务文档、Trellis specs 和当前代码证据。

### Related Specs

- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/cross-layer/architecture.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本 research 使用用户显式指定的 `.trellis/tasks/06-30-design-backlog-review` 作为写入边界。
- 未运行编译或测试，符合本 research 要求；证据来自 targeted `rg` / read。
- 未修改业务代码、spec 或其它任务文件。
- 未找到 quick convergence 后仍需要把 schema validator、extension invocation workspace resolver、renderer-aware loadability 作为 D7/D8 unresolved item 处理的证据；它们应从 D7/D8 的未决范围删除。
