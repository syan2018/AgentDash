# Research: Extension / Workspace Module Runtime Surface

- Query: 单域对抗性架构审查 Extension / Workspace Module Runtime Surface，检查 workspace-module / extension runtime / extension host / extension SDK/UI / canvas module runtime 是否为同一套模型的不同层，runtime process/env/workspace 权限 owner 是否清晰，并对照 06-14 baseline。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `crates/agentdash-workspace-module/src/extension_runtime.rs` - Project extension installation manifest 到 runtime projection 的 flatten owner。
- `crates/agentdash-workspace-module/src/workspace_module/mod.rs` - WorkspaceModule descriptor 合成层，把 extension runtime projection 和 Canvas 聚合投影成统一 module catalog。
- `crates/agentdash-workspace-module/src/workspace_module/tools.rs` - Agent-facing workspace_module list/describe/operate/invoke/present 工具。
- `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs` - WorkspaceModule runtime tool provider，持有 extension channel transport、RuntimeGateway handle 和 AgentRun bridge。
- `crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs` - workspace module 到 RuntimeGateway / AgentRun frame surface / invocation workspace 的桥接 helper。
- `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs` - Project extension runtime action dynamic provider、channel invoker、schema/permission 校验。
- `crates/agentdash-api/src/routes/extension_runtime.rs` - Project extension runtime HTTP projection、invoke-action、invoke-channel、webview asset 路由。
- `crates/agentdash-api/src/routes/workspace_module.rs` - Project workspace module list/present HTTP 路由。
- `crates/agentdash-api/src/routes/canvases.rs` - Canvas runtime invoke / bridge snapshot / promote-extension 入口。
- `crates/agentdash-application/src/canvas/promotion.rs` - Canvas promoted extension package manifest builder。
- `crates/agentdash-local/src/handlers/extension.rs` - Relay extension action/channel command handler，准备 artifact cache 与 local extension host activation。
- `crates/agentdash-local/src/extensions/host/*` - Local TS Extension Host manager、workspace/process/http Host API、permission guard、schema validation。
- `packages/extension-sdk/src/index.ts` - Extension authoring SDK：runtime actions、permissions、workspace tabs、host API facade。
- `packages/extension-ui/src/index.ts` - Extension webview bridge client API。
- `packages/app-web/src/features/extension-runtime/*` - WorkspacePanel extension tab descriptors、webview/canvas panel bridge、Project extension runtime store。
- `packages/app-web/src/features/workspace-module/*` - WorkspaceModule project catalog store / presentation helper。
- `packages/app-web/src/features/workspace-runtime/*` - Workspace runtime shared data context consumed by extension/canvas/workspace-panel。

### Shape Summary

当前主干方向基本正确：`ProjectExtensionInstallation` 是 extension 安装事实源；`extension_runtime_projection_from_installations()` 从安装 manifest 派生 Project runtime projection；`build_workspace_modules*()` 再把 extension projection 与 Canvas 聚合合并成 WorkspaceModule descriptor；Agent runtime tool surface 通过 `WorkspaceModuleRuntimeToolProvider` 注入；本机执行由 relay extension handler 激活 packaged artifact 后进入 Local TS Extension Host。

因此，Workspace Module 与 Extension 不是完全分叉的两个模型，而是“Extension/Canvas 的 Project-level runtime projection”与“Agent-facing workspace module catalog/tool surface”的两层。但仍有三处 owner 边界不够硬，会让 UI、Agent tool、Canvas RuntimeGateway surface 对同一插件能力看到不同事实。

### 1. P1 - Canvas promoted extension 的 loadability 在 Extension tab 与 WorkspaceModule descriptor 之间分叉

