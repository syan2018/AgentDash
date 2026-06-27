# Implement Plan

## Checklist

- [x] Backend audit: 列出所有 `LifecycleAgent.current_frame_id` / `set_current_frame` / `current_frame_id` 读写点，区分生产路径与测试夹具。
- [x] Create canonical resolver: 用 RuntimeSession anchor + agent id + `AgentFrameRepository.get_current(agent_id)` 解析当前 effective frame。
- [x] Refactor AgentRun Workspace query and command policy to use the canonical resolver.
- [x] Refactor Canvas runtime snapshot session VFS resolution to use the same effective frame path.
- [x] Update adoption path: ensure `adopt_persisted_agent_frame_revision` does not silently adopt a different frame than requested unless explicitly intended and tested.
- [x] Remove or neutralize `LifecycleAgent.current_frame_id` from domain, persistence, DTO-adjacent projections, and migrations.
- [x] Add migration for `lifecycle_agents.current_frame_id` removal or semantic retirement.
- [x] Frontend: centralize Canvas presentation opening so `workspace_module_presented` and menu open use the same helper.
- [x] Frontend: allow authoritative `canvas://{mount_id}` presentation to open before runtime surface refresh completes; refresh content after runtime state updates.
- [x] Tests: backend regression for Canvas expose where active runtime and AgentRun Workspace projection resolve the same frame with `cvs-*` mount.
- [x] Tests: Canvas runtime snapshot resolves binding data from the same frame after `workspace_module_present`.
- [x] Tests: frontend event arrives before runtime surface refresh and still results in an opened Canvas tab.

## Validation Commands

- `cargo check -p agentdash-domain -p agentdash-application -p agentdash-api -p agentdash-infrastructure -p agentdash-contracts`
- `cargo test -p agentdash-application delivery_runtime_selection_current_delivery_returns_binding_coordinate`
- `cargo test -p agentdash-application lifecycle_agent_view_uses_current_delivery_not_raw_latest_anchor`
- `cargo test -p agentdash-api delivery_runtime_session_context_uses_current_delivery_not_latest_anchor`
- `pnpm run contracts:check`
- `pnpm --filter app-web run check`
- `pnpm run migration:guard`

## Risky Files

- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`
- `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs`
- `crates/agentdash-application/src/agent_run/workspace/query.rs`
- `crates/agentdash-application/src/session/capability_service.rs`
- `crates/agentdash-application/src/session/hub/tool_builder.rs`
- `crates/agentdash-api/src/session_construction.rs`
- `crates/agentdash-api/src/routes/canvases.rs`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx`
- `packages/app-web/src/features/workspace-panel/model/canvasModuleOpen.ts`

## Review Gates

- Backend frame resolver has one source of truth and no production read path remains on `LifecycleAgent.current_frame_id`.
- Canvas create/present test proves `workspace_module_presented.presentation_uri` and `resource_surface.mounts` reference the same mount.
- Frontend no longer has three independent Canvas-open interpretations.
- Migration is included and matches the final domain/repository model.
