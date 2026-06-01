# 执行计划

## 顺序

1. **定义后端 target view contracts**
   - 在 `agentdash-contracts` 中新增 target view types：
     - `LifecycleRunView { id, status, graph_instances, agents, subject_associations, created_at, updated_at }`
     - `WorkflowGraphInstanceView { id, run_id, graph_id, role, status, activities }`
     - `LifecycleAgentView { id, run_id, role, status, current_frame_id }`
     - `AgentFrameRuntimeView { id, agent_id, revision, procedure_ref, capability_summary, context_summary, trace_ref? }`
     - `SubjectExecutionView { subject_kind, subject_id, run_ref, agent_ref?, projection, trace_ref? }`
     - `ProjectActiveAgentsView { project_id, agents[] }`
   - 运行 contracts generator 更新 `workflow-contracts.ts`。

2. **实现后端 view API routes**
   - `GET /runs/{run_id}` → `LifecycleRunView`。
   - `GET /agents/{agent_id}` → `LifecycleAgentView` + `AgentFrameRuntimeView`。
   - `GET /subjects/{kind}/{id}/execution` → `SubjectExecutionView`。
   - `GET /projects/{id}/active-agents` → `ProjectActiveAgentsView`。
   - 更新 `GET /stories/{id}/runs` 返回 `LifecycleRunView[]` 而非 `StoryRunOverviewDto[]`。

3. **更新 generated frontend types**
   - 运行 `cargo run -p agentdash-contracts --bin generate_contracts_ts` 重新生成。
   - 验证新 types 在 `packages/app-web/src/generated/workflow-contracts.ts` 中可用。
   - 删除废弃的 `StoryRunOverviewDto.session_id`、`EffectiveSessionContract.active_step_key`。

4. **重建前端 stores**
   - 新增 normalized stores：`lifecycleRuns`, `graphInstances`, `lifecycleAgents`, `agentFrames`, `subjectExecutions`, `runtimeTraces`。
   - 删除或废弃 `runsBySessionId` 索引。
   - 新 stores 按 id 索引，不按 session_id。

5. **替换 `session-grouping.ts`**
   - 新增 `execution-grouping.ts`：`groupExecutionsBySubject(entries: ProjectActiveAgentEntry[])` → `SubjectExecutionNode[]`。
   - `SubjectExecutionNode` 按 subject kind 分组（Story / Task orphan / Project），不按 session owner_type。
   - 删除 `groupSessionsByStory()` 及 `SessionGroupNode` 类型。

6. **迁移 Project 活跃面板**
   - Project page 的 session 面板替换为 `ProjectActiveAgentsView`。
   - 列表展示 active `LifecycleAgent` 而非 active sessions。
   - 每个 agent 可展开查看 `AgentFrameRuntimeView` 和 `RuntimeTraceView` 入口。

7. **迁移 Story / Task 页面**
   - Story page：session list → `SubjectExecutionView(kind=Story)` + child tasks。
   - Task page：session panel → `TaskProjection` + `SubjectExecutionView(kind=Task)`。
   - 导航：subject view → agent view → runtime trace（drill-down）。

8. **降级 `/session/:id` route**
   - `/session/:id` 保留但语义改为 `RuntimeTraceView`。
   - 页面 title / breadcrumb 标记为 "Runtime Trace"。
   - 移除页面中的 business owner / control 面板元素。
   - 保留 events、projection、lineage、debug 面板。

9. **清理前端废弃类型**
   - 删除 `ProjectSessionEntry` type（或 rename 为 `ProjectActiveAgentEntry`）。
   - 删除 `SessionBindingResponse` frontend usage。
   - 删除 `SessionGroupNode` / `SessionGroupNodeKind`。
   - 删除 `session-relations.ts` 中 companion 相关的 session lineage 分组（改为 `AgentLineage` view）。

## 质量门

- `runsBySessionId` 不存在于 frontend stores。
- 前端 state 支持 `run → workflowGraphInstances → activities/attempts` 导航。
- Project / Story / Task 页面可从 subject view 进入 agent view 与 runtime trace view。
- generated contracts 中 nullable `session_id` 不被前端当作必填业务主键。
- `/session/:id` 页面标记为 RuntimeTrace，不展示 business owner。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-frontend-actor-subject-views`
- `cargo run -p agentdash-contracts --bin generate_contracts_ts`
- `pnpm -F app-web type-check`
- `rg -n "runsBySessionId|SessionGroupNode|groupSessionsByStory|ProjectSessionEntry|SessionBindingResponse" packages/app-web/src/`
- `rg -n "owner_type|owner_id|binding_id" packages/app-web/src/`
- `git diff --check -- .trellis/tasks`

## 后续交接

- `session-first-api-demotion` 完成后端 API 的最终清理，与前端 view 迁移形成闭环。
- 前端 companion 面板从 session lineage 切到 `AgentLineage` view（依赖 `companion-gate-lineage-migration`）。
- RuntimeTraceView 的 events / projection / lineage 面板细节可在后续独立优化。
