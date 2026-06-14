# Research: frontend-route-state

- Query: 前端 SessionPage 到 AgentRunWorkspacePage 的 route/state/model selector 迁移路径。
- Scope: mixed
- Date: 2026-06-11

## Findings

### Current Task Context

- Parent task `06-11-session-model-delivery-state-chain` 已确认彻底迁移：交互工作台 canonical route 使用 `/agent-runs/:runId/:agentId`，Project Agent draft 使用 `/agent-runs/new`，`RuntimeSession` 只作为 delivery trace / 投递适配身份。
- Child task `06-11-agentrun-workspace-frontend-route-state` 依赖 `06-11-agentrun-workspace-api-contract` 的 generated TypeScript contracts，且依赖 command receipt 的 `client_command_id` 语义。
- API child 的目标合同是 `AgentRunWorkspaceView`、`GET /agent-runs/{run_id}/{agent_id}/workspace`、`POST /agent-runs/{run_id}/{agent_id}/messages`、`POST /projects/{project_id}/agents/{project_agent_id}/agent-runs`。

### Files Found

- `packages/app-web/src/App.tsx` - 顶层 React Router 配置，目前注册 `/session/new`、`/session/:sessionId`、`/run/:runId`、`/subject/:kind/:id`、`/agent/:agentId`。
- `packages/app-web/src/pages/SessionPage.tsx` - 当前交互工作台主体，合并 draft、runtime control、chat、workspace panel、owner navigation 和 model defaults。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx` - 聊天流、composer、executor selector hydration、session feed 和 submit/cancel 行为。
- `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts` - executor/provider/model/thinking/permission 的 localStorage 持久化和 hydrate hook。
- `packages/app-web/src/features/executor-selector/model/types.ts` - executor selector source/result 类型。
- `packages/app-web/src/features/session/ui/SessionChatViewModel.ts` - `ProjectAgentExecutor | TaskSessionExecutorSummary` 到 executor selector source 的转换。
- `packages/app-web/src/services/lifecycle.ts` - lifecycle query + session-based command endpoint service。
- `packages/app-web/src/services/project.ts` - ProjectAgent summary/launch/session start service，当前 ProjectAgent session start 仍走 `/sessions` 语义。
- `packages/app-web/src/stores/lifecycleStore.ts` - run/agent/frame/runtime trace normalized store。
- `packages/app-web/src/stores/projectStore.ts` - ProjectAgent summary 和 `createProjectAgentRuntimeSession` store action。
- `packages/app-web/src/pages/LifecyclePages.tsx` - run/subject/agent inspector 页面，当前依赖 `/agent/:agentId` + route state，并把 runtime trace 导回 `/session/:id`。
- `packages/app-web/src/features/agent/agent-tab-view.tsx` - ProjectAgent draft 入口和 active session list 容器。
- `packages/app-web/src/features/agent/project-agent-paths.ts` - draft route helper，目前返回 `/session/new?...`。
- `packages/app-web/src/features/agent/active-session-list.tsx` - active list，目前以 `ProjectSessionListEntry.runtime_session_id` 作为行 key / open 参数。
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts` - session runtime-control + VFS surface loader，目前 source 为 `session_runtime`。
- `packages/app-web/src/features/workspace-runtime/model/types.ts` - `WorkspaceRuntimeData` 字段仍以 session/runtime trace 语义命名。
- `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx` - Workspace tab composition root，目前以 `sessionId` 初始化 tab store owner。
- `packages/app-web/src/generated/workflow-contracts.ts` - generated lifecycle/session contracts，已有 `AgentRunView`、`AgentFrameRuntimeView.execution_profile`、`ProjectSessionListEntry.run_ref/agent_ref/frame_ref`，但没有 `AgentRunWorkspaceView`。
- `packages/app-web/src/generated/project-agent-contracts.ts` - generated ProjectAgent contracts，当前 start result 返回 runtime/run/agent/frame refs。
- `packages/app-web/src/pages/SessionPage.workspaceModulePresentation.ts` - workspace module presentation event 到 tab open target 的 helper。
- Tests: `agent-tab-view.test.ts`、`useSessionRuntimeState.test.ts`、`lifecycle.test.ts`、`SessionPage.workspace-module.test.ts`、`SessionPage.hook-runtime.test.tsx`。

### Code Patterns

#### Route And Page Ownership

