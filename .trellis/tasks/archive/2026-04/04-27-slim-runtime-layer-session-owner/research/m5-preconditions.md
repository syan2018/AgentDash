# Research: M5 Preconditions — compose_task_runtime 并入 compose_lifecycle_node + activate_story_step facade

- **Query**: M5 前置事实调研——`compose_task_runtime` 消费链、`LifecycleNodeSpec` 缺字段、`activate_step` facade 准入条件、Task 入口链、`resolve_workflow_via_task_sessions` 调用点、session dispatch 路径。
- **Scope**: internal
- **Date**: 2026-04-27
- **避开区**: `crates/agentdash-application/src/task/` 大部分、`api/routes/task_execution.rs`、`mcp/servers/{task,story,relay}.rs`、`application/src/story/management.rs` 处于 M1-b mid-edit 状态；本调研聚焦 `session/assembler.rs`、`workflow/`、`api/routes/workflows.rs`、`api/bootstrap/turn_dispatcher.rs`。

---

## 1. `compose_task_runtime` 的消费链

**本体**: `crates/agentdash-application/src/session/assembler.rs:832-1041`（210 行，唯一函数）。
**Spec**: `TaskRuntimeSpec` at `assembler.rs:1145-1156`；**Output**: `TaskRuntimeOutput` at `assembler.rs:1160-1172`。

**唯一调用者（1 处）**: `crates/agentdash-application/src/task/gateway/turn_context.rs:74`（`prepare_task_turn_context`）。该函数把 `TaskRuntimeOutput` 11 个字段映射成 `PreparedTurnContext`（`turn_context.rs:24-41`）。`PreparedTurnContext` 再在 `turn_dispatcher.rs:30-65`（`AppStateTurnDispatcher::dispatch_turn`）被消费，组装成 `PromptSessionRequest` 调 `session_hub.start_prompt`。

**TaskRuntimeOutput → PromptSessionRequest 字段用法**（dispatcher 视角，`turn_dispatcher.rs:37-53`）：

| TaskRuntimeOutput 字段 | dispatcher 是否使用 | 去向 |
|---|---|---|
| `built.prompt_blocks` | yes | `PromptSessionRequest.user_input.prompt_blocks` |
| `built.working_dir` | yes | `user_input.working_dir` |
| `built.mcp_servers` | yes（经 acp 转换） | `PromptSessionRequest.mcp_servers` |
| `built.system_context` | yes | `PromptSessionRequest.system_context` |
| `built.source_summary` | yes（只在 `StartedTurn` 回传） | `StartedTurn.context_sources` |
| `resolved_config` | yes | `user_input.executor_config` |
| `vfs` | yes | `PromptSessionRequest.vfs` |
| `flow_capabilities` | yes | `PromptSessionRequest.flow_capabilities` |
| `effective_capability_keys` | yes | `PromptSessionRequest.effective_capability_keys` |
| `relay_mcp_server_names` | yes | `PromptSessionRequest.relay_mcp_server_names` |
| `use_cloud_native_agent` | **no**（仅 `PreparedTurnContext` 存了，dispatcher 不用） | 死字段（M5 可删） |
| `workspace` | **no**（service 层会另做 `append_visible_canvas_mounts`，见 `task/service.rs:418-429`，它走 `self.hub.get_session_meta` + `self.repos.canvas_repo`，不依赖 `output.workspace`） | 死字段 |
| `executor_resolution` | **no**（未被 dispatcher 读） | 死字段（M5 可删） |
| `workflow` (`ActiveWorkflowProjection`) | **no** | 死字段 — compose 内部消耗它来建 vfs mount / capability directives |
| `resolved_bindings` (`ResolveBindingsOutput`) | **no** | 死字段 — compose 内部已注入到 `WorkflowContextBindingsContributor` |

**结论**（M5 合并进 `PreparedSessionInputs`）：

- **必须保留到 PreparedSessionInputs**: `prompt_blocks / working_dir / mcp_servers / system_context / source_summary / resolved_config / vfs / flow_capabilities / effective_capability_keys / relay_mcp_server_names`。注意当前 `PreparedSessionInputs` 已有 `vfs / mcp_servers / system_context / executor_config / flow_capabilities / capability_keys / bootstrap_action` 等（见 `assembler.rs:75-102` 左右），**缺的是**: `prompt_blocks`（目前通过 `with_prompt_blocks` 已支持，检查 builder 是否把它塞进 PreparedSessionInputs）、`working_dir`、`relay_mcp_server_names`、`source_summary`。`compose_lifecycle_node` 现在 build 出来的 `PreparedSessionInputs` 不含这几项（orchestrator 调用时 `base = PromptSessionRequest::from_user_input(UserPromptInput::from_text(""))`，再 `finalize_request(base, prepared)`，见 `workflow/orchestrator.rs:696-697`）。
- **可以删除**: `use_cloud_native_agent / workspace / executor_resolution / workflow / resolved_bindings` —— 这些是 compose 内部状态或 TaskRuntimeOutput-only 诊断字段，dispatcher 不消费。
- [NEED_FURTHER_INVESTIGATION]: `PreparedTurnContext.identity` 和 `post_turn_handler` 在 `task/service.rs:430, 444` 被 service 层注入（不是 compose 产出），M5 后仍需有路径把这两个挂到 facade 输出上；`PreparedSessionInputs` / dispatch 入口得支持它们。