- 分类: 概念分叉 / 重复事实源 / 命名或职责漂移。
- 代码证据:
  - Canvas promote 生成的 extension manifest 明确有 `workspace_tabs`，renderer 是 `CanvasPanel`，但 `bundles` 为空：`crates/agentdash-application/src/canvas/promotion.rs:70`、`crates/agentdash-application/src/canvas/promotion.rs:74`、`crates/agentdash-application/src/canvas/promotion.rs:79`。
  - WorkspaceModule extension descriptor 用 `ext.bundles` 是否存在判断 extension module ready，否则标记 `extension runtime bundle 缺失`：`crates/agentdash-workspace-module/src/workspace_module/mod.rs:296`、`crates/agentdash-workspace-module/src/workspace_module/mod.rs:300`、`crates/agentdash-workspace-module/src/workspace_module/mod.rs:303`。
  - 前端 Extension tab descriptor 对 `canvas_panel` 直接渲染 `ExtensionCanvasPanel`，不依赖 `bundles`：`packages/app-web/src/features/extension-runtime/model/extensionTabDescriptors.tsx:17`、`packages/app-web/src/features/extension-runtime/model/extensionTabDescriptors.tsx:33`、`packages/app-web/src/features/extension-runtime/model/extensionTabDescriptors.tsx:34`。
  - `ExtensionCanvasPanel` 通过 package artifact 内的 snapshot entry 加载 Canvas runtime snapshot：`packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx:34`、`packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx:39`、`packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx:46`。
  - Canvas extension availability 只要求 installation 有 `package_artifact`、tab 是 `canvas_panel`、renderer entry 非空：`packages/app-web/src/features/extension-runtime/model/canvasBridge.ts:70`、`packages/app-web/src/features/extension-runtime/model/canvasBridge.ts:76`、`packages/app-web/src/features/extension-runtime/model/canvasBridge.ts:79`、`packages/app-web/src/features/extension-runtime/model/canvasBridge.ts:87`。
- 影响面:
  - 同一个 Canvas-derived packaged extension 在 WorkspacePanel extension tab 中可加载，但在 WorkspaceModule descriptor 中可能被标记为 unavailable。
  - Agent 通过 `workspace_module_list/describe` 看到的 module status 与用户在 WorkspacePanel 能打开的 tab 不一致，降低 workspace-module 作为统一 module catalog 的可信度。
  - 这不是单纯 UI 文案问题；loadability 的事实分别由 “has any bundle” 和 “has package artifact + renderer entry” 两套规则表达。
- 收束边界:
  - Extension runtime projection 应产出 renderer-aware loadability，至少区分 `webview` 需要 `extension_host`/bundle、`canvas_panel` 需要 package artifact + snapshot entry。
  - WorkspaceModule descriptor、Extension tab availability、Project extension management summary 都消费同一个 loadability projection，不能各自推断。
  - 如果 Canvas promoted extension 不应作为 Agent-callable WorkspaceModule extension module，descriptor 层应显式降级为 UI-only tab，而不是用 bundle 缺失误报不可用。
- 06-14 baseline 对照:
  - 旧 baseline 关注 extension contract/schema/workspace root；本问题是新设计引入的 renderer-kind loadability 分叉，不是旧问题残留。

### 2. P1 - RuntimeGateway dynamic extension action 可 invoke 但不进入 `surface_for_actor`

- 分类: 概念分叉 / 路径冗余 / 抽象泄漏。
- 代码证据:
  - RuntimeGateway bootstrap 把 `ExtensionRuntimeActionProvider` 注册为 dynamic provider：`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:57`。
  - `invoke()` 会先查静态 provider，再查 `dynamic_providers.supports(...)`：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:90`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:95`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:97`。
  - `surface_for_actor()` 只遍历 `self.providers`，不读取 `dynamic_providers`：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:65`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:73`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/gateway.rs:78`。
  - Extension dynamic provider 的 descriptor 只是 marker `extension.runtime_action`，`supports()` 则允许 dotted session runtime action key：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:103`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:105`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:114`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:115`。
  - Canvas runtime bridge snapshot 从 `runtime_gateway.surface_for_actor(...)` 构造：`crates/agentdash-api/src/routes/canvases.rs:1242`、`crates/agentdash-api/src/routes/canvases.rs:1247`、`crates/agentdash-api/src/routes/canvases.rs:1259`。
  - WorkspaceModule descriptor 另行从 extension projection 枚举 runtime action operation：`crates/agentdash-workspace-module/src/workspace_module/mod.rs:238`、`crates/agentdash-workspace-module/src/workspace_module/mod.rs:242`、`crates/agentdash-workspace-module/src/workspace_module/mod.rs:249`。
  - 旧 baseline 已把 dynamic extension action 是否进入 RuntimeGateway surface 列为后续评估项：`.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md:17`、`.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md:220`、`.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md:231`。
