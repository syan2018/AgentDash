# Research: Phase 4 StepActivation / ContinueRoot follow-up

- Query: Phase 4 follow-up：`StepActivation` 是否已纳入 `AgentFrameBuilder` 内部阶段；`ContinueRoot` 是否已改为 Agent reuse + RuntimeSession policy 组合，并明确多 RuntimeSession selection policy。
- Scope: internal
- Date: 2026-06-01

## Findings

### Files Found

- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md` - Boundary 5 要求 `AgentFrameTransition` 改 frame truth、`RuntimeDeliveryCommand` 只负责投递到 RuntimeSession。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/structural-analysis.md` - P1-09/P1-10/P1-11 定义本次 follow-up 的结构风险与目标策略形状。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md` - Phase 4 checklist 当前标记：transition/delivery 与多 RuntimeSession selection 已完成；StepActivation、Hook/capability target、ContinueRoot policy split 未完成。
- `.trellis/spec/project-overview.md` - 项目总览定义 `AgentFrame` 是 effective runtime surface，`RuntimeSession` 是 trace 容器。
- `.trellis/spec/backend/session/architecture.md` - Session 目标语义是 RuntimeSession；runtime delivery command 与 AgentFrameTransitionRecord 的分层已经进入 spec。
- `.trellis/spec/backend/session/runtime-execution-state.md` - pending runtime command 的当前事实源 / delivery outbox 契约。
- `.trellis/spec/backend/workflow/architecture.md` - Agent Activity execution identity 由 `AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)` 定位。
- `.trellis/spec/backend/story-task-runtime.md` - Story/Task 不持有 RuntimeSession truth，Task execution 通过 SubjectRef / ExecutionIntent 进入 lifecycle。
- `crates/agentdash-application/src/workflow/step_activation.rs` - `StepActivationInput` / `StepActivation` / `activate_step_with_platform` 与 running-session applier。
- `crates/agentdash-application/src/workflow/mod.rs` - `StepActivation` family 仍 public re-export。
- `crates/agentdash-application/src/workflow/frame_builder.rs` - `AgentFrameBuilder` 当前仍是已解析 surface 的 revision writer。
- `crates/agentdash-application/src/session/assembler.rs` / `crates/agentdash-application/src/session/assembly_builder.rs` - session assembly 仍在 builder 外部调用 activation，再投影到 frame builder。
- `crates/agentdash-application/src/workflow/agent_executor.rs` - `ContinueRoot` 与 root RuntimeSession 的核心耦合点。
- `crates/agentdash-domain/src/workflow/dispatch.rs` / `crates/agentdash-application/src/workflow/dispatch_service.rs` - 已有 typed intent、`AgentPolicy` / `RuntimePolicy`，但不是目标 `AgentReusePolicy` + `RuntimeSessionPolicy`。
- `crates/agentdash-domain/src/workflow/agent_frame.rs` / `crates/agentdash-application/src/workflow/runtime_launch.rs` - 当前 `RuntimeSessionSelectionPolicy` 与 launch request 投影。
- `crates/agentdash-spi/src/session_persistence.rs` / `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` / `crates/agentdash-infrastructure/migrations/0035_session_runtime_commands.sql` / `0088_agent_frame_transition_delivery_commands.sql` - transition/delivery split 当前实现。
- `crates/agentdash-application/src/session/capability_service.rs` / `session/hub/runtime_context_transition.rs` / `session/hub/tool_builder.rs` - live/pending runtime context transition 的 session delivery 与 frame target 混合状态。
- `crates/agentdash-application/src/session/hooks_service.rs` / `workflow/frame_hook_runtime.rs` / `crates/agentdash-spi/src/hooks/mod.rs` - Hook runtime 已有 frame implementation，但 SPI/query 仍 session-shaped。
- `crates/agentdash-application/src/task/service.rs` / `task/context_builder.rs` / `companion/tools.rs` / `reconcile/terminal_cancel.rs` - RuntimeSession selection policy 与绕过 policy 的 call site。

### Related Specs

- `.trellis/spec/project-overview.md:33` 将 `LifecycleAgent` 定义为运行身份，将 `AgentFrame` 定义为 procedure/capability/context/VFS/MCP/runtime refs 的 effective runtime surface。
- `.trellis/spec/project-overview.md:37` 和 `.trellis/spec/backend/session/architecture.md:5` 都明确 RuntimeSession 不拥有 business ownership、permission scope 或 lifecycle progress truth。
- `.trellis/spec/backend/session/architecture.md:29` 要求 `AgentFrame` 是 capability/context/VFS/MCP/runtime refs 的事实源，`SessionConstructionPlan` / `LaunchPlan` 最终应降为 frame builder / runtime adapter 内部结构。
- `.trellis/spec/backend/session/architecture.md:30` 要求业务 command path 从 `ExecutionIntent`、`SubjectRef`、run/agent/frame refs 或 graph instance refs 开始。
- `.trellis/spec/backend/session/architecture.md:34` 与 `.trellis/spec/backend/session/runtime-execution-state.md:121` 到 `:138` 记录当前已落地的 `AgentFrameTransitionRecord` / `RuntimeDeliveryCommand` 分层。
- `.trellis/spec/backend/workflow/architecture.md:15` 要求 Agent Activity execution identity 由 assignment + frame 定位，RuntimeSession 只是 evidence。
- `.trellis/spec/backend/story-task-runtime.md:30` 到 `:31` 要求 Task runtime truth 属于 lifecycle / assignment / projection 层，execution 通过 SubjectRef / ExecutionIntent 进入。

### External References

- 未使用外部资料；本次为纯内部代码调研。
- 未运行测试；下方 gate 给出建议验证命令。

## StepActivation Current State

### Public / export surface

- `StepActivationInput` 是 public struct，见 `crates/agentdash-application/src/workflow/step_activation.rs:44`。
- `StepActivation` 是 public struct，见 `crates/agentdash-application/src/workflow/step_activation.rs:102`。
- `activate_step_with_platform` 是 public function，见 `crates/agentdash-application/src/workflow/step_activation.rs:123`。
- `workflow::mod` 仍 public re-export `KickoffPromptFragment`、`StepActivation`、`StepActivationInput`、`activate_step_with_platform` 及一组 activation helper，见 `crates/agentdash-application/src/workflow/mod.rs:81` 到 `:84`。
- `apply_to_running_session` 不是 public re-export，但仍是 `pub(crate)` 并被 workflow executor 直接 import，见 `crates/agentdash-application/src/workflow/step_activation.rs:289` 与 `crates/agentdash-application/src/workflow/agent_executor.rs:30`。

### Call paths

- Session assembly 仍直接 import `StepActivationInput` / `activate_step_with_platform`，见 `crates/agentdash-application/src/session/assembler.rs:81`。
- lifecycle node compose 在 builder 外调用 activation，见 `crates/agentdash-application/src/session/assembler.rs:1290` 到 `:1291`；随后 `SessionAssemblyBuilder::apply_lifecycle_activation` 消费 `StepActivation` 的 VFS/capability/MCP，见 `crates/agentdash-application/src/session/assembly_builder.rs:311` 到 `:327`。
- `project_assembly_to_frame` 再把 prepared assembly 写入 `AgentFrameBuilder`，见 `crates/agentdash-application/src/session/assembly_builder.rs:377` 到 `:415`。这说明当前 builder 是 surface writer，不是 StepActivation stage owner。
- companion compose 也在 builder 外调用 activation，并直接读写 activation 的 lifecycle mount / capability / MCP，见 `crates/agentdash-application/src/session/assembler.rs:1619` 到 `:1685`。
- companion skill projection 直接依赖 `StepActivation` 并原地修改 `activation.lifecycle_vfs` / `activation.lifecycle_mount`，见 `crates/agentdash-application/src/companion/skill_projection.rs:9` 与 `:50` 到 `:70`。这是把 `StepActivation` 降级为 builder-private stage 时必须一起收束的 call site。

### Paths that can bypass AgentFrameBuilder stage and apply session

- `AgentActivityExecutor` 的 ContinueRoot live path 用 root session 创建 hook runtime：`ensure_hook_runtime(root_runtime_session_id, None)`，见 `crates/agentdash-application/src/workflow/agent_executor.rs:353` 到 `:357`。
- 同一路径随后用 root session 读取 MCP surface，再直接计算 activation，见 `agent_executor.rs:363` 到 `:367`，并调用 `apply_to_running_session`，见 `agent_executor.rs:391` 到 `:400`。
- `apply_to_running_session` 以 `hook_runtime.session_id()` 为入口，通过 `resolve_runtime_session_frame_id(session_id)` 反查 frame，再用 `get_current_capability_state(session_id)` 读取 running projection，见 `crates/agentdash-application/src/workflow/step_activation.rs:298` 到 `:305`。
- live path 最终会进入 `replace_current_capability_state(AgentFrameRuntimeTarget { frame_id, delivery_runtime_session_id })`，见 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:110` 到 `:117`；`tool_builder` 说明该 primitive 会写 AgentFrame revision，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:101` 到 `:108`。所以当前不是完全不写 frame，而是 workflow executor 可以绕过 builder-owned resolution stage，直接以 session delivery 执行 activation apply。
- ContinueRoot no-live-hook path 用 root session 取 latest capability state，见 `agent_executor.rs:407`，再由 `resolve_runtime_session_frame_id(root_runtime_session_id)` 得到 target frame，见 `agent_executor.rs:471` 到 `:472`，最后构造 `PendingRuntimeContextTransitionInput { target_frame_id, delivery_runtime_session_id: root_runtime_session_id, ... }`，见 `agent_executor.rs:475` 到 `:479`。这条 pending path 直接生成 `AgentFrameTransitionRecord` + `RuntimeDeliveryCommand`，没有经过 AgentFrameBuilder。
- `SessionCapabilityService` 仍暴露 session-first API：`get_runtime_mcp_servers(session_id)`、`get_current_capability_state(session_id)`、`get_latest_capability_state(session_id)`、`resolve_runtime_session_frame_id(session_id)`，见 `crates/agentdash-application/src/session/capability_service.rs:32` 到 `:49`。
- Hook runtime 创建入口仍是 session lookup：`build_frame_hook_runtime(..., session_id, ...)` 调 `find_by_runtime_session(session_id)`，见 `crates/agentdash-application/src/session/hooks_service.rs:162` 到 `:194`。

### Over-coupling judgment

`AgentFrameBuilder` 已经负责 revision 写入与 carry-forward，但 `StepActivation` 仍是 public DTO / public function，且多个 application 模块在 builder 外部读取或修改 activation。ContinueRoot live/pending path 还允许 workflow executor 拿 activation output 直接对 running RuntimeSession 做 delivery。因此 P1-09 未关闭；当前状态是“frame revision 写入部分进了 builder，surface resolution / apply orchestration 没进 builder-owned stage”。

## ContinueRoot Current State

### Dependency on root RuntimeSession

- `AgentActivityLaunchContext` 直接持有 `root_runtime_session_id`，见 `crates/agentdash-application/src/workflow/agent_executor.rs:37` 到 `:40`。
- `start_continue_root` 要求 root session 非空，见 `agent_executor.rs:678` 到 `:681`。
- 它用 root session 读取 executor config，见 `agent_executor.rs:684` 到 `:687`。
- 它创建 assignment 时把 `Some(&self.context.root_runtime_session_id)` 传入 port，见 `agent_executor.rs:689` 到 `:697`；port implementation 再把该 runtime session ref 写入新 AgentFrame，见 `agent_executor.rs:216` 到 `:257`，尤其 `with_runtime_session(runtime_session_id)` 在 `:250` 到 `:251`。
- 它调用 `apply_continue_root_activity(..., &self.context.root_runtime_session_id)`，见 `agent_executor.rs:702` 到 `:708`。
- 它返回的 executor run 仍是 `ExecutorRunRef::RuntimeSession { session_id: root_runtime_session_id }`，见 `agent_executor.rs:712` 到 `:713`。
- tests 也固定以 `"root-session"` 表达 ContinueRoot，见 `agent_executor.rs:1386` 到 `:1389` 与 `:1420` 到 `:1422`。

### Existing policy types

- Dispatch 层已存在 typed intent family：`AgentLaunchIntent`、`SubjectExecutionIntent`、`LifecycleRunStartIntent`，见 `crates/agentdash-domain/src/workflow/dispatch.rs:110`、`:131`、`:151`。
- Dispatch result 已要求 assignment ref：`AgentLaunchDispatchResult.assignment_ref` 与 `SubjectExecutionDispatchResult.assignment_ref`，见 `dispatch.rs:186` 到 `:204`，并有 serialization test，见 `dispatch.rs:279` 到 `:297`。
- 已有 `AgentPolicy::{Create, Reuse, Resume, SpawnChild}`，见 `dispatch.rs:36` 到 `:40`。
- 已有 `RuntimePolicy::{CreateRuntimeSession, AttachExisting(Uuid), ContinueCurrent(Uuid)}`，见 `dispatch.rs:64` 到 `:68`。
- Capability live update 已有 `AgentFrameRuntimeTarget { frame_id, delivery_runtime_session_id }`，见 `crates/agentdash-application/src/session/types.rs:58` 到 `:65`。
- 多 RuntimeSession refs 已有 `RuntimeSessionSelectionPolicy`，见 `crates/agentdash-domain/src/workflow/agent_frame.rs:13` 到 `:19`。

### Remaining coupling

- Activity definition contract 仍是 `AgentSessionPolicy::{SpawnChild, ContinueRoot, AttachExisting}`，见 `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:51` 到 `:55`；generated contract 同步保留 `ContinueRoot`，见 `crates/agentdash-contracts/src/workflow.rs:240` 到 `:244`。
- Freeform graph 与 ProjectAgent route 仍生成 `AgentSessionPolicy::ContinueRoot` activity，见 `crates/agentdash-application/src/workflow/freeform.rs:86` 到 `:91` 与 `crates/agentdash-api/src/routes/project_agents.rs:578` 到 `:583`。
- Projection/assembler 仍把 `ContinueRoot` 映射成 PhaseNode 语义，见 `crates/agentdash-application/src/workflow/projection.rs:55` 到 `:64` 与 `crates/agentdash-application/src/session/assembler.rs:1365` 到 `:1372`。
- `LifecycleDispatchService::resolve_or_create_agent` 对 `Reuse | Resume` 只是取 run 内第一个 active agent，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:507` 到 `:518`；没有 `ReuseBySubject` / `ReuseByRoutineEntity` / `ContinueCurrentAgent` 这类 resolver boundary。
- `LifecycleDispatchService::resolve_or_create_runtime_session` 对 `AttachExisting | ContinueCurrent` 直接返回给定 UUID，见 `dispatch_service.rs:634` 到 `:641`；没有 `ResumeLatestTrace` / `DeliverToActiveTrace` / `CreateNewIfNone` 这类 RuntimeSession policy。

