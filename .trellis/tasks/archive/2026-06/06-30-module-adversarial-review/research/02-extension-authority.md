# Research: Extension / Workspace Module Runtime Surface + Authority & Capability Runtime

- Query: 对 Extension / Workspace Module Runtime Surface 与 Authority & Capability Runtime 做对抗性架构审查，重点覆盖 workspace-module、extension runtime、extension host、SDK/UI、canvas module runtime、PermissionGrant、policy、escalation、CapabilityResolver、tool catalog、MCP capability、VFS capability；Contract 仅作为跨层投影证据。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-module-adversarial-review/check.jsonl` - 当前审查任务的检查项和关联 spec 清单。
- `.trellis/tasks/06-30-module-adversarial-review/prd.md` - 当前任务目标，要求按模块边界和过度设计风险做对抗性审查。
- `.trellis/tasks/06-30-module-adversarial-review/design.md` - 当前审查方法，要求保留第一性原理和旧 baseline 对照。
- `.trellis/tasks/06-30-module-adversarial-review/implement.md` - 当前任务产物约束和 research 写入要求。
- `.trellis/tasks/06-30-module-adversarial-review/brief-review-extension-authority.md` - 本次 Extension/Authority 审查的范围提示。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 旧 baseline 总结，用于 resolved / residual / resurfaced / superseded 对照。
- `.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md` - 旧 VFS / Local Relay / Extension 审查基线。
- `.trellis/tasks/06-14-module-overdesign-review/research/04-frontend-contracts-permission.md` - 旧 Permission / Contract / Frontend 审查基线。
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` - 旧 AgentRun / Session Runtime 审查基线。
- `crates/agentdash-application/src/runtime_tools/provider.rs` - 当前 runtime tool composer，已改成 provider 组合模式。
- `crates/agentdash-api/src/bootstrap/session.rs` - runtime tool provider、RuntimeGateway、effective capability port 的启动装配点。
- `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs` - workspace module runtime tools 的工具注入入口。
- `crates/agentdash-workspace-module/src/workspace_module/tools.rs` - workspace module list / describe / operate / invoke / present 工具实现。
- `crates/agentdash-workspace-module/src/workspace_module/mod.rs` - workspace module operation schema 预校验逻辑。
- `crates/agentdash-workspace-module/src/workspace_module/visibility.rs` - workspace module 可见性过滤。
- `crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs` - workspace module 与 AgentRun runtime surface 的桥接。
- `crates/agentdash-workspace-module/src/extension_runtime.rs` - 从 extension installation 投影 runtime actions / channels / panels。
- `crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs` - RuntimeGateway 静态 provider 与 dynamic provider 的调用和 surface 入口。
- `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs` - extension action/channel invocation provider 和 JSON schema subset validator。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` - AgentRun effective capability / grant projection / admission 服务实现。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs` - AgentRun frame runtime surface 更新与 delivery runtime effective view。
- `crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs` - session tool surface 构建时的 capability projection 调用点。
- `crates/agentdash-application-ports/src/agent_run_surface.rs` - AgentRun runtime surface 与 effective capability port 定义。
- `crates/agentdash-domain/src/permission/entity.rs` - PermissionGrant 领域字段，包括 run、effect frame、runtime session 来源。
- `crates/agentdash-domain/src/permission/repository.rs` - PermissionGrant frame/run 维度查询接口。
- `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs` - PermissionGrant repository 的 frame/run 查询实现。
- `crates/agentdash-local/src/extensions/host/host_api.rs` - local extension host workspace root 解析和 override 拒绝。
- `crates/agentdash-local/src/extensions/host/process_api.rs` - local extension host process/env 权限检查。
- `crates/agentdash-local/src/extensions/host/manager.rs` - local extension action/channel output schema validation。
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts` - extension webview bridge 与 VFS target 选择。
- `packages/app-web/src/features/session/model/companionRequestViewModel.ts` - companion capability grant request 的 UI 投影。

### Related Specs

- `.trellis/spec/cross-layer/architecture.md` - 后端是业务事实源，前端只承载投影和交互。
- `.trellis/spec/backend/capability/architecture.md` - AgentRun effective capability / admission 是最终运行时 capability 入口；PermissionGrant 是 AgentRun-scoped authorization/guardrail，tool-internal grant 应保持 admission-only。
- `.trellis/spec/backend/permission/architecture.md` - PermissionGrant 查询、状态和 policy/escalation 投影边界。
- `.trellis/spec/backend/vfs/architecture.md` - VFS runtime tools 只应由 VFS provider 负责，workspace module tools 由 workspace module provider 负责。
- `.trellis/spec/backend/session/architecture.md` - runtime surface 更新应通过 AgentRunFrameSurfaceService / RuntimeSurfaceUpdateRequest。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - Local TS Extension Host 的 manifest capability、runtime.invoke、canonical channel key、workspace root 和 command router 边界。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - ProjectBackend / Workspace routing 与 VFS target 的跨层边界。
- `.trellis/spec/frontend/architecture.md` - extension runtime / canvas / VFS browser 共享 workspace runtime 和 VFS target selector；webview bridge 不应成为 runtime routing authority。

### Baseline Status Snapshot

- 06-14 `RelayRuntimeToolProvider` 过厚：resolved。`SessionRuntimeToolComposer` 现在只组合 providers，VFS / Workflow / Collaboration / Task / WorkspaceModule 各自注册 provider（`crates/agentdash-application/src/runtime_tools/provider.rs:42`, `crates/agentdash-api/src/bootstrap/session.rs:434`）。
- 06-14 Local `CommandHandler` 全局命令 hub：superseded。当前 spec 和代码已经把 local host API、extension action、VFS/local command 分层，但本次仍未重新完整审查 local command router。
- 06-14 Extension Host contract 过宽：mostly resolved。host 拒绝 workspace root override（`crates/agentdash-local/src/extensions/host/host_api.rs:86`, `crates/agentdash-local/src/extensions/host/host_api.rs:112`），process/env 权限拆细（`crates/agentdash-local/src/extensions/host/process_api.rs:14`, `crates/agentdash-local/src/extensions/host/process_api.rs:58`），action/channel output schema 已校验（`crates/agentdash-local/src/extensions/host/manager.rs:115`, `crates/agentdash-local/src/extensions/host/manager.rs:145`）。
- 06-14 Permission list/read model typed gaps：resolved。repository 支持 frame/run 和 status 过滤（`crates/agentdash-domain/src/permission/repository.rs:24`），API 返回 typed escalation/policy DTO（`crates/agentdash-api/src/routes/permission_grants.rs:86`, `crates/agentdash-api/src/routes/permission_grants.rs:101`）。
- 06-14 capability catalog 绕过 generated contract：resolved。前端 ToolDescriptor 已使用 generated DTO alias，后端 workflow routes 输出 DTO 投影（`packages/app-web/src/types/workflow.ts:198`, `crates/agentdash-api/src/routes/workflows.rs:1302`）。
- 06-14 VFS browser 与 extension bridge 重复 target 选择：resolved。extension webview bridge 复用 VFS selector（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:11`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:206`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:225`）。