- `App.tsx` lazy-loads `SessionPage` and wraps `/session/:sessionId` / `/session/new` into `SessionPage` props at `packages/app-web/src/App.tsx:155` and route registration at `packages/app-web/src/App.tsx:346`-`packages/app-web/src/App.tsx:347`.
- Existing lifecycle inspector routes are separate: `/run/:runId`、`/subject/:kind/:id`、`/agent/:agentId` at `packages/app-web/src/App.tsx:348`-`packages/app-web/src/App.tsx:350`.
- `SessionPage` treats `location.state.trace_agent` as draft/runtime visual hint at `packages/app-web/src/pages/SessionPage.tsx:130`-`packages/app-web/src/pages/SessionPage.tsx:134`.
- Draft identity is query-only (`project_id` + `project_agent_id`) and converted to `draftProjectAgent` from `agentsByProjectId` at `packages/app-web/src/pages/SessionPage.tsx:136`-`packages/app-web/src/pages/SessionPage.tsx:143`.
- Runtime identity is currently `sessionId`: `useSessionRuntimeState({ sessionId: currentSessionId, sourceKey: session:${id} })` at `packages/app-web/src/pages/SessionPage.tsx:166`.
- `taskExecutorSummary` is hardcoded `null` in `SessionPage`, so formal runtime executor defaults do not come from current frame/profile today: `packages/app-web/src/pages/SessionPage.tsx:202`.
- Draft submit calls `createProjectAgentRuntimeSession` and navigates to `/session/${response.runtime_session_id}` at `packages/app-web/src/pages/SessionPage.tsx:469` and `packages/app-web/src/pages/SessionPage.tsx:477`.
- Runtime submit calls session-based command services: message at `packages/app-web/src/pages/SessionPage.tsx:501`, steering at `packages/app-web/src/pages/SessionPage.tsx:511`, enqueue at `packages/app-web/src/pages/SessionPage.tsx:519`.
- Current workspace panel data is assembled inside `SessionPage` from `runtimeControl.run/agent/frame_runtime` at `packages/app-web/src/pages/SessionPage.tsx:660`-`packages/app-web/src/pages/SessionPage.tsx:669`.
- `SessionChatView` receives `agentDefaults={draftProjectAgent?.executor ?? taskExecutorSummary}` at `packages/app-web/src/pages/SessionPage.tsx:837`, which explains why formal workspace can lose provider/model once draft-only defaults disappear.

#### Executor Selector Hydration

- `SessionChatView` computes `initialExecutorSource` from `agentDefaults` once and passes it to `useExecutorConfig` at `packages/app-web/src/features/session/ui/SessionChatView.tsx:152`-`packages/app-web/src/features/session/ui/SessionChatView.tsx:159`.
- Hydration key is `sessionId` for existing sessions and `draft:<executor/provider/model/thinking/permission>` for drafts at `packages/app-web/src/features/session/ui/SessionChatView.tsx:171`.
- `hydratedSessionRef` prevents rehydrating the same key after first application at `packages/app-web/src/features/session/ui/SessionChatView.tsx:189`-`packages/app-web/src/features/session/ui/SessionChatView.tsx:198`.
- The actual outbound `executorConfig` is built from `execConfig.executor/providerId/modelId/thinkingLevel/permissionPolicy` at `packages/app-web/src/features/session/ui/SessionChatView.tsx:227`-`packages/app-web/src/features/session/ui/SessionChatView.tsx:236`.
- `useExecutorConfig` persists a single global key `agentdash:executor-config-v2` and recent list `agentdash:recent-executors` at `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:10`-`packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:11`.
- `useExecutorConfig` documents initial priority as `initialSource > localStorage > empty` at `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:110`.
- Initial state reads non-empty source fields, then falls back field-by-field to localStorage at `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:116`-`packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:134`.
- `hydrate` only writes non-empty normalized fields and persists them; it does not clear absent fields at `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:258`-`packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:270`.
- `setExecutor` clears provider/model/thinking/permission as a side effect at `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:175`-`packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:190`; `hydrate` intentionally bypasses that side effect at `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts:263`.
- `toExecutorConfigSource` supports `executor/provider_id/model_id/thinking_level/permission_policy` from `ProjectAgentExecutor | TaskSessionExecutorSummary` at `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:9`-`packages/app-web/src/features/session/ui/SessionChatViewModel.ts:18`.

#### Lifecycle / Agent / Run Navigation

