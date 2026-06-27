# Research: task-hook-coordinate-slice

- Query: 为下一条可并行实现线产出 task / hook / orchestration coordinate 收束设计，明确 task projection/effect、hook runtime target、journey helper 的最小实现切片。
- Scope: internal
- Date: 2026-06-15

## Findings

### Related Specs

- `.trellis/tasks/06-15-agentrun-runtime-entry-session-convergence/design.md`：目标边界是 external runtime/business API 进入 `AgentRunRuntimeAddress`，RuntimeSession 只作为 optional `MessageStreamRef`，node execution 归 `OrchestrationNodeCoordinate`。
- `.trellis/tasks/06-15-agentrun-runtime-entry-session-convergence/research/session-entry-audit.md`：前一轮审计已把 task service / projector / effect、hook runtime、journey helper 归为下一批需要收束的 node-coordinate / hook-target 切片。
- `.trellis/spec/backend/workflow/architecture.md`：Agent node execution identity 使用 `AgentInvocation(lifecycle_run_id, orchestration_id, node_path, attempt, agent_run_id, frame_id)`，RuntimeSession 只作为 terminal/runtime evidence；AgentRun lifecycle surface 的 node projection 必须显式构造 `orchestration_id + node_path + attempt`。
- `.trellis/spec/backend/session/runtime-execution-state.md`：hook runtime registry 是 delivery binding cache；业务 owner 是 `HookControlTarget { run_id, agent_id, frame_id }`。AgentRun lifecycle surface 从 `run_id + agent_id + frame_id` 构造，RuntimeSession 以 `MessageStreamProjectionRef` 进入 projector。

### Files Found

- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs`：`RuntimeSessionExecutionAnchor` 同时保存 delivery trace identity、run/agent/frame launch evidence 和 optional node refs。
- `crates/agentdash-application/src/task/service.rs`：task execution refs 当前从 task subject association 找 agent/frame，再遍历该 agent 的 session anchors，从 anchor 派生 node refs。
- `crates/agentdash-application/src/task/view_projector.rs`：启动期 task view reconcile 当前从 association -> agent/current frame -> anchors -> runtime node，并把 node 状态投影回 story task。
- `crates/agentdash-application/src/task/context_builder.rs`：task context builder 当前用 latest agent anchor 反查 active workflow projection；visible canvas mounts 仍由 optional runtime session id -> anchor -> current/launch frame 推导。
- `crates/agentdash-application/src/task/gateway/effect_executor.rs`：terminal hook effect handler 从 handler session id 反查 anchor，校验 task association，再持久化 artifact 或更新 task status。
- `crates/agentdash-application/src/session/types.rs`：`AgentFrameRuntimeTarget` 目前只有 `frame_id + delivery_runtime_session_id`，命名上仍把 delivery session 放进 target。
- `crates/agentdash-application/src/session/hooks_service.rs`：公开 ensure/get 已接受 `AgentFrameRuntimeTarget`，内部构造 `HookControlTarget`；launch adapter `resolve_hook_runtime(session_id, ...)` 仍 session-first。
- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs`：`read_session_projection` 是合法 trace reader；`attempt_session_id` / `step_session_id` / `current_step_session_id` 只从 executor ref 提取 session id。
- `crates/agentdash-application/src/workflow/projection.rs`：已经存在 target-first active workflow projection 入口 `resolve_active_workflow_projection_for_target`，可作为 task context builder 收束时的可复用方向。

### 1. Task Projection / Effect 当前如何从 RuntimeSessionExecutionAnchor 反查 node

`RuntimeSessionExecutionAnchor` 是当前 trace backlink 的唯一索引。它保存 `runtime_session_id`、`run_id`、`launch_frame_id`、`agent_id`，以及 optional `orchestration_id`、`node_path`、`node_attempt`；其中 orchestration dispatch 构造器会写入 node refs（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29`, `:35`, `:37`, `:39`, `:68`, `:83`）。

`TaskExecutionService::resolve_task_execution_refs` 的读取链路是：

```text
Task id
  -> SubjectRef(kind=task)
  -> LifecycleSubjectAssociation
  -> LifecycleAgent (association.anchor_agent_id or active agent in run)
  -> agent.current_frame_id
  -> RuntimeSessionExecutionAnchorRepository::list_by_agent(agent.id)
  -> filter anchor.run_id == run.id and anchor.launch_frame_id == current_frame_id
  -> anchor.orchestration_id / node_path / node_attempt
  -> LifecycleRun.orchestrations[].node_tree
