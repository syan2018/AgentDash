# Research: frontend naming audit

- Query: 只读盘点前端与 Lifecycle / Workflow / Session / Agent 相关的不清晰命名和概念混用点；根据用户澄清，`workflowStore` 管理 `WorkflowGraph` 的 store 名称本身是合适的，不列为清理问题。
- Scope: internal
- Date: 2026-06-03

## Findings

### Files Found

| File | Description |
| --- | --- |
| `packages/app-web/src/stores/workflowStore.ts` | 管理 WorkflowGraph definition editor，同时保存每个 Agent activity 的 AgentProcedure draft。`workflowStore` 名称本身合理；问题集中在局部 procedure draft 被称为 workflow draft 的位置。 |
| `packages/app-web/src/stores/lifecycleStore.ts` | Lifecycle run / graph instance / subject / agent / frame / runtime trace 的归一化 store。 |
| `packages/app-web/src/services/workflow.ts` | WorkflowGraph / AgentProcedure definition API，以及 human decision command。 |
| `packages/app-web/src/services/lifecycle.ts` | Lifecycle target view API 与 runtime trace / frame runtime 查询。 |
| `packages/app-web/src/types/workflow.ts` | WorkflowGraph、AgentProcedure、template、旧 run summary 类型入口。 |
| `packages/app-web/src/types/lifecycle-views.ts` | Generated lifecycle target view re-export 与 `subjectExecutionKey`。 |
| `packages/app-web/src/pages/LifecyclePages.tsx` | LifecycleRun / SubjectExecution / LifecycleAgent / AgentFrameRuntime / RuntimeTrace 钻取页面。 |
| `packages/app-web/src/features/agent/active-session-list.tsx` | 用户侧活跃“会话”列表，以 LifecycleRun + LifecycleAgent + delivery RuntimeSession meta 投影展示。 |
| `packages/app-web/src/features/agent/lifecycle-grouping.ts` | 按 lifecycle subject associations 分组用户侧会话列表 entry。 |
| `packages/app-web/src/features/agent/agent-tab-view.tsx` | ProjectAgent launch 后读取 lifecycle run/frame，并跳转到 delivery runtime trace。 |
| `packages/app-web/src/components/layout/SessionShortcutList.tsx` | 侧栏 session shortcut，从 LifecycleRun / LifecycleAgent 推导 runtime session shortcut。 |
| `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts` | 通过 RuntimeSession id 查询 AgentFrameRuntimeView 的 trace adapter hook。 |
| `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx` | Workspace session context 页面里展示 lifecycle run / active workflow metadata。 |
| `packages/app-web/src/features/workflow/model/lifecycle-port-sync.ts` | Activity artifact edge 与 AgentProcedure port contract 同步逻辑。 |
| `packages/app-web/src/features/workflow/ui/activity-inspector-sections.tsx` | Activity executor、procedure 引用、runtime session policy 和 completion policy UI。 |
| `packages/app-web/src/features/workflow/ui/lifecycle-dag-canvas.tsx` | WorkflowGraph DAG canvas，使用 AgentProcedure 列表渲染 agent activity tooltip。 |
| `packages/app-web/src/features/workflow/ui/panels/InjectionPanel.tsx` | AgentProcedure injection guidance / context binding panel。 |
| `packages/app-web/src/features/workflow/ui/panels/shared.ts` | Workflow hook trigger label 与 gate/process trigger 分组。 |
| `packages/app-web/src/features/task/task-subject-execution-panel.tsx` | Task subject execution projection panel 与 start/continue/cancel commands。 |
| `packages/app-web/src/features/story/story-subject-execution-panel.tsx` | Story subject execution projection panel。 |

### Code Patterns

