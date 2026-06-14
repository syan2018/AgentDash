# 设计

## Components

- `AgentRunWorkspacePage`: 替代交互型 `SessionPage`。
- `AgentRunChatView`: 可以从 `SessionChatView` 迁移，props 使用 AgentRun/Workspace 术语。
- `useAgentRunWorkspaceState`: 读取 `AgentRunWorkspaceView`，并暴露 delivery runtime id 给 command adapter。
- `agentRunPaths.ts`: 统一构造 `/agent-runs/new` 和 `/agent-runs/:runId/:agentId`。

## Executor State

Workspace source priority：

1. accepted command/workspace response execution profile
2. current `frame_runtime.execution_profile`
3. draft ProjectAgent executor
4. recent local preference for new draft only

State key：

```text
draft:{projectId}:{projectAgentId}
agentrun:{runId}:{agentId}:{frameId}
```

`useExecutorConfig` should support authoritative hydrate on key change. Empty authoritative fields should clear stale local values for that workspace when the server explicitly has no value.

## Command Id

Composer keeps an in-flight command id per submitted payload. On transport failure it preserves the command id and input until the server returns accepted or terminal failed state.

New input after the previous command is resolved gets a fresh command id.

## Route Migration

Canonical workspace route 使用完整 AgentRun identity：`/agent-runs/:runId/:agentId`。`runId` 只定位 LifecycleRun，`agentId` 用于选择该 run 内的交互 agent，并成为 workspace state key、command scope 和导航恢复的组成部分。

Replace:

- `projectAgentDraftSessionPath` -> `projectAgentDraftRunPath`
- `SessionShortcutList` -> AgentRun shortcut/list naming
- run/subject trace buttons that currently navigate to `/session/{runtimeSessionId}` with AgentRun route when run/agent refs exist

RuntimeSession trace-only links, if retained, use explicit RuntimeSession Trace wording.