- `LifecyclePages.tsx` is an inspector-style page set, not the current full chat workspace: `LifecycleRunPage` at `packages/app-web/src/pages/LifecyclePages.tsx:283`, `SubjectExecutionPage` at `packages/app-web/src/pages/LifecyclePages.tsx:305`, `LifecycleAgentPage` at `packages/app-web/src/pages/LifecyclePages.tsx:330`.
- `LifecycleAgentPage` gets `run_id` and `frame_id` from route state, not from URL, at `packages/app-web/src/pages/LifecyclePages.tsx:326`-`packages/app-web/src/pages/LifecyclePages.tsx:337`.
- It fetches run only when `routeState.run_id` exists and fetches frame by `frameId` at `packages/app-web/src/pages/LifecyclePages.tsx:342`-`packages/app-web/src/pages/LifecyclePages.tsx:347`.
- Run and frame runtime trace buttons navigate back to `/session/${runtime_session_id}` at `packages/app-web/src/pages/LifecyclePages.tsx:153`-`packages/app-web/src/pages/LifecyclePages.tsx:157` and `packages/app-web/src/pages/LifecyclePages.tsx:262`-`packages/app-web/src/pages/LifecyclePages.tsx:264`.
- Story/Task subject panels also navigate current agent via `/agent/${agent_id}` with route state and trace via `/session/${runtime_session_id}` (`packages/app-web/src/features/story/story-subject-execution-panel.tsx:52` and `packages/app-web/src/features/task/task-subject-execution-panel.tsx:50`; trace links at `story-subject-execution-panel.tsx:113`, `task-subject-execution-panel.tsx:99`).

#### Workspace / List / Navigation

- ProjectAgent draft path helper currently returns `/session/new?project_id=...&project_agent_id=...` at `packages/app-web/src/features/agent/project-agent-paths.ts:1`-`packages/app-web/src/features/agent/project-agent-paths.ts:6`; its test locks that expectation at `packages/app-web/src/features/agent/agent-tab-view.test.ts:7`-`packages/app-web/src/features/agent/agent-tab-view.test.ts:14`.
- `AgentTabView` navigates draft launch through that helper at `packages/app-web/src/features/agent/agent-tab-view.tsx:57`.
- `AgentTabView.handleOpenSession` still receives `runtimeSessionId` and navigates `/session/${runtimeSessionId}` at `packages/app-web/src/features/agent/agent-tab-view.tsx:75`.
- `ActiveSessionList` fetches `ProjectSessionListView` via `fetchProjectSessionList(projectId)` at `packages/app-web/src/features/agent/active-session-list.tsx:197`.
- `ActiveSessionListProps.onOpenSession` is typed `(runtimeSessionId, agentId?)` at `packages/app-web/src/features/agent/active-session-list.tsx:173`-`packages/app-web/src/features/agent/active-session-list.tsx:177`.
- Rows key and open arguments use `entry.runtime_session_id` and only pass optional `entry.agent_ref?.agent_id`, despite `ProjectSessionListEntry` already carrying optional `run_ref/agent_ref/frame_ref`: `packages/app-web/src/features/agent/active-session-list.tsx:296`-`packages/app-web/src/features/agent/active-session-list.tsx:301`.
- `WorkspaceRuntimeData` still has `sessionId` and `runtimeSessionId` as first-class fields at `packages/app-web/src/features/workspace-runtime/model/types.ts:34`-`packages/app-web/src/features/workspace-runtime/model/types.ts:37`.
- `WorkspacePanel` initializes tab store by `sessionId`, not AgentRun workspace key: destructuring at `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:29`, store `sessionId` comparison and initialize at `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:33` and `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:52`-`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:53`.
- `WorkspacePanel` renderContent still passes `sessionId` into tab descriptors at `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:123`-`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:126`.
- `useSessionRuntimeState` currently loads `fetchSessionRuntimeControl(sid)` and `resolveVfsSurface({ source_type: "session_runtime", session_id: sid })` at `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:94`-`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:97`.

#### Generated Contracts And API Consumption

- `AgentFrameRuntimeView` already exposes `execution_profile?: JsonValue` and `runtime_session_refs` at `packages/app-web/src/generated/workflow-contracts.ts:27`.
- `AgentRunView` already exposes `agent_ref`, `project_id`, `project_agent_id?`, `current_frame_id?`, and `delivery_runtime_ref?` at `packages/app-web/src/generated/workflow-contracts.ts:47`-`packages/app-web/src/generated/workflow-contracts.ts:51`.
- `ProjectSessionListEntry` already exposes `runtime_session_id`, `run_ref?`, `agent_ref?`, and `frame_ref?` at `packages/app-web/src/generated/workflow-contracts.ts:123`.
- Existing `SessionRuntimeControlView` wraps runtime session meta, control plane, run, agent, frame runtime, subject associations, actions, and pending messages at `packages/app-web/src/generated/workflow-contracts.ts:147`.
- Existing service `fetchSessionRuntimeControl` still calls `/sessions/{runtimeSessionId}/runtime-control` at `packages/app-web/src/services/lifecycle.ts:58`-`packages/app-web/src/services/lifecycle.ts:60`.
- Existing command services still call `/sessions/{runtimeSessionId}/messages` and `/sessions/{runtimeSessionId}/steering` at `packages/app-web/src/services/lifecycle.ts:76`-`packages/app-web/src/services/lifecycle.ts:90`.
- Existing ProjectAgent materialization service still calls `/projects/{projectId}/agents/{agentKey}/sessions` at `packages/app-web/src/services/project.ts:206`-`packages/app-web/src/services/project.ts:208`.
- `ProjectAgentLaunchResult` currently has `delivery_runtime_ref`, but `ProjectAgentSessionStartResult` returns `runtime_session_id` directly plus run/agent/frame refs at `packages/app-web/src/generated/project-agent-contracts.ts:25`-`packages/app-web/src/generated/project-agent-contracts.ts:27`.
- `rg -n "client_command_id" packages/app-web/src crates` found no frontend/source contract hit; only parent/child planning docs mention it. Generated request DTOs therefore cannot satisfy retry/idempotency acceptance yet.

