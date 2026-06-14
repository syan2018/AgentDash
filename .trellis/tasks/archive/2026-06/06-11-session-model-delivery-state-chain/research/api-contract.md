# Research: AgentRun Workspace API/DTO/contract generation

- Query: AgentRun Workspace API/DTO/contract generation 的当前代码路径与实施风险
- Scope: mixed
- Date: 2026-06-11

## Findings

本次研究聚焦 AgentRun Workspace 前端 route state 需要消费的后端 API、application service、Rust DTO 与 TypeScript generated contracts。`task.py current --source` 返回当前无 active task；本研究按用户明确指定的父任务目录写入。

### Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-api/src/routes/project_agents.rs` | Project Agent CRUD、launch 与创建 ProjectAgent session 的 HTTP routes。 |
| `crates/agentdash-api/src/routes/lifecycle_agents.rs` | RuntimeSession-scoped AgentRun message、steering、pending message command routes。 |
| `crates/agentdash-api/src/routes/sessions.rs` | Session detail、project session list 与 `/sessions/{id}/runtime-control` 聚合 read model。 |
| `crates/agentdash-api/src/routes/workflows.rs` | WorkflowGraph/AgentProcedure routes，以及 `/lifecycle-runs` start/get/human decision routes。 |
| `crates/agentdash-api/src/routes/lifecycle_views.rs` | Lifecycle read-side routes：run view、subject execution、frame runtime、session trace、project active agents。 |
| `crates/agentdash-api/src/routes/lifecycle_contracts.rs` | application lifecycle view 到 `agentdash-contracts::workflow` DTO 的 mapping。 |
| `crates/agentdash-application/src/workflow/project_agent_session_start.rs` | ProjectAgent session start orchestration：materialize lifecycle/runtime anchor 并投递首条 message。 |
| `crates/agentdash-application/src/workflow/agent_message.rs` | AgentRun message command service；从 runtime session anchor 反查 run/agent/frame 后投递下一轮。 |
| `crates/agentdash-application/src/workflow/agent_steering.rs` | AgentRun steering command service；校验 running turn、steering 支持度并写入 steer event。 |
| `crates/agentdash-application/src/workflow/dispatch_service.rs` | Lifecycle dispatch service；创建 graphless run/agent/frame/runtime session anchor。 |
| `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs` | LifecycleRunView read model builder；汇总 agents、subject associations 与 runtime trace refs。 |
| `crates/agentdash-application/src/session/runtime_control.rs` | Runtime cancel/recovery service；route 的 control view 还会消费 session execution state 与 steering support。 |
| `crates/agentdash-contracts/src/project_agent.rs` | ProjectAgent config/session DTO，包括 start request/result。 |
| `crates/agentdash-contracts/src/workflow.rs` | Lifecycle/AgentRun/session runtime-control DTO 的 Rust source of truth。 |
| `crates/agentdash-contracts/src/generate_ts.rs` | TypeScript contract generation entrypoint。 |
| `packages/app-web/src/generated/project-agent-contracts.ts` | Generated ProjectAgent DTO 与 shared ref DTO import/export output。 |
| `packages/app-web/src/generated/workflow-contracts.ts` | Generated lifecycle, AgentRun command, runtime-control, pending message DTO output。 |
| `packages/app-web/src/services/lifecycle.ts` | 前端 lifecycle API service，已覆盖 runtime-control/message/steer/pending queue。 |
| `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts` | 前端 workspace panel runtime state hook，已直接消费 `/sessions/{id}/runtime-control`。 |

### API Surface And Route Style

