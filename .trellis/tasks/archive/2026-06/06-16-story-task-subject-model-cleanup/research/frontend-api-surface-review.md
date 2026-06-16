# Research: frontend-api-surface-review

- Query: 根据项目实际代码审查 Story/Task/Subject 模型清理任务在前端、TypeScript DTO、状态管理和 UI 消费面的迁移影响。
- Scope: internal
- Date: 2026-06-16

## Findings

### 实际检查过的关键文件路径

任务与规范：

- `.trellis/workflow.md` - Trellis 任务与 research artifact 约束。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/prd.md` - 本任务目标语义：Story 是 subject/context，Task 是 LifecycleRun 控制树 Todo facts / plan item。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/design.md` - Story-bound AgentRun、Task assignment、SubjectExecutionView、UI 入口设计。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/implement.md` - 阶段顺序、前端风险文件与验证命令。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/task.json` - 当前任务状态。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/implement.jsonl` - 已配置的实现上下文。
- `.trellis/tasks/06-16-story-task-subject-model-cleanup/check.jsonl` - 已配置的检查上下文。
- `.trellis/spec/index.md` - spec 必读顺序与文档分层。
- `.trellis/spec/project-overview.md` - Project、SubjectRef、LifecycleRun、RuntimeSession 的顶层抽象。
- `.trellis/spec/tech-stack.md` - 前端、DTO、测试命令技术基线。
- `.trellis/spec/communication.md` - 中文沟通与提交格式。
- `.trellis/spec/frontend/index.md` - 前端 spec 索引。
- `.trellis/spec/frontend/architecture.md` - 前端架构与 generated DTO 边界。
- `.trellis/spec/frontend/type-safety.md` - generated wire 单源、禁止字段别名兼容。
- `.trellis/spec/frontend/state-management.md` - Zustand store 边界与 lifecycle/session 状态事实源。
- `.trellis/spec/frontend/hook-guidelines.md` - 流式 hook 与事件聚合约定。
- `.trellis/spec/frontend/component-guidelines.md` - model/ui 分离与组件约束。
- `.trellis/spec/shared/index.md` - 共享命名规范。
- `.trellis/spec/cross-layer/index.md` - 跨层契约索引。
- `.trellis/spec/cross-layer/architecture.md` - Rust contract type -> generated TS 的跨层 invariant。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - generated contract、drift check 与 DTO 迁移优先级。
- `.trellis/spec/backend/story-task-runtime.md` - 当前 Story/Task/SubjectContext/Lifecycle projection 基线。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - LifecycleSubjectAssociation 与 subject projection 契约。

前端与 generated code：

- `packages/app-web/src/generated/story-contracts.ts` - StoryResponse / StoryStatus generated DTO。
- `packages/app-web/src/generated/task-contracts.ts` - TaskResponse / TaskStatus / TaskDispatchPreference / Artifact generated DTO。
- `packages/app-web/src/generated/workflow-contracts.ts` - SubjectExecutionView、LifecycleSubjectAssociationDto、AgentRunWorkspace* generated DTO。
- `packages/app-web/src/generated/project-agent-contracts.ts` - CreateProjectAgentRunRequest.subject_ref 与 SubjectRefDto。
- `packages/app-web/src/types/index.ts` - 前端类型入口，对 Story/Task generated DTO 的 re-export/wrapper。
- `packages/app-web/src/types/lifecycle-views.ts` - SubjectExecutionView / SubjectRefDto re-export 与 subjectExecutionKey。
- `packages/app-web/src/types/session.ts` - SubjectRunContext、SessionTaskContext、StoryNavigationState。
- `packages/app-web/src/types/context.ts` - TaskSessionExecutorSummary、SessionOwnerContext 等旧 task/session context 类型。
- `packages/app-web/src/services/story.ts` - Story/Task API client、payload guard、旧 Task 状态集合。
- `packages/app-web/src/services/lifecycle.ts` - `/subjects/{kind}/{id}/execution`、AgentRun workspace API。
- `packages/app-web/src/services/project.ts` - ProjectAgent AgentRun start API。
- `packages/app-web/src/stores/storyStore.ts` - storiesByProjectId、tasksByStoryId、task state change reducer。
- `packages/app-web/src/stores/storyViewStore.ts` - Story board/list UI 状态。
- `packages/app-web/src/stores/lifecycleStore.ts` - lifecycleRuns、subjectExecutions 与 subject association selector。
- `packages/app-web/src/stores/eventStore.ts` - Project event stream publish 入口。
- `packages/app-web/src/pages/StoryPage.tsx` - Story detail、Story Task rows、TaskDrawer composition。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` - AgentRun workspace owner/subject display、run_context -> Story/Task 回跳。
- `packages/app-web/src/features/story/create-task-panel.tsx` - Story 下创建 Task 与 dispatch_preference/context_sources 表单。
- `packages/app-web/src/features/story/story-subject-execution-panel.tsx` - Story SubjectExecution UI。
- `packages/app-web/src/features/story/story-tab-view.tsx` - Story 列表页 task_count 消费。
- `packages/app-web/src/features/story/story-board.tsx` - Story status board 与拖拽状态推进。
- `packages/app-web/src/features/story/story-toolbar.tsx` - Story status filter 文案。
- `packages/app-web/src/features/story/story-keyboard.ts` - Story 快捷键状态推进。
- `packages/app-web/src/features/story/next-step.ts` - Story 下一步状态流。
- `packages/app-web/src/features/story/select-stories.ts` - Story active/done scope 过滤。
- `packages/app-web/src/features/story/story-list-view.tsx` - Story 列表、创建文案与 task_count 展示。
- `packages/app-web/src/features/task/task-drawer.tsx` - Task 编辑、SubjectExecution、artifacts、dispatch preference。
- `packages/app-web/src/features/task/task-subject-execution-panel.tsx` - Task SubjectExecution UI。
- `packages/app-web/src/features/task/task-card.tsx` - Task card 状态/agent 展示。
- `packages/app-web/src/features/task/task-list.tsx` - Story 内 Task list empty state。
- `packages/app-web/src/features/task/dispatch-preference.ts` - TaskDispatchPreference normalize/default/validation。
- `packages/app-web/src/features/task/dispatch-preference-fields.tsx` - Task dispatch preference UI。
- `packages/app-web/src/features/agent/agent-run-grouping.ts` - AgentRun list 按 subject_ref 分组。
- `packages/app-web/src/features/agent/active-agent-run-list.tsx` - AgentRun list UI。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts` - draft start / composer command payload。
- `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx` - Task executor summary/source label 与 runtime overview。
- `packages/app-web/src/components/ui/status-badge.tsx` - Story/Task status badge 映射。
- `packages/app-web/package.json` - app-web typecheck/test/lint 脚本。
- `package.json` - contracts:check、frontend:check、e2e:test:critical 等验证脚本。

相关后端 contract/API 源码仅用于确认 generated TS 来源：

- `crates/agentdash-contracts/src/task/contract.rs` - TaskStatus、TaskResponse、TaskDispatchPreference、Artifact 生成源。
- `crates/agentdash-contracts/src/story/contract.rs` - StoryResponse、StoryStatus 生成源。
- `crates/agentdash-api/src/dto/task_execution.rs` - 旧 TaskExecutionViewResponse。
- `crates/agentdash-api/src/routes/task_execution.rs` - `/tasks/{id}/execution`。
- `crates/agentdash-api/src/routes/story_runs.rs` - `/stories/{id}/runs` 已返回 SubjectExecutionView。

测试：

- `tests/e2e/story-context-injection.spec.ts` - Story context refs 分配给 Task dispatch preference 的 E2E。
- `tests/e2e/task-agent-binding.spec.ts` - Task 创建/编辑 dispatch_preference 的 E2E。

### 当前前端事实

前端实际主路径是 `packages/app-web/src`。未发现 `frontend/src` 或 `shared/generated/types` 目录；generated DTO 实际位于 `packages/app-web/src/generated/*.ts`。

Story/Task wire DTO 仍明显保留旧模型。`TaskResponse` 仍要求 `story_id`、`dispatch_preference`、`artifacts`，状态是执行语言 `pending | assigned | running | awaiting_verification | completed | failed | cancelled`，见 `packages/app-web/src/generated/task-contracts.ts:7`、`packages/app-web/src/generated/task-contracts.ts:11`、`packages/app-web/src/generated/task-contracts.ts:13`、`packages/app-web/src/generated/task-contracts.ts:15`。这些字段来自 Rust contract，源头在 `crates/agentdash-contracts/src/task/contract.rs:9`、`crates/agentdash-contracts/src/task/contract.rs:74`、`crates/agentdash-contracts/src/task/contract.rs:102`，所以前端不能只改 UI 类型绕过。

Story generated DTO 仍带 `task_count`，Story status 仍包含 `executing/failed/cancelled` 这类容易被理解成 runtime truth 的状态词，见 `packages/app-web/src/generated/story-contracts.ts:10`、`packages/app-web/src/generated/story-contracts.ts:12` 和 `crates/agentdash-contracts/src/story/contract.rs:35`、`crates/agentdash-contracts/src/story/contract.rs:104`。这不一定要全部删除，但迁移时需要让 UI 文案明确 Story status 是人工/Agent command 推进的产品流程，不从 LifecycleRun 自动派生。

前端 `types/index.ts` 直接把 `Story = StoryResponse`，把 `Task = TaskResponse` 加一个 `thinking_level` 扩展后的 `dispatch_preference` wrapper，见 `packages/app-web/src/types/index.ts:43`、`packages/app-web/src/types/index.ts:53`、`packages/app-web/src/types/index.ts:55`。因此 Task contract 字段变更会直接穿透到 StoryPage、TaskDrawer、storyStore、测试和状态 badge。

`services/story.ts` 是 Story/Task API 的主要客户端边界，但它仍手写 payload guard 的旧枚举集合。Story 状态集合在 `packages/app-web/src/services/story.ts:20`，Task 旧执行状态集合在 `packages/app-web/src/services/story.ts:31`，Task payload guard 强制 `story_id`、`dispatch_preference` 和 `artifacts` 存在，见 `packages/app-web/src/services/story.ts:81`。Task CRUD 仍是 Story 下创建/查询：`POST /stories/{storyId}/tasks` 与 `GET /stories/{storyId}/tasks`，见 `packages/app-web/src/services/story.ts:185`、`packages/app-web/src/services/story.ts:212`；单 Task 更新/读取/删除是 `/tasks/{taskId}`，见 `packages/app-web/src/services/story.ts:196`、`packages/app-web/src/services/story.ts:204`、`packages/app-web/src/services/story.ts:208`。

`storyStore` 仍把 Task 缓存按 Story 聚合为 `tasksByStoryId`，见 `packages/app-web/src/stores/storyStore.ts:27`、`packages/app-web/src/stores/storyStore.ts:28`。upsert 使用 `task.story_id` 作为唯一归属 key，见 `packages/app-web/src/stores/storyStore.ts:95`；删除 Story 时直接删除对应 Task map entry，见 `packages/app-web/src/stores/storyStore.ts:282`；Task 创建/查询仍以 storyId 为入口，见 `packages/app-web/src/stores/storyStore.ts:305`、`packages/app-web/src/stores/storyStore.ts:367`。Project event reducer 还监听 `task_status_changed` 与 `task_artifact_added` 并尝试把 payload 映射回 Task entity，见 `packages/app-web/src/stores/storyStore.ts:448`。

Story detail UI 把 Task 当成 Story 下的执行队列。空态文案写着“当前 Story 暂无 Task。创建 Task 后，它会在这里以执行队列形式展示”，见 `packages/app-web/src/pages/StoryPage.tsx:126`；行列包含 `Agent` 与 `验收`，见 `packages/app-web/src/pages/StoryPage.tsx:141`；行内显示 `task.dispatch_preference.agent_type ?? preset_name`，见 `packages/app-web/src/pages/StoryPage.tsx:167`；Tasks section subtitle 是“验收状态直接随 Task 行展示”，见 `packages/app-web/src/pages/StoryPage.tsx:579`。删除 Story dialog 仍提示“Story 删除后其下 Task 会一起删除”，见 `packages/app-web/src/pages/StoryPage.tsx:646`。

Task 创建表单仍以 Story 为入口，并要求选择 Agent 类型或预设。`CreateTaskPanel` props 直接接收 `story` 和 `storyId`，见 `packages/app-web/src/features/story/create-task-panel.tsx:22`；提交时调用 `createTask(storyId, ...)` 并写入 `dispatch_preference.context_sources`，见 `packages/app-web/src/features/story/create-task-panel.tsx:81`；文案写“要交给 Agent 的具体动作”和“分配给 Task 运行上下文”，见 `packages/app-web/src/features/story/create-task-panel.tsx:129`、`packages/app-web/src/features/story/create-task-panel.tsx:172`。这与新模型中的 `create_tasks_from_plan` / `assign_tasks` / `fanout_tasks` 三个命令边界需要拆开。

TaskDrawer 同时展示和编辑计划项、Agent binding、运行状态、SubjectExecution 和 Task 自有 artifacts。它从 `task.artifacts` 计算 `sortedArtifacts`，见 `packages/app-web/src/features/task/task-drawer.tsx:81`；保存时仍更新 `dispatch_preference`，见 `packages/app-web/src/features/task/task-drawer.tsx:115`；左侧字段名叫“运行状态”，见 `packages/app-web/src/features/task/task-drawer.tsx:180`；右侧 SubjectExecution 描述写“当前 Agent、attempt 与产物投影”，见 `packages/app-web/src/features/task/task-drawer.tsx:247`；下方又直接展示“执行产物”，数据来自 Task entity，见 `packages/app-web/src/features/task/task-drawer.tsx:262`。迁移后 TaskDrawer 应聚焦 Todo plan fields 与 linked runs，artifacts/runtime detail 应只来自 SubjectExecution / Lifecycle projection。

SubjectExecution 收口在前端已有基础。`TaskSubjectExecutionPanel` 顶部注释已明确“Task 本身只作为 SubjectRef，运行状态由 lifecycle target view 投影”，见 `packages/app-web/src/features/task/task-subject-execution-panel.tsx:1`；它调用 `fetchSubjectExecution("task", task.id)`，见 `packages/app-web/src/features/task/task-subject-execution-panel.tsx:120`。`StorySubjectExecutionPanel` 调用 `fetchSubjectExecution("story", story.id)`，见 `packages/app-web/src/features/story/story-subject-execution-panel.tsx:132`。`lifecycleStore` 以 `subjectExecutions: Map<string, SubjectExecutionView>` 缓存并按 `subject_ref.kind/id` 建 key，见 `packages/app-web/src/stores/lifecycleStore.ts:38`、`packages/app-web/src/stores/lifecycleStore.ts:137`；selector 能按 `lifecycleRun.subject_associations` 查询 subject 关联 runs，见 `packages/app-web/src/stores/lifecycleStore.ts:246`。`services/lifecycle.ts` 的 subject execution endpoint 是 `/subjects/{subjectKind}/{subjectId}/execution`，见 `packages/app-web/src/services/lifecycle.ts:27`。

Generated workflow contract 已有目标 read model 的核心字段：`LifecycleSubjectAssociationDto.subject_ref`，见 `packages/app-web/src/generated/workflow-contracts.ts:217`；`SubjectExecutionView` 含 subject_ref、associations、runs、current_agent、latest_runtime_node、artifacts，见 `packages/app-web/src/generated/workflow-contracts.ts:253`；AgentRun workspace list entry 也有 `subject_ref` / `subject_label`，见 `packages/app-web/src/generated/workflow-contracts.ts:106`。`agent-run-grouping.ts` 已按后端 `subject_ref/subject_label` 分组 AgentRun，不依赖 storyStore，见 `packages/app-web/src/features/agent/agent-run-grouping.ts:1`、`packages/app-web/src/features/agent/agent-run-grouping.ts:32`。这部分应成为迁移后的主消费面。

ProjectAgent run start generated contract 支持 `subject_ref?: SubjectRefDto`，见 `packages/app-web/src/generated/project-agent-contracts.ts:18`、`packages/app-web/src/generated/project-agent-contracts.ts:28`。但当前 `useAgentRunWorkspaceCommands` 的 draft start 创建 ProjectAgentRun 时没有传 subject_ref，只传 input、client_command_id、executor_config，见 `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:235`。这说明 Story-bound AgentRun 的前端入口不在当前 StoryPage 明显落地，或尚未接入 draft start flow；模型迁移时要补齐“从 Story/Task subject 启动 AgentRun”的调用面，而不是继续靠 Task dispatch_preference 暗示执行者。

AgentRun workspace 已同时存在两套 subject/owner 信息来源：一套是后端 `subject_associations`，用于 identity label，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:198`；另一套是 hook runtime snapshot 的 `run_context`，类型含 `story_id/task_id/scope`，见 `packages/app-web/src/types/session.ts:7`、`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:229`。回跳逻辑仍依赖 `runContext.scope === "task"` 且必须有 `story_id + task_id` 才返回 Story 页面打开 TaskDrawer，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:295`、`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:492`。迁移时应优先让回跳由 `subject_ref` / association projection 驱动，避免 Task durable facts 脱离 Story aggregate 后仍强依赖 `story_id`。

WorkspacePanel 的 ContextOverview 仍保留旧 task executor 来源标签，例如 `"task.dispatch_preference.agent_type"` 与 `"task.dispatch_preference.preset_name"`，见 `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:50`。相关 summary 类型叫 `TaskSessionExecutorSummary`，字段包含 executor/model/preset/source，见 `packages/app-web/src/types/context.ts:81`；但 AgentRunWorkspacePage 当前把 `taskExecutorSummary` 固定为 `null`，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:225`。这是一个隐蔽的旧 surface：没有当前主路径数据不代表可以不迁移命名和 source label。

Story board/list 是另一类 UI 风险。Story status board 的列顺序仍包括 `executing/decomposed/failed/cancelled`，见 `packages/app-web/src/features/story/story-board.tsx:26`；拖拽直接调用 `updateStory(storyId, { status: targetStatus })`，见 `packages/app-web/src/features/story/story-board.tsx:118`；下一步状态流是 `created -> context_ready -> executing -> decomposed -> completed`，见 `packages/app-web/src/features/story/next-step.ts:3`；快捷键 `x` 直接在 `completed/context_ready` 间切换，见 `packages/app-web/src/features/story/story-keyboard.ts:68`。这些可以保留为 Story command surface，但文案不能让用户以为它们是 runtime execution truth。

Task status UI 全局仍绑定旧执行状态。`status-badge.tsx` 的 `taskStatusConfig` 是 `pending/assigned/running/awaiting_verification/completed/failed/cancelled`，见 `packages/app-web/src/components/ui/status-badge.tsx:29`；`running` 还会显示 pulse，见 `packages/app-web/src/components/ui/status-badge.tsx:97`。迁移到计划语言 `open/active/review/blocked/done/dropped` 后，这里会产生集中 type error，是一个适合作为迁移进度信号的文件。

测试层也绑定旧 surface。`tests/e2e/task-agent-binding.spec.ts:5` 明确验证“Task 创建与详情编辑使用统一 dispatch_preference 结构”，并在 UI 上填写 Agent 类型、Prompt 模板、Initial Context，见 `tests/e2e/task-agent-binding.spec.ts:29`、`tests/e2e/task-agent-binding.spec.ts:72`。`tests/e2e/story-context-injection.spec.ts:179` 直接调用 `/stories/{storyId}/tasks`，并断言 `dispatch_preference.context_sources` 保存 Story context refs，见 `tests/e2e/story-context-injection.spec.ts:263`。迁移后这些测试应改成验证 Story subject context injection、Task plan create、Task assignment link 或 linked AgentRun projection，而不是保留 dispatch_preference fallback。

### 最容易遗漏的调用面与风险

1. **generated type 源头**：前端所有 Task/Story 类型从 `packages/app-web/src/generated/*` 进入，实际生成源在 `crates/agentdash-contracts/src/task/contract.rs` 与 `crates/agentdash-contracts/src/story/contract.rs`。如果只改 UI 文案，`services/story.ts` guard、`TaskStatusBadge`、`TaskDrawer`、tests 仍会继续要求旧字段。必须先收束 Rust contract，再运行 `pnpm run contracts:check` / 生成检查。

2. **`tasksByStoryId` 缓存形态**：`storyStore` 把 Task 作为 Story 私有集合缓存，upsert/delete/event 都依赖 `task.story_id`。Task durable facts 脱离 Story aggregate 后，这个 store 要么改成 projection store（例如 storyTaskProjectionByStoryId），要么新增 run/task store 并让 Story 页面只消费 projection。最容易漏的是 `task_deleted` 仍从 payload.story_id 定位缓存，见 `packages/app-web/src/stores/storyStore.ts:460`。

3. **Event payload guard**：`canMapTaskFromPayload` 要求 `dispatch_preference` 和 `artifacts`。若后端 StateChange payload 跟随新 Task plan DTO 删除这些字段，前端会静默 fallback 到 `refreshTaskById`；如果 `/tasks/{id}` 也换成新 DTO，旧 guard 和 store 会持续报错或无法 upsert。

4. **Task artifacts 双来源**：TaskDrawer 同时展示 Task entity artifacts 与 SubjectExecutionView artifacts。新模型要求 runtime artifacts/status 走 Lifecycle/AgentRun projection；保留两个入口会导致“哪个 artifacts 才是真的”的 UI 漂移。

5. **`dispatch_preference` 混同 assignment/launch hint**：CreateTaskPanel、TaskDrawer、DispatchPreferenceFields、ContextOverview labels、E2E 都把 Task 创建与 Agent 选择绑定在一起。新模型应把 Todo 创建、assignment link、AgentRun launch hint/ProjectAgent executor config 分成不同命令边界。

6. **AgentRun workspace 回跳仍依赖 `run_context.story_id/task_id`**：这会把 Task 重新绑回 Story 路由。迁移时要让回跳逻辑识别 `subject_ref.kind === "story" | "task"` 和 association metadata；Task 页面/抽屉位置若改变，不能再假设 Task 必须在 `/story/{story_id}` 下打开。

7. **Story UI 文案与状态名称**：Story 的 `executing/failed/cancelled` 状态、StoryPage “执行队列”、TaskDrawer “运行状态/执行产物”、Story list “跟进 Agent 执行”等文案，都会暗示 Story/Task 承担 runtime truth。迁移应把文案收敛到“主题流程 / Todo 计划 / linked runs / 执行投影”。

8. **未直接搜索到的旧 route**：前端主路径没有直接调用 `/tasks/{id}/execution`，但后端仍存在旧 route 和 DTO，见 `crates/agentdash-api/src/routes/task_execution.rs:16`、`crates/agentdash-api/src/dto/task_execution.rs:8`。实现阶段如删除/替换该 endpoint，需要同步确认没有 extension/example/测试间接调用；前端目前主要消费 `/subjects/{kind}/{id}/execution`。

9. **ContextOverview / Session context 类型命名**：`TaskSessionExecutorSummary` 与 source labels 可能不在当前主路径传值，但它们会污染后续 workspace runtime UI 的概念语言。迁移时应重命名为 AgentRun/Conversation executor projection 或移除 task-specific source labels。

10. **E2E locator 中仍有旧路径字符串**：`tests/e2e/story-context-injection.spec.ts:210` 使用 `frontend/src/pages/StoryPage.tsx` 作为测试数据 locator，实际仓库路径已是 `packages/app-web/src/pages/StoryPage.tsx`。这不是业务模型本身，但迁移测试时可以顺手修正，以免误导未来排查。

### 前端迁移建议顺序

1. **先改 contract 和 generated 类型，不做兼容字段**
   将 TaskStatus 收敛为 `open | active | review | blocked | done | dropped`；Task DTO 去掉 runtime truth 字段或移动到 projection DTO，包括旧 `artifacts`、execution status 语义和直接 dispatch preference。Story DTO 保留 context/flow 字段，但明确 `task_count` 是否来自 Story projection 而不是 Story-owned task collection。更新 Rust contract 后生成 TS，并用 type error 驱动前端迁移。

2. **拆分前端类型入口**
   在 `types/index.ts` 保持 generated DTO 直接消费，不引入 `oldStatus ?? newStatus` 或别名 fallback。可新增明确 view model 名称，例如 StoryTaskProjection / TaskPlanItem / TaskLinkedRunSummary，但必须由 generated DTO 显式转换而来。删除 `Task = Omit<TaskResponse, "dispatch_preference"> & ...` 这类旧 wrapper，避免 Task plan DTO 继续携带 execution/dispatch 字段。

3. **先改 service/store 边界**
   `services/story.ts` 不再承担所有 Task CRUD；把 Story command、Story projection、Task plan command、Task assignment/fanout command 分到明确 service 函数。`storyStore.tasksByStoryId` 改为 Story projection cache 或移交给新的 Task/AgentRun task store；event reducer 改成响应新事件名/新 DTO，不再监听 `task_artifact_added` 作为 Task entity 更新。

4. **迁移 SubjectExecution 消费到唯一 runtime projection**
   保留并强化 `lifecycleStore` + `fetchSubjectExecution` 作为 Story/Task linked runs 和 runtime artifacts 的唯一 UI 输入。TaskDrawer 中的 runtime node、current agent、artifacts、linked runs 都从 SubjectExecutionView 或后续 linked runs DTO 来；Task plan entity 只显示 title/body/status/priority/assignment/link metadata。

5. **重做 Task 创建/指派 UI 边界**
   `CreateTaskPanel` 先变成纯 Todo/plan item 创建；Agent 选择、subagent 派发、fanout 作为后续 action 或独立 section。`dispatch-preference-fields.tsx` 若仍需要，应迁到 assignment/launch form，命名为 AgentRun launch hint / ProjectAgent executor config，而不是 TaskDispatchPreference。

6. **调整 Story 页面为 projection 消费面**
   StoryPage 的 Tasks section 改为“Story Task Projection / Todo Projection”，数据来自显式 link + run tree projection。每条 Task 标注来源关系，避免用 `story_id` 暗示所有权。删除“Story 删除后其下 Task 会一起删除”的 UI 假设，除非新 contract 明确 Story 删除会归档 projection link。

7. **调整 AgentRun workspace 回跳和分组**
   优先使用 `AgentRunWorkspaceView.subject_associations` / list entry `subject_ref` / `subject_label` 作为归属显示和回跳依据。Hook runtime `run_context.story_id/task_id` 如果还保留，应是兼容期之前的内部诊断输入；本项目未上线，建议直接收束，不保留 UI fallback。

8. **统一状态 badge 和文案**
   `TaskStatusBadge` 切换到 `open/active/review/blocked/done/dropped`；Story 文案改成流程状态，不写成 runtime status。`running/failed/cancelled` 这类 runtime 表达只出现在 LifecycleRun / AgentRun / RuntimeSession trace UI。

9. **最后改测试**
   替换 E2E 的旧断言：不再验证 Task dispatch_preference 作为 Task 字段；改为验证 Task plan 创建、Task assignment 后 linked AgentRun/SubjectExecution 可见、Story projection 能解释 Task 来源、AgentRun workspace 按 subject_ref 分组/回跳。保留 `pnpm run contracts:check`、`pnpm run frontend:check`，按变更面选择运行 `pnpm --filter app-web test` 或关键 E2E。

### 建议验证点

- Contract drift：`pnpm run contracts:check` 必须在 Task/Story/workflow contract 改动后通过。
- TypeScript：`pnpm run frontend:check` 应暴露并清空旧 `TaskStatus`、`dispatch_preference`、`artifacts`、`story_id` 直接消费点。
- Store 行为：Story 删除不再隐式删除 Task durable facts；Story projection refresh 能展示显式 link / run tree Task；Task update 不再依赖 `tasksByStoryId[task.story_id]`。
- UI 行为：Story 页面展示 Task projection 和来源；TaskDrawer 只编辑计划字段并展示 linked runs；runtime artifacts/status 只从 SubjectExecution / Lifecycle projection 出现。
- AgentRun：Story-bound / Task-bound AgentRun 启动 payload 带正确 `subject_ref`；AgentRun list 按 subject_ref 分组；从 AgentRun workspace 能回到 Story 或 Task projection，不要求 Task 必须有 Story owner。
- 文案：页面不再把 Task status 写成执行中/失败/取消，不再把 Task artifacts 写成 Task entity 自有事实；Story status 文案表达人工流程/主题状态。
- E2E 替换：`task-agent-binding.spec.ts` 应改为 assignment/launch hint 流程测试；`story-context-injection.spec.ts` 应改为 Story subject context injection 或 Task projection context block 测试。

### 无需兼容字段或 fallback

项目当前未上线，且 spec 明确“预研阶段不需要兼容性方案，保持项目最正确状态”。前端迁移不应保留 `old_task_status ?? new_task_status`、`dispatch_preference` 可选回读、`artifacts` 双来源、`story_id` fallback 分桶或旧 route fallback。正确做法是让 Rust contract、generated TS、service/store/UI 一次性收束到新模型，并用 `contracts:check` 与 `frontend:check` 暴露遗漏面。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)` / `Source: none`；本研究按用户显式提供的任务目录 `.trellis/tasks/06-16-story-task-subject-model-cleanup` 和指定输出文件执行。
- 未发现 `frontend/src` 或 `shared/generated/types` 目录；实际前端路径是 `packages/app-web/src`，generated DTO 在 `packages/app-web/src/generated`。
- 本次只读代码和文档，未运行 `pnpm run contracts:check`、`pnpm run frontend:check`、E2E 或后端测试。
- 未做全仓所有 backend route 的完整迁移设计，只读取了前端影响判断所需的 generated contract 源与相关 route。
- 未发现前端主路径直接调用 `/tasks/{id}/execution`；当前 Story/Task execution UI 主要通过 `/subjects/{kind}/{id}/execution` 消费 SubjectExecutionView。

## External References

- 无外部资料。本研究仅基于本仓库代码、Trellis task 文档与 spec。

## Related Specs

- `.trellis/spec/project-overview.md` - SubjectRef、LifecycleRun、RuntimeSession 顶层事实源。
- `.trellis/spec/frontend/architecture.md` - 前端不创建第二套业务事实源，Story/Task/AgentRun/Workflow 状态以后端为准。
- `.trellis/spec/frontend/type-safety.md` - generated wire 单源，禁止 camelCase/snake_case 或新旧字段 fallback。
- `.trellis/spec/frontend/state-management.md` - store 不作为协议字段事实源，Lifecycle 运行态进入 lifecycleStore。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - Rust contract -> generated TS -> frontend service/reducer 标准链路。
- `.trellis/spec/backend/story-task-runtime.md` - 当前 Story/Task/SubjectContextAssignment/Lifecycle projection 边界。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - LifecycleSubjectAssociation 与 SubjectExecutionView 查询路径。
