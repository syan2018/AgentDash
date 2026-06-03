# Research: frontend runtime query

- Query: 前端哪里还在用 session-first/cache-first 推断 frame；后端 endpoint/contract 应怎样支持 session_id -> AgentFrameRuntimeView；run-level active projection 在前端是否仍被业务使用。
- Scope: internal
- Date: 2026-06-02

## Findings

### Files Found

- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/prd.md` - 父任务要求前端 runtime state 查询以后端 frame/runtime read model 为准，不在本地 cache 中猜测 session 对应 frame。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/design.md` - 目标数据流明确为 `session_id -> backend session runtime/frame endpoint -> AgentFrameRuntimeView -> lifecycleStore frames/runtimeTraces -> WorkspacePanel runtime state`。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` - 前端 contract 规定 `/session/:id` 是 `RuntimeTraceView`，`session_id` 不能作为 lifecycle 主键，目标 API 包含 `fetchAgentFrameRuntime(frameId)` 与 `fetchRuntimeTrace(runtimeSessionId)`。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - 后端 contract 规定 session-indexed lookup 只能作为 trace adapter，并必须立即反查到 frame/agent/assignment。
- `.trellis/spec/backend/session/execution-context-frames.md` - session frame 是 connector 边界投影，不应写回为 session 架构事实源。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - HTTP DTO 应进入 `agentdash-contracts` 并生成到前端，前端 service 不应长期手写后端 DTO。
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts` - 当前 WorkspacePanel runtime hook 的 session-to-frame 推断入口。
- `packages/app-web/src/stores/lifecycleStore.ts` - 前端 normalized lifecycle store，保存 frames、agents、runtimeTraces 与 runtime trace refs。
- `packages/app-web/src/services/lifecycle.ts` - 当前 lifecycle service 只有 frame_id 查询 frame runtime 与 session_id 查询 trace，没有 session_id 查询 frame runtime。
- `crates/agentdash-api/src/routes/lifecycle_views.rs` - 后端已有 `/agent-frames/{id}/runtime` 与 `/sessions/{id}/trace`，trace endpoint 内部已经可通过 runtime session 找 frame。
- `crates/agentdash-domain/src/workflow/repository.rs` - `AgentFrameRepository::find_by_runtime_session` 已定义直接反查能力。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - Postgres 实现使用 `runtime_session_refs_json` 查询 session 附着的 frame。
- `crates/agentdash-contracts/src/workflow.rs` - `AgentFrameRuntimeView`、`RuntimeSessionTraceView`、`LifecycleRunView`、`LifecycleAgentView` 的 generated contract 来源。
- `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx` - 前端 workflow 上下文 UI 使用 graph instance attempts 与 hook active workflow metadata 显示 active attempt。
- `packages/app-web/src/pages/LifecyclePages.tsx` - lifecycle debug pages 使用 `current_frame_id` 作为 agent 页面跳转和 frame runtime 查询入口。
- `packages/app-web/src/components/layout/SessionShortcutList.tsx` - 侧边栏按 run/agent/runtime trace refs 组织导航，未使用 `active_node_keys`。
- `packages/app-web/src/features/agent/active-session-list.tsx` - active session list 使用 `runtime_trace_refs[0]` 作为 primary session title/navigation。

### Code Patterns