- Frontend spec 的目标模型是 `LifecycleRunView -> WorkflowGraphInstanceView -> ActivityState / ActivityAttemptState`，并明确“不以 RuntimeSession id 或单 graph id 作为 lifecycle 主索引”；`session_id` 只在 runtime trace refs 中出现。见 `.trellis/spec/frontend/workflow-activity-lifecycle.md`。
- Backend workflow spec 规定 `WorkflowGraph` 是 executable Activity graph definition；`LifecycleRun` 是 tracked life process / control ledger，不是单个 graph 的 run；activity runtime identity 必须以 `graph_instance_id + activity_key` 为 namespace。见 `.trellis/spec/backend/workflow/activity-lifecycle.md`。
- Backend session spec 把 RuntimeSession 运行态限定为 turn claim / active / cancel / cleanup 边界，不能替代 lifecycle 事实源。见 `.trellis/spec/backend/session/runtime-execution-state.md`。
- `lifecycleStore` 的主索引方向正确：`runs`、`graphInstances`、`agents`、`frames`、`subjectExecutions`、`runtimeTraces` 分开存储，见 `packages/app-web/src/stores/lifecycleStore.ts:32`。
- `lifecycleStore.ingestRun` 从 `LifecycleRunView` 展开 `workflow_graph_instances` 和 `agents`，体现 run-first ingestion，见 `packages/app-web/src/stores/lifecycleStore.ts:145`。
- `workflowStore` 管理 WorkflowGraph definition 是合理的；当前可疑点是 store 内部把 AgentProcedure draft / AgentProcedure catalog 的局部变量、注释、UI 仍叫 `workflow`，例如 `workflowDraftsByActivityKey` 存的是 activity 对应的 AgentProcedure draft，见 `packages/app-web/src/stores/workflowStore.ts:183` 和 `packages/app-web/src/stores/workflowStore.ts:193`。
- `services/workflow.ts` 的 definition API 已清楚使用 `fetchWorkflowGraphs` / `createWorkflowGraph`，见 `packages/app-web/src/services/workflow.ts:492`；但 `submitHumanDecision` 返回 `WorkflowRun`，与目标 `LifecycleRunView` projection 不一致，见 `packages/app-web/src/services/workflow.ts:589`。
- `active-session-list.tsx` 明确说明 UI 是用户视角“会话列表”，底层由 lifecycle run -> agent -> runtime_session_ref 驱动，见 `packages/app-web/src/features/agent/active-session-list.tsx:1`。因此“会话”作为产品文案可保留，但内部命名需要标出它不是 RuntimeSession owner。
- `LifecyclePages.tsx` 的 Run 页把 RuntimeSession 明确标为 `Runtime Traces`，并导航到 `/session/:id`，见 `packages/app-web/src/pages/LifecyclePages.tsx:84`，这符合 trace drill-down 语义。

### 定义态命名问题

1. `packages/app-web/src/types/workflow.ts:229`
   - 当前命名：`WorkflowTemplateWorkflow`
   - 建议命名：`WorkflowTemplateAgentProcedure` 或 `WorkflowTemplateProcedure`
   - 为什么：该结构只有 `key/name/description/contract`，contract 类型是 `AgentProcedureContract`，不是 graph topology。Lifecycle 不变量要求 `WorkflowGraph` 表示 Activity graph definition，`AgentProcedure` 表示单个 Agent Activity 的 behavior / capability / context / hook contract；这个局部类型名会让 template 里的 procedure 被误读成 graph definition。

2. `packages/app-web/src/types/workflow.ts:241`
   - 当前命名：`workflows: WorkflowTemplateWorkflow[]`
   - 建议命名：`procedures: WorkflowTemplateAgentProcedure[]`
   - 为什么：template 里的 `lifecycle` 包含 `entry_activity_key` / `activities` / `transitions`，也就是 WorkflowGraph definition 形态；`workflows` 数组保存的是 AgentProcedure contract。字段名若保持 `workflows`，会把“一个 WorkflowGraph + 多个 activity procedures”的资产组合说成“多个 workflows + 一个 lifecycle”。

3. `packages/app-web/src/stores/workflowStore.ts:35`
   - 当前命名：`WorkflowEditorDraft`
   - 建议命名：`AgentProcedureEditorDraft`
   - 为什么：这个 draft 由 `definitionToDraft(definition: AgentProcedure)` 生成，保存 `AgentProcedureContract`，见 `packages/app-web/src/stores/workflowStore.ts:234`。`workflowStore` 管理 WorkflowGraph 没问题，但此局部 draft 是 AgentProcedure，不是 WorkflowGraph editor draft。

4. `packages/app-web/src/stores/workflowStore.ts:193`
   - 当前命名：`workflowDraftsByActivityKey`
   - 建议命名：`procedureDraftsByActivityKey`
   - 为什么：索引 key 是 activity key，值是 activity 对应的 AgentProcedure draft。`workflowDraftsByActivityKey` 会暗示每个 activity 挂一张 WorkflowGraph；实际不变量是 Agent executor 引用 procedure / procedure policy，不把整张 graph topology 塞进 procedure。

5. `packages/app-web/src/stores/workflowStore.ts:187`
   - 当前注释：`每个 activity 关联的 workflow contract`
   - 建议注释：`每个 Agent activity 关联的 AgentProcedure contract`
   - 为什么：这里描述的是 activity executor 的 procedure contract，不是 WorkflowGraph contract。注释应把 WorkflowGraph definition 与 AgentProcedure behavior contract 分开。

6. `packages/app-web/src/stores/workflowStore.ts:513`
   - 当前注释：`拉取项目下所有 workflow definitions（用于 agent activity executor.procedure_key -> contract 映射）`
   - 建议注释：`拉取项目下所有 AgentProcedure definitions（用于 agent activity executor.procedure_key -> contract 映射）`
   - 为什么：这条注释已经点出 `procedure_key`，但仍把被拉取对象叫 workflow definitions。应明确这里不是读取 WorkflowGraph 列表。