```

代码证据：subject association 读取在 `crates/agentdash-application/src/task/service.rs:56`；agent/current frame 解析在 `:66` 到 `:72`；run 读取在 `:73` 到 `:81`；按 agent 列 anchors 在 `:82` 到 `:87`；frame/run 过滤在 `:88` 到 `:90`；anchor -> node refs 在 `task_execution_refs_from_anchor` 中读取 `orchestration_id`、`node_path`、`node_attempt`，再进 `run.orchestrations` 和 `find_runtime_node`（`:145` 到 `:170`）。

`project_task_views_on_boot` 使用同一模型投影只读 task view。文件注释已经明确当前链路是 `SubjectRef -> LifecycleSubjectAssociation -> LifecycleAgent.current_frame -> RuntimeSessionExecutionAnchor -> LifecycleRun.orchestrations[].node_tree -> Story::apply_task_projection`（`crates/agentdash-application/src/task/view_projector.rs:94` 到 `:102`）。实现上，`resolve_task_runtime_projection` 读取 association、agent/current frame、run，再 `list_by_agent` 遍历 anchors（`:220` 到 `:260`）；`projection_from_anchor` 从 anchor 的 `orchestration_id/node_path/node_attempt` 进入 node tree 并取状态（`:279` 到 `:307`）。

`build_task_session_context` 也在用 session anchor 间接找 active workflow。它先按 task association 找 agent，再取该 agent 的 latest anchor 的 `runtime_session_id`，最后调用 `resolve_active_workflow_projection_for_session`（`crates/agentdash-application/src/task/context_builder.rs:177` 到 `:183`, `:220` 到 `:238`）。`resolve_visible_canvas_mount_ids` 是更明确的 session adapter：optional session id -> `find_by_session` -> anchor agent/current frame 或 launch frame（`:246` 到 `:272`）。

`TaskHookEffectExecutor` 是 terminal trace callback 场景。它在 event artifact 和 `task:set_status` effect 前调用 `validate_runtime_task_anchor`（`crates/agentdash-application/src/task/gateway/effect_executor.rs:83` 到 `:100`, `:134` 到 `:150`）。校验链路是 handler 持有的 `self.session_id` -> `execution_anchor_repo.find_by_session` -> `LifecycleAgent` -> `LifecycleSubjectAssociationRepository::list_by_anchor(run, Some(agent)) + list_by_anchor(run, None)` -> task association 匹配（`:176` 到 `:239`）。当前校验没有显式验证 `anchor.orchestration_id/node_path/node_attempt` 对应的 runtime node，只验证 session anchor 所属 run/agent 与 task association。

结论：task projection 已经有 association/run/agent/frame 这些业务事实，但 node coordinate 目前由 `RuntimeSessionExecutionAnchor` 派生，anchor 既当 trace evidence 又当 node selector。下一步应把 node selector 提前为显式 `OrchestrationNodeCoordinate`，anchor 只补充 delivery trace evidence。

### 2. Hook runtime target 当前哪些地方已是 AgentFrame / HookControlTarget，哪些仍 session-first

已 target 化的部分：

- `AgentFrameRuntimeTarget` 已作为 hook service ensure/get 的输入。它表达 `frame_id` 和 `delivery_runtime_session_id`，注释说明 `frame_id` 是 effective runtime surface，delivery session 用于 live connector / registry 同步（`crates/agentdash-application/src/session/types.rs:61` 到 `:69`）。
- `SessionHookService::ensure_hook_runtime_for_target` 和 `get_hook_runtime_for_target` 都接受 `&AgentFrameRuntimeTarget`（`crates/agentdash-application/src/session/hooks_service.rs:25` 到 `:30`, `:105` 到 `:114`）。registry cache 仍按 delivery session id 存取，但缓存命中会校验 runtime session 与 frame id（`:54` 到 `:63`, `:261` 到 `:279`）。
- `resolve_hook_target_frame_from_frame` 通过 frame + session anchor 构造真正的 `HookControlTarget { run_id, agent_id, frame_id }`（`crates/agentdash-application/src/session/hooks_service.rs:325` 到 `:370`）。这已经符合 spec 的业务 owner 形态。
- `build_frame_hook_runtime` 的参数是 `session_id + HookControlTarget + frame + provider + snapshot`。它验证 frame agent 与 target agent，再验证 target agent 拥有 delivery session，最后用 `AgentFrameHookRuntime::from_frame(target.run_id, &frame, session_id, ...)` 创建 runtime（`crates/agentdash-application/src/session/hooks_service.rs:391` 到 `:448`）。这里 session 是 runtime adapter binding，而不是 hook policy owner。

仍 session-first 的部分：

- `resolve_hook_runtime(session_id, turn_id, expected_frame_id, ...)` 是 launch-path adapter。它从 session id 和 expected frame id 临时构造 `AgentFrameRuntimeTarget`（`crates/agentdash-application/src/session/hooks_service.rs:116` 到 `:131`），随后继续用 session id 查 registry cache（`:132` 到 `:140`）。
- `ensure_hook_runtime_for_target` 第一件事仍是 `get_session_meta(&target.delivery_runtime_session_id)`，并用 delivery session id 清理或写入 registry hook cache（`crates/agentdash-application/src/session/hooks_service.rs:31` 到 `:50`, `:94` 到 `:101`）。这是合理的 live connector binding，但命名还容易让调用方把 session 当 target owner。
- `resolve_hook_target_frame_from_frame` 和 `build_frame_hook_runtime` 都需要 `find_by_session` 校验 delivery session anchor（`crates/agentdash-application/src/session/hooks_service.rs:336` 到 `:356`, `:423` 到 `:440`）。该行为应保留在 adapter 层，因为 hook runtime 必须确认 live delivery session 属于目标 agent。

结论：hook runtime 的事实模型已经接近目标，只差命名拆分。建议不要先大改 hook control 逻辑，而是把 `AgentFrameRuntimeTarget` 拆名或包成 `AgentRunRuntimeAddress + MessageStreamRef`，并保留 session registry adapter。

### 3. 推荐最小实现切片

#### Slice A: 先改 task coordinate resolver

优先级最高。原因是 task service、view projector、context builder、effect executor 是当前最明显的 node execution session-first 消费方；它们直接影响 task 状态、artifact、workflow VFS projection。这个切片可以与 mailbox/address 第一切片并行，因为不需要修改 `session/agent_run_mailbox.rs` 或 `session/mailbox_delegate.rs`。

实现方向：

- 在 application 层为 task 内部引入小型 `OrchestrationNodeCoordinate` / `TaskRuntimeNodeProjection` helper。可以先放在 `task/service.rs` 或 `task/view_projector.rs` 的私有 helper，若 service/projector/context/effect 都复用，再提到 `task/runtime_coordinate.rs` 或 workflow projection helper。不要先改 domain 公共类型，避免扩散。
- resolver 输入保持 `LifecycleSubjectAssociation + LifecycleRun + LifecycleAgent/current_frame`，输出显式 coordinate：`run_id, agent_id, frame_id, orchestration_id, node_path, attempt, node_status, observed_at, optional trace_session_id`。
- 从 `LifecycleRun.orchestrations[].node_tree` coordinate-first 遍历。`RuntimeSessionExecutionAnchor` 只用于填充 optional `trace_session_id` 或作为 terminal callback adapter 的 `MessageStreamRef` 证据。
- `TaskHookEffectExecutor::validate_runtime_task_anchor` 作为 session callback adapter 保留 `find_by_session` 起点，但校验必须升级：anchor -> coordinate 后确认 coordinate 对应 node 存在，并确认 task association 覆盖 run/agent；artifact/status context 可继续带 `session_id`，同时补充 `orchestration_id/node_path/node_attempt`。
- `build_task_session_context` 优先用 target/coordinate-first workflow projection。`workflow/projection.rs` 已有 `resolve_active_workflow_projection_for_target(&HookControlTarget, ...)`（`crates/agentdash-application/src/workflow/projection.rs:230` 到 `:258`），可作为收束方向；现有 `resolve_active_workflow_projection_for_session` 保留给 trace lookup（`:203` 到 `:228`）。

推荐写入文件范围：

- `crates/agentdash-application/src/task/service.rs`
- `crates/agentdash-application/src/task/view_projector.rs`
- `crates/agentdash-application/src/task/context_builder.rs`
- `crates/agentdash-application/src/task/gateway/effect_executor.rs`
- 可选新增：`crates/agentdash-application/src/task/runtime_coordinate.rs` 或 `crates/agentdash-application/src/workflow/orchestration/coordinate.rs`

潜在冲突：

- 与 mailbox/address 第一切片没有直接文件冲突。
- 可能与正在推进的 `AgentRunLifecycleSurfaceProjector` 类型命名产生重叠。若已有 `AgentRunRuntimeAddress` / `MessageStreamRef` 被主任务落地，应复用它们，不在 task 下另造公开类型。
- `task/view_projector.rs` 有 in-memory anchor repo test double（前一轮审计定位在该文件后半段），如果生产逻辑不再以 anchor 为主索引，测试夹具需要同步从 anchor-driven 改为 coordinate-driven + optional trace evidence。

测试命令：

- `cargo test -p agentdash-application task::service`
- `cargo test -p agentdash-application task::view_projector`
- `cargo test -p agentdash-application task::gateway`
- `cargo test -p agentdash-application task::context_builder`
- `cargo check -p agentdash-application`

#### Slice B: 再加 journey coordinate helper

优先级第二，建议与 Slice A 同一 implement agent 或紧随其后。原因是 journey helper 是 task coordinate 的低风险支撑：它可以在不删除 session helper 的情况下新增 coordinate-first helper，让 UI/CLI 打开 node trace 和定位 node execution 分层。

实现方向：

- 新增 `attempt_coordinate(run_id, orchestration_id, attempt)` 或更轻的 `node_coordinate(orchestration_id, attempt)` helper，返回 `OrchestrationNodeCoordinate` 形态。
- 新增 `step_coordinate(nodes, orchestration_id, key)` 与 `current_step_coordinate(nodes, orchestration_id)`。
- 原 `attempt_session_id`、`step_session_id`、`current_step_session_id` 保留，语义改为 trace-open helper：从 node executor ref 提取 delivery RuntimeSession，用于打开 transcript / trace。
- 不改 `read_session_projection(session_id, rest)`。它读取 meta/events/items/messages/tools/writes/summaries/turns/terminal，是明确的 session trace reader（`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:83` 到 `:170`）。

推荐写入文件范围：

- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs`
- 若 Slice A 抽了共享 coordinate 类型，可复用对应 module。

