# Session Runtime 控制面事实源实现计划

## Step 0: Context

- 读取 backend、frontend、cross-layer specs。
- 读取当前任务 `prd.md` 与 `design.md`。
- 当前工作树存在其它 graphless/model 重构改动；实现时只修改本任务必要文件，避免回滚或混入无关改动。

## Step 1: Backend Anchor Repository And Schema

- 确认 `runtime_session_execution_anchors` schema 覆盖 runtime session、run、agent、launch frame、assignment、graph instance、activity、attempt。
- 补齐索引：`run_id`、`agent_id`、`launch_frame_id`、`(run_id, agent_id)`。
- 扩展 `RuntimeSessionExecutionAnchorRepository` 查询能力：
  - `list_by_run`
  - `list_by_agent`
  - `latest_for_agent`
  - 批量按 session id 查询
- 更新 Postgres repository 与 memory/test repository。
- 收敛 `AgentFrameRepository.find_by_runtime_session`，通过 anchor 定位 agent 当前 frame；anchor 缺失返回 `None`。

## Step 2: Backend Read Models

- 更新 `lifecycle_run_view_builder`：
  - `runtime_trace_refs` 从 anchors by run 投影。
  - `LifecycleAgentView.delivery_runtime_ref` 从 anchor by agent 投影。
  - `AgentFrameRuntimeView.runtime_session_refs` 从 anchor 投影。
- 新增 `SessionRuntimeControlView`、`SessionShellDto`、`RuntimeSessionExecutionAnchorDto` contract。
- 新增 `ProjectSessionListView` 与 `ProjectSessionListEntry` contract。
- 新增 route：
  - `GET /sessions/{runtime_session_id}/runtime-control`
  - `GET /projects/{project_id}/sessions`
- route 权限：
  - session shell project 必须可 view。
  - anchor run project 必须与 session shell project 一致。

## Step 3: Backend Write Paths

- 检查并修正写入 anchor 的入口：
  - Project Agent launch。
  - Story/Task execution。
  - LifecycleAgent message continuation。
  - freeform project session 创建。
- 确保 assignment 创建后回填 anchor attempt 证据。
- 保持 `SessionRuntimeService::start_prompt` 只作为 control-plane use case 内部 delivery 实现。

## Step 4: Contract Generation

- 更新 Rust contract exports。
- 生成 TS contracts。
- 更新 frontend service mapper。

## Step 5: Frontend Session Page And Workspace Runtime

- `SessionPage` 改为加载 `fetchSessionRuntimeControl(runtimeSessionId)`。
- 标题、发送状态、运行详情、WorkspacePanel input 均来自 control view。
- 删除临时 `SessionPage.lifecycle.ts` 控制面 resolver，或仅保留无事实推断的 UI helper。
- `WorkspaceRuntimeData` 删除 `lifecycleRuns: LifecycleRunView[]`，改为单一 control projection。
- `ContextOverviewTab` 改为单 `lifecycleRun` 输入。

## Step 6: Frontend Session Lists

- 新增 `fetchProjectSessionList(projectId)` service。
- 侧边栏 `SessionShortcutList` 消费 `ProjectSessionListView`。
- Agent 页 `ActiveSessionList` 消费同一 entry 数据。
- 删除前端 `sessionMetas` 列表拼装路径。
- 会话标题只显示后端 entry title 或“会话加载中…”。

## Step 7: Cleanup And Specs

- 清理 frontend/backend 中 session-first prompt 命名残留。
- 清理 `lifecycleRuns: LifecycleRunView[]` 在 WorkspacePanel/SessionPage 的残留。
- 更新 specs：
  - backend workflow/session specs 记录 anchor 事实源。
  - frontend specs 记录 SessionRuntimeControlView 标准入口。
  - cross-layer specs 记录 DTO 生成与 Session control contract。

## Validation

- Targeted backend tests：
  - anchor repository queries。
  - lifecycle run view anchor projection。
  - session runtime control route。
  - project session list route。
  - lifecycle agent message with anchor resolution。
- Targeted frontend tests：
  - SessionPage runtime-control rendering。
  - ContextOverviewTab single lifecycleRun projection。
  - Session list title and navigation。
- Commands：
  - `pnpm -C packages/app-web exec vitest run <targeted files>`
  - `pnpm -C packages/app-web exec tsc --noEmit -p tsconfig.app.json`
  - targeted Rust tests when current backend model changes are stable
  - `rg -n "sendSessionPrompt|/sessions/.*/prompt|lifecycleRuns: LifecycleRunView\\[]|primary.*title|agent_role.*title" crates packages/app-web/src .trellis/spec`
  - `git diff --check`

## Notes

Browser smoke is useful after backend is runnable, but this task's hard gate is data structure, contract, projection, targeted tests, and type-check.