### Migration Path

Recommended implementation entry, once API contract child has generated `AgentRunWorkspaceView`:

1. Add route helpers first:
   - Replace `projectAgentDraftSessionPath` with `projectAgentDraftRunPath(projectId, agentKey) -> /agent-runs/new?...`.
   - Add `agentRunWorkspacePath(runId, agentId) -> /agent-runs/:runId/:agentId`.
   - Keep helpers centralized under `features/agent` or a new `features/agent-run-workspace/model/paths.ts` so `App`, lists, subject panels, and tests share one encoding rule.

2. Add new page/hook instead of mutating `SessionPage` in place:
   - `pages/AgentRunWorkspacePage.tsx` or `features/agent-run-workspace/ui/AgentRunWorkspacePage.tsx`.
   - `useAgentRunWorkspaceState({ runId, agentId, sourceKey })`, modeled after `useSessionRuntimeState` but backed by `GET /agent-runs/{run_id}/{agent_id}/workspace`.
   - Workspace key should follow parent design: `agentrun:${runId}:${agentId}:${frameId ?? "no-frame"}`. Draft key: `draft:${projectId}:${projectAgentId}`.
   - Ingest returned `run/agent/frame` into `lifecycleStore` similarly to `useSessionRuntimeState`.

3. Reuse `SessionChatView` only as a chat view, but change its props/keying semantics:
   - Keep `sessionId` as delivery runtime id for stream subscription until session stream is also renamed.
   - Add a distinct `workspaceKey` or `executorHydrationKey` prop so executor hydration keys on AgentRun/frame, not `sessionId`.
   - Feed `agentDefaults` from `draft ProjectAgent executor` or mapped `frame_runtime.execution_profile`, not from `taskExecutorSummary`.
   - Consider renaming UI types later (`SessionChatControlState`) only if scope allows; behavior can migrate before cosmetic naming.

4. Add typed mapper for `frame_runtime.execution_profile`:
   - Input is generated `JsonValue`, so parse as `AgentConfig`-like snake_case object.
   - Output should be `ExecutorConfigSource`.
   - Required fields: `executor`, `provider_id`, `model_id`, `thinking_level`, `permission_policy`.
   - Do not let global localStorage override non-empty workspace execution profile. The current `useExecutorConfig` can do this if the source is complete and hydration key changes; if profile has intentionally empty fields, `hydrate` currently cannot clear stale localStorage fields and should be adjusted or wrapped for authoritative workspace source.

5. Migrate service layer:
   - Add `fetchAgentRunWorkspace(runId, agentId)`.
   - Add `sendAgentRunMessage(runId, agentId, request)`, `steerAgentRun(...)`, `enqueuePendingMessage(...)`, `delete/promote`, and cancel endpoint functions under AgentRun paths.
   - Add ProjectAgent materialization service for `POST /projects/{project_id}/agents/{project_agent_id}/agent-runs`.
   - Include `client_command_id` only after generated DTOs expose it.

6. Update navigation callers:
   - `AgentTabView` draft launch: `/agent-runs/new`.
   - `ActiveSessionList` row click: prefer `entry.run_ref?.run_id + entry.agent_ref?.agent_id`; if absent, row cannot open interactive workspace and should surface trace-only or disabled state.
   - `LifecyclePages`, Story/Task subject panels: agent buttons should navigate to `/agent-runs/{runId}/{agentId}` when opening interactive workspace; keep `/agent/:agentId` only as lightweight inspector if desired.
   - Runtime trace links should be renamed to `/runtime-sessions/:runtimeSessionId/trace` if trace page remains.

7. Update WorkspacePanel ownership:
   - Add `workspaceKey` / `ownerKey` to `WorkspaceRuntimeData`.
   - Use that key for `workspaceTabStore.initialize`, not `sessionId`.
   - Keep `runtimeSessionId` as delivery/trace metadata for tabs that need stream/session APIs.

