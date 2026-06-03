# Research: frontend lifecycle/session/workflow/task gap

- Query: 扫描 `packages/app-web/src` 中 session / workflow / story / task / project-agent / context / permission 的 services、stores、features、pages、generated types，以及 `tests/e2e` 中 task / story / session / lifecycle 的使用，判断前端谓词与 `Lifecycle -> Actor -> ActorFrame -> RuntimeSession` 目标体系的差距。
- Scope: internal
- Date: 2026-06-01

## Findings

### Related specs and task context

- `.trellis/workflow.md`：当前任务属于 Trellis task workflow，研究产物应落在任务目录。
- `.trellis/spec/frontend/architecture.md`：前端不应创建第二套业务事实源，store 负责查询缓存和 UI draft。
- `.trellis/spec/frontend/state-management.md`：跨模块状态要以服务层/后端 DTO 为准，避免在 store 中制造新的协议事实。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`：run view 应读取 `activity_state.attempts / outputs / inputs`，不再读取旧 `step_states`。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：generated contracts 是前后端契约入口，手写类型只能做窄化和 UI 投影。
- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/agent-operation-predicates.md`：目标谓词是 `Lifecycle -> Actor -> ActorFrame -> RuntimeSession`；Task 只是数据/SubjectRef/Activity payload，不承载 runtime 含义。

### Files found

- `packages/app-web/src/generated/project-agent-contracts.ts`：ProjectAgent 创建/更新 DTO、打开 agent session 的返回 DTO。
- `packages/app-web/src/generated/session-contracts.ts`：session event、lineage、projection view 的 generated DTO。
- `packages/app-web/src/generated/workflow-contracts.ts`：ActivityAttemptState、ActivityLifecycleRunState、ExecutorRunRef、LifecycleRunLinkDto、StoryRunOverviewDto、WorkflowBindingKind。
- `packages/app-web/src/generated/core-contracts.ts`：TaskResponse / TaskStatus。
- `packages/app-web/src/types/workflow.ts`：前端 workflow/lifecycle/run 类型包装。
- `packages/app-web/src/types/session.ts`：SessionRunContext、SessionReturnTarget、HookSessionRuntimeInfo、SessionNavigationState。
- `packages/app-web/src/types/context.ts`：SessionContextSnapshot、TaskSession*Summary、StorySessionInfo、ProjectSessionInfo。
- `packages/app-web/src/types/acp.ts`：ProjectSessionEntry active session 列表模型。
- `packages/app-web/src/types/permission.ts`：PermissionGrant / GrantScope。
- `packages/app-web/src/services/workflow.ts`：workflow definition、activity lifecycle definition、lifecycle run、human decision API mapper。
- `packages/app-web/src/services/story.ts`：Story/Task CRUD、Task execution、TaskSessionPayload、StoryRuns、StorySession binding API。
- `packages/app-web/src/services/session.ts`：Session meta/events/context/projection/lineage/hook-runtime/state/project sessions API。
- `packages/app-web/src/services/executor.ts`：prompt/tool approval/companion request 均以 sessionId 为入口。
- `packages/app-web/src/services/project.ts`：ProjectAgent session、ProjectSessionInfo、ProjectAgentSummary mapper。
- `packages/app-web/src/services/permission.ts`：permission grant 列表按 session_id / run_id 过滤。
- `packages/app-web/src/stores/workflowStore.ts`：workflow/lifecycle editor draft、runsBySessionId、startRun/fetchRunsBySession。
- `packages/app-web/src/stores/storyStore.ts`：tasksByStoryId、runsByStoryId、sessionsByStoryId、Task execution actions。
- `packages/app-web/src/stores/projectStore.ts`：project agents、open/forceNew project agent session、project agent session history。
- `packages/app-web/src/stores/sessionHistoryStore.ts`：全局 free session 历史。
- `packages/app-web/src/stores/workspaceTabStore.ts`：workspace tab layout 按 sessionId 保存。
- `packages/app-web/src/pages/SessionPage.tsx`：session 页面汇聚 runtime context、workflow runs、LifecycleSessionView、SessionChatView。
- `packages/app-web/src/pages/StoryPage.tsx`：Story 页面展示 Task lifecycle_step_key/status 和 StorySessionPanel。
- `packages/app-web/src/features/workflow/lifecycle-session-view.tsx`：按 sessionId 展示 lifecycle run attempts / artifacts。
- `packages/app-web/src/features/task/task-agent-session-panel.tsx`：Task 抽屉内启动/继续 session。
- `packages/app-web/src/features/task/task-drawer.tsx`：Task 状态、agent binding、执行产物、TaskAgentSessionPanel。
- `packages/app-web/src/features/story/story-session-panel.tsx`：Story 手工 session binding 和内嵌 SessionChatView。
- `packages/app-web/src/features/agent/agent-tab-view.tsx`：ProjectAgent Hub、ActiveSessionList、SessionChatView 和 lifecycle breadcrumb。
- `packages/app-web/src/features/agent/session-grouping.ts`：按 ProjectSessionEntry.owner_type / story_id / parent_session_id 组织 session 树。
- `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx`：当前 session 的 context/runtime/workflow 概览。
- `packages/app-web/src/features/session-context/context-inspector-panel.tsx`：按 sessionId 查询 context audit。
- `packages/app-web/src/features/permission/PermissionGrantCard.tsx`：permission grant 审批卡。
- `tests/e2e/task-drawer-return.spec.ts`：Task start 后从 Task DTO 取 session_id，再跳 `/session/{sessionId}`。
- `tests/e2e/task-agent-binding.spec.ts`：Task 创建/编辑 agent_binding。
- `tests/e2e/story-context-injection.spec.ts`：Story session binding、session prompt、ACP session stream。
- `tests/e2e/local-hello-extension.spec.ts`、`tests/e2e/extension-assets-panel.spec.ts`、`tests/e2e/canvas-promote-extension.spec.ts`：ProjectAgent session 打开后进入 session 工作空间。