---

## 2. `LifecycleNodeSpec` 现状与需要补齐的字段

**当前 Spec**（`assembler.rs:1175-1181`，5 个字段）:

```rust
pub struct LifecycleNodeSpec<'a> {
    pub run: &'a LifecycleRun,
    pub lifecycle: &'a LifecycleDefinition,
    pub step: &'a LifecycleStepDefinition,
    pub workflow: Option<&'a WorkflowDefinition>,
    pub inherited_executor_config: Option<AgentConfig>,
}
```

**当前实现**（`assembler.rs:1065-1099`）—— 仅调 `activate_step_with_platform` → `SessionAssemblyBuilder::apply_lifecycle_activation` → build。**不做**: executor resolve / VFS workspace build / context contributor pipeline / prompt blocks 注入 / context_bindings 解析 / workspace declared_sources。

**对比 TaskRuntimeSpec（1145-1156）**，M5 需要补到 `LifecycleNodeSpec`（或新 spec 结构）的字段清单：

1. `task: Option<&'a Task>` — 承载 task-specific agent_binding，用于 executor 解析和 context sources。（M5 story-as-owner 后，这是 story 的 task 而非独立）
2. `story: &'a Story` — `build_task_agent_context` 的必要入参；declared sources 来自 `story.context.source_refs`。
3. `project: &'a Project` — executor 默认值、 capability 查找、VFS build 的 scope。
4. `workspace: Option<&'a Workspace>` — `resolve_workspace_declared_sources` 的 scope + VFS workspace mount。
5. `phase: TaskRuntimePhase` — 决定 `TaskExecutionPhase::Start | Continue`，进入 prompt 模板。
6. `override_prompt: Option<&'a str>` — Task 入口透传（start_task 的 req.override_prompt）。
7. `additional_prompt: Option<&'a str>` — Task 入口透传（continue_task 的 req.additional_prompt）。
8. `explicit_executor_config: Option<AgentConfig>` — HTTP 请求中用户指定的 executor；当前 `inherited_executor_config` 语义更窄（companion 继承），需要区分。
9. `strict_config_resolution: bool` — 当前 task 路径 `true`，orchestrator 路径 `false`（需要的话）。

**现有 compose_lifecycle_node 已有但需要扩展的逻辑**:

- VFS workspace build（当前 compose_lifecycle_node **没做**，它只走 `apply_lifecycle_activation`，lifecycle_mount 来自 activation 产出）。M5 需要把 `vfs_service.build_vfs(project, story, workspace, SessionMountTarget::Task, agent_type)` 纳入（见 `compose_task_runtime:868-896`）。
- context_bindings 解析（`compose_task_runtime:898-916`）+ `WorkflowContextBindingsContributor`。
- `build_task_agent_context` 走 contributor pipeline（`compose_task_runtime:1011-1026`）—— contributor 流要以参数形式注入还是下沉到 compose 内部？[NEED_FURTHER_INVESTIGATION]

**补字段遗漏清单**（PRD/spec 未明确的）:

- `contributor_registry: &'a ContextContributorRegistry` — `build_task_agent_context` 第二个入参；`compose_lifecycle_node` 当前不需要它，M5 扩展后需要。
- `availability: &dyn BackendAvailability` — `resolve_workspace_declared_sources` 的入参（`compose_task_runtime:971-979`）。
- `vfs_service: &RelayVfsService` — 已被 `SessionRequestAssembler` 持有，但 `compose_lifecycle_node` 当前是 free function（不走 assembler 实例），M5 要么把它升级到 service 方法，要么在 spec 里传 &RelayVfsService。
- [BLOCKER] **入参传递风格**: 当前 `compose_lifecycle_node` 是 free function 签名 `(repos, platform_config, spec)`；`compose_task_runtime` 是 `&self` 方法持有 `vfs_service / availability / contributor_registry`。M5 合并后要统一成 method 形式，还是把这几个服务塞 spec？spec `story-task-runtime.md:301-302` 未指定。