8. Remove old interactive `/session` routes last:
   - After AgentRun workspace route and navigation callers pass focused tests, remove `/session/new` and `/session/:sessionId` from `App.tsx`.
   - Delete or rename `SessionPage` tests/helpers once migrated.

### Risk Files

- `packages/app-web/src/pages/SessionPage.tsx`: high blast radius; contains navigation, command dispatch, runtime state, workspace panel, draft and chat wiring in one component.
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`: executor hydration currently session-keyed and only-once; easy to preserve stale model/provider by accident.
- `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts`: global localStorage fallback and non-clearing hydrate can conflict with authoritative frame execution profile.
- `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx`: tab store owner is `sessionId`; migration must prevent tab leakage across AgentRun/frame boundaries.
- `packages/app-web/src/features/workspace-runtime/model/types.ts`: public workspace data type is session-shaped; renaming/adding fields affects many tabs.
- `packages/app-web/src/features/agent/active-session-list.tsx`: list shape already includes AgentRun refs but current open path ignores `run_ref`; row UX needs state for entries missing refs.
- `packages/app-web/src/pages/LifecyclePages.tsx`, `features/story/story-subject-execution-panel.tsx`, `features/task/task-subject-execution-panel.tsx`: navigation surface area for old `/agent` and `/session` links.
- `packages/app-web/src/services/lifecycle.ts` and `packages/app-web/src/services/project.ts`: endpoint migration and tests will change from `/sessions/...` to `/agent-runs/...`.
- Generated files under `packages/app-web/src/generated/`: must be regenerated by API contract child; frontend should not hand-write temporary DTOs for `AgentRunWorkspaceView`.

### Test / Validation Pattern

- Existing focused tests to update:
  - `packages/app-web/src/features/agent/agent-tab-view.test.ts` should expect `/agent-runs/new?...`.
  - `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.test.ts` should become/seed `useAgentRunWorkspaceState` owner-key tests.
  - `packages/app-web/src/services/lifecycle.test.ts` should add AgentRun endpoint assertions and move session commands to trace/internal-only status if still used.
  - `packages/app-web/src/pages/SessionPage.workspace-module.test.ts` can survive if helper is moved/renamed to AgentRun workspace presentation helper.
- New focused tests recommended:
  - Draft ProjectAgent route -> first submit accepted -> navigates `/agent-runs/{runId}/{agentId}`.
  - `AgentRunWorkspacePage` maps `frame_runtime.execution_profile` into executor selector source and displays provider/model/thinking.
  - Authoritative workspace execution profile clears or overrides stale localStorage provider/model when workspace key changes.
  - Active list row opens `run_ref + agent_ref` and disables/trace-routes rows without those refs.
  - Workspace tab store initializes by AgentRun workspace key, not runtime session id.

### Suggested Validation Commands

Run after frontend implementation:

```powershell
pnpm --filter app-web run typecheck
pnpm --filter app-web test
pnpm run frontend:lint
pnpm run contracts:check
rg -n "/session/new|/session/:sessionId|SessionPage" packages/app-web/src
rg -n "navigate\\(`/session|projectAgentDraftSessionPath|createProjectAgentRuntimeSession|sendAgentRunMessageByRuntimeSession" packages/app-web/src
```

Manual verification after backend children are available:

```powershell
pnpm dev
```

Then launch a ProjectAgent draft, send the first message, verify URL becomes `/agent-runs/{runId}/{agentId}`, model selector shows provider/model/thinking from ProjectAgent or frame `execution_profile`, and retry after a transport interruption reuses the same `client_command_id`.

### External References

- `package.json`: workspace package manager `pnpm@10.33.3`; root validation scripts include `contracts:check`, `frontend:check`, `frontend:lint`, `frontend:test`.
- `packages/app-web/package.json`: React `^19.2.0`, React Router DOM `^7.13.1`, Zustand `^5.0.11`, TanStack React Query `^5.100.14`, Vitest `^4.0.18`, TypeScript `~5.9.3`.
- No external docs were needed; migration is constrained by internal Trellis specs and generated contracts.

### Related Specs

- `.trellis/spec/frontend/architecture.md`: frontend does not create a second business fact source; lifecycle runtime state is backend-owned; `/session/:id` is RuntimeTraceView, not business runtime root.
- `.trellis/spec/frontend/state-management.md`: Zustand/global state boundaries; Session control actions derive from backend `SessionRuntimeControlView.actions`; chat view executes passed action and should not own business dispatch rules.
- `.trellis/spec/frontend/hook-guidelines.md`: session feed/NDJSON reducer boundaries and event aggregation contracts remain relevant while chat still consumes session stream by delivery runtime id.
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: lifecycle primary state is indexed by run/orchestration/runtime node/subject/agent/frame; `session_id` is delivery/debug ref only.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: generated contracts are the source for HTTP/NDJSON DTOs; frontend service should not define cross-feature route DTOs manually.
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`: WorkspacePanel action target may use Project workspace binding while session/story/project default workspace resolves backend target.