7. `packages/app-web/src/stores/workflowStore.ts:1061`
   - 当前注释：`先 upsert 每个 Agent activity 关联的 workflow contract`
   - 建议注释：`先 upsert 每个 Agent activity 关联的 AgentProcedure contract`
   - 为什么：保存顺序本身合理：先保存 procedure，再把 activity.executor.procedure_key 写回 WorkflowGraph。注释应表达这个 definition 边界。

8. `packages/app-web/src/features/workflow/model/lifecycle-port-sync.ts:30`
   - 当前命名：`workflowForStep(step, workflowByKey)`，参数 `workflows: AgentProcedure[]`
   - 建议命名：`procedureForActivity(activity, procedureByKey)`，参数 `procedures: AgentProcedure[]`
   - 为什么：函数查找的是 `step.executor.procedure_key` 对应的 `AgentProcedure`，不是 WorkflowGraph。Artifact edge 的 port 同步应表达“Activity port 从 AgentProcedure contract 补齐”。

9. `packages/app-web/src/features/workflow/ui/activity-inspector-sections.tsx:333`
   - 当前命名：`availableWorkflows: AgentProcedure[]`
   - 建议命名：`availableProcedures: AgentProcedure[]`
   - 为什么：UI 选择的是 executor 的 `procedure_key`，见 `packages/app-web/src/features/workflow/ui/activity-inspector-sections.tsx:374`。用 `availableWorkflows` 会把 AgentProcedure 列表误导成可引用的 WorkflowGraph 列表。

10. `packages/app-web/src/features/workflow/ui/activity-inspector-sections.tsx:345`
    - 当前命名 / UI 文案：`Workflow 来源`、`Workflow Key`、`引用 Workflow`
    - 建议命名 / UI 文案：`Procedure 来源`、`Procedure Key`、`引用 Procedure`
    - 为什么：该 panel 在编辑 Agent activity executor 的 `procedure_key`，不是在引用另一张 WorkflowGraph。WorkflowGraph 是当前 DAG definition；被 activity 引用的是 AgentProcedure。

11. `packages/app-web/src/features/workflow/ui/lifecycle-dag-canvas.tsx:102`
    - 当前命名：`workflowDefs: AgentProcedure[]`、`wfMap`、`wf`
    - 建议命名：`procedureDefs: AgentProcedure[]`、`procedureByKey`、`procedure`
    - 为什么：canvas 用这些值为 agent activity label 查 procedure name，不参与 graph definition 本身。局部命名应表达 procedure lookup，避免 DAG graph 与 procedure catalog 都叫 workflow。

12. `packages/app-web/src/features/workflow/ui/activity-inspector.tsx`
    - 当前命名：`workflowDraft`、`onWorkflowChange`、`availableWorkflows`
    - 建议命名：`procedureDraft`、`onProcedureDraftChange`、`availableProcedures`
    - 为什么：inspector 一边编辑 ActivityDefinition，一边编辑 AgentProcedure contract。把 procedure draft 叫 workflow draft 会模糊“WorkflowGraph definition”和“Agent activity behavior contract”的编辑边界。

13. `packages/app-web/src/features/workflow/lifecycle-editor-shell.tsx:50`
    - 当前命名：`workflowDraftsByActivityKey`、`allWorkflowDefs`
    - 建议命名：`procedureDraftsByActivityKey`、`allProcedureDefs`
    - 为什么：shell 从 store 取的是 activity 关联的 AgentProcedure drafts 和 project AgentProcedure 列表。这里不需要改 `workflowStore` 名称，而是把局部 catalog / draft 名称从 workflow 改成 procedure。

### 运行态命名问题

1. `packages/app-web/src/types/workflow.ts:94`
   - 当前命名：`WorkflowRunStatus = LifecycleRunStatus`
   - 建议命名：移除别名，直接使用 `LifecycleRunStatus`
   - 为什么：`LifecycleRun` 不是单个 WorkflowGraph 的 run；给 `LifecycleRunStatus` 套 `WorkflowRunStatus` 会把 control ledger 说成 workflow run。运行态 status 应跟 LifecycleRun 绑定。

2. `packages/app-web/src/types/workflow.ts:284`
   - 当前命名：`WorkflowRun`
   - 建议命名：`LifecycleRunSummary`、`LegacyLifecycleRunDto`，或直接以 generated `LifecycleRunView` 替代
   - 为什么：结构包含 `root_graph_id` 和 `execution_log`，但没有 `workflow_graph_instances`。目标 run contract 是 `LifecycleRunView`，同一个 run 可包含多个 WorkflowGraphInstance；`WorkflowRun` 名称会强化“一个 workflow graph 的 run”的旧心智。

