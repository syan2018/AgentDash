# AgentRun 启动主线架构收束实施计划

## Planning Gate

- 本任务处于 planning 状态；不得直接开始实现。
- 本任务按一个大任务执行，不拆 child task。
- 实施时按 phase 分阶段提交。每个 phase 的提交都必须保持代码可编译，并尽量让阶段性 tests 能说明当前边界已经稳定。
- Phase 5 是专门的旧模型清理 phase；没有通过 Phase 5 的 check-agent review gate，不得进入最终规格更新和归档。

## Optimal Path

最优路径是先建立新模型主干，再逐步把旧模型依赖剥离，最后用专项清理审计删除残留：

```text
Phase 0 Baseline guard
  -> Phase 1 AgentRun intake/receipt/mailbox 状态机
  -> Phase 2 FrameLaunchEnvelope closure 和 accepted 边界
  -> Phase 3 Runtime tools declaration/invocation 分层
  -> Phase 4 Runtime delegate 阶段化
  -> Phase 5 旧模型清理专项 phase + trellis-check gate
  -> Phase 6 Frontend/contracts/spec/final validation
```

这个顺序的原因是：如果先清 runtime tools/delegate，ProjectAgent 启动仍会通过旧同步包装制造半成品状态；如果先做 frontend/contracts，后端 accepted 边界仍不稳定。先把 intake 和 frame closure 做正确，后续清理才有明确目标。

## Phase Plan

### Phase 0: Baseline Guard And Search Map

- Record current old-model symbols and call graph before edits。
- Add or identify failing/expected regression targets for startup crash path where feasible。
- Build a search checklist for Phase 5 cleanup。

Review output:

- A short note in task artifacts or commit message listing old-model search terms and expected removal/rewrite targets。

Commit shape:

- Optional docs/test-only commit if baseline tests are added before behavior changes。

### Phase 1: AgentRun Intake And Mailbox State Machine

- Review `project_agent_run_start.rs`、`agent_run_mailbox.rs`、`workflow/agent_message.rs`、API route response contract。
- Make ProjectAgent start create AgentRun thread/anchor/initial mailbox envelope and return durable scheduler projection。
- Remove nested synchronous dependency where ProjectAgent start waits for inner launch as its own accepted boundary。
- Make first user message and composer submit share mailbox command/outcome path。
- Add recovery coverage for consuming launch message without accepted refs。

Review gate:

- ProjectAgent first message and composer submit have the same scheduler outcome contract。
- Existing DB half-state from the incident maps to a defined recovery projection。

Commit shape:

- `refactor(agentrun): 收束启动输入状态机`

### Phase 2: FrameLaunchEnvelope Closure And Accepted Boundaries

- Review frame construction owner bootstrap path and `SessionLaunchOrchestrator` planning/preparation split。
- Move VFS/MCP/capability/executor/context closure validation to frame construction boundary。
- Ensure `TurnPreparer` only derives tools/context/connector projection from launch-ready facts。
- Split command receipt accepted, mailbox delivery accepted, session turn accepted and frame/bootstrap accepted in code and tests。
- Add tests for missing/mismatched launch surface rejection before connector prompt。

Review gate:

- No launch path can reach connector prompt with incomplete launch surface。
- Empty initial AgentFrame is either a controlled transient before envelope closure or a recoverable failure state, not a normal accepted frame。

Commit shape:

- `refactor(session): 收束启动闭包与 accepted 边界`

### Phase 3: Runtime Tool Declaration/Invocation Split

- Review `SessionRuntimeToolComposer` and VFS/workflow/collaboration/workspace-module providers。
- Ensure `build_tools(context)` is side-effect-light and does not call gateway invoke/session launch/control-plane mutation。
- Keep provider-visible schema generation bounded and testable。
- Add regression around tool schema/token estimation path if stack overflow reproduction is reachable。

Review gate:

- Tool declaration path only builds names, descriptions, schemas and capability gates。
- RuntimeGateway/local backend/extension action calls appear only in invocation adapters or equivalent tool-call execution paths。

Commit shape:

- `refactor(runtime): 拆分工具声明与调用边界`

### Phase 4: Runtime Delegate Stage Separation

- Fix no-inner `transform_context` behavior so it preserves input semantics。
- Review `before_stop` + `allow_empty` continuation to prevent empty infinite continuation。
- Split mailbox boundary drain and hook routing into named internal components, or define new narrow traits if implementation pressure justifies it。
- Add agent loop tests for empty continue, mailbox boundary, and hook follow-up semantics。

Review gate:

- Missing inner delegate cannot erase user input or provider-visible messages。
- Empty continue cannot spin without a concrete queued message, hook event, or bounded stop policy reason。

Commit shape:

- `refactor(agent): 拆分运行时 delegate 阶段语义`

### Phase 5: Old Model Cleanup And Check-Agent Gate

This phase is mandatory and exists specifically to prevent incomplete cleanup.

Cleanup checklist:

- Search and remove/rewrite old ProjectAgent synchronous launch bridge:
  - `launch_initial_user_message`
  - `ProjectAgentRunInitialMessagePort`
  - `AgentRunMessageLaunchDeliveryPort`
  - `accepted_refs_from_initial_launch`
  - any path where `project_agent_start` accepted depends on `SessionLaunchService::launch_command` completing。