- Project Agent routes are project-scoped under `/projects/{id}/agents`; launch and session creation are commands on a concrete ProjectAgent: `/projects/{id}/agents/{project_agent_id}/launch` and `/projects/{id}/agents/{project_agent_id}/sessions` (`crates/agentdash-api/src/routes/project_agents.rs:66`).
- `launch_project_agent` builds an `AgentLaunchIntent` with `source=ProjectAgent`, `run_policy=CreateLinkedRun`, `agent_policy=Create`, `context_policy=Isolated`, `capability_policy=Baseline`, and `runtime_policy=CreateRuntimeSession` (`crates/agentdash-api/src/routes/project_agents.rs:165`). It maps the dispatch result into `ProjectAgentLaunchResult` with `run_ref`, `agent_ref`, `frame_ref`, optional `delivery_runtime_ref`, and `subject_ref` (`crates/agentdash-api/src/routes/project_agents.rs:214`).
- `create_project_agent_session` delegates to `ProjectAgentSessionStartService`, passes generated request fields `input`, `executor_config`, `subject_ref`, and returns `ProjectAgentSessionStartResult` including `runtime_session_id`, `turn_id`, `run_ref`, `agent_ref`, `frame_ref`, `subject_ref` (`crates/agentdash-api/src/routes/project_agents.rs:262`, `crates/agentdash-api/src/routes/project_agents.rs:287`).
- RuntimeSession-scoped AgentRun command routes use `/sessions/{runtime_session_id}/messages`, `/sessions/{runtime_session_id}/steering`, and `/sessions/{runtime_session_id}/pending-messages...` (`crates/agentdash-api/src/routes/lifecycle_agents.rs:25`). These names are session delivery routes, not lifecycle primary routes.
- `send_session_message` validates non-empty input, resolves `RuntimeSessionExecutionAnchor`, loads `LifecycleAgent` and `LifecycleRun`, checks run/agent consistency, enforces project edit permission, then calls `AgentRunMessageService` (`crates/agentdash-api/src/routes/lifecycle_agents.rs:50`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:59`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:87`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:107`).
- `steer_session` follows the same anchor-to-run permission chain, then calls `AgentRunSteeringService`; response includes `accepted=true` plus `RuntimeSessionCommandStateDto` from inspected execution state (`crates/agentdash-api/src/routes/lifecycle_agents.rs:144`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:154`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:180`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:196`).
- Session routes expose `/sessions/{id}/runtime-control` as the current aggregate workspace control read model (`crates/agentdash-api/src/routes/sessions.rs:84`). The handler returns `UnboundTrace` if no anchor exists; anchored sessions include anchor, run, agent, frame runtime, subject associations, action availability, and pending messages (`crates/agentdash-api/src/routes/sessions.rs:156`, `crates/agentdash-api/src/routes/sessions.rs:167`, `crates/agentdash-api/src/routes/sessions.rs:340`).
- Project session list is `/projects/{id}/sessions`; implementation joins session metas with execution anchors, filters by run project, and emits `ProjectSessionListView` (`crates/agentdash-api/src/routes/sessions.rs:95`, `crates/agentdash-api/src/routes/sessions.rs:390`, `crates/agentdash-api/src/routes/sessions.rs:417`).
- Workflow lifecycle routes include `/lifecycle-runs` POST, `/lifecycle-runs/{id}` GET, and `/lifecycle-runs/{id}/orchestration-human-decisions` POST (`crates/agentdash-api/src/routes/workflows.rs:120`). `GET /lifecycle-runs/{id}` returns the same contract view through `lifecycle_run_to_contract_view` (`crates/agentdash-api/src/routes/workflows.rs:466`, `crates/agentdash-api/src/routes/workflows.rs:813`).
- Lifecycle read-side routes also expose `/lifecycle-runs/{id}/view`, `/subjects/{kind}/{id}/execution`, `/agent-frames/{id}/runtime`, `/sessions/{id}/trace`, and `/projects/{id}/active-agents` (`crates/agentdash-api/src/routes/lifecycle_views.rs:35`). Existing frontend service uses `/lifecycle-runs/{id}` for `fetchLifecycleRun`, which is valid via `workflows.rs` (`packages/app-web/src/services/lifecycle.ts:31`).

### Application Service Paths

- `ProjectAgentSessionStartService::start_session` validates non-empty input, loads ProjectAgent by project/id, defaults `subject_ref` to project subject, validates subject, builds graphless `AgentLaunchIntent`, calls `LifecycleDispatchService::launch_agent`, requires `delivery_runtime_ref`, binds `project_agent_id` onto the created `LifecycleAgent`, and dispatches the initial user message via `AgentRunMessageService` (`crates/agentdash-application/src/workflow/project_agent_session_start.rs:127`, `crates/agentdash-application/src/workflow/project_agent_session_start.rs:141`, `crates/agentdash-application/src/workflow/project_agent_session_start.rs:153`, `crates/agentdash-application/src/workflow/project_agent_session_start.rs:157`, `crates/agentdash-application/src/workflow/project_agent_session_start.rs:183`, `crates/agentdash-application/src/workflow/project_agent_session_start.rs:193`, `crates/agentdash-application/src/workflow/project_agent_session_start.rs:217`).
- `ProjectAgentSessionStartService` has cleanup logic when binding or first message dispatch fails; it attempts to delete empty runtime/lifecycle drafts only when no events exist (`crates/agentdash-application/src/workflow/project_agent_session_start.rs:193`, `crates/agentdash-application/src/workflow/project_agent_session_start.rs:225`).
- `AgentRunMessageService::dispatch_user_message` validates runtime session id and input, resolves control plane from `RuntimeSessionExecutionAnchor`, loads current frame or launch frame, validates frame ownership, then calls `SessionLaunchService.launch_command` through `AgentRunMessageLaunchDeliveryPort` (`crates/agentdash-application/src/workflow/agent_message.rs:115`, `crates/agentdash-application/src/workflow/agent_message.rs:130`, `crates/agentdash-application/src/workflow/agent_message.rs:158`, `crates/agentdash-application/src/workflow/agent_message.rs:194`, `crates/agentdash-application/src/workflow/agent_message.rs:60`).
- `AgentRunSteeringService::steer` validates input, resolves anchor/agent/run/frame, rejects terminal agents, requires `SessionExecutionState::Running` with active turn id, checks connector steering support, calls `SessionControlService.steer_session`, then emits `UserInputSubmissionKind::Steer` into session eventing (`crates/agentdash-application/src/workflow/agent_steering.rs:61`, `crates/agentdash-application/src/workflow/agent_steering.rs:76`, `crates/agentdash-application/src/workflow/agent_steering.rs:102`, `crates/agentdash-application/src/workflow/agent_steering.rs:135`, `crates/agentdash-application/src/workflow/agent_steering.rs:155`, `crates/agentdash-application/src/workflow/agent_steering.rs:165`, `crates/agentdash-application/src/workflow/agent_steering.rs:177`).
- `LifecycleDispatchService` owns runtime session creation and anchor upsert. For graphless dispatch it creates or resolves a runtime session, creates the initial frame, sets `current_frame`, and upserts `RuntimeSessionExecutionAnchor::new_dispatch(session_id, run.id, frame.id, agent.id)` (`crates/agentdash-application/src/workflow/dispatch_service.rs:458`, `crates/agentdash-application/src/workflow/dispatch_service.rs:461`, `crates/agentdash-application/src/workflow/dispatch_service.rs:486`). `RuntimePolicy::CreateRuntimeSession` requires a `RuntimeSessionCreator` (`crates/agentdash-application/src/workflow/dispatch_service.rs:682`).
- `LifecycleRunView` is assembled from lifecycle agents, subject associations, orchestration views, active runtime node refs and runtime trace refs. Runtime refs come from `execution_anchor_repo.list_by_run` (`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:184`, `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:213`, `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:306`). Agent delivery runtime refs are filled from anchor-derived runtime session ids in `lifecycle_agent_to_view_with_delivery` (`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:351`).