- 影响面:
  - 同一 extension runtime action 在 `workspace_module_describe` 可发现、在 `RuntimeGateway.invoke` 可执行，但在 `RuntimeGateway.surface_for_actor` 不可发现。
  - Canvas runtime source 暴露 `window.agentdash.invoke(actionKey, input)`：`packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:295`、`packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:296`、`packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts:313`。如果 Canvas/SDK/diagnostic UI 依赖 runtime surface 进行 action discovery，会漏掉 Project enabled extension actions。
  - 这会让 “extension action 是 RuntimeGateway action” 和 “extension action 是 WorkspaceModule operation” 两个模型并存。
- 收束边界:
  - 二选一明确 owner：要么 RuntimeGateway surface 根据 `(project_id, session_id)` 动态展开 Project enabled extension actions，并复用 `ExtensionRuntimeProjection` 的 schema/permission；要么声明 extension action discovery 只属于 WorkspaceModule descriptor，RuntimeGateway surface 只服务非-extension built-in actions。
  - 预研阶段建议不要保留 marker action + out-of-band catalog；最终调用和发现应共享一套可执行 projection。
- 06-14 baseline 对照:
  - 这是 06-14 明确遗留的 residual，不是回归。旧任务当时判断 RuntimeGateway invoke/provider 方向基本可接受，但 dynamic surface manifest 需要单独决定。

### 3. P2 - Extension invocation workspace 选择策略在 API route 与 WorkspaceModule runtime bridge 重复

- 分类: 路径冗余 / 重复事实源 / workspace 权限 owner 不清。
- 代码证据:
  - API route 本地实现 `select_extension_invocation_workspace()`：有 `backend_anchor.root_ref` 时按 mount root_ref 匹配，否则退回 `vfs.default_mount()`：`crates/agentdash-api/src/routes/extension_runtime.rs:321`、`crates/agentdash-api/src/routes/extension_runtime.rs:332`、`crates/agentdash-api/src/routes/extension_runtime.rs:338`、`crates/agentdash-api/src/routes/extension_runtime.rs:349`。
  - WorkspaceModule runtime bridge 另有同构 `select_invocation_workspace()`：`crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:127`、`crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:140`、`crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:150`、`crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:161`。
  - 两条路径都只返回 `mount_id + root_ref`，本机 relay handler 直接把 `root_ref` 转成 `PathBuf`：`crates/agentdash-local/src/handlers/extension.rs:282`、`crates/agentdash-local/src/handlers/extension.rs:286`、`crates/agentdash-local/src/handlers/extension.rs:290`。
  - Runtime backend anchor 的注释规定下游只能消费 anchor，不能从 VFS mount 或在线 backend 列表重新选择 backend：`crates/agentdash-domain/src/backend/runtime_anchor.rs:4`、`crates/agentdash-domain/src/backend/runtime_anchor.rs:6`、`crates/agentdash-domain/src/backend/runtime_anchor.rs:8`。
- 影响面:
  - UI extension panel invoke 与 Agent `workspace_module_invoke` 目前行为相同，但策略 owner 不唯一，任何一处修改都会造成 extension host 默认 workspace 分叉。
  - fallback 到 default mount 没有显式检查 mount provider、backend binding 或本机路径语义；当 `RuntimeBackendAnchor.root_ref` 缺失且 default mount 是 inline/canvas/lifecycle 等非本机 workspace mount 时，本机 host 会收到一个路径语义不确定的 `root_ref`。
  - 这正好落在本次审查重点：runtime process/env/workspace 权限 owner 应清晰；当前 default workspace root 是 API route 与 workspace-module bridge 两处临时推导，不是单一 runtime workspace policy。