3. `packages/app-web/src/services/workflow.ts:467`
   - 当前命名：`mapWorkflowRun`
   - 建议命名：`mapLifecycleRunSummary` 或删除并改用 generated `LifecycleRunView`
   - 为什么：mapper 从 raw run 映射出 `WorkflowRun`，错误点不在 map 行为，而在 run 类型语义。Human decision command 属于 `LifecycleRun + graph_instance + activity attempt`。

4. `packages/app-web/src/services/workflow.ts:589`
   - 当前命名：`submitHumanDecision(...): Promise<WorkflowRun>`
   - 建议命名：返回 `Promise<LifecycleRunView>`；函数名可保持 `submitHumanDecision`，或更明确为 `submitActivityHumanDecision`
   - 为什么：URL 已包含 `/lifecycle-runs/{run_id}/graph-instances/{graph_instance_id}/activities/{activity_key}/attempts/{attempt}`，见 `packages/app-web/src/services/workflow.ts:599`。返回 `WorkflowRun` 会让 command 结果脱离 run view 的 graph instance / agent / frame / runtime trace projection。

5. `packages/app-web/src/features/workflow/shared-labels.ts:20`
   - 当前命名：`RUN_STATUS_LABEL: Record<WorkflowRunStatus, string>`
   - 建议命名：`LIFECYCLE_RUN_STATUS_LABEL: Record<LifecycleRunStatus, string>`
   - 为什么：UI status label 对应 LifecycleRunStatus，不是 WorkflowGraphInstance status。显式命名能避免后续把 graph instance status 与 run status 复用到同一 label map。

6. `packages/app-web/src/features/task/task-subject-execution-panel.tsx:62`
   - 当前命名 / 展示：`Latest Attempt` 只显示 `{activity_key} #{attempt} · {status}`
   - 建议命名 / 展示：`Latest Activity Attempt`，并同时显示 `graph_instance_id` 的短标识，例如 `graph <id8> · activity_key #attempt`
   - 为什么：Activity runtime identity 必须以 `graph_instance_id + activity_key` 为 namespace。只显示 activity_key + attempt 在多 graph instance run 下不够准确。

7. `packages/app-web/src/features/story/story-subject-execution-panel.tsx:62`
   - 当前命名 / 展示：`Latest Attempt` 只显示 `{activity_key} #{attempt} · {status}`
   - 建议命名 / 展示：`Latest Activity Attempt`，并补充 `graph_instance_id`
   - 为什么：SubjectExecution 是业务 subject 投影，但 latest attempt 仍属于 graph instance scoped activity attempt。

8. `packages/app-web/src/pages/LifecyclePages.tsx:49`
   - 当前命名 / UI 文案：`graph {run.workflow_graph_instances.length}`
   - 建议命名 / UI 文案：`graph instance {count}` 或 `instances {count}`
   - 为什么：run view 里展示的是 `workflow_graph_instances`，不是 WorkflowGraph definition 数量。LifecycleRun 可以包含多个 WorkflowGraphInstance，UI 文案应突出 instance。

9. `packages/app-web/src/pages/LifecyclePages.tsx:193`
   - 当前命名 / 展示：Frame Runtime 只显示 `frame_id` 与 `activity_key`
   - 建议命名 / 展示：同时显示 `graph_instance_id`，标签为 `graph instance`
   - 为什么：`AgentFrameRuntimeView` generated contract 有 `graph_instance_id?: string`。Frame 与 Activity attempt 的桥接应保留 graph instance 维度，否则 activity_key 在多 graph instance 下不唯一。

10. `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:36`
    - 当前命名：`workflowRuns: LifecycleRunView[]`
    - 建议命名：`lifecycleRuns: LifecycleRunView[]`
    - 为什么：prop 类型是 `LifecycleRunView[]`。在 workspace session context 中继续叫 `workflowRuns`，会把 LifecycleRun projection 误读为 WorkflowGraph execution list；应把 `activeWorkflow` metadata 与 `activeRun` 分开命名。

11. `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:70`
    - 当前命名：`resolveActiveRun(workflowRuns, activeWorkflow)`
    - 建议命名：`resolveActiveLifecycleRun(lifecycleRuns, activeWorkflowMetadata)`
    - 为什么：`activeWorkflow` 来自 hook metadata，`workflowRuns` 实际是 LifecycleRunView array。命名应明确这一步是从 runtime metadata 反查 lifecycle run projection，而不是选择一个 WorkflowGraph definition。