### DTO Naming And Contract Shape

- `LifecycleRunRefDto`, `AgentRunRefDto`, `AgentFrameRefDto`, `RuntimeSessionRefDto`, and `RuntimeSessionExecutionAnchorDto` live in `agentdash-contracts::workflow` and use `snake_case` serde fields (`crates/agentdash-contracts/src/workflow.rs:681`, `crates/agentdash-contracts/src/workflow.rs:687`, `crates/agentdash-contracts/src/workflow.rs:694`, `crates/agentdash-contracts/src/workflow.rs:704`, `crates/agentdash-contracts/src/workflow.rs:725`).
- `AgentRunMessageRequest.input`, `AgentRunSteeringRequest.input`, `EnqueuePendingMessageRequest.input`, and `CreateProjectAgentSessionRequest.input` all use `Vec<codex::UserInput>` / generated `Array<UserInput>`; this is intentionally the same user input semantics for first message, next message, steering, and queued pending message (`crates/agentdash-contracts/src/workflow.rs:746`, `crates/agentdash-contracts/src/workflow.rs:766`, `crates/agentdash-contracts/src/workflow.rs:1033`, `crates/agentdash-contracts/src/project_agent.rs:68`).
- `AgentRunView` has `agent_ref`, `project_id`, `agent_kind`, `agent_role`, optional `project_agent_id`, optional `current_frame_id`, optional `delivery_runtime_ref`, optional `last_delivery_status`, and timestamps (`crates/agentdash-contracts/src/workflow.rs:857`).
- `LifecycleRunView` exposes `run_ref`, `project_id`, `topology`, `status`, `orchestrations`, `active_runtime_node_refs`, `agents`, `subject_associations`, `runtime_trace_refs`, `execution_log`, and timestamps (`crates/agentdash-contracts/src/workflow.rs:888`).
- `AgentFrameRuntimeView` exposes frame ref plus effective capability/context/VFS/MCP surfaces and runtime session refs (`crates/agentdash-contracts/src/workflow.rs:912`).
- `SessionRuntimeControlView` is the richest current API DTO for AgentRun Workspace route state: `runtime_session_ref`, `session_meta`, `control_plane`, optional `anchor`, optional `run`, optional `agent`, optional `frame_runtime`, `subject_associations`, `actions`, and `pending_messages` (`crates/agentdash-contracts/src/workflow.rs:999`).
- `ProjectSessionListEntry` is intentionally runtime-session-oriented while still carrying optional lifecycle refs (`run_ref`, `agent_ref`, `frame_ref`, `subject_ref`) for routing and display (`crates/agentdash-contracts/src/workflow.rs:1048`).
- `ProjectAgentLaunchResult` and `ProjectAgentSessionStartResult` reuse lifecycle refs from workflow contracts rather than defining ProjectAgent-local duplicates (`crates/agentdash-contracts/src/project_agent.rs:53`, `crates/agentdash-contracts/src/project_agent.rs:80`).

### Contract Generation

