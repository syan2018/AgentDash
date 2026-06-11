# Research: companion-tools executable plan

- Query: 审查 `crates/agentdash-application/src/companion/tools.rs` 及相关 companion/session/capability 模块，判断表面代码质量、职责过宽、重复链路、裸字段传递、旧/过渡语义是否可模块级快速收敛。
- Scope: internal
- Date: 2026-06-11

## Findings

`python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`，本研究使用本轮 prompt 明确给出的任务目录：`.trellis/tasks/06-11-review-refactor-quality-sweep`。

### Related Specs

- `.trellis/spec/backend/architecture.md`：application 负责 session/context/workflow/VFS/capability 服务，API 只做入口和 DTO；RuntimeSession 是 trace/delivery substrate，业务事实通过 lifecycle/control-plane 回查。
- `.trellis/spec/backend/quality-guidelines.md`：禁止 `unwrap()` / panic，新增 launch 字段时同步 HTTP/local relay/task/workflow/routine/companion/hook auto-resume 入口。
- `.trellis/spec/backend/runtime-gateway.md`：runtime action 消费端必须走 actor/context 校验；adapter 不直接绕过 gateway/provider。
- `.trellis/spec/backend/session/architecture.md:30`：runtime session 只是 delivery/control 入口，通过 `RuntimeSessionExecutionAnchor` 回到 run/agent/frame。
- `.trellis/spec/backend/session/session-startup-pipeline.md:23`：Companion dispatch / parent resume 是 `LaunchCommand` source adapter；`.trellis/spec/backend/session/session-startup-pipeline.md:44` 明确 parent session id 只作为 trace provenance。
- `.trellis/spec/backend/session/execution-context-frames.md`：`ExecutionContext` 是 connector-facing projection，不是 application 事实源。
- `.trellis/spec/backend/session/runtime-execution-state.md:84`：companion parent resume 等内部 follow-up 仍应走 `LaunchCommand -> SessionConstructionPlan -> LaunchPlan` 主数据流。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md:53`：hook runtime 调用方通过 `RuntimeAdapterProvenance` 传 runtime session / turn；业务 owner 固定在 runtime 内部的 frame target。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md:150`：companion/subagent dispatch 需要 before/after hook，回流结果进入 `companion_result` runtime event / pending action。
- `.trellis/spec/backend/capability/architecture.md:9`：工具集由 `CapabilityResolver` 或 capability dimension pipeline 统一计算。
- `.trellis/spec/backend/capability/architecture.md:71`：PermissionGrant applied 后进入 `CapabilityResolver` 授权 keys。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md:135`：新 session 类型必须通过 `CapabilityResolver::resolve()` 获取工具集。
- `.trellis/spec/backend/permission/grant-lifecycle.md:11`：Agent runtime 通过 `companion_request(capability_grant_request)` 发起 capability 申请。

### External References

- None. 本研究只基于本地代码、Trellis specs 和任务文件。

### Files Found

| File | Description |
| --- | --- |
| `crates/agentdash-application/src/companion/tools.rs` | `companion_request` / `companion_respond` 工具实现；当前同时承担 request 分发、sub dispatch、human/platform 请求、gate 轮询、hook trace、pending action、slice/prompt/VFS 裁剪和测试。 |
| `crates/agentdash-application/src/companion/gate_control.rs` | Durable `LifecycleGate` 控制服务，已集中 parent request / child result / delivery runtime session 选择与通知投递。 |
| `crates/agentdash-application/src/companion/payload_types.rs` | Companion payload type registry，已声明 `capability_grant_request -> capability_grant_result`，并校验 required fields。 |
| `crates/agentdash-application/src/companion/notifications.rs` | Companion event / human response Backbone notification helpers。 |
| `crates/agentdash-application/src/vfs/tools/provider.rs` | Runtime tool composer；当前在 VFS provider 中装配 companion tools，并调用 `resolve_lifecycle_anchors()`。 |
| `crates/agentdash-application/src/session/construction_provider.rs` | 定义 `CompanionLaunchSource { parent_session_id, slice_mode, companion_executor_config, dispatch_prompt }`，是 companion dispatch 进入 session construction 的 source hint。 |
| `crates/agentdash-application/src/session/launch/command.rs` | 已有 `LaunchCommand::companion_dispatch_input(...)` 和 `companion_parent_resume_input(...)`，但 `rg` 未找到生产调用点。 |
| `crates/agentdash-application/src/session/launch/service.rs` | 生产 session launch 入口 `SessionLaunchService::launch_command_with_outcome()`。 |
| `crates/agentdash-application/src/workflow/dispatch_service.rs` | `LifecycleDispatchService` 创建 run/agent/frame/runtime session/anchor/gate，但不启动 connector turn。 |
| `crates/agentdash-application/src/workflow/frame_construction/composer_companion.rs` | 已能消费 `CompanionLaunchSource` 并走 `compose_companion_to_frame` / `compose_companion_with_workflow_to_frame`。 |
| `crates/agentdash-application/src/session/assembler.rs` | `resolve_companion_parent_facts()` 和 companion frame 组装；当前 parent capability 缺失会转成空 VFS/MCP。 |
| `crates/agentdash-application/src/session/assembly_builder.rs` | `apply_companion_slice()` 已把 dispatch prompt、executor config、VFS/MCP/capability slice 组装为 launch extras。 |
| `crates/agentdash-application/src/permission/service.rs` | `PermissionGrantService` 已存在，可创建 grant、执行 policy、写 AgentFrame capability effect。 |
| `crates/agentdash-domain/src/companion/skills/companion-system/references/capability-grant-request.md` | Companion skill 文档明确 platform broker 应映射到 permission/grant，并声明 result payload 不是工具授权事实源。 |

### Current Code Patterns

- `tools.rs` 是明显过宽文件：2376 行，其中生产代码到约 2072 行，测试从 2073 行开始。它同时定义工具 DTO、两个 `AgentTool` 实现、四个 request target 分支、respond 三路副作用、hook trace、slice builder、execution slice 和测试。
- `CompanionRequestTool` 把 `current_session_id/current_turn_id/current_agent_id/current_frame_id/current_run_id/project_id` 裸字段挂在工具实例上，构造时从 `ExecutionContext.turn.hook_runtime` 反推 session id（`tools.rs:91-107`、`tools.rs:121-128`），再由 provider 调 `resolve_lifecycle_anchors()` 异步填充（`tools.rs:136-153`、`provider.rs:221-228`）。这个长程传递可模块级收敛为 `CompanionToolContext` / `CompanionLifecycleAnchor`。
- `resolve_lifecycle_anchors()` 对 anchor 解析错误静默返回（`tools.rs:137-153`）。后续分支再分别报“缺少 run/agent/frame/project”，导致失败原因远离事实缺失点。
- `session_services_handle.get().await` 初始化错误重复且语义不一致：sub/parent/human wait 分支 fail closed（`tools.rs:297-301`、`tools.rs:604-608`、`tools.rs:769-771`），human non-wait 分支可静默丢通知（`tools.rs:892-912`），respond gate 路径用 `NoopCompanionGateDelivery` 吞掉 delivery（`tools.rs:1182-1190`、`tools.rs:1331-1339`）。这属于实现级收敛，不是架构项。
- Sub dispatch 当前只创建 lifecycle/runtime refs，没有启动 child session turn：`LifecycleDispatchService` 返回 `delivery_runtime_ref`（`dispatch_service.rs:260-303`），但 `tools.rs` 只保存 run/agent/frame/gate refs（`tools.rs:379-445`），没有使用 `delivery_runtime_ref`，也没有调用 `SessionLaunchService`。`build_companion_dispatch_prompt()` 只被测试引用（`tools.rs:1730-1785`，`rg` 未找到生产调用）。
- 已有闭链材料未接上：`CompanionLaunchSource` 定义在 `construction_provider.rs:42-48`，`LaunchCommand::companion_dispatch_input()` 定义在 `launch/command.rs:147-157`，frame construction companion composer 消费该 source（`composer_companion.rs:24-57`），`SessionLaunchService::launch_command_with_outcome()` 是生产入口（`launch/service.rs:29-36`）。
- `let _companion_executor_config = ...` 解析了指定 companion agent config 但后续未使用（`tools.rs:275-280`）。这和上面的未启动 child turn 是同一链路未闭合问题，不应拆成单 helper 修复。
- `build_companion_execution_slice()` 对 `Full` 返回 `None` VFS、对 `Compact` 缺 parent VFS 时返回空 VFS（`tools.rs:1928-1962`）；`compose_companion_with_workflow()` 随后 `slice.vfs.unwrap_or_default()`（`assembler.rs:1405-1409`）。这会把缺失 parent capability/VFS 事实伪装成“空能力”。
- `resolve_companion_parent_facts()` 在 parent capability state 缺失时仍返回 `parent_vfs=None`、`parent_mcp_servers=[]`（`assembler.rs:979-998`）。按 session startup spec，construction 应拒绝缺少 launch-ready facts，而不是默认化。
- `target=platform` 的 `capability_grant_request` 当前只是转成人类 companion request（`tools.rs:932-945`），注释还说“授权事实落地由后续 grant 持久化任务承接”。但 PermissionGrant service/API/spec 已存在（`permission/service.rs:71-79`、`grant-lifecycle.md:11`），companion skill 文档也说 result payload 不是授权事实源（`capability-grant-request.md:31-42`）。这是旧/过渡语义，不能继续作为有效业务语义。
- `build_subagent_pending_action()` 用 `fallback_request_id` 填 request id 和 `turn_id`（`tools.rs:1621-1669`）。request id fallback 可以作为兼容痕迹理解，但把 `turn_id` fallback 成 request id 会污染 trace/provenance，应收敛为 typed input，缺 source turn 时显式 `None` 或错误。
- Hook provenance 构造集中在 `evaluate_subagent_hook()`，但调用方仍传裸 `turn_id: Option<String>` / source 字符串（`tools.rs:1529-1568`）。可用小型 `CompanionHookProvenance` helper 收束，不需要改 hook public contract。
- `legacy:session_plan` 仍是 documented source prefix（`context/injection.rs:193-199`），并由 `build_session_plan_fragments()` 生产（`session/plan.rs:90-170`）。它不是 dead fallback；如果要改名应走单独 context source taxonomy 批次，不应在 companion 工具清理中顺手删除。

### Immediate Implementation Batches

#### Batch 1: Companion tool runtime context + session service fail-closed

并行性：先做。Batch 2/3/4 都依赖更清晰的 runtime context 与 session services 入口。

Write scope:

- `crates/agentdash-application/src/companion/tools.rs`
- Optional new file: `crates/agentdash-application/src/companion/tool_context.rs`
- `crates/agentdash-application/src/companion/mod.rs`
- `crates/agentdash-application/src/vfs/tools/provider.rs`
- `crates/agentdash-application/src/companion/gate_control.rs` only if test helper visibility needs adjustment

Core changes:

- Introduce module-local `CompanionToolContext` containing `delivery_runtime_session_id`, `turn_id`, `hook_runtime`, and optional resolved `CompanionLifecycleAnchor { project_id, run_id, agent_id, frame_id }`.
- Replace mutable `CompanionRequestTool::resolve_lifecycle_anchors()` with provider-side async construction, e.g. `CompanionToolContext::resolve(context, repos)` returning clear errors or a context with explicit missing-anchor state.
- Add one `require_session_services(action)` helper for companion tools; production paths should fail closed when services are unavailable.
- Remove production use of `NoopCompanionGateDelivery` in `CompanionRespondTool`; keep noop only for direct service tests if needed.
- Create a small `CompanionGateControlFactory` helper from repos + `SessionEventingService`, instead of constructing `CompanionGateControlService::new(...)` in multiple branches.
- Add `CompanionHookProvenance` helper that produces `RuntimeAdapterProvenance::runtime_session(...)` with a source enum/string centralized in one place.

Risk:

- Tool exposure timing can change if provider starts rejecting companion tools without runtime context. Prefer keeping tools exposed but giving deterministic execution errors unless capability policy requires hiding them.
- Non-wait human requests currently “succeed” even if notification injection fails; fail-closed behavior may expose existing bootstrap ordering bugs. That is desired in pre-release.

Validation:

- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application vfs::tools::provider`
- `cargo check -p agentdash-application`