12. `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:86`
    - 当前逻辑 / 命名：`collectAttempts(run)` 跨 `run.workflow_graph_instances.flatMap(...)` 收集 attempts
    - 建议命名：若保持跨 instance 收集，命名为 `collectRunActivityAttempts`；在展示 label 时附带 `graph_instance_id`
    - 为什么：该逻辑正确遍历 graph instances，但名称和后续 label 容易淡化 graph instance namespace。Activity attempt 不是 run 内按 activity_key 全局唯一。

### 用户会话视角与 RuntimeSession 混用

1. `packages/app-web/src/features/agent/active-session-list.tsx:1`
   - 当前命名：`ActiveSessionList`
   - 建议命名：`ActiveConversationList`、`ActiveLifecycleSessionList` 或 `ActiveAgentRunList`
   - 为什么：文件注释说明 UI “会话列表”是用户视角，底层数据由 lifecycle run -> agent -> runtime_session_ref 驱动，见 `packages/app-web/src/features/agent/active-session-list.tsx:4`。用户文案可保留“会话”，但组件名需要限定它不是 RuntimeSession list。

2. `packages/app-web/src/features/agent/lifecycle-grouping.ts:13`
   - 当前命名：`SessionEntry`
   - 建议命名：`LifecycleSessionEntry`、`AgentConversationEntry` 或 `RunAgentEntry`
   - 为什么：该 entry 持有 `run: LifecycleRunView`、`agent: LifecycleAgentView` 与可选 `deliveryRuntimeSessionId`，不是 RuntimeSession event/thread entry。项目内另有 session UI 的 `SessionEntry` 组件，命名碰撞会让阅读者误判实体来源。

3. `packages/app-web/src/features/agent/lifecycle-grouping.ts:21`
   - 当前命名：`SessionGroupKind` / `SessionGroup`
   - 建议命名：`LifecycleSessionGroupKind` / `LifecycleSessionGroup`，或 `SubjectConversationGroup`
   - 为什么：分组依据是 lifecycle `subject_associations`，见 `packages/app-web/src/features/agent/lifecycle-grouping.ts:55`。命名应表达 subject-scoped lifecycle grouping，而不是 RuntimeSession grouping。

4. `packages/app-web/src/features/agent/active-session-list.tsx:307`
   - 当前命名：`sessionTitle`
   - 建议命名：`runtimeSessionTitle` 或 `conversationTitle`
   - 为什么：值来自 `SessionMeta`，见 `packages/app-web/src/features/agent/active-session-list.tsx:325`。如果表示 RuntimeSession meta 字段，应叫 `runtimeSessionTitle`；如果表示用户侧会话列表标题，应叫 `conversationTitle`。当前命名夹在两者之间。

5. `packages/app-web/src/stores/lifecycleStore.ts:39`
   - 当前命名：`sessionMetas: Map<string, SessionMeta>`
   - 建议命名：`runtimeSessionMetas`
   - 为什么：key 是 `runtime_session_id`，注释也说明用于 session title。Lifecycle store 内部如果保留 SessionMeta 缓存，应把 RuntimeSession 技术事实显式化，避免看起来像 lifecycle session meta。

6. `packages/app-web/src/stores/lifecycleStore.ts:65`
   - 当前命名：`hydrateSessionMetas(sessionIds: string[])`
   - 建议命名：`hydrateRuntimeSessionMetas(runtimeSessionIds: string[])`
   - 为什么：调用方传入的是 `agent.delivery_runtime_ref.runtime_session_id`，见 `packages/app-web/src/stores/lifecycleStore.ts:189`。参数名和 action 名都应标出 runtime session。

7. `packages/app-web/src/stores/lifecycleStore.ts:73`
   - 当前命名：`deliveryRuntimeSessionId(runId)`
   - 建议命名：`primaryDeliveryRuntimeSessionIdForRun(runId)` 或 `runtimeTraceSessionIdForRunNavigation(runId)`
   - 为什么：函数遍历 run 下 agents，返回第一个 `delivery_runtime_ref`，见 `packages/app-web/src/stores/lifecycleStore.ts:276`。名称应表达这是导航/展示用的 delivery runtime trace，不是 run 的主键、owner 或唯一 runtime session。

8. `packages/app-web/src/features/agent/agent-tab-view.tsx:71`
   - 当前行为 / 命名：launch ProjectAgent 后拿 `runtimeSessionId` 并 `navigate(/session/:id)`
   - 建议命名：变量改为 `deliveryRuntimeTraceSessionId`，或导航辅助命名为 `openRuntimeTrace`
   - 为什么：`/session/:id` 根据 spec 是 RuntimeTraceView，不是业务 runtime root。launch 结果的 `run_ref` / `agent_ref` / `frame_ref` 才是 lifecycle 主链路，RuntimeSession 只是 trace evidence。