- Search and remove/rewrite route-local launch inference:
  - frontend/API paths treating ProjectAgent start success as connector accepted。
  - hand-written pending/message aliases that duplicate generated mailbox/workspace DTOs。
- Search and remove/rewrite launch surface fallback:
  - `TurnPreparer` or launch deps filling missing VFS/MCP/capability/executor facts。
  - duplicated `build_tools_for_execution_context` paths that can diverge。
- Search and remove/rewrite runtime tool declaration side effects:
  - provider-visible `build_tools` invoking gateway/session/control-plane side effects。
  - schema generation requiring live runtime action execution。
- Search and remove/rewrite delegate black-box semantics:
  - no-inner transform returning empty provider-visible messages。
  - `allow_empty=true` continue without guard。

Check-agent review gate:

- Spawn `trellis-check` after Phase 5 implementation with task description:
  - Active task: `.trellis/tasks/06-15-agentrun-startup-architecture-convergence`
  - You are already the `trellis-check` sub-agent; review directly and do not spawn implement/check sub-agents.
  - Perform a legacy-model cleanup audit. Confirm whether old ProjectAgent synchronous launch, route-local start inference, launch-surface fallback, tool declaration side effects, and delegate black-box semantics have been removed or rewritten into the new model.
  - Include exact source searches performed, residual hits, and whether each residual hit is valid under the new model.
  - Fix any clear residual cleanup issues directly; otherwise report blockers.

Gate pass criteria:

- The check agent reports no executable old-model path remains。
- Any residual old names are either removed, renamed, or documented as new-model implementation details with no old behavior。
- Targeted tests plus source-search audit pass。

Commit shape:

- `refactor(agentrun): 清理旧启动模型残留`

### Phase 6: Frontend, Contracts, Specs, Final Validation

- Ensure generated contracts expose the durable AgentRun command/mailbox/workspace projections the UI should consume。
- Remove frontend inference that treats ProjectAgent start response as proof that connector accepted the first turn。
- Keep UI behavior based on backend scheduler outcome and stream/projection updates。
- Update session startup, AgentRun mailbox, runtime tool/capability, and runtime gateway specs with final decisions。
- Record why launch-ready frame closure and split accepted boundaries are the stable model。
- Run `pnpm dev` to start the full local development stack, including Rust binary compilation, cloud backend, local backend and frontend。
- Through the frontend, create a new session and send two consecutive user messages in the same conversation。Confirm startup, first response, second response, stream/WebSocket and backend process remain healthy。

Review gate:

- Frontend consumes generated contract DTOs for AgentRun command/mailbox/workspace projection。
- Specs explain the new model in terms of why the boundaries exist。
- Manual end-to-end session smoke test passes through `pnpm dev` with two user turns in one session。

Commit shape:

- `refactor(frontend): 对齐 AgentRun 启动投影`
- `docs(spec): 记录 AgentRun 启动主线模型`

## Validation Commands

Run targeted checks as implementation lands:

```powershell
cargo test -p agentdash-application agent_run_mailbox
cargo test -p agentdash-application session::launch
cargo test -p agentdash-agent runtime_alignment
cargo check -p agentdash-api -p agentdash-application -p agentdash-executor -p agentdash-agent
pnpm run contracts:check
pnpm run frontend:check
pnpm dev
```

Use narrower commands when a phase only touches one area. Avoid unrelated full-suite expansion unless code changes cross broad contracts.

`pnpm dev` is mandatory only in final validation. Before running it after Rust backend changes, stop any previous dev stack because the Rust backend does not hot reload reliably in this project.

## Risky Files

- `crates/agentdash-application/src/workflow/project_agent_run_start.rs`
- `crates/agentdash-application/src/session/agent_run_mailbox.rs`
- `crates/agentdash-application/src/session/launch/orchestrator.rs`
- `crates/agentdash-application/src/session/launch/preparation.rs`
- `crates/agentdash-application/src/session/launch/planner.rs`
- `crates/agentdash-application/src/session/mailbox_delegate.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-application/src/vfs/tools/provider.rs`
- `crates/agentdash-api/src/bootstrap/session.rs`
- `crates/agentdash-api/src/routes/project_agents.rs`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts`

## Review Gates

- Before implementation: user confirms this single-task phase plan and approves `task.py start`。
- After each phase: inspect receipt/mailbox/frame/session consistency and run targeted tests for touched surface。
- After Phase 5: mandatory `trellis-check` legacy-model cleanup audit; final phase cannot start until this passes。
- Before each phase commit: run targeted Rust tests or checks aligned to touched files。
- Before archive: update `.trellis/spec` with final model, run final targeted backend checks, contract check, frontend check, then run `pnpm dev` and complete the two-message frontend session smoke test。

## Rollback Points

- Phase 1 must land before FrameLaunchEnvelope changes unless implementation proves the current boundary makes that impossible。
- Runtime tools/delegate changes should not be mixed into Phase 1 unless they are required to keep startup tests green。
- If Phase 5 cleanup audit finds a surviving old-model path, stop finalization and either fix in Phase 5 or return to design before proceeding。
- If any phase reveals the design is incomplete, return to planning and update `prd.md`/`design.md` before continuing。