### Code patterns by module

| Frontend module | Current DTO / fields | Current runtime interpretation | Target predicate layer |
| --- | --- | --- | --- |
| Generated project-agent contracts | `CreateProjectAgentRequest.default_lifecycle_key/default_workflow_key/is_default_for_story/is_default_for_task`，`OpenProjectAgentSessionResult.session_id/binding_id`，`ProjectAgentSession.session_id` (`packages/app-web/src/generated/project-agent-contracts.ts:6`, `:8`, `:14`) | ProjectAgent 被建模为可直接打开/拥有 RuntimeSession 的主体；default workflow/lifecycle 挂在 agent 配置上。 | ProjectAgent 应成为 Actor launch profile / default ActorProcedure + Lifecycle launch policy；打开动作返回 Actor/ActorFrame/RuntimeSession 投影，而不是只返回 session。 |
| Generated workflow contracts | `ActivityAttemptState.executor_run`，`ActivityLifecycleRunState.attempts/outputs/inputs`，`ExecutorRunRef.agent_session.session_id`，`WorkflowBindingKind = "project" | "story"` (`packages/app-web/src/generated/workflow-contracts.ts:6`, `:22`, `:58`, `:106`) | Activity evidence 已基本对齐，但 agent executor 仍以 session 作为 child run ref；Workflow binding 只支持 project/story，缺 Actor/Subject 语义。 | `ActivityAttemptState` 保留为 evidence；agent executor ref 应指向 Actor/ActorFrame，RuntimeSession 是其底层 trace；Workflow graph 应绑定 Lifecycle/Activity/Subject capability，不由 session 决定。 |
| Generated core contracts / Task | `TaskResponse.lifecycle_step_key/status/agent_binding/artifacts`，`TaskStatus.running/completed/failed` (`packages/app-web/src/generated/core-contracts.ts:92`, `:94`) | Task DTO 自带运行状态、步骤映射、执行产物，UI 会把 Task 当 runtime owner。 | Task 应降级为 SubjectRef / Activity payload / 用户查看对象；状态和产物从 SubjectExecutionView 或 ActivityAttemptState 投影。 |
| Workflow service | `mapWorkflowRun` 强制 `session_id`，`fetchWorkflowRunsBySession(sessionId)` 调 `/lifecycle-runs/by-session/{sessionId}`，`startWorkflowRun` 必传 `session_id` (`packages/app-web/src/services/workflow.ts:504`, `:509`, `:631`, `:640`) | Lifecycle run 的查询和启动入口是 session-first。 | 新查询应按 `run_id`、`actor_id`、`subject_ref` 或 `lifecycle_id` 查询；RuntimeSession 只作为 ActorFrame 下的 trace。 |
| Workflow store/editor | `runsBySessionId`，`fetchRunsBySession`，`startRun({ session_id })` (`packages/app-web/src/stores/workflowStore.ts:331`, `:366`, `:377`, `:481`, `:494`) | Store 以 sessionId 作为 run 缓存主键；编辑器把 ActivityLifecycleDefinition + WorkflowDefinition 作为一个“workflow asset”保存。 | run cache 应转为 `runsByActorId` / `runsBySubjectRef` / `runsByRunId`；ActivityLifecycleDefinition 应成为目标 Workflow(graph config)，当前 WorkflowDefinition 更像 ActivityProcedure/ActorProcedure。 |
| Story service/store | Story 有 run-oriented API `fetchStoryRuns`，同时还有 `/stories/{id}/sessions` binding；store 同时保存 `runsByStoryId` 与 `sessionsByStoryId` (`packages/app-web/src/services/story.ts:456`, `:458`, `:486`; `packages/app-web/src/stores/storyStore.ts:40`, `:41`, `:472`, `:483`) | Story 同时被看作 Subject 的 run 聚合对象和 session binding owner，形成两套入口。 | Story 应作为 SubjectRef/Subject view；运行面由 LifecycleSubjectAssociation + Actor/ActivityAttempt 投影提供，手工 session binding 需要收束为 Actor association/launch。 |
| Task service/store | Task CRUD 接收 `lifecycle_step_key/agent_binding`，Task execution 调 `/tasks/{id}/start|continue|cancel`，`TaskSessionPayload.session_id/task_status/runtime_surface/context_snapshot` (`packages/app-web/src/services/story.ts:363`, `:397`, `:405`, `:413`, `:427`; `packages/app-web/src/stores/storyStore.ts:99`, `:378`) | Task 是运行命令入口、状态事实源和 session 归属查询入口。 | Task start/continue/cancel 语义应迁到 Activity/Actor command；Task 查询应只返回数据，运行状态从 SubjectExecutionView / ActivityAttemptState 派生。 |
| Session services | session meta/events/context/projection/lineage/hook-runtime/state 都以 sessionId 查询 (`packages/app-web/src/services/session.ts:47`, `:65`, `:121`, `:140`, `:153`, `:177`) | RuntimeSession 是前端 runtime 的主事实源。 | 保留 RuntimeSession trace API，但 UI 上层入口应从 ActorFrameRuntimeView 进入，再下钻到 RuntimeSession events/projection/lineage。 |
| Executor service / chat | `promptSession(sessionId)`、tool approval、companion response 均以 sessionId 为命令目标 (`packages/app-web/src/services/executor.ts:26`, `:33`, `:51`) | 用户 prompt / approval 直接发给 RuntimeSession。 | 命令目标应为 ActorFrame/Actor turn；服务端可解析到 RuntimeSession。UI 可在底层 trace 中仍显示 sessionId。 |
| ProjectAgent services/stores/UI | `openProjectAgentSession`、`forceNewProjectAgentSession`、`fetchProjectAgentSessions` (`packages/app-web/src/services/project.ts:206`, `:217`, `:228`; `packages/app-web/src/stores/projectStore.ts:72`, `:373`, `:390`) | ProjectAgent 是打开 session 的高层入口，但没有 Actor 概念。 | ProjectAgent 应启动/选择 Actor；返回 ActorFrame + RuntimeSession refs，ProjectAgentSession 列表变为 Actor history。 |
| Project active sessions | `ProjectSessionEntry.owner_type = project|story|task`、`parent_session_id` (`packages/app-web/src/types/acp.ts:132`, `:137`; `packages/app-web/src/services/session.ts:293`) | 项目运行概览是 session 树，并用 owner_type 解释 Story/Task 归属。 | 应暴露 ProjectActiveActorsView / ProjectRuntimeOverview，用 Actor lineage + SubjectRef + RuntimeSession refs 组织。 |
| SessionPage | 进入页由 `/session/:sessionId` 驱动，拉 `runsBySessionId`，`useSessionRuntimeState(sessionId)`，再把 `workflowRuns` 注入工作区 (`packages/app-web/src/pages/SessionPage.tsx:43`, `:96`, `:139`, `:383`) | Session 是页面根；Lifecycle view 也依赖 sessionId。 | 页面可保留 RuntimeSession detail，但主入口需要 Actor/ActorFrame 页面或 route state；SessionPage 成为 trace/detail。 |
| LifecycleSessionView | props 是 `sessionId`；从 `runsBySessionId[sessionId]` 取 run；attempt 的 `agent_session.session_id` 内嵌 `SessionList` (`packages/app-web/src/features/workflow/lifecycle-session-view.tsx:284`, `:291`, `:297`, `:59`, `:110`) | Lifecycle run 展示已用 `ActivityAttemptState`，但 agent attempt 的主体仍是 SessionList。 | ActivityAttemptCard 应展示 Actor attempt / ActorFrame runtime summary，再可下钻到 RuntimeSession stream。 |
| Task drawer/session panel | 首次发送 `startTaskExecution`，后续 `promptSession`；Task drawer 展示 `TaskStatusBadge` 和 `执行产物` (`packages/app-web/src/features/task/task-agent-session-panel.tsx:7`, `:154`, `:184`, `:197`; `packages/app-web/src/features/task/task-drawer.tsx:149`, `:252`, `:263`) | Task 被 UI 体验为“可执行主体 + 会话拥有者 + 产物容器”。 | Task 抽屉应展示 SubjectExecutionView：关联 Actor、ActivityAttempt、artifacts；发送命令走 Actor/Activity command。 |
| Story page/session panel | Story 页面展示“关联 Session”；StorySessionPanel 创建 session binding 并内嵌 SessionChatView (`packages/app-web/src/pages/StoryPage.tsx:401`, `:405`; `packages/app-web/src/features/story/story-session-panel.tsx:61`, `:73`) | Story 有手动 session binding 面。 | Story 应展示 Subject/Activity 状态和 Actors；绑定面应迁为 Actor association / launch history。 |
| Agent tab | `useProjectSessions` + `selectedSessionId`，选 session 后渲染 `SessionChatView`，breadcrumb 用 `fetchRunsBySession(primarySessionId)` (`packages/app-web/src/features/agent/agent-tab-view.tsx:62`, `:79`, `:116`, `:143`, `:273`) | Agent Hub 是“项目 session 列表 + 选中 session 聊天”。 | Agent Hub 应是 Actor Hub：左侧 ProjectAgent launch profile，右侧 Actor list / ActorFrame runtime，session 为底层 trace。 |
| Workspace/context runtime | `WorkspaceRuntimeData.sessionId`，`SessionContextSnapshot`，`hookRuntime`，`sessionCapabilities`，`workflowRuns` (`packages/app-web/src/features/workspace-runtime/model/types.ts:29`; `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:28`) | 这块已经像未来 ActorFrame runtime projection，但名字和查询仍是 session。 | 迁移为 ActorFrameRuntimeView 最直接：context snapshot、runtime surface、capability、hook runtime、permission pending 都归 ActorFrame。 |
| Context audit/inspector | `ContextInspectorPanel({ sessionId })` 查询 `/sessions/{id}/context/audit` (`packages/app-web/src/features/session-context/context-inspector-panel.tsx:69`, `:77`, `:87`) | context audit 是 session-scoped。 | audit 应至少能按 ActorFrame 查询；session audit 保留为 trace detail。 |
| Permission | `PermissionGrant.run_id/session_id/grant_scope = turn|session|workflow_step`，list 可按 `session_id/run_id` 过滤 (`packages/app-web/src/types/permission.ts:3`, `:28`; `packages/app-web/src/services/permission.ts:4`, `:10`; `packages/app-web/src/features/permission/PermissionGrantCard.tsx:135`) | Grant 被绑定到 session/run/step，UI 不知道 ActorFrame capability source。 | PermissionGrantFrameView 应可按 actor_id / actor_frame_id / run_id / subject_ref 查询；scope 应表达 ActorFrame/ActivityAttempt/turn。 |
| Session shortcut/layout | `SessionShortcutList` 和 `session-shortcut-rows` 按 `session.session_id/parent_session_id` 构造导航树 (`packages/app-web/src/components/layout/SessionShortcutList.tsx:56`, `:144`, `:167`; `packages/app-web/src/components/layout/session-shortcut-rows.ts:27`, `:37`) | 全局导航把 session lineage 当运行树。 | 全局导航可改为 Actor/Subject runtime list；session lineage 放入 detail。 |