潜在冲突：

- 低冲突。只要不删除 `step_session_id` 等旧 helper，就不会影响现有 trace path。
- 如果 frontend 或 route 已经直接调用 session helper，需要后续单独迁移调用方；本切片只提供 coordinate helper。

测试命令：

- `cargo test -p agentdash-application workflow::lifecycle::journey`
- `cargo check -p agentdash-application`

#### Slice C: 最后做 hook target naming cleanup

优先级第三。原因是 hook runtime 已经有 `HookControlTarget`，当前问题主要是命名与 DTO 形态，不是事实链路错误；过早改 hook 命名会碰 launch path、runtime registry 和 hook tests，容易和 mailbox/address 类型落地互相等待。

实现方向：

- 先不要改 `build_frame_hook_runtime` 的 runtime behavior。它以 session id 建 registry binding、用 `HookControlTarget` 建业务 owner，是正确形态。
- 将 `AgentFrameRuntimeTarget` 重命名或包裹为更明确的结构，例如：

```rust
pub struct AgentFrameHookRuntimeBinding {
    pub address: AgentRunRuntimeAddress,
    pub message_stream: MessageStreamRef,
}
```

如果 `AgentRunRuntimeAddress` 尚未落地，则最小改法是新增 wrapper 而不改全部调用方：