---

## 3. `LifecycleRunService::activate_step` 作为 facade 的准入条件

**当前签名**（`workflow/run.rs:138-147`）：

```rust
pub async fn activate_step(&self, cmd: ActivateLifecycleStepCommand)
    -> Result<LifecycleRun, WorkflowApplicationError>;
// cmd = { run_id: Uuid, step_key: String }
```

**`bind_session_and_activate_step`**（`workflow/run.rs:196-207`）签名多一个 `session_id: String`，内部 `bind_step_session + activate_step` 两个领域调用都转成一次 `run_repo.update`。

**调用者（当前 3 处 + orchestrator 内部使用 1 处）**:

1. `crates/agentdash-api/src/routes/workflows.rs:343`（HTTP `POST /workflows/runs/{run_id}/steps/{step_key}/activate`）—— 直接透传 `ActivateLifecycleStepCommand`，不 compose session、不 dispatch。
2. `crates/agentdash-application/src/workflow/orchestrator.rs:395`（PhaseNode 激活，`apply_activated_phase_nodes` 分支之前的 loop）。
3. `crates/agentdash-application/src/workflow/orchestrator.rs:602`（`bind_session_and_activate_step`，AgentNode session 创建时）。
4. `crates/agentdash-api/src/routes/project_agents.rs:1002`（额外发现的调用点）。

**Facade `activate_story_step(story_id, step_key, user_input)` 所需补齐**:

- **Story → Run 查询**:
  - `LifecycleRunRepository` 现有方法（`crates/agentdash-domain/src/workflow/repository.rs:54-63`）: `get_by_id / list_by_project / list_by_lifecycle / list_by_session / create / update / delete`。**没有 `list_by_story` / `find_active_run_for_story`**。
  - 现有 `select_active_run(Vec<LifecycleRun>) -> Option<LifecycleRun>`（`workflow/run.rs:59-71`）已能从 list 里挑出 Ready/Running/Blocked 的最新一条。
  - 两条可选路径（PRD 允许二选一）:
    1. 新增 `LifecycleRunRepository::list_by_story(story_id) -> Vec<LifecycleRun>`（需同时改 domain trait + pg / memory 实现）。
    2. 两跳 via SessionBinding：`session_binding_repo.list_by_owner(SessionOwnerType::Story, story_id) → binding.session_id → lifecycle_run_repo.list_by_session(session_id)`，再 `select_active_run`。前例有：`session/assembler.rs:1376-1403 resolve_workflow_via_task_sessions` 就是这个两跳模式。
  - [BLOCKER] 选哪条？repo 层新方法更 performant，但要碰 M1-b 正在改的文件（`session_binding_repo` 相关）；两跳可复用现有代码。
- **user_input 参数**: 当前 `ActivateLifecycleStepCommand` 无此字段；facade 签名里 `user_input: Option<UserPromptInput>`（spec 283-313）需要从 facade 内部透传到 `compose_lifecycle_node` 的 `override_prompt`/`additional_prompt`，而不改 `activate_step` 自己。
- **事务边界**: `activate_step` 是 "load_run → run.activate_step → run_repo.update" 两步 IO 非事务（已在 tbd-verifications.md:100 确认）。facade 额外追加 compose + dispatch 后，链路变成 "activate_step IO → compose_lifecycle_node IO（读 inline_file_repo、 workflow_definition_repo）→ session_hub.dispatch（远端 IO）"。这些 **全部在单个 async 方法里串联可行**，但不是 ACID 事务；failure 场景需要补偿。
- **Composer pattern 可参考**:
  - `workflow/orchestrator.rs:657-704 start_agent_node_prompt` —— 已经示范了 "compose_lifecycle_node → finalize_request(base, prepared) → session_hub.start_prompt" 串联（3 步 IO）。
  - `companion/tools.rs:406` —— companion dispatch 同样走 `finalize_request(base, prepared)` 后 `send prompt`。

**结论**: facade 可在一个 async method 内把三步（activate_step + compose + dispatch）串起来，有 orchestrator 原型可模仿；非事务语义与现状一致。

---

## 4. Task 启动路径的入口命名链

**HTTP handlers**（`crates/agentdash-api/src/routes/task_execution.rs`）:

- `start_task(Path(id), Json(req)) -> StartTaskResponse`（L80-115）
- `continue_task` (L117-152)
- `cancel_task` (L154-174)

**均直接走** `state.services.task_lifecycle_service.{start_task|continue_task|cancel_task}(TaskExecutionCommand { task_id, phase, prompt, executor_config, identity })`（`task_execution.rs:96-104`）。