9. `packages/app-web/src/components/layout/SessionShortcutList.tsx:59`
   - 当前命名：`SessionShortcutList`
   - 建议命名：`RuntimeTraceShortcutList` 或 `LifecycleSessionShortcutList`
   - 为什么：组件从 `LifecycleRunView` / `LifecycleAgentView` 推导 `deliveryRuntimeSessionId`，再导航到 `/session/:sessionId`，见 `packages/app-web/src/components/layout/SessionShortcutList.tsx:83` 和 `packages/app-web/src/components/layout/SessionShortcutList.tsx:91`。若它是 trace shortcut，就应显式 RuntimeTrace；若它是用户会话 shortcut，就应显式 LifecycleSession。

10. `packages/app-web/src/components/layout/SessionShortcutList.tsx:78`
    - 当前命名：`sessionEntries`
    - 建议命名：`runtimeTraceShortcuts` 或 `lifecycleSessionShortcuts`
    - 为什么：entry 的 primary key 是 `runtimeSessionId`，同时带 `runId`，见 `packages/app-web/src/components/layout/SessionShortcutList.tsx:97`。当前名容易让 RuntimeSession 成为业务 entry owner。

11. `packages/app-web/src/services/lifecycle.ts:45`
    - 当前命名：`fetchSessionFrameRuntime(runtimeSessionId)`
    - 建议命名：`fetchFrameRuntimeByRuntimeSession(runtimeSessionId)` 或 `fetchAgentFrameRuntimeByTrace`
    - 为什么：已有标准 API `fetchAgentFrameRuntime(frameId)`，见 `packages/app-web/src/services/lifecycle.ts:35`。通过 RuntimeSession 查 frame runtime 是 trace adapter 入口，命名应避免看起来像 RuntimeSession 拥有 frame runtime。

12. `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:1`
    - 当前命名 / 注释：`Session Runtime State`、`通过 /sessions/{id}/frame-runtime 直接查询`
    - 建议命名 / 注释：`RuntimeTraceFrameState` 或 `FrameRuntimeBySessionTraceState`
    - 为什么：hook 通过 runtime session id 找 frame runtime view，见 `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:73`。这是 RuntimeSession trace adapter，不是 lifecycle business owner。

13. `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:27`
    - 当前命名：`SessionRuntimeProjectionState`
    - 建议命名：`RuntimeTraceFrameProjectionState`
    - 为什么：state 里真正有业务意义的对象是 `frame: AgentFrameRuntimeView | null`，`session_id` 只是查询入口，见 `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:27`。命名应避免 RuntimeSession projection 变成 lifecycle owner。

14. `packages/app-web/src/features/workflow/ui/panels/InjectionPanel.tsx:38`
    - 当前命名 / UI 文案：`Session 指引`
    - 建议命名 / UI 文案：`Agent 指引` 或 `Procedure 指引`
    - 为什么：字段位于 `WorkflowInjectionSpec`，实际被 AgentProcedure contract 消费。若叫 Session 指引，容易让用户以为它绑定 RuntimeSession，而不是 activity procedure 的 injection guidance。

15. `packages/app-web/src/features/workflow/ui/panels/shared.ts:21`
    - 当前命名 / UI 文案：`before_stop: "Session 结束前"`、`session_terminal: "Session 终态"`
    - 建议命名 / UI 文案：`Runtime 结束前` / `Runtime 终态`，或若 hook contract 确认只针对 RuntimeSession，则用 `RuntimeSession 结束前` / `RuntimeSession 终态`
    - 为什么：hook trigger enum 仍是 `session_terminal` 等 wire contract，但 UI 层可以避免把 runtime terminal 当 Lifecycle terminal。Lifecycle terminal 应由 ActivityEvent / LifecycleRun status 推进，RuntimeSession terminal 只是 executor trace terminal。

### 可以立即小范围清理

1. `packages/app-web/src/stores/workflowStore.ts:187`
   - 当前命名：`workflow contract` 注释
   - 建议命名：`AgentProcedure contract`
   - 为什么：只改注释即可让 WorkflowGraph definition 与 activity procedure contract 分开，不影响 `workflowStore` 本身命名。

2. `packages/app-web/src/features/workflow/model/lifecycle-port-sync.ts:30`
   - 当前命名：`workflowForStep`、`workflowByKey`、`workflows`
   - 建议命名：`procedureForActivity`、`procedureByKey`、`procedures`
   - 为什么：纯局部 helper 与参数改名，不触碰 API；能立刻把 AgentProcedure 与 WorkflowGraph 的定义态边界说清楚。

3. `packages/app-web/src/features/workflow/ui/activity-inspector-sections.tsx:333`
   - 当前命名：`availableWorkflows`
   - 建议命名：`availableProcedures`
   - 为什么：prop 局部跨组件范围较小，配合 UI 文案 `Workflow 来源` -> `Procedure 来源` 可以立即降低误读。