```rust
pub struct AgentFrameRuntimeTarget {
    pub frame_id: Uuid,
    pub delivery_runtime_session_id: String,
}
```

继续保留，但注释改为 "adapter binding target"，并新增 conversion helper 输出 `HookControlTarget`。
- `resolve_hook_runtime(session_id, ...)` 明确命名为 launch/session adapter，内部立刻构造 binding target；`ensure_hook_runtime_for_target` 保持 target-first。
- `resolve_hook_target_frame_from_frame` 的 `find_by_session` 校验保留，因为它验证 delivery stream 是否属于 target agent。

推荐写入文件范围：

- `crates/agentdash-application/src/session/types.rs`
- `crates/agentdash-application/src/session/hooks_service.rs`
- `crates/agentdash-application/src/workflow/frame_hook_runtime.rs`（如 runtime constructor 类型签名需要跟随）
- hook 相关测试文件，尤其是 `session/hook_delegate.rs` 或 hooks service 单测所在文件

潜在冲突：

- 可能与 mailbox/address 第一切片共享 `AgentRunRuntimeAddress` / `MessageStreamRef` 命名，应等待或复用已落地类型。
- 会碰 launch hot path；只做命名和 wrapper 时风险可控，避免同时改 provider snapshot / registry cache 行为。

测试命令：