- The package scripts are `pnpm run contracts:generate` -> `cargo run -p agentdash-contracts --bin generate_contracts_ts`, and `pnpm run contracts:check` -> same command with `-- --check` (`package.json:44`, `package.json:45`).
- `agentdash-contracts/src/generate_ts.rs` writes into `packages/app-web/src/generated` (`crates/agentdash-contracts/src/generate_ts.rs:151`) and emits file headers with the generation command plus "Do not edit manually" (`crates/agentdash-contracts/src/generate_ts.rs:715`).
- Generation order makes `project-agent-contracts.ts` the upstream owner for shared agent construct ref DTOs (`crates/agentdash-contracts/src/generate_ts.rs:171`). `workflow-contracts.ts` then exports workflow/lifecycle DTOs and imports upstream ref DTOs when needed (`crates/agentdash-contracts/src/generate_ts.rs:379`).
- The generated `workflow-contracts.ts` imports `AgentFrameRefDto`, `AgentRunRefDto`, `LifecycleRunRefDto`, `RuntimeSessionRefDto`, and `SubjectRefDto` from `./project-agent-contracts` (`packages/app-web/src/generated/workflow-contracts.ts:6`). This is generated import deduplication, not a hand-authored frontend contract.
- Workspace uses `ts-rs` version `11.1` with `serde-json-impl`, `no-serde-warnings`, and `chrono-impl` (`Cargo.toml:94`).

### Frontend Consumption

- `packages/app-web/src/services/lifecycle.ts` already wraps the relevant endpoints with generated types and uses URL encoding through `sessionCommandPath` for session-scoped commands (`packages/app-web/src/services/lifecycle.ts:1`, `packages/app-web/src/services/lifecycle.ts:27`, `packages/app-web/src/services/lifecycle.ts:56`, `packages/app-web/src/services/lifecycle.ts:74`, `packages/app-web/src/services/lifecycle.ts:84`, `packages/app-web/src/services/lifecycle.ts:94`, `packages/app-web/src/services/lifecycle.ts:102`).
- `useSessionRuntimeState` already loads `/sessions/{id}/runtime-control` and session runtime VFS surface together, ingests returned `run`, `agent`, and `frame_runtime` into lifecycle store, and stores the whole `SessionRuntimeControlView` for UI decisions (`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:1`, `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:95`, `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:110`, `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:119`).
- `lifecycle.test.ts` already asserts URL encoding for message, steering, pending message list/enqueue/delete/promote (`packages/app-web/src/services/lifecycle.test.ts:42`, `packages/app-web/src/services/lifecycle.test.ts:65`, `packages/app-web/src/services/lifecycle.test.ts:78`, `packages/app-web/src/services/lifecycle.test.ts:86`, `packages/app-web/src/services/lifecycle.test.ts:101`, `packages/app-web/src/services/lifecycle.test.ts:109`).

### Suggested Implementation Entry Points

- For AgentRun Workspace route state, frontend implementation should start at `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts` and `packages/app-web/src/services/lifecycle.ts`. The backend already provides a purpose-built runtime-control aggregate, so route state should prefer `SessionRuntimeControlView` over local reconstruction from session meta or lifecycle store caches.
- For route navigation/list entries, prefer `fetchProjectSessionList(projectId)` and `ProjectSessionListEntry` for the left/sidebar session list because it already carries runtime session id plus optional run/agent/frame/subject refs.
- For "send next message", call `sendAgentRunMessageByRuntimeSession(runtimeSessionId, AgentRunMessageRequest)` only when `control.actions.send_next.enabled` is true. For running sessions, use `control.actions.steer` and `control.actions.enqueue` to decide between steering and pending queue UX.
- If backend contract additions are needed, add Rust DTOs to `crates/agentdash-contracts/src/workflow.rs` or `project_agent.rs` according to semantic ownership, add them to `generate_ts.rs`, run `pnpm run contracts:generate`, then `pnpm run contracts:check`. Do not hand-edit `packages/app-web/src/generated/*`.
- If route behavior changes are needed, likely backend implementation files are `crates/agentdash-api/src/routes/sessions.rs`, `crates/agentdash-api/src/routes/lifecycle_agents.rs`, `crates/agentdash-application/src/workflow/agent_message.rs`, and `crates/agentdash-application/src/workflow/agent_steering.rs`.

### Implementation Risks

