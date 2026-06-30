# Work Item 02: Extension 与 Workspace Module 一致性收束

## Goal

收束 Extension / Workspace Module Runtime Surface 中三个局部但会造成事实分叉的问题：schema validator、extension invocation workspace resolver、Canvas promoted extension loadability。

## Source Issues

- `adversarial-review.md` Issue 11。
- `adversarial-review.md` Issue 13。
- `adversarial-review.md` Issue 14。
- `research/07-extension-workspace-module-surface.md`。
- `research/02-extension-authority.md`。

## Evidence

### Loadability 分叉

- `crates/agentdash-application/src/canvas/promotion.rs:70` / `:74` / `:79` 生成 CanvasPanel tab，但 bundles 为空。
- `crates/agentdash-workspace-module/src/workspace_module/mod.rs:296` / `:300` / `:303` 用 `ext.bundles` 判断 module ready。
- `packages/app-web/src/features/extension-runtime/model/extensionTabDescriptors.tsx:17` / `:33` / `:34` 对 `canvas_panel` 直接渲染。
- `packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx:34` / `:46` 从 package artifact snapshot 加载。

### Workspace resolver 重复

- `crates/agentdash-api/src/routes/extension_runtime.rs:321` 起实现 `select_extension_invocation_workspace`。
- `crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:127` 起实现同构 `select_invocation_workspace`。
- `crates/agentdash-local/src/handlers/extension.rs:282` / `:286` / `:290` 本机 handler 直接消费 root_ref。

### Schema validator 分叉

- `crates/agentdash-workspace-module/src/workspace_module/mod.rs:49` / `:66` 的 validator 只检查 type/required。
- `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:169` / `:361` 有更完整 JSON schema subset validator。

## Requirements

- Extension runtime projection 产出 renderer-aware loadability。
- WorkspaceModule descriptor 与 Extension tab availability 使用同一 loadability 规则。
- Canvas promoted extension 如果是 UI-only tab，descriptor 必须显式表达 UI-only，而不是误报 bundle missing。
- Extension invocation workspace 选择只有一个 owner。
- 缺少可验证 local workspace mount 时显式返回 no workspace / typed error，不静默 fallback 到任意 default mount。
- WorkspaceModule invoke 复用 shared JSON schema subset validator。

## Suggested Implementation Shape

- 将 schema subset validator 移到可被 runtime gateway 与 workspace module 共同依赖的位置。
- 新增 shared extension invocation workspace resolver：
  - 输入：`RuntimeBackendAnchor + Vfs`。
  - 输出：typed local workspace target 或明确 none/error。
  - API route 与 workspace module runtime bridge 都调用它。
- 在 extension runtime projection 增加 loadability 信息：
  - `webview` 需要 bundle/extension host 条件。
  - `canvas_panel` 需要 package artifact + renderer entry。
  - WorkspaceModule descriptor 仅消费 projection，不重新推断。

## Tests / Verification

- Backend tests：
  - workspace module schema rejects same invalid input as extension runtime gateway。
  - API route 与 workspace module resolver 对同一 VFS/anchor 返回同一 workspace target。
  - Canvas promoted extension loadability 不因 `bundles` empty 误判 unavailable。
- Frontend tests/typecheck：
  - extension tab descriptor 与 workspace module presentation 消费一致 projection。

## Out of Scope

- 不决定 RuntimeGateway dynamic extension action discovery owner。
- 不重构 WorkspaceModule tools 文件。
- 不重做 extension host permission model。