#### Batch 2: Sub companion dispatch closes the session launch chain

并行性：after Batch 1. Do not run in parallel with Batch 3 because both edit `tools.rs`.

Write scope:

- `crates/agentdash-application/src/companion/tools.rs`
- Optional new file: `crates/agentdash-application/src/companion/dispatch_plan.rs`
- `crates/agentdash-application/src/companion/mod.rs`
- `crates/agentdash-application/src/session/assembler.rs`
- `crates/agentdash-application/src/session/assembly_builder.rs`
- Tests under companion/session launch as needed

Core changes:

- Preserve `LifecycleDispatchService` as control-plane creator, but capture `delivery_runtime_ref` from `launch_agent()` / `open_interaction_gate()` result and include it in `CompanionDispatchOutcome`.
- Use the already-built dispatch plan to create a real prompt with `build_companion_dispatch_prompt(&plan, prompt)`.
- Use the resolved companion executor config as `CompanionLaunchSource.companion_executor_config`; remove `_companion_executor_config`.
- Launch the child runtime session through `session_services.launch.launch_command_with_outcome(child_session_id, LaunchCommand::companion_dispatch_input(UserPromptInput::from_text(dispatch_prompt.clone()), CompanionLaunchSource { ... }))`.
- Return child `turn_id` / `delivery_runtime_session_id` in `AgentToolResult.details` for both wait and async paths.
- Remove dead `CompanionAgentRef` if still unused.
- Make parent facts missing fail explicitly: `resolve_companion_parent_facts()` should return an error if parent capability state is absent. `Full` and `Compact` companion slices should not turn missing parent VFS into empty VFS; `WorkflowOnly` / `ConstraintsOnly` can keep explicit empty VFS because that is their business meaning.