### Gaps and duplicated state

- **Session-first runtime root**：`SessionPage`、`SessionChatView`、`useSessionRuntimeState`、`SessionContextSnapshot`、context audit、permission approval、tool approval、workspace tab layout 都以 sessionId 为根。目标体系里 RuntimeSession 应是 ActorFrame 的底层运行轨迹，不能继续作为 UI 上层事实根。
- **Task runtime leakage**：Task DTO 带 `status/artifacts/lifecycle_step_key/agent_binding`，Task service/store 有 `start/continue/cancel`，Task drawer 和 e2e 都把 Task 当可执行 runtime subject。目标下 Task 应只作为 SubjectRef 或 Activity payload，执行状态应来自 ActivityAttemptState 或 SubjectExecutionView。
- **Story 双入口**：Story 已有 `/stories/{id}/runs` 的 run-oriented API，同时还保留 `/stories/{id}/sessions` 和 StorySessionPanel。两个入口都能解释“Story 的运行态”，容易造成 UI 状态不一致。
- **Workflow 命名反向**：前端 `ActivityLifecycleDefinition` 才是目标语义里的 Workflow graph config；当前 `WorkflowDefinition` 是单个 agent activity 的 contract/procedure。`workflowStore` 里统一编辑器保存两种实体，会让“Workflow 是图还是活动执行契约”持续混淆。
- **ProjectAgent 与 Actor 缺口**：ProjectAgent 是高层封装的雏形，但现在只返回 session，并以 `ProjectAgentSession` 历史展示。缺少 Actor/ActorFrame 标识，导致 agent hub、extension e2e、workspace tab 都只能围绕 session 编排。
- **Context/permission 缺少 ActorFrame 锚点**：`WorkspaceRuntimeData` 和 `SessionContextSnapshot` 已经承载 capability/context/runtime surface/hook runtime，但命名和查询是 session；PermissionGrant 也只有 run/session/workflow_step。
- **Run cache 维度不对**：`workflowStore.runsBySessionId` 与 `StoryStore.runsByStoryId` 并存。迁移后应以 run_id 为权威索引，再提供 actor/subject/lifecycle 派生视图，避免同一个 run 在多个 store 中出现分叉状态。
- **owner_type 树与目标 subject association 不同**：ProjectSessionEntry 用 `owner_type project|story|task` 和 `parent_session_id` 组织列表。目标应表达 Actor lineage + SubjectRef association；Task 不再因为 owner_type 成为运行主体。