- 收束边界:
  - 抽成一个 extension invocation workspace resolver，owner 放在 extension/runtime workspace context 边界，API route 与 workspace module tool 都只调用它。
  - resolver 应消费 `RuntimeBackendAnchor + Vfs` 并返回 typed local workspace target；缺少可验证 local workspace mount 时显式返回 no workspace，而不是静默 default mount。
  - local host activation 继续只消费 relay payload 的 workspace context，不自行选择 root。
- 06-14 baseline 对照:
  - 06-14 的 raw `workspace_root` 覆盖问题已解决；当前问题是修复后出现的策略重复，不是同一漏洞残留。

### Resolved / Healthy Baseline Checks

- Runtime tool composer 已从旧的过厚 VFS provider 收束为 `SessionRuntimeToolComposer` + domain providers：`crates/agentdash-api/src/bootstrap/session.rs:434`、`crates/agentdash-api/src/bootstrap/session.rs:437`、`crates/agentdash-api/src/bootstrap/session.rs:443`、`crates/agentdash-api/src/bootstrap/session.rs:448`、`crates/agentdash-api/src/bootstrap/session.rs:459`。对照 06-14 旧问题：`.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md:67`、`.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md:79`。
- Extension Host raw workspace root 覆盖已收束：Host API 现在拒绝 `workspace_root` 参数并强制使用 active extension 的 default workspace root：`crates/agentdash-local/src/extensions/host/host_api.rs:86`、`crates/agentdash-local/src/extensions/host/host_api.rs:90`、`crates/agentdash-local/src/extensions/host/host_api.rs:102`、`crates/agentdash-local/src/extensions/host/host_api.rs:112`。
- Extension process/env permission 已比 06-14 收窄：shell 与 argv exec 分别要求 `process.shell` / `process.exec`，env overlay 要求 `process.env.set` 或 `process.env.set:{KEY}`：`crates/agentdash-local/src/extensions/host/process_api.rs:22`、`crates/agentdash-local/src/extensions/host/process_api.rs:62`、`crates/agentdash-local/src/extensions/host/process_api.rs:147`、`crates/agentdash-local/src/extensions/host/process_api.rs:149`。
- Extension action/channel input/output schema 已形成执行校验：Gateway 校验 action input 与 channel input：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:170`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:650`；local host manager 校验 action/channel output：`crates/agentdash-local/src/extensions/host/manager.rs:124`、`crates/agentdash-local/src/extensions/host/manager.rs:136`、`crates/agentdash-local/src/extensions/host/manager.rs:155`、`crates/agentdash-local/src/extensions/host/manager.rs:168`。对照 06-14 旧问题：`.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md:132`、`.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md:145`。

### Related Specs

- `.trellis/spec/backend/architecture.md` - 明确 Project extension runtime projection、management、package artifact、Canvas promote-extension 的 owner。
- `.trellis/spec/backend/session/architecture.md` - AgentRun frame/surface update boundary；Canvas / WorkspaceModule / Permission 等业务 owner 不应直接拥有 AgentFrame write/adoption。
- `.trellis/spec/backend/capability/architecture.md` - WorkspaceModule / extension runtime 属于 AgentRun effective capability/admission 的最终可见面。
- `.trellis/spec/backend/vfs/architecture.md` - SessionRuntimeToolComposer 与 WorkspaceModuleRuntimeToolProvider 的装配边界。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - Local TS Extension Host、artifact cache、relay extension action/channel payload、workspace/process/env Host API contract。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - workspace/backend/root_ref 与 ProjectBackendAccess 的 owner。
- `.trellis/spec/frontend/architecture.md` - Workspace tab、extension runtime、canvas panel、workspace runtime surface 的前端 owner。

### External References

- None. 本次审查为代码与项目内 spec/baseline 对照，不需要外部资料。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件使用用户显式指定的任务路径写入，未依赖 session active task pointer。
- 未运行全量测试，符合本次要求；只做静态代码审查和 baseline 对照。
- 未修改业务代码、spec、任务计划或 git 状态。
- 未发现 P0 级别问题。当前最值得拆后续实现任务的是 P1 的 Canvas promoted extension loadability 分叉，以及 RuntimeGateway dynamic extension action surface owner 决策。