4. `packages/app-web/src/features/workflow/ui/activity-inspector-sections.tsx:345`
   - 当前命名 / UI 文案：`Workflow 来源`、`Workflow Key`、`引用 Workflow`
   - 建议命名 / UI 文案：`Procedure 来源`、`Procedure Key`、`引用 Procedure`
   - 为什么：不需要后端改约定；只是让 activity executor 的 `procedure_key` 在 UI 中回到真实实体。

5. `packages/app-web/src/features/workflow/ui/panels/InjectionPanel.tsx:38`
   - 当前命名 / UI 文案：`Session 指引`
   - 建议命名 / UI 文案：`Agent 指引` 或 `Procedure 指引`
   - 为什么：字段归属 AgentProcedure injection；文案调整不改变 wire field，能减少用户把 guidance 误解成 RuntimeSession property 的概率。

6. `packages/app-web/src/features/workflow/ui/panels/shared.ts:21`
   - 当前命名 / UI 文案：`Session 结束前`、`Session 终态`
   - 建议命名 / UI 文案：`Runtime 结束前`、`Runtime 终态`
   - 为什么：wire enum 不动，仅 UI label 调整；能把 hook trigger 与 Lifecycle terminal 区分开。

7. `packages/app-web/src/features/agent/lifecycle-grouping.ts:13`
   - 当前命名：`SessionEntry`
   - 建议命名：`LifecycleSessionEntry` 或 `RunAgentEntry`
   - 为什么：该文件范围小，改名能避免和 `features/session/ui/SessionEntry` 产生跨 feature 心智碰撞。

8. `packages/app-web/src/features/agent/active-session-list.tsx:307`
   - 当前命名：`sessionTitle`
   - 建议命名：若继续来自 `SessionMeta`，用 `runtimeSessionTitle`；若强调用户侧列表，用 `conversationTitle`
   - 为什么：局部变量与 prop 名可小范围改，帮助维护者分清 RuntimeSession meta 与用户侧会话标题。

9. `packages/app-web/src/pages/LifecyclePages.tsx:49`
   - 当前命名 / UI 文案：`graph {count}`
   - 建议命名 / UI 文案：`instances {count}` 或 `graph instance {count}`
   - 为什么：只改展示文案即可贴合 run 内多个 WorkflowGraphInstance 的不变量。

10. `packages/app-web/src/features/task/task-subject-execution-panel.tsx:62`
    - 当前命名 / UI 文案：`Latest Attempt`
    - 建议命名 / UI 文案：`Latest Activity Attempt`，显示 graph instance 短 id
    - 为什么：只改投影展示，不改命令；能补齐 activity attempt namespace。

11. `packages/app-web/src/features/story/story-subject-execution-panel.tsx:62`
    - 当前命名 / UI 文案：`Latest Attempt`
    - 建议命名 / UI 文案：`Latest Activity Attempt`，显示 graph instance 短 id
    - 为什么：同 Task panel，且两处可以保持一致。

12. `packages/app-web/src/components/layout/SessionShortcutList.tsx:78`
    - 当前命名：`sessionEntries`
    - 建议命名：`runtimeTraceShortcuts` 或 `lifecycleSessionShortcuts`
    - 为什么：局部 collection 改名能明确这是从 lifecycle run / agent projection 推导出的 shortcut，而不是 RuntimeSession 自己拥有业务列表。

### 需要计划性重构

1. `packages/app-web/src/types/workflow.ts:284` 与 `packages/app-web/src/services/workflow.ts:467`
   - 当前命名：`WorkflowRun` / `mapWorkflowRun`
   - 建议命名：以 generated `LifecycleRunView` 替代，或先收敛为 `LifecycleRunSummary`
   - 为什么：这涉及 `submitHumanDecision` 返回 contract、status label 类型和调用方预期。目标 API spec 已写 `submitHumanDecision(input): Promise<LifecycleRunView>`；应作为 contract/API 计划性收敛，而不是只在前端改别名。

2. `packages/app-web/src/services/workflow.ts:589`
   - 当前命名 / 返回：`submitHumanDecision(...): Promise<WorkflowRun>`
   - 建议命名 / 返回：`Promise<LifecycleRunView>`
   - 为什么：human decision 的 command path 已包含 `run_id + graph_instance_id + activity_key + attempt`，返回值应回到完整 run view，避免 command 之后丢失 graph instance/agent/frame/runtime trace projection。需要确认后端 route 返回体并配合 generated contract。