Risk:

- This is behavior-changing: today sub dispatch may only materialize a shell session and gate; after the fix it should actually start the child agent turn.
- If existing tests assumed dispatch-only behavior, they should be updated around real launch command invocation, not around silent no-op.

Validation:

- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application session::launch`
- `cargo test -p agentdash-application workflow::frame_construction`
- `cargo check -p agentdash-application`

#### Batch 3: Request/respond routing split and typed ids

并行性：after Batch 1; can run after Batch 2 to reduce conflict.

Write scope:

- `crates/agentdash-application/src/companion/tools.rs`
- Optional new files:
  - `crates/agentdash-application/src/companion/request_handlers.rs`
  - `crates/agentdash-application/src/companion/respond_router.rs`
  - `crates/agentdash-application/src/companion/dispatch_plan.rs` if Batch 2 did not create it
- `crates/agentdash-application/src/companion/mod.rs`
- `crates/agentdash-application/src/companion/gate_control.rs`
- `crates/agentdash-application/src/companion/notifications.rs` only if notification builders need typed event constructors

Core changes:

- Keep `CompanionRequestTool` and `CompanionRespondTool` as thin `AgentTool` adapters: parse/validate args, then delegate.
- Move target handlers by responsibility, not helper size: `SubDispatchHandler`, `ParentRequestHandler`, `HumanRequestHandler`, `PlatformRequestHandler`.
- Move respond side effects into `CompanionRespondRouter` with named routes: `resolve_parent_request_gate`, `resolve_pending_action`, `complete_child_result_to_parent`.
- Introduce module-local typed ids / refs (`CompanionRequestId`, `CompanionDispatchId`, `DeliveryRuntimeSessionId` or simple newtype wrappers) only where they remove current request-id/session-id/turn-id confusion.
- Change `build_subagent_pending_action()` to consume a typed payload/input. Do not fallback `turn_id` to request id; if source turn is missing, use `None` or return a clear validation error depending on the caller.
- Keep `PayloadTypeRegistry` and notification helpers where they are; they are already good boundaries and should not be copied into the new handlers.

Risk:

- Splitting files can move tests. Keep behavior-preserving except for removing fake/fallback ids and swallowed notification failures.
- Avoid adding public exports unless another module truly consumes them; most new types can remain `pub(crate)`.

Validation:

- `cargo test -p agentdash-application companion`
- `cargo check -p agentdash-application`

#### Batch 4: Platform capability grant request stops pretending to be human approval

并行性：after Batch 1. Can run independently of Batch 2/3 if file conflicts are coordinated.

Write scope:

- `crates/agentdash-application/src/companion/tools.rs`
- Optional new file: `crates/agentdash-application/src/companion/platform_grant.rs`
- `crates/agentdash-application/src/companion/payload_types.rs` only if stricter typed parsing helpers are added
- `crates/agentdash-application/src/permission/service.rs` only if a small companion-facing adapter method avoids duplicated parsing
- Tests in companion + permission service

Core changes:

- Parse `capability_grant_request` into `GrantRequest`: `requested_paths`, `reason`, `grant_scope`, `ttl_seconds`, `source_runtime_session_id`, `source_turn_id`, `effect_frame_id`, `run_id`.
- Use `PermissionGrantService::request(...)` as the authority path when the current lifecycle anchor and policy inputs are available.
- Return a `capability_grant_result` payload containing at least `grant_id`, `status`, and granted/rejected/requested paths. The companion result remains conversation feedback only; permission grant state is the tool access authority.
- If policy inputs are not yet available in the companion context, fail with an explicit unsupported/missing-policy error. Do not convert to `execute_human_request()` as if that grants capability.
- Remove or rewrite the stale comment at `tools.rs:932-933`.

Risk:

- Full live tool-schema update after auto-approval may require `SessionCapabilityService` integration. If that cannot fit in this batch, make the grant state authoritative and document whether capability becomes visible on next turn; do not silently claim immediate tool access.
- This touches permission/capability facts. Keep the minimum bridge small; broader broker UX can be architecture backlog.

Validation:

- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application permission`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-application`

### Architecture Backlog

#### COMP-ARCH-001: Full platform broker / permission grant / live capability update convergence

- Priority: P1 if platform grants are a current product target; otherwise P2.
- Status: candidate, only for the full broker design.
- Evidence: `capability_grant_request` currently routes to a human companion event (`tools.rs:932-945`), while permission grant lifecycle and API already exist (`permission/service.rs:71-79`, `permission_grants.rs:160-176`) and capability grant docs say broker flow must apply `RuntimeCapabilityTransition` (`capability-grant-request.md:31-42`).
- Impact: companion tool semantics, PermissionGrant fact source, AgentFrame capability state, runtime tool updates, user approval UI/API, capability resolver visibility.
- Suggested direction: define a single platform broker use case that owns grant creation, policy decision, user approval handoff, frame effect application, and live runtime capability/tool-schema update.
- Why not fold into quick cleanup: the minimal Batch 4 can stop the fake human-approval path, but a complete broker may cross companion, permission, session capability service, API routes, frontend cards, and runtime event projection.

#### Existing related backlog: runtime tool composer migration out of VFS

- This research also confirms existing `ARCH-006` remains valid: `RelayRuntimeToolProvider` still composes VFS, workflow, companion, canvas, workspace module and runtime gateway tools (`provider.rs:1-30`, `provider.rs:160-245`). Do not block the companion quick cleanup on that larger migration.

### Non-Deferred Review Items

- The sub companion dispatch chain is non-deferred. `build_companion_dispatch_prompt()` and `CompanionLaunchSource` are already present, but production code never launches the child turn. This is a behavioral chain gap, not just cleanup.
- Session service initialization and `NoopCompanionGateDelivery` fallbacks are non-deferred. Current code can report “sent/responded” while notification delivery was skipped or impossible.
- Parent capability/VFS defaulting is non-deferred. Missing parent facts should not become empty VFS/MCP through `unwrap_or_default()` in companion construction.
- `target=platform` capability grant as human request is non-deferred. The current path contradicts the permission grant spec and companion skill reference; either bridge to `PermissionGrantService` or fail explicitly.
- `fallback_request_id` as `turn_id` is non-deferred. It corrupts trace identity and can be fixed locally while splitting respond routing.

### Validation commands

Use these after implementation batches, choosing the smallest relevant subset:

```powershell
cargo test -p agentdash-application companion
cargo test -p agentdash-application permission
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application workflow::frame_construction
cargo test -p agentdash-application vfs::tools::provider
cargo check -p agentdash-application
cargo check -p agentdash-api
```

## Caveats / Not Found

- No git commands were used.
- I did not modify source code.
- `LaunchCommand::companion_dispatch_input()` and `build_companion_dispatch_prompt()` had no production callers in `crates/agentdash-application/src`, `crates/agentdash-api/src`, or `crates/agentdash-local/src`; only tests referenced the prompt builder.
- I did not find a companion-specific Trellis spec file beyond session/capability/hooks/permission specs and the embedded `companion-system` skill docs.
- I did not verify actual runtime behavior with tests; this file is an implementation-level research plan, not a completed fix.