### Over-coupling judgment

P1-10 未关闭。当前系统已经有初版 `AgentPolicy` / `RuntimePolicy`，但 ContinueRoot execution path 未使用它们来先解析 agent/assignment/frame，再选择 delivery trace。`ContinueRoot` 仍是 activity executor policy，并且 root RuntimeSession 同时参与 executor config、assignment frame ref、activation delivery、返回值 identity。

## Multi RuntimeSession Selection Current State

### Existing policy and tests

- `RuntimeSessionSelectionPolicy` 当前有 `Specific { runtime_session_id }`、`LaunchPrimary`、`LatestAttached` 三种，见 `crates/agentdash-domain/src/workflow/agent_frame.rs:13` 到 `:19`。
- `select_runtime_session_id(policy)` 要求调用方传 policy，见 `agent_frame.rs:135` 到 `:145`。
- domain test `runtime_session_selection_requires_explicit_policy` 覆盖 LaunchPrimary / LatestAttached / Specific / missing specific，见 `agent_frame.rs:174` 到 `:205`。
- `RuntimeLaunchRequest::from_frame(frame, runtime_policy)` 已要求 explicit policy，见 `crates/agentdash-application/src/workflow/runtime_launch.rs:89` 到 `:90`。
- runtime launch test 覆盖多 refs 下 LaunchPrimary 取第一个、LatestAttached 取最后一个，见 `runtime_launch.rs:313` 到 `:322`。