### Issue 1: PermissionGrant Projection Uses Run-Wide Grants To Mutate Visible Capability State

- Priority: P1
- Baseline: resurfaced。06-14 的 “PermissionGrant 与 companion capability grant 双事实源” 已被弱化，但新的主要风险变成 PermissionGrant 在运行期 projection 的粒度和语义不正确。
- Problem classification: authority boundary leak / admission 与 visible surface 混淆 / cross-agent grant bleed。
- Code evidence:
  - `PermissionGrant` 同时保存 source lifecycle run、target AgentFrame 和 source runtime session，领域注释已经区分 source 与 effect frame（`crates/agentdash-domain/src/permission/entity.rs:17`, `crates/agentdash-domain/src/permission/entity.rs:25`）。
  - repository 同时提供 frame 和 run 维度 active grant 查询（`crates/agentdash-domain/src/permission/repository.rs:38`）。
  - effective capability 服务按 session 找 anchor 后调用 `list_active_by_run(anchor.run_id)`，没有按 current effect frame / current agent 过滤（`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:306`, `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:313`）。
  - `AgentRunGrantProjection::apply_to_execution_capability_state` 会把 active grant 插入 `capability_state.capabilities`、`enabled_clusters` 和 `tool_policy`，直接改变模型可见 tool surface（`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:116`, `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:123`, `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:132`）。
  - session tool builder 在组装工具前调用 `execution_context_with_agent_run_admission_projection`，实际通过 `RuntimeSessionEffectiveCapabilityPort` 拉取上述 projection（`crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs:189`, `crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs:221`）。