- `SessionRuntimeControlView.control_plane.status` and `actions.*.enabled` are the authoritative UI affordance source. Recomputing "can send/steer/enqueue/cancel" from `session_meta.last_delivery_status` alone can drift because the route also checks agent terminal status, frame availability, inspected execution state, and connector steering support (`crates/agentdash-api/src/routes/sessions.rs:254`, `crates/agentdash-api/src/routes/sessions.rs:272`, `crates/agentdash-api/src/routes/sessions.rs:293`, `crates/agentdash-api/src/routes/sessions.rs:302`, `crates/agentdash-api/src/routes/sessions.rs:313`, `crates/agentdash-api/src/routes/sessions.rs:318`).
- Runtime session id is the delivery/trace key, while LifecycleRun/AgentFrame are the control-plane model. Specs explicitly say `/session/:id` is a RuntimeTraceView and `session_id` must not become the lifecycle primary key (`.trellis/spec/frontend/workflow-activity-lifecycle.md:12`, `.trellis/spec/frontend/workflow-activity-lifecycle.md:14`).
- `RuntimeSessionExecutionAnchor` is the authoritative reverse index from runtime trace to run/agent/frame; frontend route state should keep anchor optional because `UnboundTrace` sessions can exist (`crates/agentdash-api/src/routes/sessions.rs:167`, `.trellis/spec/backend/workflow/architecture.md:86`).
- `ProjectAgentSessionStartService` creates a runtime session before sending the first message. If initial dispatch fails, cleanup is best effort; UI should be prepared for short-lived idle/empty runtime sessions appearing in list/read paths during failure windows (`crates/agentdash-application/src/workflow/project_agent_session_start.rs:183`, `crates/agentdash-application/src/workflow/project_agent_session_start.rs:225`).
- `AgentRunMessageService` and `AgentRunSteeringService` load current frame with fallback to anchor launch frame. UI should preserve `frame_ref.revision` where supplied, but should not assume frame id never changes across revisions (`crates/agentdash-application/src/workflow/agent_message.rs:194`, `crates/agentdash-application/src/workflow/agent_steering.rs:117`).
- There are two lifecycle read endpoints returning contract views: `/lifecycle-runs/{id}` in `workflows.rs` and `/lifecycle-runs/{id}/view` in `lifecycle_views.rs`. Existing frontend service uses `/lifecycle-runs/{id}`. Avoid introducing a third alias or mixing route assumptions without a cleanup decision (`crates/agentdash-api/src/routes/workflows.rs:120`, `crates/agentdash-api/src/routes/lifecycle_views.rs:37`, `packages/app-web/src/services/lifecycle.ts:31`).
- Generated contracts import some shared DTOs from `project-agent-contracts.ts`; this is by generator design. Frontend code should import through existing app type facades or generated contracts, not redeclare `AgentRunRefDto`/`LifecycleRunRefDto` locally (`crates/agentdash-contracts/src/generate_ts.rs:171`, `packages/app-web/src/generated/workflow-contracts.ts:6`).
- Tests involving Chinese browser input should avoid PowerShell inline Node/Playwright piping per `AGENTS.md`; use UTF-8 script file or escaped strings if E2E adds Chinese prompts.

### Verification Commands

- Contract drift: `pnpm run contracts:check`
- Backend compile: `pnpm run backend:check`
- Focused frontend typecheck: `pnpm run frontend:check`
- Existing lifecycle service tests: `pnpm --filter app-web test -- lifecycle.test.ts`
- Broader task gate if backend DTO/routes are touched: `pnpm run contracts:check && pnpm run backend:check && pnpm run frontend:check`

## External References

- `ts-rs` workspace dependency: version `11.1`, features `serde-json-impl`, `no-serde-warnings`, `chrono-impl` (`Cargo.toml:94`).
- Contract generation command: `cargo run -p agentdash-contracts --bin generate_contracts_ts` (`package.json:44`).
- Contract check command: `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check` (`package.json:45`).

## Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: business HTTP DTOs belong in `agentdash-contracts`, generated TypeScript lives under `packages/app-web/src/generated`, frontend trusts generated wire types (`.trellis/spec/cross-layer/frontend-backend-contracts.md:19`, `.trellis/spec/cross-layer/frontend-backend-contracts.md:25`, `.trellis/spec/cross-layer/frontend-backend-contracts.md:28`).
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: ProjectAgent and Workflow/lifecycle contract sources and local AgentRun DTO naming decisions (`.trellis/spec/cross-layer/frontend-backend-contracts.md:81`, `.trellis/spec/cross-layer/frontend-backend-contracts.md:84`, `.trellis/spec/cross-layer/frontend-backend-contracts.md:99`, `.trellis/spec/cross-layer/frontend-backend-contracts.md:100`, `.trellis/spec/cross-layer/frontend-backend-contracts.md:101`, `.trellis/spec/cross-layer/frontend-backend-contracts.md:103`).
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: frontend lifecycle view model centers on `LifecycleRunView`, `AgentRunView`, `AgentFrameRuntimeView`, not raw runtime session id as lifecycle root (`.trellis/spec/frontend/workflow-activity-lifecycle.md:3`, `.trellis/spec/frontend/workflow-activity-lifecycle.md:9`, `.trellis/spec/frontend/workflow-activity-lifecycle.md:10`, `.trellis/spec/frontend/workflow-activity-lifecycle.md:12`).
- `.trellis/spec/backend/workflow/architecture.md`: graphless Agent runtime creates run/agent/frame/runtime session anchor; `RuntimeSessionExecutionAnchor` is read model projection source (`.trellis/spec/backend/workflow/architecture.md:25`, `.trellis/spec/backend/workflow/architecture.md:56`, `.trellis/spec/backend/workflow/architecture.md:83`, `.trellis/spec/backend/workflow/architecture.md:86`).

## Addendum: SessionMeta Responsibility Split

- Query: `SessionMeta` 当前职责在新 AgentRun / AgentFrame 模型下的归属与迁移风险
- Scope: internal
- Date: 2026-06-11

### Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-spi/src/session_persistence.rs` | `SessionMeta` / `TitleSource` 的 SPI 定义和 SessionStore trait。 |
| `crates/agentdash-application/src/session/types.rs` | `RuntimeTraceLaunchState` 从 `SessionMeta` 只取 runtime trace launch 所需字段。 |
| `crates/agentdash-application/src/session/core.rs` | 创建、读取、更新 session meta；仍以 `SessionMeta` 作为 session shell 元信息。 |
| `crates/agentdash-application/src/session/launch/commit.rs` | turn start 时把 `last_delivery_status` / `last_turn_id` 写入 meta。 |
| `crates/agentdash-application/src/session/eventing.rs` | 来源标题事件投影到 `SessionMeta.title` / `title_source`，并发送 `session_meta_updated`。 |
| `crates/agentdash-infrastructure/src/persistence/session_core.rs` | 从 Backbone envelope 投影 `last_delivery_status`、`turn_id`、terminal message、executor session id。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` | `sessions` 表读写、save merge 规则与事件投影 SQL 更新。 |
| `crates/agentdash-api/src/routes/sessions.rs` | `SessionMeta` 投影到 `SessionShellDto`、`ProjectSessionListEntry`、`SessionRuntimeControlView`。 |
| `crates/agentdash-domain/src/workflow/lifecycle_agent.rs` | 新模型中 `LifecycleAgent` 承载 agent identity、status、bootstrap、current frame。 |
| `crates/agentdash-domain/src/workflow/agent_frame.rs` | 新模型中 `AgentFrame` 承载 effective runtime surface revision。 |
| `packages/app-web/src/services/session.ts` | 旧前端 `SessionMeta` 手写接口，字段名为 camelCase / legacy execution naming。 |
| `packages/app-web/src/pages/SessionPage.tsx` | 当前 SessionPage 从 `runtimeControl.session_meta` 读取标题，并把 runtime-control 注入 workspace runtime data。 |

### Current SessionMeta Shape

- SPI 层 `SessionMeta` 字段包括 `id`, `title`, `title_source`, `created_at`, `updated_at`, `last_event_seq`, `last_delivery_status`, `last_turn_id`, `last_terminal_message`, `executor_session_id` (`crates/agentdash-spi/src/session_persistence.rs:302`).
- `SessionStore` 仍以 `SessionMeta` 为 session shell 的 create/get/list/save/delete 单位 (`crates/agentdash-spi/src/session_persistence.rs:784`).
- 数据库 `sessions` 表直接保存这些字段，`last_delivery_status` 默认 `idle`，`last_turn_id`、`last_terminal_message`、`executor_session_id` 可空，`title_source` 默认 `auto` (`crates/agentdash-infrastructure/migrations/0001_init.sql:658`).
- `SessionCoreService::create_session_with_title_source` 创建普通 session 时初始化 meta：标题、时间戳、`last_event_seq=0`、`last_delivery_status=Idle`、turn/terminal/executor id 为空 (`crates/agentdash-application/src/session/core.rs:43`).
- `SessionPersistenceRuntimeSessionCreator` 在 lifecycle dispatch 创建 runtime session 时也创建同形 `SessionMeta`，标题来自 `runtime_session_title(&request)`，初始状态为 idle，无 executor session id (`crates/agentdash-application/src/workflow/dispatch_service.rs:59`).

### Field Responsibility Classification

| Field / DTO property | Current source | Current consumers | Responsibility in AgentRun model |
| --- | --- | --- | --- |
| `SessionMeta.id` / `runtime_session_id` | Session creation / runtime session creator | route path, anchors, session list, trace navigation | RuntimeSession trace identity；仍应保留在 runtime trace substrate。 |
| `title`, `title_source` | user title patch, source title event, auto title | session list, SessionPage title, shortcut list | RuntimeSession trace shell title today；可作为 workspace tab/list label，但不是 LifecycleAgent identity。若需要 AgentRun Workspace display name，应新增 AgentRun/LifecycleAgent-facing display projection，而不是把 ProjectAgent/subject identity塞进 trace title。 |
| `created_at`, `updated_at` | session row timestamps | SessionShellDto, list sort/display | RuntimeSession trace metadata；workspace list 可复制/投影，但事实源仍是 runtime trace row。 |
| `last_event_seq` | event append projection | runtime-control shell, stream/projection coordination | RuntimeSession event log cursor；应留在 trace metadata。 |
| `executor_session_id` | `PlatformEvent::ExecutorSessionBound` projection | launch planner follow-up, source title validation | Connector-native continuation id；明确是 RuntimeSession trace launch state，不应上移到 AgentFrame。 |
| `last_turn_id` | turn start / envelope projection | command state, runtime-control shell, recovery | RuntimeSession delivery cursor / latest command receipt id；不属于 AgentFrame。AgentRun Workspace 若要展示“最近命令/回执”，应通过 AgentRun command receipt/read model 暴露，而不是让 UI 直接读 `SessionShellDto.last_turn_id`。 |
| `last_terminal_message` | terminal/error envelope projection | hub support/recovery; not in generated `SessionShellDto` | RuntimeSession terminal trace summary；可用于 trace/detail diagnostics，AgentRun Workspace 如需展示失败原因应通过 control-plane status/recent receipt聚合。 |
| `last_delivery_status` / `delivery_status` | turn start / terminal event projection | `SessionCoreService.list_active_sessions`, runtime-control action gating, project session list filtering/display | RuntimeSession delivery status summary。AgentRun Workspace command availability must consume `SessionRuntimeControlView.actions`; AgentRun list should eventually project this into `AgentRunView.last_delivery_status` or a command receipt status instead of coupling UI to `ProjectSessionListEntry.delivery_status`. |

### Code Patterns

- `RuntimeTraceLaunchState` already narrows launch-time trace state to `executor_session_id` and `last_event_seq`, with `From<&SessionMeta>` only copying those fields (`crates/agentdash-application/src/session/types.rs:107`, `crates/agentdash-application/src/session/types.rs:123`). This is the cleanest existing boundary for trace-only metadata.
- Launch planning uses `runtime_trace_state.executor_session_id` as the implicit follow-up session id when no explicit follow-up id is provided (`crates/agentdash-application/src/session/launch/planner.rs:195`). This confirms `executor_session_id` is connector continuation state, not AgentRun/Frame state.
- Turn start writes running status and active `turn_id` to meta (`crates/agentdash-application/src/session/launch/commit.rs:139`). Event projection also derives status from `TurnStarted`, `TurnCompleted`, `Error`, `ExecutorSessionBound`, and `turn_terminal` platform events (`crates/agentdash-infrastructure/src/persistence/session_core.rs:670`).
- Postgres `save_session_meta` uses merge semantics so ordinary meta writes do not roll back event projection fields; `last_event_seq`, status, turn id and terminal message only advance when the excluded event sequence is at least current sequence (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:216`).
- Direct event projection updates `last_delivery_status`, `last_turn_id`, terminal message, and `executor_session_id` onto the `sessions` row (`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:382`).
- Source-provided title updates are accepted only when current title is not user-owned and, when both ids exist, executor session id matches current meta executor session id (`crates/agentdash-application/src/session/eventing.rs:153`). The event then emits `session_meta_updated` with title fields only (`crates/agentdash-application/src/session/eventing.rs:206`).
- `SessionRuntimeControlView` currently embeds `session_meta: SessionShellDto` and computes `delivery_running` from both `meta.last_delivery_status == Running` and live inspected `SessionExecutionState::Running` (`crates/agentdash-api/src/routes/sessions.rs:161`, `crates/agentdash-api/src/routes/sessions.rs:259`). Action availability is then derived from delivery running, terminal agent status, frame presence, and steering support (`crates/agentdash-api/src/routes/sessions.rs:272`).
- `ProjectSessionListEntry.delivery_status` is currently serialized from `meta.last_delivery_status` for both unanchored trace sessions and anchored AgentRun sessions (`crates/agentdash-api/src/routes/sessions.rs:557`, `crates/agentdash-api/src/routes/sessions.rs:596`). `run_status` separately comes from `LifecycleRun.status` when an anchor exists (`crates/agentdash-api/src/routes/sessions.rs:600`).
- `SessionShellDto` generated contract exposes `last_turn_id` and `last_delivery_status` but intentionally omits `last_terminal_message` and `executor_session_id` (`crates/agentdash-contracts/src/workflow.rs:710`).
- `AgentRunView.last_delivery_status` already exists in the generated contract as optional “agent 最新 execution status” (`crates/agentdash-contracts/src/workflow.rs:857`), but the current lifecycle view builder sets it to `None` (`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:351`) while only resolving latest delivery runtime session id from the anchor repo (`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:390`).
- `LifecycleAgent` already carries run-scoped agent identity, `status`, `bootstrap_status`, and `current_frame_id`; its comment explicitly says `bootstrap_status` replaces original `SessionMeta.bootstrap_state` (`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:13`, `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:17`). `AgentFrame` carries effective capability/context/VFS/MCP/execution profile runtime surface revision (`crates/agentdash-domain/src/workflow/agent_frame.rs:6`).
- Legacy frontend `services/session.ts` still defines a hand-written `SessionMeta` with camelCase fields such as `createdAt`, `updatedAt`, `lastEventSeq`, `lastExecutionStatus` (`packages/app-web/src/services/session.ts:20`, `packages/app-web/src/services/session.ts:29`). Current `SessionPage` title path has moved to `runtimeControl.session_meta.title` (`packages/app-web/src/pages/SessionPage.tsx:187`).