### New queries / view models to expose

- `ActorFrameRuntimeView`：以 `actor_id` 或 `actor_frame_id` 查询，包含 RuntimeSession refs、context snapshot、runtime_surface、session_capabilities、hook runtime、pending permissions、active activity/run。
- `LifecycleRunView`：以 `run_id` 查询，包含 lifecycle graph metadata、activity_state、actors/actor frames、subject links、artifacts；替代纯 `by-session` run 查询。
- `SubjectExecutionView`：以 `SubjectRef(kind, id)` 查询，返回关联 lifecycle runs、Actor assignment、latest ActivityAttemptState、artifacts、RuntimeSession trace refs；Task/Story 页面使用它展示运行状态。
- `ProjectRuntimeOverview` / `ProjectActiveActorsView`：替代 `ProjectSessionEntry`，按 Actor/Subject/Lifecycle grouping，包含 RuntimeSession lineage 作为下钻信息。
- `ProjectAgentActorLaunchView`：ProjectAgent 创建/打开入口返回 Actor/ActorFrame/RuntimeSession refs，承载 default lifecycle / procedure / executor。
- `PermissionGrantFrameView`：按 actor_id / actor_frame_id / run_id / subject_ref 过滤，展示 grant scope、capability path、frame revision、来源 ActivityAttempt。
- `RuntimeSessionTraceView`：保留当前 session events/projection/lineage/chat stream，但明确作为 ActorFrame 下钻页面。