- Impact:
  - tool-internal grant 按 spec 应是 admission-only，但当前可把 grant 变成模型可见工具 schema 的扩张。
  - run-wide 查询意味着同一 lifecycle run 内某个 frame/session 的 active grant 可能影响另一个 agent 的 tool surface，尤其是多 agent 或 continuation 场景。
  - policy/escalation 的业务语义变得难以验证：同一 grant 既像审批记录，又像可见 capability mutation。
- Suggested boundary convergence:
  - 运行期可见 tool surface 和 execution admission 都应收敛到一个 production `AgentRunEffectiveCapabilityPort`，入参必须包含 `run_id + agent_id + current frame/delivery session`。
  - tool-internal grant 不应直接 mutate global `CapabilityState`；要么只在 execution entry 调 `admit_tool`，要么只把按 current effect frame 过滤后的 grant projection 应用到本次 delivery 的 effective view。
  - `list_active_by_run` 保留给审计/read model；execution/admission projection 应使用 `effect_frame_id` 或显式 current agent/frame 解析后的查询。

### Issue 2: AgentRun Effective Capability Port Exists As Contract But Is Not The Production Boundary

- Priority: P1
- Baseline: residual。06-14 的 RuntimeGateway / AgentRun effective capability 边界未完全收束；本次表现为 port 定义存在，但调用方仍各自接不同实现入口。
- Problem classification: unused abstraction / split authority / runtime capability reader fork。
- Code evidence:
  - `AgentRunEffectiveCapabilityPort` 定义了 `effective_capability` 和 `admit_tool`（`crates/agentdash-application-ports/src/agent_run_surface.rs:309`），但未找到 production `impl AgentRunEffectiveCapabilityPort`。
  - session bootstrap 注入的是 `runtime_session_effective_capability_port`，不是 `AgentRunEffectiveCapabilityPort`（`crates/agentdash-api/src/bootstrap/session.rs:269`, `crates/agentdash-api/src/bootstrap/session.rs:290`）。
  - session tool builder 使用 `RuntimeSessionEffectiveCapabilityPort` 做 execution capability projection（`crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs:221`）。
  - workspace module visibility 使用 `AgentRunEffectiveCapabilityView` 过滤 workspace modules（`crates/agentdash-workspace-module/src/workspace_module/visibility.rs:28`, `crates/agentdash-workspace-module/src/workspace_module/visibility.rs:40`）。
  - delivery runtime effective view 从当前 runtime surface frame 直接计算，并传入 empty grant projection（`crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs:187`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs:204`）。
- Impact:
  - session tool surface、workspace module visibility、extension runtime projection 读取 capability 的路径不一致；同一 AgentRun 在不同入口可能得到不同的可见性/准入结果。
  - `admit_tool` 作为清晰的 execution boundary 存在于 port，却没有成为实际调用链入口，导致 Issue 1 的 admission 与 visible surface 混用更难被局部修复。
  - MCP / VFS / workspace module tool catalog 后续接入时会继续选择“离自己最近”的 capability reader，而不是同一个 AgentRun boundary。
- Suggested boundary convergence:
  - 实现 production `AgentRunEffectiveCapabilityPort`，并让 session tool builder、workspace module visibility、VFS/MCP capability projection 都消费这个 port。
  - `RuntimeSessionEffectiveCapabilityPort` 只保留为 delivery-session adapter：解析 runtime session 到 `run_id / agent_id / current frame` 后委托 AgentRun port。
  - 将 `admit_tool` 接到 runtime tool execution entry，避免只在 tool assembly 阶段做一次可见性变换。

### Issue 3: Workspace Module Operation Schema Uses A Weaker Validator Than Extension Runtime

- Priority: P1
- Baseline: residual / resurfaced。06-14 的 extension host schema 问题已大幅修复，但 workspace module facade 又形成了新的 schema authority fork。
- Problem classification: duplicated schema authority / facade preflight drift / diagnostic mismatch。
- Code evidence:
  - workspace module 的 `validate_input_against_schema` 只检查 schema 顶层 type 和 required fields（`crates/agentdash-workspace-module/src/workspace_module/mod.rs:49`, `crates/agentdash-workspace-module/src/workspace_module/mod.rs:66`）。
  - `workspace_module_invoke` 在 dispatch 前用这个弱校验报告 `input_schema_mismatch`（`crates/agentdash-workspace-module/src/workspace_module/tools.rs:1364`, `crates/agentdash-workspace-module/src/workspace_module/tools.rs:1376`）。
  - extension runtime action input 使用 `validate_json_schema_subset` 校验（`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:169`），该 validator 支持 type、required、properties、additionalProperties、items、enum、const 等约束（`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:361`）。
  - extension channel method input 同样走 `validate_json_schema_subset`（`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:649`）。
  - local extension host output schema 也在 manager 层校验（`crates/agentdash-local/src/extensions/host/manager.rs:115`, `crates/agentdash-local/src/extensions/host/manager.rs:145`）。
- Impact:
  - 同一个 `operation.input_schema` 在 workspace module facade 和 extension runtime gateway 里有两套解释，invalid additional properties、enum、array item 等可能先通过 facade，再在后层失败。
  - 对 HostCanvas 类操作，弱校验可能成为唯一通用 preflight gate，导致 schema 文档与实际 runtime contract 的可信度下降。
  - SDK/UI 面向用户展示的是 workspace module operation schema，但错误来源可能是 workspace module facade 或 runtime gateway，诊断不稳定。
- Suggested boundary convergence:
  - 将 JSON schema subset validator 移到共享 crate/module，由 extension runtime、local host、workspace module tool、SDK/manifest 校验共用。
  - `workspace_module_invoke` 不应维护一个更弱的 schema authority；它要么调用同一个 validator，要么只做 ownership/visibility/dispatch 检查，把 operation input 校验交给 owning executor。
  - schema mismatch 的 error code 和 message 应由唯一 schema authority 产生，workspace module facade 只透传上下文。

### Issue 4: Workspace Module Tool File Is A Product Facade Plus Canvas Lifecycle Plus Extension Invocation Hub

- Priority: P2
- Baseline: superseded / residual。06-14 关注 extension 与 workspace module 是否应该合并；当前产品抽象上合并是合理方向，但 implementation owner 仍过厚。
- Problem classification: module over-thickness / hidden canvas ownership / change blast radius。
- Code evidence:
  - `workspace_module/tools.rs` 文件头描述 list/describe 工具，但同文件实际承载 list、describe、operate、invoke、present，以及 extension/canvas/VFS/AgentRun imports（`crates/agentdash-workspace-module/src/workspace_module/tools.rs:1`, `crates/agentdash-workspace-module/src/workspace_module/tools.rs:10`）。
  - `WorkspaceModuleOperateTool` 名称是通用 workspace module operate，但实际只支持 `canvas.create`、`canvas.attach`、`canvas.copy`（`crates/agentdash-workspace-module/src/workspace_module/tools.rs:565`, `crates/agentdash-workspace-module/src/workspace_module/tools.rs:601`, `crates/agentdash-workspace-module/src/workspace_module/tools.rs:664`）。
  - `WorkspaceModuleInvokeTool` 同时持有 extension installation repo、canvas repos/runtime state、execution anchor repo、project id、delivery session、backend、runtime gateway、channel invoker、visibility、current user（`crates/agentdash-workspace-module/src/workspace_module/tools.rs:1084`）。
  - invoke dispatch 同时处理 RuntimeAction、ProtocolChannel 和 HostCanvas（`crates/agentdash-workspace-module/src/workspace_module/tools.rs:1388`, `crates/agentdash-workspace-module/src/workspace_module/tools.rs:1430`, `crates/agentdash-workspace-module/src/workspace_module/tools.rs:1491`）。
- Impact:
  - “workspace module” 作为产品投影是统一的，但 code owner 不是统一业务领域：canvas lifecycle、extension invocation、presentation、visibility 被压到同一工具文件，新增任一 runtime 类型都会扩大该文件的变更面。
  - `workspace_module_operate` 的名字暗示通用 module lifecycle，但语义是 canvas bootstrap；未来添加非 canvas module lifecycle 时会被迫沿用 canvas 形状，或再造一个并行工具。
  - canvas module runtime 与 extension runtime 的 failure mode、schema 校验、presentation binding 被同一 facade 吸收，架构风险隐藏在通用命名下。
- Suggested boundary convergence:
  - 保留 `WorkspaceModuleDescriptor / WorkspaceModuleOperation` 作为模型面向的产品投影，但拆 implementation owner。
  - 建议拆为 `catalog/visibility`、`extension_invocation`、`canvas_operations`、`presentation` 等内部模块；runtime tool provider 只组装这些 owner。
  - 若 canvas create/attach/copy 不是所有 workspace module 的共性生命周期，应改成 canvas-owned operation tool，或至少在实现边界上保持 canvas module owner。

### Issue 5: RuntimeGateway Surface Omits Dynamic Extension Actions While Invocation Accepts Them

- Priority: P2
- Baseline: residual。06-14 已指出 RuntimeGateway dynamic surface manifest 可作为后续任务；当前仍存在 catalog 与 invocation 的权威分叉。
- Problem classification: catalog/invocation split / dynamic provider projection gap。
- Code evidence:
  - `RuntimeGateway` 同时持有 `providers` 和 `dynamic_providers`（`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:11`）。
  - `surface_for_actor` 只遍历静态 `providers`，不读取 dynamic providers（`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:65`, `crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:71`）。
  - `invoke` 会先找静态 provider，找不到再从 `dynamic_providers` 里按 `supports` 解析（`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:90`, `crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:96`）。
  - bootstrap 把 extension runtime action provider 注册为 dynamic provider（`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:57`）。
  - extension action provider 的 `describe_action` 只给 generic marker `extension.runtime_action`，`supports` 接受 dotted action key（`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:103`, `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:114`）。
  - 具体 extension runtime actions 由 workspace-module 的 installation projection 生成（`crates/agentdash-workspace-module/src/extension_runtime.rs:167`, `crates/agentdash-workspace-module/src/extension_runtime.rs:257`）。
- Impact:
  - 如果任何调用方把 `RuntimeGateway::surface_for_actor` 当成完整可调用 action catalog，就会漏掉 extension dynamic actions。
  - 现在 extension action catalog 实际由 workspace-module projection 持有，RuntimeGateway 只负责 invocation；这个分工可以成立，但必须被明确化，否则 tool catalog、SDK/UI 或 policy 层会重新从 RuntimeGateway surface 推导错误事实。
  - dynamic provider 的 generic marker 对审计和 policy 不足以表达具体 extension action 的可见性。
- Suggested boundary convergence:
  - 明确 RuntimeGateway surface 只代表 built-in static actions，extension action catalog 由 Extension Runtime Projection 负责；或给 dynamic provider 增加 context-aware surface projection hook。
  - policy/capability/catalog 层不得从 `surface_for_actor` 推导完整 extension runtime surface，必须消费 workspace-module/extension projection 或统一 AgentRun effective surface。
  - 审计事件应记录 canonical extension action key，而不只记录 generic dynamic provider marker。

### Issue 6: Companion Capability Grant Request Is Non-Authoritative But Still Names A Capability Grant Flow

- Priority: P2
- Baseline: residual / mostly superseded。06-14 的双事实源风险已被缓解，因为 UI 已声明 PermissionGrant 为准，平台 handler 也不提交授权结果；但命名仍保留 capability grant request。
- Problem classification: stale concept / UX projection drift。
- Code evidence:
  - companion payload 仍注册 `capability_grant_request`（`crates/agentdash-application/src/companion/payload_types.rs:87`）。
  - 平台 handler 对该 payload 返回 unsupported，并说明缺少 platform permission grant broker、policy inputs、live runtime handoff（`crates/agentdash-application/src/companion/tools.rs:2008`）。
  - 前端把该卡片解释为“能力授权以 PermissionGrant 审批为准，此会话卡片不提交授权结果”（`packages/app-web/src/features/session/model/companionRequestViewModel.ts:43`, `packages/app-web/src/features/session/model/companionRequestViewModel.ts:88`）。
- Impact:
  - 当前不会形成第二个授权结果事实源，但名称仍暗示 companion 可发起 capability grant flow，容易让 SDK/UI 或后续 agent 把它当成授权入口。
  - 旧 payload 与新 PermissionGrant/policy/escalation 语义并行存在，增加新开发者判断成本。
- Suggested boundary convergence:
  - 若保留该 payload，应重命名为 read-only advisory / permission prompt projection，并把 PermissionGrant broker 作为唯一可提交审批路径。
  - 如果没有兼容约束，应删除或迁移到 PermissionGrant request projection，避免保留 capability grant 的旧概念名。

### Positive Boundary Evidence

- Runtime tool composer 已从 RelayRuntimeToolProvider 过厚实现收束为 provider 组合，workspace module tools 不再由 VFS provider 混合注入（`crates/agentdash-application/src/runtime_tools/provider.rs:42`, `crates/agentdash-api/src/bootstrap/session.rs:434`）。
- workspace module runtime provider 只在 `ToolCluster::WorkspaceModule`、project/VFS/current user 等前置条件满足时注入 module tools（`crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:156`, `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs:184`）。
- Local extension host 已拒绝 raw workspace root override，并按细粒度 process/env capability 检查（`crates/agentdash-local/src/extensions/host/host_api.rs:112`, `crates/agentdash-local/src/extensions/host/process_api.rs:18`, `crates/agentdash-local/src/extensions/host/process_api.rs:142`）。
- Extension webview bridge 已复用 VFS selector，未再成为独立 VFS routing authority（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:11`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:206`）。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回当前 active task 为空；本文件按用户显式给出的 `.trellis/tasks/06-30-module-adversarial-review` 目录写入。
- 本次未跑全量测试，也未修改任何业务代码；仅执行只读检索和写入本 research 文件。
- 未使用外部资料；External references: none。审查依据为本仓库 `.trellis/spec/`、当前任务文档、06-14 baseline 和目标代码。
- 未找到 production `impl AgentRunEffectiveCapabilityPort`；该结论来自仓库文本检索，若后续通过宏或外部 crate 注入实现，需要重新核对。
- Contract 文件只作为跨层 DTO / projection 证据使用，没有把 contract 当成独立业务模块审查。