- `cargo test -p agentdash-application session::hooks_service`
- `cargo test -p agentdash-application session::hook_delegate`
- `cargo check -p agentdash-application`

### 4. 推荐顺序与并行策略

推荐顺序是：

1. `task coordinate resolver`：先把 task projection/effect 的 node ownership 从 anchor 派生改成 coordinate-first；收益最大，且不碰 mailbox/address 并行文件。
2. `journey coordinate helper`：给 coordinate-first consumer 一个轻量公共读法，同时保留 session helper 作为 trace adapter。
3. `hook target naming cleanup`：等 address/ref 类型稳定后做命名收束；行为改动最小化。

不建议先做 hook target naming。虽然 hook 文件看起来最接近完成态，但它会碰 launch/session runtime registry，且当前已经有 `HookControlTarget` 作为业务 owner；相比之下，task projection/effect 还在把 anchor 当 node selector，语义风险更高。

不建议先单独做 journey helper 后结束。helper 本身低风险，但如果 task service/projector 仍继续从 anchor 派生 node，coordinate helper 只会变成未消费的工具函数。

### 5. 应继续保留的 session-first trace adapters

以下入口应继续保留 session-first，原因是它们的对象就是 message stream / trace / live delivery binding，而不是业务 ownership：

- `LifecycleJourneyService::read_session_projection(session_id, rest)`：读取 session meta、events、items、messages、tools、writes、summaries、turns、terminal（`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:83` 到 `:170`）。
- `journey::attempt_session_id`、`step_session_id`、`current_step_session_id`：保留为 "open trace for node" helper，从 `ExecutorRunRef::RuntimeSession` 提取 session id（`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:572` 到 `:590`）。新增 coordinate helper 后，这些函数不再承担 node execution identity。
- `workflow::projection::resolve_active_workflow_projection_for_session`：保留为 trace lookup adapter，其注释已经限定生产链路只能把 RuntimeSession 作为 trace lookup 起点（`crates/agentdash-application/src/workflow/projection.rs:201` 到 `:228`）。
- `SessionHookService::resolve_hook_runtime(session_id, ...)`：保留为 launch/session adapter，但内部应尽快构造 target/binding，不能向外传播 session-owned hook target（`crates/agentdash-application/src/session/hooks_service.rs:116` 到 `:140`）。
- `SessionHookService` 的 runtime registry cache access by delivery session id：保留为 live connector binding cache；业务 owner 继续是 `HookControlTarget`（`crates/agentdash-application/src/session/hooks_service.rs:54` 到 `:63`, `:94` 到 `:101`）。
- `TaskHookEffectExecutor::validate_runtime_task_anchor` 的初始 `find_by_session`：保留为 terminal callback adapter，因为 hook effect 从 runtime trace 回调进入；但 adapter 后必须转到 coordinate/address/task association 校验（`crates/agentdash-application/src/task/gateway/effect_executor.rs:176` 到 `:239`）。
- `task/context_builder.rs::resolve_visible_canvas_mount_ids(runtime_session_id)`：短期可保留为 optional trace adapter，用于从 delivery stream 找 visible frame；若后续 task context builder 已持有 AgentRun address/current frame，应降级为 fallback-free trace evidence reader，而不是 context owner（`crates/agentdash-application/src/task/context_builder.rs:246` 到 `:272`）。

## Caveats / Not Found

- 本研究只读了用户指定的 task/hook/journey 文件、相关 spec、前置 research，以及为确认类型语义补充读取的 `runtime_session_anchor.rs`、`session/types.rs`、`workflow/projection.rs`。没有全量审计 API route、frontend 调用方、数据库 repository 实现或 generated DTO。
- 当前工作树存在并行改动约束：不要修改 `crates/agentdash-api/src/routes/lifecycle_agents.rs`，也不要修改 `crates/agentdash-application/src/session/agent_run_mailbox.rs` 或 `crates/agentdash-application/src/session/mailbox_delegate.rs`。本推荐切片已避开这些文件。
- `cargo test` 命令是基于模块路径的建议；若 crate 当前没有这些精确 test target，implement agent 可退到更宽的 `cargo test -p agentdash-application task::` / `workflow::lifecycle::journey` / `session::hook` 与 `cargo check -p agentdash-application`。
- 若主线已经落地 `AgentRunRuntimeAddress`、`MessageStreamRef` 或 `OrchestrationNodeCoordinate` 公共类型，后续实现必须复用主线类型，避免 task/hook 层各自定义同名公开合同。
