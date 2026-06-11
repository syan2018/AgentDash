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
