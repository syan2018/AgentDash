# 实施计划

1. Inventory all `SessionMeta` consumers that affect workspace shell/list/status.
2. Define `RuntimeSessionTraceMeta` and `AgentRunWorkspaceShell` contract names for API child.
3. Mark session list/sidebar and title edit paths as frontend child responsibilities.
4. Mark runtime-control status/action derivation as API contract plus command receipt responsibility.
5. Preserve `RuntimeTraceLaunchState` use of `executor_session_id` and `last_event_seq`.
6. Update parent task review notes with final meta boundary.
7. Validate task artifacts before implementation starts.

## Validation

- `rg -n "SessionMeta|session_meta|ProjectSessionListEntry|SessionShortcutList" crates packages/app-web/src`
- `python ./.trellis/scripts/task.py validate ./.trellis/tasks/06-11-agentrun-runtime-trace-meta-convergence`

## Dependencies

This child precedes `06-11-agentrun-workspace-api-contract`. API contract child consumes the shell/meta boundary and names the generated DTOs.

## Spec Update Notes

- 已更新 `.trellis/spec/backend/session/runtime-execution-state.md`：定义 `SessionMeta` 是 RuntimeSession repository 内部 trace-head projection，浏览器合同以 `RuntimeSessionTraceMeta` 表达 trace facts，服务 trace/feed/debug、repository rehydrate、connector follow-up、事件游标、delivery recovery 与终态诊断。
- 已更新 `.trellis/spec/backend/session/architecture.md`：明确 AgentRun delivery/control commands 使用 AgentRun Workspace public identity，RuntimeSession 负责 trace refs、event log、connector continuation 与 repository rehydrate。
- 已更新 `.trellis/spec/backend/story-task-runtime.md`：区分 Project 下 AgentRun Workspace 列表查询与 RuntimeSession trace inventory 查询。
- 已更新 `.trellis/spec/frontend/state-management.md`：明确 AgentRun Workspace title/status/list/action state 来自 AgentRun Workspace projection 与 `AgentRunWorkspaceView.actions`；RuntimeSession trace metadata 只用于 trace/feed/debug 展示。
- 已更新 `.trellis/spec/frontend/architecture.md`：明确用户可见执行工作台展示 AgentRun Workspace，RuntimeSession trace view 只展示 trace。
- 已更新 `.trellis/spec/cross-layer/frontend-backend-contracts.md`：固化 `RuntimeSessionTraceMeta`、`AgentRunWorkspaceShell` 与 AgentRun command receipt contract 的长期 DTO 边界。