## Addendum: SessionMeta Migration Classification

The follow-up query was to inspect `SessionMeta` consumers in title/history/sidebar/active sessions/runtime-control and classify what should migrate to AgentRun Workspace, what should become RuntimeSession Trace read-only display, and what can remain as trace metadata.

### Files Found For SessionMeta Consumption

- `packages/app-web/src/services/session.ts`: defines frontend `SessionMeta`, fetches `/sessions/{id}`, updates `/sessions/{id}/meta`, fetches execution state, and persists session-keyed tab layout (`packages/app-web/src/services/session.ts:29`, `packages/app-web/src/services/session.ts:42`, `packages/app-web/src/services/session.ts:63`, `packages/app-web/src/services/session.ts:127`, `packages/app-web/src/services/session.ts:153`).
- `packages/app-web/src/pages/SessionPage.tsx`: derives runtime header title from `runtimeControl.session_meta.title`, listens for `session_meta_updated`, updates the title through `/sessions/{id}/meta`, and passes `sessionMeta` into workspace runtime data (`packages/app-web/src/pages/SessionPage.tsx:189`, `packages/app-web/src/pages/SessionPage.tsx:192`, `packages/app-web/src/pages/SessionPage.tsx:576`, `packages/app-web/src/pages/SessionPage.tsx:650`, `packages/app-web/src/pages/SessionPage.tsx:664`).
- `packages/app-web/src/components/layout/SessionShortcutList.tsx`: sidebar quick list is session-shaped, maps `ProjectSessionListEntry.title/delivery_status/updated_at/runtime_session_id`, highlights `/session/:sessionId`, and navigates to `/session/{runtimeSessionId}` (`packages/app-web/src/components/layout/SessionShortcutList.tsx:54`, `packages/app-web/src/components/layout/SessionShortcutList.tsx:77`, `packages/app-web/src/components/layout/SessionShortcutList.tsx:89`, `packages/app-web/src/components/layout/SessionShortcutList.tsx:154`).
- `packages/app-web/src/components/layout/workspace-layout.tsx`: treats `/session/` as an Agent navigation prefix and renders the session shortcut list in the Agent nav group (`packages/app-web/src/components/layout/workspace-layout.tsx:36`, `packages/app-web/src/components/layout/workspace-layout.tsx:114`, `packages/app-web/src/components/layout/workspace-layout.tsx:125`, `packages/app-web/src/components/layout/workspace-layout.tsx:192`).
- `packages/app-web/src/features/agent/active-session-list.tsx`: active session rows use `ProjectSessionListEntry.title`, `delivery_status`, `updated_at`, `runtime_session_id`, and optional `agent_ref`; selection/open still centers the runtime session id rather than the AgentRun key (`packages/app-web/src/features/agent/active-session-list.tsx:133`, `packages/app-web/src/features/agent/active-session-list.tsx:186`, `packages/app-web/src/features/agent/active-session-list.tsx:221`, `packages/app-web/src/features/agent/active-session-list.tsx:296`).
- `packages/app-web/src/features/agent/lifecycle-grouping.ts`: lifecycle grouping model still has `sessionTitle`, `deliveryRuntimeSessionId`, and `executionStatus`, which are trace/delivery concepts inside an Agent lifecycle grouping (`packages/app-web/src/features/agent/lifecycle-grouping.ts:12`).
- `packages/app-web/src/services/lifecycle.ts`: runtime-control and trace APIs are session paths: `/sessions/{id}/runtime-control`, `/sessions/{id}/trace`, and session command paths (`packages/app-web/src/services/lifecycle.ts:27`, `packages/app-web/src/services/lifecycle.ts:50`, `packages/app-web/src/services/lifecycle.ts:58`, `packages/app-web/src/services/lifecycle.ts:68`).
- `packages/app-web/src/generated/workflow-contracts.ts`: generated contracts still expose `SessionRuntimeControlView.session_meta`, `SessionShellDto`, and `ProjectSessionListEntry` with both session fields and AgentRun refs (`packages/app-web/src/generated/workflow-contracts.ts:123`, `packages/app-web/src/generated/workflow-contracts.ts:147`, `packages/app-web/src/generated/workflow-contracts.ts:149`).
- `packages/app-web/src/features/workspace-runtime/model/types.ts`: workspace runtime data carries `sessionId`, `runtimeSessionId`, and `sessionMeta`, making the workspace public context session-shaped (`packages/app-web/src/features/workspace-runtime/model/types.ts:36`).
- `packages/app-web/src/stores/workspaceTabStore.ts`: tab layout ownership is described as SessionPage/session metadata and is initialized/persisted by `sessionId` (`packages/app-web/src/stores/workspaceTabStore.ts:1`, `packages/app-web/src/stores/workspaceTabStore.ts:67`, `packages/app-web/src/stores/workspaceTabStore.ts:110`, `packages/app-web/src/stores/workspaceTabStore.ts:277`).
- `packages/app-web/src/features/session/ui/SessionList.tsx`: consumes `useSessionFeed({ sessionId })` as the chronological event feed; this is a RuntimeSession Trace view concern, not AgentRun workspace ownership.
- `packages/app-web/src/features/session/model/platformEvent.ts`, `packages/app-web/src/features/session/model/sessionStreamReducer.ts`, `packages/app-web/src/features/session/ui/SessionChatViewModel.ts`: parse `session_meta_update` platform events, including `context_frame`, `acp_passthrough`, and `session_meta_updated`; these are still trace/event-stream facts, even if the workspace identity migrates (`packages/app-web/src/features/session/model/platformEvent.ts:22`, `packages/app-web/src/features/session/model/sessionStreamReducer.ts:244`, `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:113`).

