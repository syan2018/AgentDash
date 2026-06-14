# Research: frontend misleading path implementation map

- Query: 为后续 frontend worker 准备精确迁移地图，覆盖 model selector、composer keyboard、pending UI、workspace panel/resource browser、command store 吞错，以及会误导后续开发者的旧路径清算。
- Scope: internal
- Date: 2026-06-12

## Findings

### Summary

前端重构的核心边界是把 `AgentRunWorkspaceView.actions + control_plane + local executor state + session_runtime surface` 收束为一份后端权威 conversation snapshot。当前前端仍有五个本地事实源：`deriveAgentRunWorkspaceChatControlState` 派生命令、`SessionChatView` 本地解释 Ctrl/Cmd+Enter、`useExecutorConfig`/localStorage 持有模型事实、`PendingMessageList` 直接用 `pending_queue.paused` 决定展示、`useAgentRunWorkspaceState` 额外解析 `session_runtime` VFS。后续实现应先引入 generated snapshot fields，再逐层删除这些旧事实源。

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/workflow.md` | Trellis workflow 与 sub-agent research 输出要求；研究必须写入 task `research/`。 |
| `.trellis/tasks/06-12-agent-run-lifecycle-convergence/prd.md` | 任务目标：snapshot、command intent、model_required、pending projection、resource_surface 与误导路径清算。 |
| `.trellis/tasks/06-12-agent-run-lifecycle-convergence/design.md` | 目标架构：`AgentConversationSnapshot`、command/model/pending/resource resolver、keyboard mapping。 |
| `.trellis/tasks/06-12-agent-run-lifecycle-convergence/implement.md` | 分阶段执行计划，Phase 6/7 是 frontend integration 与 misleading path eradication。 |
| `.trellis/tasks/06-12-agent-run-lifecycle-convergence/research/current-state.md` | 已有代码证据索引，覆盖启动模型、消息命令、pending、resource surface 和旧路径。 |
| `.trellis/tasks/06-12-agent-run-lifecycle-convergence/implement.jsonl` | implementation context；包含 frontend state/type safety spec 与 current-state research。 |
| `.trellis/tasks/06-12-agent-run-lifecycle-convergence/check.jsonl` | check context；包含 frontend state/type safety/quality spec。 |
| `.trellis/spec/frontend/state-management.md` | Store 与 AgentRun workspace state ownership 规范；当前仍描述旧 action model，需要实现后更新为 snapshot model。 |
| `.trellis/spec/frontend/type-safety.md` | Generated DTO 单源、snake_case 与 mapper 边界规范。 |
| `.trellis/spec/frontend/quality-guidelines.md` | frontend check/test 要求。 |
| `.trellis/spec/guides/cross-layer-thinking-guide.md` | 跨层边界指南，明确避免前端自行推断状态。 |
| `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` | AgentRun page 当前组装 chat control、执行 command API、传 workspace runtime data 给 WorkspacePanel。 |
| `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts` | 当前从 draft/workspace action 派生 `SessionChatControlState.primaryAction/secondaryAction` 的旧主路径。 |
| `packages/app-web/src/features/session/ui/SessionChatView.tsx` | Composer orchestration：executor hydration、keyboard mapping、submit dispatch、pending rendering。 |
| `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts` | `SessionChatControlState` 与 `SessionChatPrimaryActionKind` 旧类型。 |
| `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx` | Composer presentation：`InlineModelSelector`、helper text、`ComposerSendButton` 旧 action props。 |
| `packages/app-web/src/features/session/ui/composer/ComposerSendButton.tsx` | 根据 `primaryKind === "enqueue"` 和 `canSteer` 渲染 queue/steer/stop 的旧按钮状态机。 |
| `packages/app-web/src/features/session/ui/composer/PendingMessageRow.tsx` | 当前 pending row/list；`queue.paused` 可在无消息时单独展示 banner。 |
| `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts` | 本地 executor/provider/model/thinking/permission state 与 localStorage/recent usage。 |
| `packages/app-web/src/features/executor-selector/ui/InlineModelSelector.tsx` | 当前模型 selector 消费本地 `execConfig` 与 discovery options。 |
| `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts` | AgentRun workspace fetch 后又用 `resolveVfsSurface({ source_type: "session_runtime" })` 取 resource surface。 |
| `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts` | Session runtime diagnostic/control hook；仍消费 `SessionRuntimeControlView` 与 `session_runtime` surface。 |
| `packages/app-web/src/features/workspace-runtime/model/types.ts` | `WorkspaceRuntimeData.runtimeSurface` 当前是 WorkspacePanel/VFS tab 的 surface 输入。 |
| `packages/app-web/src/features/workspace-panel/tab-types/vfs-tab.tsx` | VFS tab 直接消费 `useWorkspaceData().runtimeSurface`。 |
| `packages/app-web/src/stores/projectStore.ts` | Project store command methods，包括 `launchProjectAgent` 与 `createProjectAgentRun` 返回 `null` 吞错。 |
| `packages/app-web/src/services/project.ts` | Project service，仍有 `launchProjectAgent` mapper/path 与 `createProjectAgentRun`。 |
| `packages/app-web/src/services/lifecycle.ts` | AgentRun command endpoints 与 session runtime control service。 |
| `packages/app-web/src/features/agent-run-workspace/model/workspaceCommandState.ts` | `client_command_id` 去重与 frame execution profile -> executor source helper。 |
| `packages/app-web/src/generated/workflow-contracts.ts` | 当前 generated `AgentRunWorkspaceView` 仍含 `actions/pending_queue/pending_messages`，未有 snapshot command list/model/resource_surface。 |
| `packages/app-web/src/generated/project-agent-contracts.ts` | 当前 generated `ProjectAgentLaunchResult` 与 `ProjectAgentRunStartResult`。 |
| `packages/app-web/src/types/index.ts` | Re-export/alias legacy generated ProjectAgent types。 |
| `crates/agentdash-contracts/src/workflow.rs` | Contract source，仍有 `SessionRuntimeActionSetView`/`SessionRuntimeControlView`。 |
| `crates/agentdash-contracts/src/project_agent.rs` | Contract source，仍有 `ProjectAgentLaunchResult`。 |
| `crates/agentdash-api/src/routes/project_agents.rs` | Backend ProjectAgent `/launch` 与 `/agent-runs` routes。 |
| `crates/agentdash-api/src/routes/sessions.rs` | Backend `/sessions/{id}/runtime-control` route。 |

### Related Specs

- `.trellis/spec/frontend/state-management.md:55` states store consumers should use typed DTO/view model from service/generated types, not become protocol field source. This supports moving command/model/pending/resource facts into generated snapshot DTOs.
- `.trellis/spec/frontend/state-management.md:62` currently describes `AgentRunWorkspaceView.control_plane/actions -> SessionChatControlState` as the control authority. This is now a stale spec relative to `design.md`; after implementation it should be updated to snapshot command ownership.
- `.trellis/spec/frontend/state-management.md:85` says workspace title/status/action state come from AgentRun workspace projection. Keep the projection authority idea, but replace action derivation with snapshot command list.
- `.trellis/spec/frontend/type-safety.md:11` requires generated wire DTOs as single source. New `AgentConversationSnapshot`, command list, keyboard mapping, model state, pending view and resource surface must be generated types, not hand-built unions in frontend.
- `.trellis/spec/frontend/type-safety.md:39` says feature view models may be explicit conversions from generated DTOs. If frontend needs composer-friendly command rows, convert from snapshot DTO at one boundary.
- `.trellis/spec/frontend/type-safety.md:57` documents Session Runtime Projection DTO as session-panel surface authority. AgentRun target intentionally supersedes this for AgentRun workspace: session runtime can remain diagnostic/session route, but AgentRun workspace panel should consume snapshot `resource_surface`.
- `.trellis/spec/frontend/quality-guidelines.md:12` names `pnpm --filter app-web run check` as frontend quality gate.
- `.trellis/spec/guides/cross-layer-thinking-guide.md:23` calls out frontend state inference as a cross-layer error pattern; this is exactly the problem to eradicate here.

### Current Frontend File / Function / Test Index

| Area | Current code index | Current behavior / risk |
| --- | --- | --- |
| Page root | `AgentRunWorkspacePage` at `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:76` | Owns run/draft routing, fetches workspace state, constructs chat props and WorkspacePanel data. |
| Chat control derivation | `chatControlState` memo at `AgentRunWorkspacePage.tsx:335`; `deriveAgentRunWorkspaceChatControlState` at `AgentRunWorkspacePage.chatControlState.ts:37` | Converts draft/workspace status/action bits into `primaryAction/secondaryAction`; this is the main frontend command inference path to delete. |
| Draft submit | `handleAgentRunPrimaryAction` at `AgentRunWorkspacePage.tsx:360`; `createProjectAgentRun` call at `AgentRunWorkspacePage.tsx:401` | Only requires `executorConfig.executor`; provider/model may be omitted and store can return `null`, causing generic "创建 ProjectAgent AgentRun 失败。". |
| Runtime command submit | `handleAgentRunPrimaryAction` at `AgentRunWorkspacePage.tsx:360` | Branches on `send_next` / `steer` / `enqueue`; validates against primary/secondary local state rather than snapshot command token. |
| Steer stale token | `expected_turn_id: runtimeControl?.delivery_trace_meta?.last_turn_id` at `AgentRunWorkspacePage.tsx:462` | Uses trace last turn as expected turn, not a snapshot command precondition token. |
| Pending promote | `handlePromotePending` at `AgentRunWorkspacePage.tsx:521` | Gates promote with `secondaryAction?.enabled` and `pending_queue.paused`; should become per-message/per-command availability from snapshot. |
| Workspace panel data | `workspaceRuntimeData` at `AgentRunWorkspacePage.tsx:677`; `<WorkspacePanel>` at `AgentRunWorkspacePage.tsx:874` | Passes `runtimeSurface` from `useAgentRunWorkspaceState`; currently that surface comes from `session_runtime` resolver. |
| Chat view | `SessionChatView` at `packages/app-web/src/features/session/ui/SessionChatView.tsx:62` | Mixes stream display, executor local state, submit dispatch, keyboard mapping, pending UI and optimistic running. |
| Executor hydration | `useExecutorConfig` usage at `SessionChatView.tsx:163`; initial agent defaults at `SessionChatView.tsx:156` | Hydrates from `agentDefaults`, frame profile, hint and localStorage; not a backend resolved model fact. |
| Submit target | `handleSubmit` around `SessionChatView.tsx:395` | Determines target from `controlState.primaryAction.kind` or action override; accepts `steer`/`enqueue` override. |
| Keyboard mapping | `handleKeyDown` at `SessionChatView.tsx:486`; Ctrl/Cmd+Enter steer branch at `SessionChatView.tsx:500` | Special-cases `primaryAction.kind === "enqueue"` + secondary enabled to steer; must consume snapshot keyboard mapping only. |
| Pending rendering | `<PendingMessageList>` at `SessionChatView.tsx:654` | Renders when `pendingMessages.length > 0 || pendingQueue?.paused`; should use `pending.user_attention` / `visible_messages`. |
| Composer props | `SessionChatComposer` at `SessionChatViewParts.tsx:265`; `InlineModelSelector` at `SessionChatViewParts.tsx:458`; `ComposerSendButton` at `SessionChatViewParts.tsx:474` | Presentation is still shaped by `primaryAction`, `secondaryAction`, `isEnqueueMode`. |
| Send button | `ComposerSendButton` at `SessionChatView.tsx` imported and defined at `composer/ComposerSendButton.tsx:24` | Has local state machine keyed by `primaryKind === "enqueue"` and `canSteer`. |
| Submit disabled helper | `isSessionComposerPrimaryDisabled` at `SessionChatComposerState.ts:9` | Only sees `primaryActionEnabled`, prompt requirement, sending/cancelling. It does not know model_required or command-specific disabled reasons. |
| Model selector | `InlineModelSelector` at `InlineModelSelector.tsx:24` | Reads local `execConfig.modelId/providerId`, discovery options and `readonly`; no `model_required/resolved/source/validity` contract. |
| Executor local state | `useExecutorConfig` at `useExecutorConfig.ts:116` | Initial priority is `initialSource > localStorage > empty`; persists agent defaults to localStorage, which conflicts with snapshot-as-authority for AgentRun model config. |
| AgentRun resource state | `useAgentRunWorkspaceState` at `useAgentRunWorkspaceState.ts:113`; `resolveVfsSurface` call at `useAgentRunWorkspaceState.ts:138` | Fetches workspace, then separately resolves `source_type: "session_runtime"`; this is the session_runtime resource main path to remove for AgentRun. |
| Session runtime diagnostic state | `useSessionRuntimeState` at `useSessionRuntimeState.ts:66`; `fetchSessionRuntimeControl` and `resolveVfsSurface` at `useSessionRuntimeState.ts:96` | Can remain for session diagnostic/detail, but must not be used as AgentRun interactive control/resource source. |
| WorkspacePanel VFS consumption | `WorkspaceRuntimeData.runtimeSurface` in `workspace-runtime/model/types.ts:51`; `VfsTabContent` in `vfs-tab.tsx:43` | VFS tab already consumes one `runtimeSurface`; switch producer to snapshot `resource_surface` for AgentRun. |
| Project store launch | `launchProjectAgent` type at `projectStore.ts:66`, implementation at `projectStore.ts:369`; service at `services/project.ts:191` | Legacy `/launch` path remains visible to frontend developers. |
| Project store run start | `createProjectAgentRun` type at `projectStore.ts:67`, implementation at `projectStore.ts:386`; service at `services/project.ts:202` | Returns `ProjectAgentRunStartResult | null`; catch sets store error and returns null, hiding API error from caller. |
| Session runtime control service | `fetchSessionRuntimeControl` at `services/lifecycle.ts:57` | Should not be consumed by interactive AgentRun workspace after snapshot. |
| AgentRun command services | `sendAgentRunMessage` / `steerAgentRun` / `enqueueAgentRunPendingMessage` / `promoteAgentRunPendingMessage` / `resumeAgentRunPendingQueue` / `cancelAgentRun` in `services/lifecycle.ts:82-132` | Endpoints may stay, but caller should send/validate `ConversationCommandIntent` from snapshot rather than locally selected action kind. |
| Client command id helper | `resolveAgentRunClientCommandId` at `workspaceCommandState.ts:14` | Can remain if command intent payload still uses client id; command key should include snapshot command id/token rather than old action string only. |
| Frame executor helper | `executorSourceFromExecutionProfile` at `workspaceCommandState.ts:56` | Should stop being the UI model authority once snapshot exposes resolved/effective model config. |
| Generated action DTO | `AgentRunWorkspaceActionSetView` at `generated/workflow-contracts.ts:59`; `AgentRunWorkspaceView` at `generated/workflow-contracts.ts:71` | Current DTO lacks command list, keyboard mapping, model_config and resource_surface. |
| Generated pending DTO | `PendingQueueStateView` at `generated/workflow-contracts.ts:135` | Current DTO only has `paused/message/can_resume`; target needs `visible_messages/user_attention/resume_command`. |
| Generated launch DTO | `ProjectAgentLaunchResult` at `generated/project-agent-contracts.ts:29` | Must be removed/internalized so frontend cannot pick `/launch` as run start path. |

Current focused tests to migrate:

| Test path | Current assertions | Replacement direction |
| --- | --- | --- |
| `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:131` | `deriveAgentRunWorkspaceChatControlState` turns running action bits into enqueue + secondary steer. | Replace with snapshot adapter tests: command list + keyboard mapping render exact enabled commands; terminal/cancelling/read-only come from snapshot command availability, not local action bits. |
| `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:141` | Refreshing projection is read-only even with retained running actions. | Keep stale projection safety, but assert stale snapshot commands are not executable; retained snapshot is display/diagnostic only. |
| `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts:150` | Terminal projection with stale running bits hides steer/enqueue. | Replace with no command list / readonly terminal snapshot test; no stale `actions` should exist in DTO after cleanup. |
| `packages/app-web/src/features/session/ui/SessionChatView.test.tsx:180` | `isSessionComposerPrimaryDisabled` only checks primary enabled/input/sending/cancelling. | Add model_required and command availability disabled tests; helper should consume current command availability/requires_input/model policy. |
| `packages/app-web/src/features/session/ui/composer/PendingMessageRow.test.tsx:33` | Pending list shows messages; promote depends on `canPromote`. | Replace `canPromote` boolean with message/command availability from `pending.visible_messages[*].commands` or snapshot command list. |
| `packages/app-web/src/features/session/ui/composer/PendingMessageRow.test.tsx:48` | Paused queue always shows banner and resume. | Add `user_attention=false + visible_messages=[]` no-render test; `user_attention=true` renders banner/rows/resume command. |
| `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.test.ts:94` | Refresh retains `runtime_surface` from `session_runtime`. | Replace with snapshot `resource_surface` retention; no `resolveVfsSurface(session_runtime)` mocking in AgentRun hook tests. |
| `packages/app-web/src/features/workspace-panel/ContextOverviewTab.projection.test.tsx` | Runtime surface display including lifecycle mounts. | Keep as presentation coverage; feed it snapshot resource surface for AgentRun and session runtime surface only in session diagnostic route. |

### Snapshot Command List Migration Boundary

Target generated fields needed by frontend:

```ts
type AgentConversationSnapshot = {
  identity: ...;
  lifecycle_context: ...;
  execution: {
    state: "draft" | "model_required" | "ready" | "starting_claimed" | "running_active" | "cancelling" | "terminal" | ...;
    active_turn_id?: string;
  };
  model_config: ConversationModelConfigView;
  commands: {
    items: ConversationCommandView[];
    keyboard: {
      enter?: ConversationCommandShortcutTarget;
      ctrl_enter?: ConversationCommandShortcutTarget;
      meta_enter?: ConversationCommandShortcutTarget;
    };
  };
  pending: ConversationPendingQueueView;
  resource_surface?: ResolvedVfsSurface;
  diagnostics: ConversationDiagnosticView[];
};
```

The exact Rust names can differ, but these semantics must be generated from contracts.

Component migration map:

| Current component / function | Old input | New input | Migration action |
| --- | --- | --- | --- |
| `AgentRunWorkspacePage.chatControlState.ts` | `isProjectAgentDraft`, route ids, projection status, `workspace.actions/control_plane` | `AgentConversationSnapshot.commands`, `execution`, `model_config` | Delete file or reduce to a pure `snapshot -> composer view model` adapter with no business inference. It may format labels/icons, but must not decide command availability. |
| `SessionChatViewTypes.ts` | `SessionChatPrimaryActionKind`, `SessionChatControlState.primaryAction/secondaryAction/cancelAction` | `ConversationCommandView[]`, current primary command id, keyboard mapping, cancel command availability | Replace action-kind union with generated `ConversationCommandKind` and command availability view. No `secondaryAction`. |
| `AgentRunWorkspacePage.tsx:360` | `action: SessionChatPrimaryActionKind` | `intent: ConversationCommandIntent` or `command: ConversationCommandView` chosen from snapshot | Page should dispatch by snapshot command kind/id/precondition token. It should reject if command is absent/stale, not recompute primary/secondary match. |
| `SessionChatView.tsx:395` | target action from `controlState.primaryAction.kind` or override | target command from explicit button/keyboard mapping | `handleSubmit` receives a concrete command target from snapshot. No `actionOverride?: "steer" | "enqueue"`; no local `isValidPrimary/isValidSecondary`. |
| `SessionChatView.tsx:486` | Keydown branches on `primaryAction.kind === "enqueue"` | Keydown reads `commands.keyboard.enter` / `ctrl_enter` / `meta_enter` | Keyboard handler only maps key chord to snapshot command target, then calls submit if target is enabled and input/model policy is satisfied. |
| `SessionChatViewParts.tsx:321` | `isEnqueueMode = primaryAction.kind === "enqueue"` | command placements / command kind from snapshot for visual mode only | Visual treatment can use command placement/kind after command selection, but must not change business target. |
| `ComposerSendButton.tsx:36` | `primaryKind`, `canSteer`, `isRunning` | `primaryCommand`, optional `alternateCommand`, `cancelCommand`, keyboard labels from snapshot | Render buttons from command views. For running with steer support, show steer only if snapshot exposes a steer command placement; for enqueue-only no hidden steer. |
| `PendingMessageRow.tsx:23` | `messages`, `queue`, `canPromote` | `pending.visible_messages`, message command availability, `pending.user_attention`, `pending.resume_command` | Row/list should render only when snapshot says user attention or visible messages exist. Promote/resume/delete buttons come from command availability. |
| `InlineModelSelector.tsx:24` | local `execConfig` + discovered options | snapshot `model_config` + generated/discovered choices + explicit override callback | Selector displays resolved/model_required/source/validity; edits emit explicit override to page/snapshot command input. |
| `useAgentRunWorkspaceState.ts:113` | workspace fetch + `session_runtime` VFS resolve | workspace/snapshot fetch only | Store/hook sets `runtime_surface` from `snapshot.resource_surface`; no separate resource fetch in AgentRun path. |
| `WorkspaceRuntimeData.runtimeSurface` | `agentRunWorkspaceState.runtime_surface` from session runtime resolver | `snapshot.resource_surface` | Keep WorkspacePanel API stable if desired, but producer must be snapshot resource surface. |
| `projectStore.createProjectAgentRun` | `Promise<ProjectAgentRunStartResult | null>` | `Promise<ProjectAgentRunStartResult>` or typed command response/errors | Stop catching command API failures into `null`; callers catch real API error. |

### Enter / Ctrl+Enter Snapshot Keyboard Mapping

Current code:

- `SessionChatView.tsx:486` owns keyboard mapping.
- `SessionChatView.tsx:500` maps Ctrl/Cmd+Enter to `steer` only when `primaryAction.kind === "enqueue"` and `secondaryAction?.enabled`.
- `SessionChatView.tsx:508` maps all other Enter/Ctrl+Enter to the primary action.

Target rules:

1. File picker keys remain local UI behavior: ArrowUp/ArrowDown/Escape and Enter while picker is open can keep current local handling.
2. Once a key chord is a composer submit chord, frontend must not inspect execution state, `primaryAction.kind`, `isRunning`, `isEnqueueMode`, `delivery_trace_meta`, or pending queue.
3. Resolve the command target only from snapshot keyboard mapping:
   - plain Enter -> `snapshot.commands.keyboard.enter`
   - Ctrl+Enter -> `snapshot.commands.keyboard.ctrl_enter`
   - Meta/Cmd+Enter -> `snapshot.commands.keyboard.meta_enter ?? snapshot.commands.keyboard.ctrl_enter`
4. If the mapping is absent, disabled, stale, or model policy is not satisfied, consume the key only if the UI needs to suppress newline; show command unavailable reason/model_required reason from snapshot.
5. `steer` and `promote_pending_to_steer` must carry command precondition token from snapshot, not `delivery_trace_meta.last_turn_id`.
6. Ready/idle/completed states never become steer by key choice. If snapshot maps Ctrl+Enter to `send_next`, submit `send_next`; if no mapping, do nothing.

Practical component shape:

```ts
function resolveKeyboardCommand(snapshotCommands, event) {
  if (event.key !== "Enter" || event.shiftKey) return null;
  if (event.metaKey) return snapshotCommands.keyboard.meta_enter ?? snapshotCommands.keyboard.ctrl_enter ?? null;
  if (event.ctrlKey) return snapshotCommands.keyboard.ctrl_enter ?? null;
  return snapshotCommands.keyboard.enter ?? null;
}
```

This helper may live near composer view-model code, but it should be a direct selector over generated snapshot fields.

### Model Required / Resolved Model UI Contract

Current code:

- `AgentRunWorkspacePage.tsx:360` throws only when `executorConfig.executor` is empty; provider/model can be absent.
- `SessionChatView.tsx:156` takes `agentDefaults` once as `useExecutorConfig` initial source.
- `SessionChatView.tsx:163` creates local `execConfig`.
- `useExecutorConfig.ts:116` initializes from `initialSource > localStorage > empty`.
- `InlineModelSelector.tsx:24` displays local model/provider/discovery state and does not know `model_required`.

Target projection:

```ts
type ConversationModelConfigView =
  | {
      state: "resolved";
      effective_executor_config: {
        executor: string;
        provider_id: string | null;
        model_id: string | null;
        thinking_level?: string | null;
        permission_policy?: string | null;
      };
      source: "project_agent_preset" | "frame_profile" | "user_override" | "discovery_default" | ...;
      validity: { valid: true };
    }
  | {
      state: "model_required";
      partial_executor_config: ...;
      missing: Array<"executor" | "provider_id" | "model_id">;
      message: string;
      selector_options?: ...;
    };
