# Runtime action availability split 收束

## Goal

实现 design backlog Slice 3 / D8：把 runtime action availability 收束为三层不重叠 owner。AgentRun effective capability 只决定 WorkspaceModule 可见性；RuntimeGateway actor/context catalog 决定 concrete runtime action support；WorkspaceModule / Extension presentation 只表达 UI entry、operation projection 与 typed readiness diagnostics。

## Source

- Design review: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d8-runtime-action-availability-layers`
- Implementation slice: `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md#slice-3-runtime-action-availability-split`
- Research: `.trellis/tasks/06-30-design-backlog-review/research/02-extension-action-availability.md`
- Completed prerequisite: `.trellis/tasks/06-30-runtime-gateway-dynamic-action-catalog`

## Confirmed Facts

- D7 is complete: `RuntimeGateway::surface_for_actor` returns concrete dynamic extension action descriptors for session/project context and no longer exposes `extension.runtime_action` as actor-visible surface.
- `crates/agentdash-workspace-module/src/workspace_module/mod.rs` still builds extension runtime operations directly from `ExtensionRuntimeProjection.runtime_actions`.
- `crates/agentdash-workspace-module/src/workspace_module/tools.rs` already routes `WorkspaceModuleOperationDispatch::RuntimeAction` through `RuntimeGateway::invoke`; the dispatch execution path is not the main split.
- `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs` already exposes a diagnostic `workspace_module_invoke` tool when RuntimeGateway/channel/backend anchor dependencies are missing, but operation descriptors do not carry typed invocation readiness.
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts` still uses `workspaceData.extensionRuntime.projection.runtime_actions.find(...)` as an execution availability gate before invoking backend.
- Extension tab `loadability.available` is renderer/UI loadability and must not be reused as runtime action invocation readiness.

## Requirements

- WorkspaceModule extension runtime-action operations must be produced from RuntimeGateway actor/context catalog descriptors, not directly from raw `ExtensionRuntimeProjection.runtime_actions`.
- AgentRun effective capability must remain the visibility owner: module filtering still uses `CapabilityState.workspace_module` and `visible_workspace_module_refs`, not RuntimeGateway action presence.
- WorkspaceModule descriptor operations must expose typed invocation readiness distinct from module status and renderer loadability.
- Runtime action readiness must cover at least: ready, missing RuntimeGateway, missing channel transport, missing runtime backend anchor, backend unavailable, and extension artifact missing / action absent from Gateway catalog.
- Missing runtime dependencies must remain diagnostics. They must not suppress module visibility, UI entries, list/describe, or session launch.
- Protocol channel operations remain channel operations, not RuntimeGateway actions. They can use the same readiness vocabulary for channel transport/backend dependency diagnostics.
- Frontend webview bridge must stop treating Project-level `extensionRuntime.projection.runtime_actions` as the execution availability authority. It may either consume a current session runtime action surface when available or call backend and surface typed denial.
- Do not reintroduce `extension.runtime_action` marker surface or a second extension action catalog parallel to RuntimeGateway.

## Out Of Scope

- Do not redesign extension package artifact storage or extension runtime projection shape beyond fields needed for operation readiness.
- Do not change WorkspaceModule capability visibility policy or AgentRun grant/admission semantics; D1 handles execution admission.
- Do not move protocol channel invocation into RuntimeGateway in this slice.
- Do not solve D5 command availability or D9 VFS path policy here.

## Acceptance Criteria

- [x] `workspace_module_list` / `workspace_module_describe` for extension runtime actions use RuntimeGateway `surface_for_actor` results as the operation descriptor source.
- [x] If an extension manifest action is absent from RuntimeGateway catalog, WorkspaceModule does not present it as a ready runtime-action operation; any retained diagnostic is typed and explicit.
- [x] WorkspaceModule visibility tests still prove AgentRun effective capability allowlist/runtime refs own module visibility.
- [x] Missing RuntimeGateway / channel transport / backend anchor produces typed operation or invoke diagnostics without removing visible modules or UI entries.
- [x] Frontend webview bridge no longer calls `projection.runtime_actions.find(...)` to gate `runtime.invoke_action`.
- [x] Targeted backend tests and frontend bridge tests pass.

## Completion Notes

- `WorkspaceModuleOperation` now carries typed invocation readiness separate from module status and renderer loadability.
- Extension runtime-action operations use RuntimeGateway `surface_for_actor` descriptors for schema, description and permission summary. Project extension runtime projection only joins action keys back to owning extension modules or produces typed diagnostics when Gateway catalog does not expose an action.
- `workspace_module_invoke` rejects not-ready operations before dispatch, so manifest-only diagnostic operations do not route into Gateway invoke.
- Frontend extension webview bridge sends `action_key + input` to backend without using Project projection runtime actions as a preflight execution gate.