3. `packages/app-web/src/stores/workflowStore.ts:35`
   - 当前命名：`WorkflowEditorDraft` / `workflowDraftsByActivityKey`
   - 建议命名：`AgentProcedureEditorDraft` / `procedureDraftsByActivityKey`
   - 为什么：这是 workflowStore 内的公共 editor API，涉及 `lifecycle-editor-shell`、`activity-inspector`、DAG canvas、测试和调用方。注意：这里不是要求改 `workflowStore` 名称；store 仍管理 WorkflowGraph，计划性重构只针对 activity procedure draft 这条局部边界。

4. `packages/app-web/src/types/workflow.ts:229`
   - 当前命名：`WorkflowTemplateWorkflow` / `WorkflowTemplate.workflows`
   - 建议命名：`WorkflowTemplateAgentProcedure` / `WorkflowTemplate.procedures`
   - 为什么：template shape 可能对应后端/shared library payload。若 payload 仍是 `workflows` wire field，需要先确认是否要更新 contract 或仅在 UI type entrypoint 做 view model rename。

5. `packages/app-web/src/stores/lifecycleStore.ts:39`
   - 当前命名：`sessionMetas` / `hydrateSessionMetas`
   - 建议命名：`runtimeSessionMetas` / `hydrateRuntimeSessionMetas`
   - 为什么：这属于 lifecycle store public API，并被 `ActiveSessionList`、layout session shortcut 等跨 feature 消费。建议和“用户会话列表”命名一起规划，确保 RuntimeSession trace 与 conversation list 两条语义都清楚。

6. `packages/app-web/src/features/agent/active-session-list.tsx:285`
   - 当前命名：`ActiveSessionList`
   - 建议命名：`ActiveLifecycleSessionList`、`ActiveConversationList` 或 `ActiveAgentRunList`
   - 为什么：组件承担用户会话入口，不是 RuntimeSession browser。若项目产品语言决定继续叫“会话”，代码名应加 `Lifecycle` / `Conversation` 限定；如果产品也要消除 Session 歧义，则需要同步路由、侧栏、快捷入口和文案。

7. `packages/app-web/src/components/layout/SessionShortcutList.tsx:59`
   - 当前命名：`SessionShortcutList`
   - 建议命名：`RuntimeTraceShortcutList` 或 `LifecycleSessionShortcutList`
   - 为什么：侧栏 shortcut 同时承担用户会话入口和 `/session/:id` runtime trace 导航。应先定产品语义，再统一组件名、entry 名和导航参数名。

8. `packages/app-web/src/services/lifecycle.ts:45`
   - 当前命名：`fetchSessionFrameRuntime`
   - 建议命名：`fetchFrameRuntimeByRuntimeSession`
   - 为什么：这是 trace adapter API，不应成为 frame runtime 的主入口。需要盘点其它调用方，避免让通过 runtime session 查 frame 的路径扩散。

9. `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:27`
   - 当前命名：`SessionRuntimeProjectionState`
   - 建议命名：`RuntimeTraceFrameProjectionState`
   - 为什么：hook 以 RuntimeSession id 查询 AgentFrameRuntimeView。要避免 RuntimeSession 被误当业务 owner，需要和 workspace panel 的 session context 命名一起调整。

10. `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:36`
    - 当前命名：`workflowRuns: LifecycleRunView[]`
    - 建议命名：`lifecycleRuns: LifecycleRunView[]`
    - 为什么：该 prop 可能跨 workspace runtime/model 类型传递。应计划性调整，确保 WorkflowGraph definition、LifecycleRun runtime projection、hook metadata 中的 active workflow 三个概念各自清楚。

## External References

- 未使用外部网络资料。本次研究只依据项目内 spec、generated contracts 与前端源码。
- Generated contract baseline observed in `packages/app-web/src/generated/workflow-contracts.ts` includes `LifecycleRunView` with `workflow_graph_instances`, `RuntimeSessionRefDto`, `RuntimeSessionTraceView`, `AgentFrameRuntimeView.graph_instance_id?`, and `SubjectExecutionView.latest_attempt?`.

## Related Specs

- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/state-management.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this sub-agent environment. The task directory was explicitly provided by the user, so this research was written to `.trellis/tasks/06-03-lifecycle-concept-naming-cleanup/research/`.
- 未修改任何业务代码；只更新本 research markdown。
- 本次未跑 frontend tests/type-check，因为任务是只读命名盘点，不涉及实现变更。
- 本文件已按用户澄清修正：`workflowStore` 管理 `WorkflowGraph` 的 store 名称本身不列为清理问题；建议只针对局部 AgentProcedure draft/catalog 命名、注释和 UI 文案。
- `active-session-list.tsx` 的“会话”文案不是直接判定为错误：文件注释明确这是用户视角。问题主要在内部类型名与 RuntimeSession trace 命名缺少限定词。