```

Frontend behavior:

- `InlineModelSelector` chip label is driven by snapshot `model_config`: resolved model name/source for resolved state; clear required/invalid state for `model_required`.
- `InlineModelSelector` edits produce an explicit user override patch. That patch is command input / draft override state; it is not treated as ProjectAgent default until backend resolves it into the next snapshot.
- `useExecutorConfig` can be retained only as a small "explicit override form state" helper or replaced with snapshot-aware `useConversationModelOverride`; localStorage/recent entries may seed convenience suggestions but must not decide effective model.
- Submit disabled uses command availability + model policy:
  - In `model_required`, `start_draft` / `send_next` / `enqueue` commands should be disabled or have `executor_config_policy` requiring a complete override.
  - `ComposerSendButton` disabled reason should show snapshot command/model reason, not a generic "请选择模型配置后再发送。".
  - `SessionChatComposerState.ts:9` should include model policy or be replaced by command availability selector.
- Draft start should send full resolved executor/provider/model when state is `resolved`, or refuse locally because snapshot command is disabled when state is `model_required`.
- `ProjectAgentSummary`/draft page should stop relying on `ProjectAgentSummary.executor` as the effective model display once backend exposes `effective_executor_config`.

### Pending UI Contract

Current code:

- `SessionChatView.tsx:654` renders pending list when `pendingMessages.length > 0 || pendingQueue?.paused`.
- `PendingMessageRow.tsx:31` returns null only when no messages and not paused.
- `PendingMessageRow.tsx:34` always shows "Pending 队列已暂停" when `queue.paused`.
- `AgentRunWorkspacePage.tsx:521` enables promote based on `secondaryAction?.enabled` and not paused.
- `generated/workflow-contracts.ts:135` has only `paused`, `pause_reason`, `message`, `can_resume`.

Target projection:

```ts
type ConversationPendingQueueView = {
  paused: boolean;
  user_attention: boolean;
  message?: string;
  visible_messages: PendingMessageView[];
  resume_command?: ConversationCommandAvailabilityView;
};
```

Migration:

- `SessionChatView` passes a single `pending` snapshot object to `PendingMessageList`.
- `PendingMessageList` renders nothing when `!pending.user_attention && pending.visible_messages.length === 0`.
- The paused banner renders only when `pending.user_attention` and snapshot message/resume command say so. A terminal/ready cleanup pause with no visible work should not render.
- Promote/delete/resume controls come from command availability:
  - If promote is per message, use `message.commands.promote_pending` or `message.promote_command`.
  - If promote is global, use snapshot command list filtered by message id precondition.
- Remove `canPromote={Boolean(controlState.secondaryAction?.enabled) && !pendingQueue?.paused}`. Steer availability for active turn is not a proxy for whether a specific pending message may be promoted.

### Workspace Panel / Resource Browser Contract

Current code:

- `useAgentRunWorkspaceState.ts:134` fetches `AgentRunWorkspaceView`.
- `useAgentRunWorkspaceState.ts:138` resolves `resolveVfsSurface({ source_type: "session_runtime", session_id: runtimeSessionId })`.
- `AgentRunWorkspacePage.tsx:677` passes `deliveryRuntimeSurface` into `WorkspaceRuntimeData.runtimeSurface`.
- `WorkspaceRuntimeData.runtimeSurface` is the WorkspacePanel/VFS input at `workspace-runtime/model/types.ts:51`.
- `vfs-tab.tsx:43` consumes `useWorkspaceData().runtimeSurface`; it is already correctly isolated from how the surface is produced.
- `useSessionRuntimeState.ts:96` also fetches `SessionRuntimeControlView` plus `session_runtime` VFS; this should remain session diagnostic/detail only.

Target migration:

- Extend generated `AgentConversationSnapshot` or upgraded `AgentRunWorkspaceView` with `resource_surface: ResolvedVfsSurface | null` and optional diagnostics.
- In `useAgentRunWorkspaceState`, remove `resolveVfsSurface` import and Promise.allSettled block. Set `runtime_surface` from `workspace.resource_surface` / `snapshot.resource_surface`.
- Rename frontend state if possible:
  - `runtime_surface` in AgentRun hook may become `resource_surface` to avoid implying session runtime ownership.
  - `WorkspaceRuntimeData.runtimeSurface` can remain temporarily as UI panel prop if changing panel naming is too broad, but construction site must clearly pass snapshot resource surface.
- Keep `useSessionRuntimeState` and `/sessions/{id}/runtime-control` only for runtime trace/detail pages. Audit imports so AgentRun route no longer consumes it for interaction/resource decisions.
- VFS tab and ContextOverviewTab need minimal changes if their input remains `ResolvedVfsSurface`; the important change is producer ownership.

### Command Store Error Propagation

Current code:

- `projectStore.ts:66` exposes `launchProjectAgent(...): Promise<ProjectAgentLaunchResult | null>`.
- `projectStore.ts:67` exposes `createProjectAgentRun(...): Promise<ProjectAgentRunStartResult | null>`.
- `projectStore.ts:369` catches launch errors, sets `error`, returns `null`.
- `projectStore.ts:386` catches run start errors, sets `error`, returns `null`.
- `AgentRunWorkspacePage.tsx:401` calls `createProjectAgentRun`; if the response is null, it throws generic "创建 ProjectAgent AgentRun 失败。".

Target migration:

- Command-like store methods should return concrete response or throw. For this task, at minimum `createProjectAgentRun` must become `Promise<ProjectAgentRunStartResult>` and let `projectService.createProjectAgentRun` errors propagate.
- Page-level catch in `SessionChatView.handleSubmit` already displays `e.message`; preserving backend API error there will fix the misleading generic failure.
- Store may still set `error` for global display, but it must rethrow:

```ts
try {
  ...
  return result;
} catch (e) {
  set({ error: extractMessage(e) });
  throw e;
}
```

- `launchProjectAgent` should be removed/internalized with the legacy `/launch` cleanup. If kept temporarily for backend transition, it must not be exposed in the store as a product command.

### Must-Kill Misleading Paths

These paths should fail grep/audit after the frontend migration, except where explicitly scoped as diagnostics.

| Old path | Current locations | Required outcome |
| --- | --- | --- |
| `launchProjectAgent` | `projectStore.ts:66`, `projectStore.ts:369`, `services/project.ts:191` | Delete from frontend store/service unless backend keeps an internal-only materialization helper not reachable as product launch. |
| `ProjectAgentLaunchResult` | `generated/project-agent-contracts.ts:29`, `types/index.ts:275`, `projectStore.ts:6`, `services/project.ts` mapper | Remove from generated public frontend contracts or stop exporting/using it. Regenerate contracts after backend deletion/internalization. |
| ProjectAgent `/launch` | `services/project.ts:195`; backend `crates/agentdash-api/src/routes/project_agents.rs:128`; contract `crates/agentdash-contracts/src/project_agent.rs:55` | Delete route or rename/internalize so it cannot be mistaken for AgentRun start. |
| `SessionRuntimeControlView` interactive consumption | `generated/workflow-contracts.ts:167`, `types/lifecycle-views.ts:26`, `services/lifecycle.ts:57`, `useSessionRuntimeState.ts:30` | Allowed only for runtime diagnostic/session detail. AgentRun page/composer/workspace panel must not consume it for commands/resources. |
| `SessionRuntimeActionSetView` | `generated/workflow-contracts.ts:161`, `crates/agentdash-contracts/src/workflow.rs:1144` | Remove from interactive frontend command model; if backend keeps it, rename/scope to diagnostic. |
| `primaryAction/secondaryAction` | `SessionChatViewTypes.ts:35-38`, `AgentRunWorkspacePage.chatControlState.ts`, `SessionChatView.tsx`, `SessionChatViewParts.tsx`, `ComposerSendButton.tsx`, tests | Delete as business command semantics. Any replacement view model must be derived directly from snapshot command list without command inference. |
| `SessionRuntimeControlView` route as main path | `crates/agentdash-api/src/routes/sessions.rs:152`, `services/lifecycle.ts:57`, `useSessionRuntimeState.ts:66` | Keep only trace/detail/diagnostic if needed. No AgentRun interactive path should import/use it. |
| `session_runtime` surface main path | `useAgentRunWorkspaceState.ts:138`; tests at `useAgentRunWorkspaceState.test.ts:59`; `useSessionRuntimeState.ts:97` | Remove from AgentRun hook/page. Keep only session diagnostic hook if still needed. |
| Store `Promise<T | null>` for commands | `projectStore.ts:66-71`, `projectStore.ts:369-399`; broader store grep will find many non-command CRUD methods | For command APIs in this task, return concrete response or throw. Do not use `return null` to represent command failure. |
| `primaryAction.kind === "enqueue"` keyboard branch | `SessionChatView.tsx:500`, `SessionChatViewParts.tsx:321`, `ComposerSendButton.tsx:37` | Delete; keyboard/button behavior comes from snapshot command placement and shortcut mapping. |
| `delivery_trace_meta.last_turn_id` as steer token | `AgentRunWorkspacePage.tsx:462` | Replace with snapshot command precondition token/active turn guard. |

### Recommended Tests

Focused frontend tests:

1. `AgentRunWorkspacePage` / snapshot adapter:
   - draft `model_required` snapshot disables `start_draft`, shows selector required reason.
   - draft resolved model snapshot submits `start_draft` with full executor/provider/model from snapshot or explicit override.
   - ready snapshot maps Enter and Ctrl/Cmd+Enter to `send_next`, never `steer`.
   - running active snapshot with `enqueue` + `steer` commands maps Enter to `enqueue` and Ctrl/Cmd+Enter to `steer`.
   - running active snapshot without steer maps Ctrl/Cmd+Enter to snapshot fallback (`enqueue` or none), not hidden steer.
   - starting/claimed snapshot has no steer/promote; composer is disabled or readonly per command list.
   - terminal snapshot remains readonly even if retained old workspace/action state exists.
2. `SessionChatView` / keyboard helper:
   - file picker Enter still confirms selection and does not submit.
   - Shift+Enter does not submit.
   - submit chord calls exactly the command referenced by snapshot keyboard mapping.
   - absent/disabled keyboard command does not call command API and surfaces unavailable reason.
3. `InlineModelSelector`:
   - resolved state displays backend effective model/source.
   - model_required state displays required/invalid state and submit disabled reason.
   - selecting provider/model emits explicit override patch without mutating generated snapshot object.
   - localStorage recent entry does not override snapshot resolved model.
4. `PendingMessageList`:
   - `user_attention=false`, `visible_messages=[]`, `paused=true` renders nothing.
   - `user_attention=true`, `visible_messages=[]`, resume command enabled renders banner/resume only.
   - visible messages render rows; promote/delete buttons follow per-message command availability.
   - paused queue with visible messages does not infer promote from global steer availability.
5. `useAgentRunWorkspaceState`:
   - uses `workspace.resource_surface` / `snapshot.resource_surface` directly.
   - does not call `resolveVfsSurface` for AgentRun workspace.
   - refresh retains previous snapshot resource surface only as stale display; commands remain non-executable while refreshing.
6. `WorkspacePanel` / VFS:
   - AgentRun explicit lifecycle snapshot resource surface shows lifecycle mount in VFS tab.
   - ProjectAgent graphless run shows owner surface mounts.
   - resource diagnostics display when snapshot resource surface is absent/invalid.
7. `projectStore`:
   - `createProjectAgentRun` propagates API error message; page displays backend error instead of generic null failure.
   - no `launchProjectAgent` store action remains after cleanup.

Suggested commands:

```powershell
pnpm --filter app-web run test -- AgentRunWorkspacePage
pnpm --filter app-web run test -- SessionChatView
pnpm --filter app-web run test -- PendingMessageRow
pnpm --filter app-web run test -- useAgentRunWorkspaceState
pnpm --filter app-web run typecheck
pnpm run contracts:check
```

### Grep Gate

Run after contracts are regenerated and frontend migration is complete:

```powershell
rg -n "primaryAction|secondaryAction|SessionChatControlState" packages/app-web/src
rg -n "primaryAction|secondaryAction|SessionRuntimeActionSetView|SessionRuntimeControlView" packages/app-web/src crates/agentdash-contracts/src
rg -n "launchProjectAgent|ProjectAgentLaunchResult|/launch" packages/app-web/src crates/agentdash-api/src crates/agentdash-contracts/src
rg -n "resolveVfsSurface\\(\\{ source_type: \"session_runtime\"" packages/app-web/src/features/workspace-panel packages/app-web/src/pages
rg -n "delivery_trace_meta\\?\\.last_turn_id|last_turn_id" packages/app-web/src/pages/AgentRunWorkspacePage.tsx packages/app-web/src/features/session
rg -n "primaryAction\\.kind === \"enqueue\"|isEnqueueMode" packages/app-web/src/features/session packages/app-web/src/pages
rg -n "createProjectAgentRun:.*Promise<.*null|launchProjectAgent:.*Promise<.*null|return null" packages/app-web/src/stores/projectStore.ts
rg -n "model_required|resource_surface|user_attention|visible_messages|keyboard" packages/app-web/src/generated packages/app-web/src/features packages/app-web/src/pages
```

Expected gate results:

- First six old-path greps should return no interactive frontend hits.
- `SessionRuntimeControlView` may remain only in session diagnostic/detail files if backend keeps diagnostic DTOs; no `AgentRunWorkspacePage`, composer, workspace panel AgentRun hook, or command store hit.
- `return null` grep is broad in stores; for this task, command store methods (`createProjectAgentRun` and any retained command API) must not return null on API failure.
- The final positive grep should show generated snapshot fields and frontend consumption.

### External References

No external documentation was needed. This research is based on task artifacts, Trellis specs and local code inspection.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned `Current task: (none)` even though the dispatch prompt supplied `.trellis/tasks/06-12-agent-run-lifecycle-convergence`. I used the explicit user-provided task path to avoid guessing.
- This research did not edit generated contracts or business code. Some target names (`AgentConversationSnapshot`, `ConversationCommandView`, `resource_surface`) may shift during backend contract implementation; frontend migration should follow generated DTO names while preserving the ownership boundaries above.
- `.trellis/spec/frontend/state-management.md` still documents the old `AgentRunWorkspaceView.actions -> SessionChatControlState` flow. That spec should be updated after implementation via the Trellis spec-update phase; this research file does not modify specs.
- I did not run frontend tests because the assignment was research-only and no code was changed.
