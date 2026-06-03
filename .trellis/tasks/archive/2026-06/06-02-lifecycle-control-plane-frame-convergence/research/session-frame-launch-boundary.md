# Research: Session / Frame Launch Boundary

- Query: 当前 Session launch 中哪些 owner/context/capability/VFS/MCP/runtime fact 解析应上提到 Frame construction / FrameLaunchEnvelope？RuntimeLaunchRequest 应如何拆分？
- Scope: internal
- Date: 2026-06-02

## Findings

### Files Found

- `.trellis/spec/backend/session/architecture.md` — Session 子系统目标职责与 RuntimeSession 降级边界。
- `.trellis/spec/backend/session/session-startup-pipeline.md` — 当前 launch pipeline 与目标 construction / launch contract。
- `.trellis/spec/backend/session/execution-context-frames.md` — connector-facing `ExecutionContext` frame 字段来源。
- `.trellis/spec/backend/session/runtime-execution-state.md` — runtime state、pending runtime command、terminal effect 边界。
- `.trellis/spec/backend/capability/architecture.md` — capability dimension / runtime transition 不变量。
- `.trellis/spec/backend/vfs/architecture.md` — VFS surface resolution 与 runtime mount 不变量。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` — RuntimeSession 通过 AgentFrame 反查 subject / run 的目标路径。
- `crates/agentdash-api/src/bootstrap/session_construction_provider.rs` — 当前 API 层 `SessionConstructionProvider` 实现，负责从 runtime session 找 frame 并按 owner/association 重新 compose frame。
- `crates/agentdash-application/src/workflow/runtime_launch.rs` — `RuntimeLaunchRequest` 定义与 `from_frame` projection。
- `crates/agentdash-application/src/workflow/frame_builder.rs` — `AgentFrameBuilder` 写入 capability/context/VFS/MCP/execution profile surface。
- `crates/agentdash-domain/src/workflow/agent_frame.rs` — `AgentFrame` revision、runtime session refs 与 selection policy。
- `crates/agentdash-application/src/session/assembly_builder.rs` — Session assembly 到 frame surface 与 launch extras 的当前桥接。
- `crates/agentdash-application/src/session/construction_use_case.rs` — finalize construction projection，包含 VFS/MCP/capability/runtime command replay。
- `crates/agentdash-application/src/session/construction.rs` — `RuntimeContextInspectionPlan` 字段，仍承载旧 `SessionConstructionPlan` 形态。
- `crates/agentdash-application/src/session/launch/command.rs` — `LaunchCommand` source intent 与 source-local facts。
- `crates/agentdash-application/src/session/launch/planner.rs` — `LaunchPlanner` 单 turn planning、hook/restore/follow-up/backend placement。
- `crates/agentdash-application/src/session/launch/plan.rs` — `LaunchPlan` 到 `ExecutionContext` projection。
- `crates/agentdash-spi/src/connector/mod.rs` — `ExecutionSessionFrame` / `ExecutionTurnFrame` connector contract。

### Code Patterns

- Spec 已规定 `LaunchCommand` 只表达来源意图，不携带最终 VFS/MCP/capability/context/connector facts；`SessionConstructionPlan` 必须在 launch 前产出 owner、workspace、working directory、VFS、MCP、capability、context、identity 与 trace；目标上 `AgentFrame` 是 capability/context/VFS/MCP/runtime refs 的事实源（`.trellis/spec/backend/session/architecture.md:21`, `.trellis/spec/backend/session/architecture.md:22`, `.trellis/spec/backend/session/architecture.md:29`）。
- Spec 已明确 Construction 可以消费 runtime facts，但进入 construction 后必须写入 resolution trace；LaunchPlanner 不允许再读取 cached profile、hub default VFS、local relay workspace root 或 source MCP declaration 补齐 VFS/MCP/capability/executor facts（`.trellis/spec/backend/session/session-startup-pipeline.md:76`）。
- 当前 `RuntimeLaunchRequest` 混合了 frame projection、launch-only payload、typed runtime surface 与 pending/base capability data：frame id/revision/surface 在字段前半段，executor/working dir/prompt/env/identity/terminal hook/context bundle 在同一 struct 中，typed capability/VFS/MCP 与 continuation/base capability 又继续追加（`crates/agentdash-application/src/workflow/runtime_launch.rs:48`, `crates/agentdash-application/src/workflow/runtime_launch.rs:63`, `crates/agentdash-application/src/workflow/runtime_launch.rs:74`）。
- `RuntimeLaunchRequest::from_frame` 已从 `AgentFrame` 投影 execution profile、typed capability、typed VFS、typed MCP，并通过 VFS default mount 推导 working directory。这说明 working directory 仍在 envelope projection 时从 VFS 派生，而不是 frame construction 的显式 launch gate 产物（`crates/agentdash-application/src/workflow/runtime_launch.rs:89`, `crates/agentdash-application/src/workflow/runtime_launch.rs:115`）。
- `RuntimeLaunchRequest` 的 builder methods 继续把 prompt/env、identity、terminal hook、context bundle、executor override、working dir 写回同一个 request，导致它既像 Frame surface，又像 Launch intent，又像 Resolved envelope（`crates/agentdash-application/src/workflow/runtime_launch.rs:181`, `crates/agentdash-application/src/workflow/runtime_launch.rs:192`, `crates/agentdash-application/src/workflow/runtime_launch.rs:198`, `crates/agentdash-application/src/workflow/runtime_launch.rs:216`, `crates/agentdash-application/src/workflow/runtime_launch.rs:222`, `crates/agentdash-application/src/workflow/runtime_launch.rs:228`）。
- `AppStateSessionConstructionProvider::build_frame_construction` 当前先从 runtime session 反查 `AgentFrame`，缺失 frame 直接拒绝 launch；然后按 companion/story/lifecycle node/task/project agent 等路径 compose frame。这条路径已经是 Frame construction 的雏形（`crates/agentdash-api/src/bootstrap/session_construction_provider.rs:80`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:89`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:94`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:144`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:154`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:160`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:170`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:175`）。
- Provider 的 direct ready path 允许在 plain lifecycle 且 request ready 时跳过 compose，只调用 `apply_command_and_extras`。这适合成为 “reuse current frame envelope” 的 fast path，但仍应通过统一 `FrameLaunchEnvelope::validate_for_launch`，而不是由 Session launch 判断 request readiness（`crates/agentdash-api/src/bootstrap/session_construction_provider.rs:103`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:105`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:856`）。
- Provider 通过 `resolve_story_association` / `has_task_association` 从 lifecycle subject association 推导 owner compose path；这类 owner/context 解析应保留在 Frame construction，而不是 Session launch。当前代码已位于 provider，但 provider 名称和返回类型仍是 session construction + `RuntimeLaunchRequest`（`crates/agentdash-api/src/bootstrap/session_construction_provider.rs:229`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:245`）。
- `AssemblyLaunchExtras` 注释说明 context bundle / prompt / executor / MCP / VFS / capability 等 “不写入 AgentFrame，而是传递给 RuntimeLaunchRequest 或 launch pipeline”。这正是拆分点：其中 capability/VFS/MCP/execution profile 已经应进入 frame surface；prompt/env 属于 launch intent；context bundle 应拆成 frame context slice summary + envelope 的 full context payload（`crates/agentdash-application/src/session/assembly_builder.rs:380`, `crates/agentdash-application/src/session/assembly_builder.rs:409`, `crates/agentdash-application/src/session/assembly_builder.rs:411`）。
- `finalize_session_construction_projection` 当前从 source local relay workspace root、source MCP declaration、routine hint、requested runtime commands、extension installations 等 runtime/domain facts 得到 final VFS/MCP/capability/working directory/guidelines/extension runtime，并写 resolution trace。这部分是目标 `FrameConstruction` / `FrameLaunchEnvelope` 的核心，不应在 Session launch 或 planner 中补齐（`crates/agentdash-application/src/session/construction_use_case.rs:56`, `crates/agentdash-application/src/session/construction_use_case.rs:73`, `crates/agentdash-application/src/session/construction_use_case.rs:83`, `crates/agentdash-application/src/session/construction_use_case.rs:93`, `crates/agentdash-application/src/session/construction_use_case.rs:111`, `crates/agentdash-application/src/session/construction_use_case.rs:119`, `crates/agentdash-application/src/session/construction_use_case.rs:146`, `crates/agentdash-application/src/session/construction_use_case.rs:154`, `crates/agentdash-application/src/session/construction_use_case.rs:196`, `crates/agentdash-application/src/session/construction_use_case.rs:210`）。
- `LaunchCommand` 目前携带 user input、identity、task/companion/routine hints、local relay MCP declarations、local relay workspace root、follow-up session id。按 spec，这些应保持 “source intent / source-local hints” 身份；最终 VFS/MCP/capability/working dir 必须由 construction/envelope 解析（`crates/agentdash-application/src/session/launch/command.rs:22`, `crates/agentdash-application/src/session/launch/command.rs:64`, `crates/agentdash-application/src/session/launch/command.rs:68`, `crates/agentdash-application/src/session/launch/command.rs:72`, `crates/agentdash-application/src/session/launch/command.rs:76`, `crates/agentdash-application/src/session/launch/command.rs:80`, `crates/agentdash-application/src/session/launch/command.rs:84`, `crates/agentdash-application/src/session/launch/command.rs:92`）。
- `LaunchPlanner` 当前只 gate `working_directory`、`executor_config`、`typed_capability_state`，但仍计算 default mount root、resolve hook runtime、repository restore、follow-up、terminal hook handler、backend execution placement。它应只消费 envelope，保留 turn-only planning；不应再拥有任何 owner/context/capability/VFS/MCP 解析职责（`crates/agentdash-application/src/session/launch/planner.rs:48`, `crates/agentdash-application/src/session/launch/planner.rs:53`, `crates/agentdash-application/src/session/launch/planner.rs:58`, `crates/agentdash-application/src/session/launch/planner.rs:63`, `crates/agentdash-application/src/session/launch/planner.rs:124`, `crates/agentdash-application/src/session/launch/planner.rs:179`, `crates/agentdash-application/src/session/launch/planner.rs:201`, `crates/agentdash-application/src/session/launch/planner.rs:218`, `crates/agentdash-application/src/session/launch/planner.rs:221`）。
- Backend placement currently uses `req.typed_vfs` to infer preferred backend and writes lease root from default mount. This is runtime lease planning and can stay in Session launch, but its input should be a resolved `BackendSelectionHint` / connector input projection from envelope, not raw VFS resolution logic in planner（`crates/agentdash-application/src/session/launch/planner.rs:291`, `crates/agentdash-application/src/session/launch/planner.rs:311`, `crates/agentdash-application/src/session/launch/planner.rs:379`, `crates/agentdash-application/src/session/launch/planner.rs:411`）。
- `LaunchPlan::build` converts request fields into `ExecutionSessionFrame` and currently leaves `ExecutionTurnFrame.context_frames` and `assembled_tools` empty at this stage. This supports a split where FrameLaunchEnvelope owns resolved facts, while TurnPreparer remains responsible for runtime tool assembly and final context frame materialization（`crates/agentdash-application/src/session/launch/plan.rs:150`, `crates/agentdash-application/src/session/launch/plan.rs:260`, `crates/agentdash-application/src/session/launch/plan.rs:275`, `crates/agentdash-application/src/session/launch/plan.rs:280`, `crates/agentdash-application/src/session/launch/plan.rs:281`）。
- SPI `ExecutionSessionFrame` / `ExecutionTurnFrame` already express the final connector projection: working directory, env, executor config, MCP, VFS, backend placement, identity on session frame; capability state, hook runtime, context frames, assembled tools on turn frame. This should be generated from envelope + launch plan, not used as a fact source（`crates/agentdash-spi/src/connector/mod.rs:64`, `crates/agentdash-spi/src/connector/mod.rs:66`, `crates/agentdash-spi/src/connector/mod.rs:74`, `crates/agentdash-spi/src/connector/mod.rs:75`, `crates/agentdash-spi/src/connector/mod.rs:82`, `crates/agentdash-spi/src/connector/mod.rs:97`, `crates/agentdash-spi/src/connector/mod.rs:99`, `crates/agentdash-spi/src/connector/mod.rs:109`, `crates/agentdash-spi/src/connector/mod.rs:114`, `crates/agentdash-spi/src/connector/mod.rs:122`）。

### What Should Move Up To Frame Construction / FrameLaunchEnvelope

1. Owner and business context resolution should be fully above Session launch:
   - RuntimeSession -> AgentFrame lookup and the hard requirement that every launch has a frame.
   - LifecycleAgent / LifecycleRun loading.
   - `LifecycleSubjectAssociation` inspection for story/task/project-agent compose path.
   - graph instance / activity key / procedure resolution for lifecycle node execution.
   - companion parent/session/workflow slice resolution.
   - routine source projection into VFS/skills.

2. Workspace and working directory should become explicit envelope facts:
   - Frame construction should resolve workspace owner and final VFS default mount.
   - Envelope should carry `working_directory` as required, traced output.
   - `RuntimeLaunchRequest::from_frame` should not silently derive or override working directory from typed VFS; derivation should happen in construction with a validation trace.

3. Capability/VFS/MCP should be a single normalized frame projection:
   - Frame construction should fold owner baseline, agent preset directives, workflow/lifecycle activation, local relay MCP declarations, visible canvas mounts, routine mounts, extension runtime projection, pending frame transitions, and skill/guideline derivation.
   - Envelope should carry the final `CapabilityState`, final `Vfs`, final `Vec<SessionMcpServer>`, `SessionBaselineCapabilities`, discovered guidelines, and resolution trace.
   - Session launch should only validate equality/invariants and project to connector/tool assembly.

4. Runtime delivery command replay should move into frame construction:
   - Requested runtime delivery commands already reference `AgentFrameTransitionRecord`.
   - Replay should produce final capability/VFS/MCP and base capability state before Session launch.
   - Session launch should retain only accepted-after-connector side effect planning: mark requested commands `applied` or `failed`.

5. Identity, source policy, audit/query/inspector projections should be construction/envelope facts:
   - `LaunchCommand.identity()` can remain source intent.
   - Frame construction should decide effective identity and record the source.
   - Context endpoint, inspector, audit, and connector launch should all observe the same envelope/construction projection.

6. Context bundle handling should split summary vs full payload:
   - `AgentFrame.context_slice_json` can keep durable summary (`bundle_id`, phase tag, fragment count).
   - `FrameLaunchEnvelope` should carry the full launch context bundle / context frames needed for this turn.
   - `LaunchPlanner` may merge hook snapshot contribution because hook runtime is turn lifecycle, but the resulting connector context should be an explicit launch-plan projection, not mutate a generic request.

7. Extension runtime and discovered guidelines belong in envelope:
   - They are derived from project installations and VFS/skill baseline during construction.
   - They should not be optional extras patched onto a request after frame persistence.

8. Backend selection should be split:
   - Business/backend hints from VFS should be computed as envelope `BackendSelectionHint` or connector input hint.
   - Claiming/releasing backend execution lease remains Session launch/turn runtime responsibility because it is a per-turn runtime effect.

### Recommended RuntimeLaunchRequest Split

Replace the current monolithic `RuntimeLaunchRequest` with four smaller concepts:

1. `FrameLaunchIntent`
   - Source adapter output for a launch against an existing or newly constructed frame.
   - Fields: runtime session id, command source, prompt payload or prompt source hint, executor override, env override, identity source, follow-up hint, task/story/routine/companion/local-relay hints.
   - It must not carry final VFS/MCP/capability/context/executor/working directory.
   - This is close to today’s `LaunchCommand`, so it may be implemented as a renamed/narrowed `LaunchCommand` plus typed source hints.

2. `AgentFrameConstructionPlan` / `FrameConstructionResult`
   - Internal builder output before/after persisting a frame revision.
   - Fields: owner refs, run/agent/frame refs, subject association refs, graph/activity/procedure refs, resolved workspace, final capability state, VFS, MCP servers, execution profile, context summary, extension runtime, discovered guidelines, base capability state, runtime transition replay result, resolution trace.
   - Used to write `AgentFrame` revision and to build the envelope.
   - Not public API and not connector input.

3. `FrameLaunchEnvelope`
   - The launch-ready immutable handoff from frame construction to session launch.
   - Fields should be required where launch cannot proceed: `runtime_session_id`, `frame_ref`, `agent_ref`, optional `procedure_ref`, optional graph/activity refs, `working_directory`, `executor_config`, `capability_state`, `vfs`, `mcp_servers`, `context_bundle` or `context_frames_seed`, `identity`, `environment_variables`, `prompt_blocks`, `terminal_effect_binding`, `discovered_guidelines`, `extension_runtime`, `base_capability_state`, `requested_runtime_commands`, `pending_transitions`, `backend_selection_hint`, `resolution_trace`.
   - Has `validate_for_launch()` enforcing working dir, executor, VFS/capability/MCP equality, runtime command target-frame consistency, and trace completeness.
   - This is the main replacement for today’s `RuntimeLaunchRequest` in `LaunchPlannerInput`.

4. `ConnectorLaunchInput` / `ExecutionContextProjection`
   - Pure connector projection built from `FrameLaunchEnvelope + LaunchPlan`.
   - Fields map directly to `ExecutionSessionFrame` and `ExecutionTurnFrame`: session frame facts, turn capability, hook runtime, restored state, assembled tools, context frames, backend placement.
   - It should not expose owner association or construction trace except diagnostics.

### Session Launch Responsibilities To Keep

- Turn claim / active turn / cancel / cleanup.
- Reading session meta only for runtime lifecycle facts that are still session-owned: existing runtime status, session event restore, follow-up executor session id if this remains runtime trace metadata.
- Connector capability checks such as repository restore support.
- Hook runtime handle creation and hook delegate wiring, because this is per-turn runtime setup. The hook snapshot source should still be frame-aware.
- Resolving prompt payload from envelope prompt blocks plus source payload.
- Backend execution lease claim/activate/fail/release, using envelope hints.
- `connector.prompt` accepted boundary and accepted-after side effects.
- Persisting user message / `TurnStarted` / context-capability projection events / runtime command applied or failed / title derivation.
- Stream ingestion, terminal event persistence, terminal effect outbox, and cleanup.

### External References

- No external references used. This was a repository-only boundary review.

### Related Specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`

## Caveats / Not Found

- No `FrameLaunchEnvelope` type exists yet in the searched codebase; this research uses it as the proposed target contract name.
- Current specs still mention `SessionConstructionPlan` as the production baseline while also saying it will be lowered into AgentFrame builder internals; implementation is mid-migration, so some naming is intentionally transitional.
- I did not inspect every route or frontend consumer; this research is scoped to backend Session/Frame launch construction boundaries.
- I did not run tests or modify code; only this research artifact was written.