### Migration Reading

- `SessionMeta` should remain the compact runtime trace shell for event log cursor, connector continuation, and trace title. This is why `RuntimeTraceLaunchState` only consumes `executor_session_id` and `last_event_seq`, and why title source rules are expressed around source executor session identity.
- `last_delivery_status` is valid as a RuntimeSession delivery summary, but it is now overloaded as AgentRun Workspace state. Runtime-control has already started correcting that by returning derived `actions` and `control_plane`, but project/session lists still expose `delivery_status` directly and UI components use it for status grouping.
- `last_turn_id` is a trace cursor and current/last delivery receipt id. AgentRun command responses already return `turn_id` (`AgentRunMessageResponse`, `ProjectAgentSessionStartResult`) and steering returns `RuntimeSessionCommandStateDto`; if the workspace needs persistent command history or latest command receipt, the target should be an AgentRun command receipt/read model keyed by `run_ref + agent_ref + runtime_session_ref + turn_id`.
- `executor_session_id` should not be exposed in workspace DTOs except diagnostics. It is connector-native continuation state, and the launch planner depends on it for follow-up routing.
- `title` should be treated as RuntimeSession tab/list label. If the product wants AgentRun Workspace names such as project agent display name, subject label, or frame label, those belong in `ProjectSessionListEntry` / `AgentRunView` / dedicated workspace view fields derived from ProjectAgent + subject association + LifecycleAgent, not in `SessionMeta.title`.
- `AgentRunView.last_delivery_status` is the likely intended bridge for agent list/read views, but it is currently unhydrated. Hydrating it from the latest anchored runtime session meta would make AgentRun views less dependent on `ProjectSessionListEntry.delivery_status`; however, action gating should still use `SessionRuntimeControlView.actions`.