Backend context that explains the frontend shape:

- `crates/agentdash-contracts/src/workflow.rs`: `SessionShellDto`, `SessionRuntimeControlView`, and `ProjectSessionListEntry` are generated into the frontend and currently encode trace metadata plus optional `run_ref/agent_ref/frame_ref`.
- `crates/agentdash-api/src/routes/sessions.rs`: `get_session_runtime_control` returns `session_meta`, `get_project_sessions` returns session list entries, and `project_session_entry` maps anchored sessions to AgentRun refs while preserving `runtime_session_id`, `title`, and `delivery_status`.
- `crates/agentdash-application/src/session/title_service.rs`, `crates/agentdash-application/src/session/eventing.rs`, `crates/agentdash-application/src/session/launch/deps.rs`: session title generation, source-title projection, user title update, and `session_meta_updated` broadcasts are trace/session metadata behavior today.

### Should Migrate To AgentRun Workspace

- Workspace page identity/title: `SessionPage` currently treats `runtimeControl.session_meta.title` as the workspace header and edits it via `/sessions/{id}/meta`. In the new model, the primary workspace title should come from `AgentRunWorkspaceView` or a dedicated AgentRun/ProjectAgent display contract. If manual title editing remains in the workspace, it should update the AgentRun workspace title, not the delivery trace title. `SessionMeta.title` can remain visible as trace title metadata.
- Sidebar navigation: `SessionShortcutList` and `workspace-layout` should stop making `/session/{runtimeSessionId}` the Agent nav destination. Entries that have `run_ref` and `agent_ref` should open the AgentRun workspace route, for example `/agent-runs/{runId}/{agentId}` or the final route helper chosen by the API/frontend implementation. Rows without AgentRun refs should be trace-only or disabled with a trace route.
- Active sessions list: `ActiveSessionList` should be renamed/reframed as active AgentRun/workspace list or use a new AgentRun list endpoint. It can still display trace-derived `title`, `delivery_status`, and `updated_at` while the API is transitional, but row identity and selection should be `run_ref + agent_ref` or `frame_ref`, not `runtime_session_id`.
- Runtime-control page data: the new workspace loader should not expose `SessionRuntimeControlView.session_meta` as the primary UI state. The replacement should separate `workspace` facts (AgentRun, ProjectAgent, AgentFrame, execution profile, command capabilities) from `delivery_trace_meta` or `runtime_trace_meta` facts (session id/title/event seq/status).
- Workspace tab layout owner: `workspaceTabStore` and `saveSessionTabLayout/loadSessionTabLayout` should be keyed by a stable AgentRun workspace key, not the runtime session id. The trace session id can still be stored inside workspace data for tabs that call session trace/runtime endpoints.
- `WorkspaceRuntimeData`: split the public shape into workspace identity (`workspaceKey`, `runRef`, `agentRef`, `frameRef`, `workspaceTitle`, `executionProfile`) and delivery trace (`deliveryRuntimeSessionId`, optional `deliveryTraceMeta`). Keeping `sessionMeta` as a top-level workspace field will preserve the old mental model.

### Should Become RuntimeSession Trace Read-Only Display