**Service methods**（`crates/agentdash-application/src/task/service.rs`）:

- `pub async fn start_task(&self, cmd) -> TaskExecutionResult`（L66-75）→ `lock_map.with_lock(task_id, start_task_inner)`（内部在 L141，M1-b mid-edit 不写行号结论）。
- `pub async fn continue_task` (L77-86) → `continue_task_inner` (L265)。
- `pub async fn cancel_task` (L88-97) → `cancel_task_inner` (L346)。

**当前调用链**:

```
HTTP start_task handler
  → TaskLifecycleService::start_task(cmd)
    → start_task_inner
      → dispatch_prepared_turn(task, session_id, phase, override_prompt, additional_prompt, executor_config, identity)  (service.rs:392-449)
        → prepare_task_turn_context (turn_context.rs:47)
          → SessionRequestAssembler::compose_task_runtime (assembler.rs:832)
        → self.dispatcher.dispatch_turn(session_id, ctx)
          → AppStateTurnDispatcher (api/bootstrap/turn_dispatcher.rs:32)
            → session_hub.start_prompt
```

**M5 后目标调用链**（依 spec `story-task-runtime.md:291-297`）:

```
HTTP start_task handler
  → TaskLifecycleService::start_task(cmd)  [保留名字, 作为 Service-level facade]
    → 定位 task.story_id + 解析 step_key
    → activate_story_step(story_id, step_key, user_input)
      → LifecycleRunService::activate_step(run_id, step_key)
      → compose_lifecycle_node(full spec)  → PreparedSessionInputs
      → session_hub.dispatch (start_prompt)
```

**Facade 粒度**: PRD 与 spec 均倾向 **service-level facade**——`TaskLifecycleService::start_task` 方法名保留，内部委托 `activate_story_step`。HTTP 路由层（`task_execution.rs:80-115`）**不改签名、不改 URL**，只是服务内部实现变。这样 axum 路由表、`AppState.services.task_lifecycle_service` 依赖链、前端 `TaskResponse` DTO 都不需要动。

[BLOCKER] `activate_story_step` 归属 **哪个 Service**？ 三个候选：
1. 新建 `StoryLifecycleService` 或 `TaskLifecycleService` 内部新方法（沿用现有服务集合）。
2. 挂到 `LifecycleRunService`（但它当前是 thin repo wrapper，不持有 `SessionHub` / `vfs_service` / `contributor_registry`）。
3. 新 facade struct（独立于 task/lifecycle_run，纯组合）。
PRD 未明确。spec `story-task-runtime.md:306-318` 写签名但未指定 impl 归属。

---

## 5. 孤立的 `resolve_workflow_via_task_sessions` 消费点

**两处同名定义**（重复代码 smell）:

1. `crates/agentdash-application/src/session/assembler.rs:1376-1403`（私有 free function, 被同文件 L865 `compose_task_runtime` 调）。
2. `crates/agentdash-application/src/task/session_runtime_inputs.rs:225-250`（私有 free function, 被同文件 L67 `build_task_session_runtime_inputs` 调）。

**调用者合计 2 处**，都在 M5 删除面内：

- `session/assembler.rs:865` `compose_task_runtime` 内部 → 随 `compose_task_runtime` 删除。
- `task/session_runtime_inputs.rs:67` `build_task_session_runtime_inputs` → **整个文件 `task/session_runtime_inputs.rs` 随 M5 删除**（spec `story-task-runtime.md:303` 明确）。

[NEED_FURTHER_INVESTIGATION] `task/session_runtime_inputs.rs::build_task_session_runtime_inputs` 还有哪些调用者？M1-b 可能正在改 `task/` 下文件，这个函数的上游调用者需要确认——grep 当前未查（避开 mid-edit 文件），建议 M5 实施前由 agent 再次 grep 确认。

**替代路径（M5 后）**:

- 主路径通过 Story session 定位 run：`find_active_run_for_story(story_id)`（见调研项 3），用在 `activate_story_step` facade 内部。
- 被删函数原本目的是 "从 task 反查 active workflow projection"；M5 story-as-owner 后，story 直接持有 session，task 不再需要反查。

---

## 6. Session Dispatch 路径

`compose_lifecycle_node` 产出 `PreparedSessionInputs` 之后的典型消费模式（已在代码中存在）:

**Pattern A: orchestrator 自启动 prompt**（`workflow/orchestrator.rs:683-702`）:

```rust
let prepared = compose_lifecycle_node(&self.repos, &self.platform_config, LifecycleNodeSpec {...}).await?;
let base = PromptSessionRequest::from_user_input(UserPromptInput::from_text(""));
let req = finalize_request(base, prepared);  // assembler.rs:99
self.session_hub.start_prompt(session_id, req).await
```