### Suggested Implementation Entry Points

- Backend read-model cleanup: start in `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs` and decide whether `AgentRunView.last_delivery_status` should be hydrated from latest anchor -> session meta, or replaced by a more explicit AgentRun delivery summary DTO.
- Runtime-control contract cleanup: start in `crates/agentdash-contracts/src/workflow.rs` and `crates/agentdash-api/src/routes/sessions.rs` if `SessionRuntimeControlView` should expose a narrower `runtime_trace` / `session_shell` section and a separate AgentRun command/control summary.
- Project session list cleanup: start in `crates/agentdash-api/src/routes/sessions.rs::project_session_entry` and `ProjectSessionListEntry`; split `delivery_status` into a trace status and, for anchored sessions, an AgentRun-facing status/command status if the UI should group AgentRun workspaces rather than raw runtime traces.
- Frontend cleanup: `packages/app-web/src/features/agent/active-session-list.tsx` and `packages/app-web/src/components/layout/SessionShortcutList.tsx` currently normalize `entry.delivery_status`; if backend adds AgentRun status, these are the likely consumers to migrate.
- Legacy type cleanup: `packages/app-web/src/services/session.ts` still hand-writes `SessionMeta`. If that service remains needed for trace/session detail, align it with generated `SessionShellDto` / session contracts instead of preserving camelCase legacy names.

### Additional Risks

- Treating `last_delivery_status` as AgentRun status can conflict with `LifecycleAgent.status` (`active` / terminal strings) and `LifecycleRun.status` (`draft` / `ready` / `running` / terminal). UI should not merge these status namespaces without a read-model field that states the projection’s meaning.
- Moving `executor_session_id` out of trace metadata would break continuation routing and source-title validation because both depend on connector-native session identity (`crates/agentdash-application/src/session/launch/planner.rs:195`, `crates/agentdash-application/src/session/eventing.rs:183`).
- Removing or renaming `last_delivery_status` before adding an AgentRun delivery summary would break active-session recovery and runtime-control gating. Existing specs still require persisted `SessionMeta.last_delivery_status` as the execution state summary (`.trellis/spec/backend/quality-guidelines.md:102`, `.trellis/spec/backend/session/runtime-execution-state.md:201`).
- `ProjectSessionListEntry` currently supports unanchored runtime traces by returning only session fields (`crates/agentdash-api/src/routes/sessions.rs:557`). Any new AgentRun Workspace list DTO needs to keep unbound trace handling explicit, because runtime-control also has an `UnboundTrace` branch (`crates/agentdash-api/src/routes/sessions.rs:167`).
- No `CommandReceipt` / AgentRun receipt model was found in code search. Current persistent receipt-like facts are `last_turn_id`, event stream entries, pending messages, and immediate command responses.

## Caveats / Not Found

- 本次只做静态研究，未改源码，未启动服务，未运行测试命令。
- `task.py current --source` 返回 `(none)`，研究目录使用用户明确指定的父任务路径。
- 未发现需要手写前端 DTO 的理由；现有 route state 所需合同基本已经在 `SessionRuntimeControlView`、`ProjectSessionListView`、`AgentRunMessage*`、`AgentRunSteering*` 与 `ProjectAgentSessionStart*` 中生成。
- 未检查数据库 migration 细节；本主题聚焦 API/DTO/contracts 与前端 route state。若后续改变 anchor persistence 字段或 lifecycle read model，需要单独检查 `crates/agentdash-infrastructure/migrations` 与 repository tests。