- Session history/event feed: `SessionList`, `useSessionFeed`, stream reducer, projection frames, compaction summaries, lineage, and event cards should remain session-id based because they render the chronological trace. The route/page should be trace-named, for example `/runtime-sessions/{runtimeSessionId}/trace`, rather than being the interactive workspace root.
- Trace header/details: `SessionShellDto.title`, `title_source`, `last_event_seq`, `last_turn_id`, and `last_delivery_status` are useful in trace details, diagnostics, and history views. They should not decide the AgentRun workspace identity.
- Runtime trace API: `/sessions/{id}/trace`, `/sessions/{id}/events`, context projection, fork/rollback, and raw NDJSON stream readers are naturally trace-bound. Workspace pages may consume them through `deliveryRuntimeSessionId`, but the UI should present them as delivery trace/history, not as the source of the workspace model selector or title.
- Runtime control actions that are tied to an active delivery trace can remain reachable from an AgentRun workspace as command capabilities, but the rendered state should say "this delivery trace is running/cancellable" rather than "this session is the workspace".

### Can Remain As Trace Metadata Or Adapter State

- `runtime_session_id`: delivery/debug reference for stream, events, terminal/session-scoped tools, and trace links.
- `SessionMeta.title` and `title_source`: trace title and source title projection. It can provide a fallback display label when an AgentRun list has not produced a stronger workspace title, but that fallback should be explicit in the mapper.
- `last_event_seq`, `last_turn_id`, `last_delivery_status`: trace cursor/status metadata and list badges.
- `session_meta_update` platform events: still valid as event-stream side-channel names for trace projection (`context_frame`, compaction, `workspace_module_presented`, source title update). Renaming protocol events can be separate; frontend should not remove handling during the workspace migration.
- Session-based bridge identifiers in extension/canvas/terminal/context tabs: these can continue to call session endpoints when the tab is operating on the current delivery trace, but the outer workspace data should make the distinction visible (`deliveryRuntimeSessionId` rather than generic `sessionId`).

### API / Contract Decisions Needed

- Add or confirm an `AgentRunWorkspaceView` contract with explicit workspace display fields and a nested trace metadata object. Suggested separation: `workspace_title`/`title_source` for AgentRun workspace display, `execution_profile` for selector hydration, `delivery_runtime_session_id`, and optional `delivery_trace_meta`.
- Add or confirm an AgentRun list/workspace endpoint. The current `/projects/{projectId}/sessions` endpoint already returns `run_ref`, `agent_ref`, and `frame_ref`, but the route name, primary key, and frontend consumers still communicate "session list".
- Decide whether `/sessions/{id}/meta` remains user-editable as a trace-title endpoint. If it remains, workspace UI should label it as trace metadata or move it into a trace details panel.
- Decide whether source session title updates should influence only trace metadata or also update an AgentRun workspace title when the workspace has no user/workspace-specific title.

### SessionMeta-Specific Implementation Entry Points

1. Create the AgentRun workspace route/service/model first so `SessionPage` is not adapted in place around `SessionRuntimeControlView.session_meta`.
2. Move header/title state and executor hydration into `AgentRunWorkspacePage`, mapping generated AgentRun/AgentFrame contracts to UI state.
3. Replace sidebar and active session navigation helpers to prefer AgentRun refs, with trace-only handling for entries that do not have refs.
4. Rename/split `WorkspaceRuntimeData.sessionId/sessionMeta` before migrating workspace tabs, so tab code cannot accidentally keep treating trace session id as workspace id.
5. Keep trace feed and platform event parsing intact, then mount it as a trace panel or trace page behind the AgentRun workspace.

## Caveats / Not Found

- Active Trellis task pointer was absent (`task.py current --source` returned none). The user supplied the parent task path explicitly, so this research was written to `.trellis/tasks/06-11-session-model-delivery-state-chain/research/`.
- No `AgentRunWorkspaceView` generated frontend type exists yet; frontend child should wait for API contract child to generate it.
- No `client_command_id` exists in current frontend generated request DTOs; retry/idempotency behavior cannot be implemented correctly until command receipt/API contract children land.
- Current `SessionRuntimeControlView` has most of the data shape needed for a transitional implementation, but using it directly in new frontend code would preserve session-first public identity and conflict with parent requirements.
- `frame_runtime.execution_profile` is currently `JsonValue`; implementation needs a narrow runtime validator/mapper before passing it to executor selector.
- `sessionHistoryStore.ts` and `sidebarSessionsStore.ts` were referenced by older frontend spec language but are not present under `packages/app-web/src/stores/`; the implemented sidebar/session history surfaces are component/service based.
- The research did not start a dev server, run tests, modify source code, or update specs.