- `useSessionRuntimeState` 的文件注释称新版通过 lifecycle frame 投影驱动，但实际仍说“通过 `lifecycleStore` 查找 session 关联的 AgentFrame”。这正是 cache-first read model，而不是后端事实查询：`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:7`。
- `findFrameIdForSession` 先遍历本地 `frames`，用 `frame.runtime_session_refs.some(ref.runtime_session_id === sessionId)` 反查 frame；如果本地 frame cache 没有命中，再遍历所有 `agents` 并取第一个 `current_frame_id`。第二段没有验证 session 与 agent/frame 的关系，是明确的 cache-first/session-first fallback：`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:73`、`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:78`、`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:83`。
- `useSessionRuntimeState` 在拿到推断出的 `frameId` 后调用 `fetchAgentFrameRuntime(frameId)`；如果没找到 frameId，则返回 `ready` 且 `frame: null`，这会把“后端未查询”表现成“没有 frame”：`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:99`、`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:100`、`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:114`。
- `lifecycleStore` 的主索引已经是 run / graph instance / subject / agent / frame，并声明替代 session-first 索引；但它只有 `fetchFrame(frameId)` 和 `fetchRuntimeTrace(runtimeSessionId)`，缺少 `fetchFrameRuntimeBySession(runtimeSessionId)` 这种事实查询 action：`packages/app-web/src/stores/lifecycleStore.ts:4`、`packages/app-web/src/stores/lifecycleStore.ts:63`、`packages/app-web/src/stores/lifecycleStore.ts:64`。
- `lifecycleStore.primarySessionId(runId)` 直接取 `run.runtime_trace_refs[0]`。这不是 frame 推断，但仍是 run-level “primary trace”启发式，主要服务导航/标题，而非 frame read model：`packages/app-web/src/stores/lifecycleStore.ts:73`、`packages/app-web/src/stores/lifecycleStore.ts:274`。
- 前端 service 当前只有 `/agent-frames/{frameId}/runtime` 与 `/sessions/{runtimeSessionId}/trace` 两个入口，没有 `/sessions/{runtimeSessionId}/frame-runtime` 或等价接口：`packages/app-web/src/services/lifecycle.ts:35`、`packages/app-web/src/services/lifecycle.ts:39`。
- 后端 `/sessions/{id}/trace` 已经使用 `agent_frame_repo.find_by_runtime_session` 定位 frame，并在 `RuntimeSessionTraceView.frame_ref` 返回 frame ref；这说明 session_id -> frame 的权限路径和 repository 能力已经存在，但 trace DTO 不包含完整 `AgentFrameRuntimeView`：`crates/agentdash-api/src/routes/lifecycle_views.rs:112`、`crates/agentdash-api/src/routes/lifecycle_views.rs:117`、`crates/agentdash-api/src/routes/lifecycle_views.rs:152`。
- 后端 `/agent-frames/{id}/runtime` 只接受 frame_id；内部做 frame -> agent -> run -> project permission，然后调用 `agent_frame_runtime_to_view`。新 session endpoint 可复用同一个 mapping，但入口应先 `find_by_runtime_session(session_id)`：`crates/agentdash-api/src/routes/lifecycle_views.rs:82`、`crates/agentdash-api/src/routes/lifecycle_views.rs:88`、`crates/agentdash-api/src/routes/lifecycle_views.rs:100`、`crates/agentdash-api/src/routes/lifecycle_views.rs:109`。
- `agent_frame_runtime_to_view` 已能从 `AgentFrame` 生成完整 `AgentFrameRuntimeView`，包括 `graph_instance_id`、`activity_key`、capability/context/VFS/MCP surface、`runtime_session_refs` 与 execution profile：`crates/agentdash-api/src/routes/lifecycle_views.rs:253`。
- repository trait 已明确 `find_by_runtime_session` 是 `RuntimeSession -> AgentFrame -> Agent -> Assignment` 链路的一环；该方法是后端 contract 支持 session_id -> frame runtime 的自然底座：`crates/agentdash-domain/src/workflow/repository.rs:131`。
- Postgres `find_by_runtime_session` 目前通过 `agent_frames.runtime_session_refs_json::jsonb @> [{kind:'runtime_session', session_id:$1}]` 查找，并 `ORDER BY created_at DESC LIMIT 1`。这支持当前查询，但 target contract 若要求唯一 session delivery frame，应考虑 DB 结构或唯一性约束，而不是长期依赖 JSON contains + latest：`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:509`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:520`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:523`。
- `AgentFrameRuntimeView` contract 已是合适的返回体，字段位于 `agentdash-contracts::workflow` 并生成到前端；新增 endpoint 不需要新建前端手写 DTO：`crates/agentdash-contracts/src/workflow.rs:742`。
- `RuntimeSessionTraceView` 当前只返回 `runtime_session_ref`、可选 `frame_ref`、events、turns；它适合 trace drill-down，不适合替代 frame runtime view：`crates/agentdash-contracts/src/workflow.rs:788`。
- `LifecycleAgentView.current_frame_id` 是合法的 agent current 指针，但前端不应拿它作为 session_id -> frame 的 fallback；它可继续用于 agent 页面从 agent 查当前 frame：`crates/agentdash-contracts/src/workflow.rs:702`、`packages/app-web/src/pages/LifecyclePages.tsx:273`。
- `LifecyclePages` 的 run/agent debug 页面从 agent 进入 frame runtime 时使用 `agent.current_frame_id`，这是 agent-first navigation，不是 session-first runtime query；但如果页面由 session route 进入，仍应靠 session endpoint 拿 frame：`packages/app-web/src/pages/LifecyclePages.tsx:67`、`packages/app-web/src/pages/LifecyclePages.tsx:281`。
- `ContextOverviewTab` 的 workflow active UI 不消费 `LifecycleRun.active_node_keys`；它先用 hook runtime metadata 的 `active_workflow.run_id/activity_key` 匹配 run/attempt，缺失时从 `WorkflowGraphInstanceView.activities[].attempts[]` 中按状态选 running/claiming/ready attempt：`packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:70`、`packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:92`、`packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:108`、`packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:112`。
- 前端检索 `active_node_keys` 没有业务引用；唯一匹配来自 generated contract 之外的前端代码是 hook metadata 的 `active_activity_key` / `ActiveWorkflowHookMetadata` 展示路径，不是 `LifecycleRun.active_node_keys`：`packages/app-web/src/types/session.ts:81`、`packages/app-web/src/features/session-context/hook-runtime-cards.tsx:65`。
- 后端 domain 仍持久化 `LifecycleRun.active_node_keys`，且它是从 `WorkflowGraphInstance.activity_state` 派生的 graph-scoped string projection，格式为 `{graph_instance_id}:{activity_key}`：`crates/agentdash-domain/src/workflow/entity.rs:190`、`crates/agentdash-domain/src/workflow/entity.rs:223`、`crates/agentdash-domain/src/workflow/entity.rs:229`、`crates/agentdash-domain/src/workflow/entity.rs:234`。
- 后端业务仍有 run-level active projection 使用：`select_active_run` 以 `run.current_activity_key().is_some()` 判断 active run；agent tool `advance_node` 会把 `run.active_node_keys` 展示/返回给工具调用方。这不是前端业务使用，但说明后端 runtime/control path 尚未完全迁出 run-level active projection：`crates/agentdash-application/src/workflow/run.rs:3`、`crates/agentdash-application/src/workflow/tools/advance_node.rs:217`、`crates/agentdash-application/src/workflow/tools/advance_node.rs:247`。