### Current call-site policy usage

- API session construction 对 direct runtime session 使用 `Specific`，见 `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:706` 到 `:731`。这是当前最接近“adapter callback 指定 trace”的正确形态。
- Task cancel 使用 `LatestAttached` 后直接 cancel session，见 `crates/agentdash-application/src/task/service.rs:216` 到 `:220`。
- Task context builder 用 `LatestAttached` 解析 session projection，见 `crates/agentdash-application/src/task/context_builder.rs:232` 到 `:238`。
- Companion parent notification 用 `LatestAttached`，见 `crates/agentdash-application/src/companion/tools.rs:658` 到 `:668` 与 `:1367` 到 `:1377`。
- Permission test 仍用 `LaunchPrimary` 验证 runtime ref carry-forward，见 `crates/agentdash-application/src/permission/service.rs:696` 到 `:704`。

### Remaining implicit / first selection

- `LaunchPrimary` 仍是 named first：implementation 是 `ids.into_iter().next()`，见 `crates/agentdash-domain/src/workflow/agent_frame.rs:144`。它避免了“无 policy 的 first”，但业务语义仍只是 array order。
- `LatestAttached` 是 named last：implementation 是 `ids.into_iter().next_back()`，见 `agent_frame.rs:145`。它同样依赖 refs array order。
- `AgentFrameBuilder` carry-forward runtime refs，并 append 新 refs，见 `crates/agentdash-application/src/workflow/frame_builder.rs:355` 到 `:381`；array order 因此继续承载业务含义。
- `find_by_runtime_session` 对同一 RuntimeSession 命中的多 frame 使用 `ORDER BY created_at DESC LIMIT 1`，见 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:473` 到 `:486`。ContinueRoot / shared root session 场景下，这仍是隐式“最新 frame”选择。
- `reconcile/terminal_cancel` 仍直接读取 `runtime_session_refs_json` array 并 `arr.first()`，见 `crates/agentdash-application/src/reconcile/terminal_cancel.rs:137` 到 `:159`。这是明确绕过 `RuntimeSessionSelectionPolicy` 的 remaining call site。
- 一些 view builder 里有 `agents.first()` fallback，见 `crates/agentdash-api/src/routes/lifecycle_views.rs:610` 到 `:615` 与 `crates/agentdash-api/src/routes/story_runs.rs:410` 到 `:415`；这不是 RuntimeSession ref selection，但会影响“哪个 agent/frame 的 runtime refs 被展示”的上游选择。

### Judgment

P1-11 已部分关闭：`RuntimeLaunchRequest::from_frame` 不再默认取 first，主要 call site 也开始显式传 policy。但 policy 语义还很薄，`LaunchPrimary` / `LatestAttached` 仍依赖 refs array order，且 `terminal_cancel` 存在 raw `arr.first()` 绕过 policy。`implement.md` 标记的“多 RuntimeSession selection policy”可视为基础 gate 已完成，不应视为最终 selection policy 完成。

## Suggested Minimal Encapsulation Batch

### 1. AgentFrameBuilder internal StepActivation stage

- 新增 `AgentFrameSurfaceService` 或 `AgentFrameBuilder::from_activation_intent(...)` 高层入口，输入 lifecycle/story/companion owner + activity/procedure context，内部执行当前 `activate_step_with_platform`、companion skill projection、capability/context/VFS/MCP resolution，并只输出 frame revision + launch extras。
- 将 `StepActivationInput` / `StepActivation` 从 `workflow::mod` public re-export 移除，先改成 `workflow::frame_surface` 内部 stage 类型；保留旧纯函数实现可以，但调用面只留在 surface service 内。
- 把 `SessionRequestAssembler` 中 `compose_lifecycle_node_with_audit`、`compose_story_step`、companion workflow overlay 的 activation resolution 迁到 surface service；`SessionAssemblyBuilder` 只保留 launch-only extras 或被 surface service 内部使用。
- 将 `project_companion_system_skill_to_activation` 迁为 builder stage，例如 `apply_companion_skill_projection(surface_stage)`，避免 companion 模块直接 mut `StepActivation`。

### 2. ContinueRoot policy resolver

- 在 activity executor boundary 新增 resolver，将旧 `AgentSessionPolicy::ContinueRoot` 映射为显式组合：`AgentReusePolicy::ContinueCurrentAgent` + `RuntimeSessionPolicy::DeliverToActiveTrace` 或等价命名。
- resolver 必须先定位 `LifecycleAgent` / current `AgentFrame` / `AgentAssignment`，再选择 RuntimeSession delivery target。root session 只能作为 provenance 或 delivery candidate。
- `create_agent_activity_assignment` 不再接收 `Option<&str>` runtime session；改为接收 resolved frame / assignment evidence，runtime ref attach 由 `RuntimeSessionPolicy` 单独生成 delivery/ref attach command。
- ContinueRoot 的 live/pending activation 改为“写 frame transition，然后按 RuntimeSessionPolicy 投递”，调用点不再直接调用 `apply_to_running_session`。

### 3. RuntimeSession selection policy hardening

- 将 `RuntimeSessionSelectionPolicy` 扩展为业务语义更强的 variants：`SpecificTrace`、`ActiveTurnOwner`、`LatestWritable`、`ResumeLatestTrace`、`CreateNewIfNone`。
- 限制 `LaunchPrimary` 为 migration/test 或明确 root launch provenance；常规 delivery / cancel / notification 使用 `LatestWritable` 或 `ActiveTurnOwner`。
- 将 `terminal_cancel` 的 raw array first 改为 shared selection helper；view builder 的 agent fallback 另设 `LifecycleAgentSelectionPolicy`，避免 agent selection 与 runtime ref selection 混在一起。
- 对 `find_by_runtime_session(... ORDER BY created_at DESC LIMIT 1)` 加 resolver 边界说明：只允许 runtime adapter trace-to-frame lookup 使用；业务 command 不调用它来选择 frame owner。

## Acceptance Gates

### Gate A: StepActivation is builder-internal

- Static check: `rg -n "pub use step_activation|StepActivationInput|activate_step_with_platform|apply_to_running_session|project_companion_system_skill_to_activation" crates/agentdash-application/src`.
- Expected: `StepActivationInput` / `StepActivation` 只出现在 frame surface service / builder stage 及其 tests；`workflow::mod` 不再 public re-export；workflow executor 不直接 import `apply_to_running_session`。
- Unit tests: `cargo test -p agentdash-application workflow::frame_builder` plus a new test proving one surface build writes procedure/context/capability/VFS/MCP/runtime refs to the same frame revision.

### Gate B: ContinueRoot selects agent before runtime delivery

- Static check: `rg -n "ContinueRoot|root_runtime_session_id|create_agent_activity_assignment\\(|ExecutorRunRef::RuntimeSession|apply_to_running_session" crates/agentdash-application/src/workflow crates/agentdash-domain/src/workflow crates/agentdash-contracts/src`.
- Expected: `ContinueRoot` is translated at boundary into `AgentReusePolicy + RuntimeSessionPolicy`; assignment creation takes agent/frame/assignment evidence, not root runtime session id.
- Unit tests: add `continue_root_selects_agent_before_runtime_delivery` and `continue_root_with_multiple_runtime_refs_uses_runtime_session_policy`.

### Gate C: Runtime delivery remains delivery-only

- Static check: `rg -n "AgentFrameTransitionRecord|RuntimeDeliveryCommand|session_runtime_commands|payload_json" crates/agentdash-spi/src crates/agentdash-application/src crates/agentdash-infrastructure/src`.
- Expected: delivery payload still contains only delivery kind / frame_transition_id / target_frame_id; transition records remain in `agent_frame_transitions`.
- Existing evidence: `crates/agentdash-application/test-support/session_memory_persistence.rs:1328` to `:1334` asserts delivery serialization has `frame_transition_id` and no transition/state payload.

### Gate D: RuntimeSession selection is explicit and semantically named

- Static check: `rg -n "runtime_session_refs_json.*first|arr\\.first\\(|select_runtime_session_id\\(RuntimeSessionSelectionPolicy::LaunchPrimary\\)|RuntimeSessionSelectionPolicy::LatestAttached|find_by_runtime_session" crates`.
- Expected: raw `arr.first()` is gone from runtime session ref selection; `LaunchPrimary` / `LatestAttached` are either replaced by semantic variants or justified in adapter/test-only contexts.
- Unit tests: keep `runtime_session_selection_requires_explicit_policy`; add tests for `SpecificTrace` missing ref, `LatestWritable` with multiple refs, `ActiveTurnOwner`, and `CreateNewIfNone`.

## Caveats / Not Found

- 未修改代码，未运行测试。
- 未发现目标命名的 `AgentReusePolicy` / `RuntimeSessionPolicy` 类型；当前只有 `AgentPolicy` / `RuntimePolicy`，且 `RuntimePolicy::ContinueCurrent(Uuid)` 仍以 RuntimeSession id 表达。
- transition/delivery split 当前已落地到 type、migration、repository join 和 memory tests；这部分不再按旧调研结论处理。
- 多 RuntimeSession selection 已有显式 enum 和测试，但仍有 raw `arr.first()` 与 order-based policy；因此只能算基础 selection gate，不能算完整 selection policy。
- `apply_to_running_session` 当前 live path 会通过 `AgentFrameRuntimeTarget` 写 AgentFrame revision；风险点不是“完全绕过 frame 写入”，而是 workflow executor 仍绕过 builder-owned resolution stage，以 RuntimeSession delivery 为主语直接应用 activation。