### Tests affected

- `tests/e2e/task-drawer-return.spec.ts:159`：通过 `/tasks/{taskId}/start` 绑定 session，并从 Task DTO `session_id` 跳 `/session/{sessionId}`；迁移后应改断言 SubjectExecutionView / ActorFrame 返回的 runtime session ref。
- `tests/e2e/task-agent-binding.spec.ts:3`：验证 Task 创建/详情编辑 `agent_binding`；若 Task 不再直接携带执行 agent binding，应改到 Task subject 的 launch/assignment 配置或 Actor launch command。
- `tests/e2e/story-context-injection.spec.ts:215`、`:230`、`:256`：Story session binding、session prompt、ACP session stream；迁移后 Story 应通过 Actor/Subject execution 取 RuntimeSession ref。
- `tests/e2e/local-hello-extension.spec.ts:172`、`tests/e2e/extension-assets-panel.spec.ts:175`、`tests/e2e/canvas-promote-extension.spec.ts:165`：ProjectAgent open session helper；迁移后应打开 Actor/ActorFrame，再下钻 session workspace。
- `packages/app-web/src/stores/workflowStore.test.ts:30`：断言 lifecycle editor 自动派生 `workflow_key` 和 `session_policy: spawn_child`；命名迁移时应改为 ActivityProcedure/ActorProcedure 的 key/policy。
- `packages/app-web/src/services/workflow.test.ts:42`：断言 ActivityLifecycle agent executor 的 `workflow_key/session_policy`；需要对齐新 procedure 引用和 ActorFrame session policy。
- `packages/app-web/src/features/workflow/lifecycle-editor-shell.test.tsx:21`：编辑器 selection/ActivityInspector 仍可保留，但命名和 draft 类型要改。
- `packages/app-web/src/features/workflow/model/lifecycle-port-sync.test.ts:43`、`packages/app-web/src/features/workflow/ui/activity-inspector.test.tsx:22`：依赖 `workflow_key/session_policy`。
- `packages/app-web/src/features/agent/session-grouping.test.ts:68`：按 `owner_type story/task/project` 和 `parent_session_id` 构造树；应改为 Actor/Subject/RuntimeSession refs。
- `packages/app-web/src/features/agent/session-filter.test.ts:45`：按 session title/agent/owner/status 过滤；应改为 Actor/Subject/runtime status 过滤。
- `packages/app-web/src/components/layout/session-shortcut-rows.test.ts:27`：全局 session shortcut 按 `parent_session_id` 排序；应改为 Actor/RuntimeSession trace 列表。
- `packages/app-web/src/pages/SessionPage.hook-runtime.test.tsx:47`：Hook runtime metadata 仍使用 `active_workflow.step_key/workflow_key`；迁移后应调整为 Activity/Workflow/ActorFrame 命名。
- `packages/app-web/src/features/workspace-panel/ContextOverviewTab.projection.test.tsx:12`：SessionContextSnapshot / WorkflowRun[] 是 context overview 的输入；应改为 ActorFrameRuntimeView 输入。
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.test.ts:19`：runtime state selector 按 session_id/source_key；应新增 actor frame selector。
- `packages/app-web/src/features/session/ui/SessionChatView.test.tsx:20`、`SessionProjectionView.test.tsx:64`、`SessionLineageView.test.tsx:24`：底层 RuntimeSession trace 可保留，但路由入口和父级 context 需要更新。

### External references

- No external references used. 本切片只做源码与本地 Trellis 规范扫描。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回当前 active task 为空；本研究按用户明确给出的 `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment` 写入。
- 未修改任何 `packages/app-web/src` 源码、spec 或测试文件。
- 未发现前端还有读取旧 `step_states` 的路径；当前 run view 主要使用 `activity_state.attempts/outputs/inputs`，这一点与现有前端规范一致。
- `generated/project-agent-contracts.ts` 中 create/update request 有 `default_workflow_key`，但 generated `ProjectAgent` type 没有该字段；这可能是契约生成或后端响应不一致，需在后续迁移中确认。
- 本切片没有验证后端实际返回 shape，只基于 generated contracts、services mappers、stores、features/pages 和 tests/e2e 使用面做静态源码研究。