**Pattern B: routine executor**（`routine/executor.rs:505`）: `finalize_request(base, prepared)` 返回 `PromptSessionRequest` 交给外部 dispatch。

**Pattern C: companion dispatch**（`companion/tools.rs:406`）: 同 finalize_request 后交 `send_prompt`。

**Pattern D: acp_sessions 两处**（`api/routes/acp_sessions.rs:1190, 1271`）: 同 finalize_request。

**`api/routes/workflows.rs::activate_workflow_step`**（L324-346）**当前不 dispatch session**——它只调 `LifecycleRunService::activate_step` 更新 run 状态后返回 `LifecycleRun` JSON，**不触发任何 compose / prompt**。这是 M5 要改变的行为：facade 化后应包含 compose + dispatch。

**M5 后 `activate_story_step` 内部 dispatch**:

- 走 Pattern A（orchestrator 模式）最直接: 内部持有 `session_hub: SessionHub`（现 `TaskLifecycleService.hub` 已有）+ `finalize_request` + `start_prompt`。
- HTTP 路由是否复用? `task_execution.rs::start_task` 的 handler 保留签名，内部委托 facade；`workflows.rs::activate_workflow_step` **可以**（但不强制）升级为调用 `activate_story_step`（此时行为发生变化：从 "只更新 run 状态" 变为 "激活 + 装配 + dispatch"）——**[BLOCKER]** 这个路由是否升级需要用户决策，因为行为语义会变，前端 `services/workflow.ts:496` 已依赖当前"只状态变更"的语义。

---

## Caveats / 待决

- [BLOCKER] **字段补齐风格**: `LifecycleNodeSpec` 扩成大结构 vs. 新建 `StoryStepSpec` 包装；PRD/spec 只说"补齐 override_prompt / additional_prompt / explicit_executor_config / story 上下文"，未定结构名。
- [BLOCKER] **facade 归属 Service**: `activate_story_step` 挂在 `TaskLifecycleService` / 新 `StoryLifecycleService` / `LifecycleRunService` / 独立 facade struct 中的哪一个未决。
- [BLOCKER] **Story → Run 查询路径**: 新增 `list_by_story` repo 方法 vs. 两跳走 SessionBinding。两跳方案需碰 session_binding 相关代码（M1-b 可能 mid-edit）。
- [BLOCKER] **`workflows.rs::activate_workflow_step` HTTP 路由是否升级**: 现仅改 run 状态，不 dispatch；若升级成"激活+装配+dispatch"，前端消费契约改变。
- [NEED_FURTHER_INVESTIGATION] `PreparedTurnContext.identity` / `post_turn_handler` 在 M5 合并后如何传递——`PreparedSessionInputs` 当前是否支持这两字段？需读 `assembler.rs:75-130` 确认。
- [NEED_FURTHER_INVESTIGATION] `build_task_session_runtime_inputs` 的其他调用者（避开 mid-edit 文件未查），M5 前必须确认没有遗留。
- [NEED_FURTHER_INVESTIGATION] `append_visible_canvas_mounts`（`task/service.rs:418-429`）在 M5 后仍要保留到 `start_task` facade 末尾（或下沉到 compose）——当前依赖 `self.hub.get_session_meta(session_id)`，facade 化时路径需明确。
- **行号锚定声明**: 本文件所有行号来自本次调研时的 HEAD（非 mid-edit 文件）；`session/assembler.rs`、`workflow/run.rs`、`workflow/orchestrator.rs`、`api/routes/workflows.rs`、`api/routes/task_execution.rs`、`api/bootstrap/turn_dispatcher.rs`、`task/gateway/turn_context.rs`、`task/session_runtime_inputs.rs`、`workflow/repository.rs` 这些文件不在 M1-b 改动面，行号可依赖；`task/service.rs` 虽然列了行号但只锚定到方法入口/明显不易变的 helper (dispatch_prepared_turn 外部签名)，inner 逻辑行号未写结论。

## Related Specs

- `.trellis/spec/backend/story-task-runtime.md` §7 "Task 启动路径统一" (L280-319)
- `.trellis/spec/backend/capability/tool-capability-pipeline.md:292` (提及 `resolve_workflow_via_task_sessions`)
- `.trellis/tasks/04-27-slim-runtime-layer-session-owner/prd.md` M5 section (L109-113, L158, L171, L224)
- `.trellis/tasks/04-27-slim-runtime-layer-session-owner/research/tbd-verifications.md` (LifecycleRunService::activate_step 现状 L90-117)