### Recommended Backend Endpoint / Contract Shape

- Add a generated-contract-backed read endpoint that directly returns `AgentFrameRuntimeView` by runtime session id, e.g. `GET /sessions/{runtime_session_id}/frame-runtime` or `GET /runtime-sessions/{runtime_session_id}/agent-frame-runtime`.
- Implementation path should be:
  1. `agent_frame_repo.find_by_runtime_session(runtime_session_id)`.
  2. Load `LifecycleAgent` by `frame.agent_id`.
  3. Load `LifecycleRun` by `agent.run_id`.
  4. Authorize project view via `run.project_id` or `agent.project_id`.
  5. Return `agent_frame_runtime_to_view(&frame)`.
- The endpoint should return 404 when the session is not attached to a frame. The frontend should treat this as an error/empty runtime association from backend, not silently pick an unrelated `current_frame_id`.
- `RuntimeSessionTraceView.frame_ref` can remain for trace drill-down, but frontend runtime context should not have to call `/sessions/{id}/trace` just to discover a frame id and then call `/agent-frames/{id}/runtime`; that two-step would still encode session-first indirection in UI code.
- Prefer using existing `AgentFrameRuntimeView` contract rather than creating a special session DTO. If additional context is required later, add an explicit wrapper such as `{ runtime_session_ref, frame }` in `agentdash-contracts::workflow`, then generate TypeScript.
- Longer-term target should replace JSON-array reverse lookup with a direct delivery anchor or unique relation if a runtime session must map to exactly one frame. Current JSON contains query is usable for planning and endpoint implementation, but the `ORDER BY created_at DESC LIMIT 1` policy should be made an explicit invariant or removed by schema design.

### Frontend Convergence Shape

- Replace `findFrameIdForSession(frames, agents, sid)` in `useSessionRuntimeState` with a service/store action such as `fetchFrameRuntimeBySession(sid)`.
- On success, store the returned frame through `setFrame(frameView)` and set hook state from that authoritative view.
- Remove the fallback that returns the first `agent.current_frame_id`; it can associate an arbitrary agent frame with the session when local cache is cold or stale.
- Keep `fetchRuntimeTrace(runtimeSessionId)` for session page event drill-down only.
- Keep `current_frame_id` for agent route/page navigation, not for resolving a runtime session.
- Keep `runtime_trace_refs[0]` only as a UI title/navigation heuristic if the product wants a “primary session” row; do not use it to identify frame runtime.

### Run-Level Active Projection

- In frontend business UI, `LifecycleRun.active_node_keys` is not consumed. `LifecycleRunView` generated type does not expose it, and searches found no frontend references to `active_node_keys`.
- Frontend active workflow display is currently driven by hook runtime metadata plus `WorkflowGraphInstanceView.activities[].attempts[]` state, which aligns with the target read model.
- The remaining frontend run-level heuristic is `runtime_trace_refs[0]` for primary session navigation/title, not active activity selection.
- Backend still persists and uses `LifecycleRun.active_node_keys` in runtime/control paths. It should be treated as a derived backend projection until replaced by structured `ActiveActivityRef` or direct graph instance attempt queries.

## External References

- None. This research is internal codebase/spec inspection only.

## Related Specs

- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## Caveats / Not Found

- `task.py current --source` returned no active task in this session; the user-provided active task path `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence` was used as the research output target.
- No implementation or tests were changed.
- This pass did not audit every backend session/action tool that uses `find_by_runtime_session`; it focused on frontend runtime query, lifecycle read model endpoints/contracts, and active projection usage relevant to the planning question.
- The endpoint naming is a recommendation; final route name should be chosen with the project's route naming convention before implementation.
