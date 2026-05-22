# Session Startup Pipeline

本 spec 定义 session 构建与 prompt launch 的生产主线。长期目标只认一条数据流：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> ExecutionContext connector projection
  -> SessionEvent / TerminalEffectOutbox
```

`LaunchCommand` 表达来源意图；`SessionConstructionPlan` 是构建事实源；
`LaunchExecution` 是单次 launch 的执行计划；`ExecutionContext` 只在 connector
边界投影。

## Stage Responsibilities

| 阶段 | 输入 | 输出 | 职责 |
|---|---|---|---|
| Source adapter | HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 请求 | `LaunchCommand` | 保留来源身份、请求意图、source policy、prompt payload、executor override、follow-up hint |
| Construction | `LaunchCommand` + session/domain/runtime facts | `SessionConstructionPlan` | 解析 owner、workspace、working dir、VFS、MCP、capability、context bundle/frame、identity、query/audit/inspector projection、resolution trace |
| Launch planning | `LaunchCommand` + `SessionConstructionPlan` + runtime facts | `LaunchExecution` | 解析 resolved prompt payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input |
| Execution | `LaunchExecution` | connector prompt + session events | claim/activate turn，写 start/user events，调用 connector，connector accepted 后提交 bootstrap/pending/title 成功副作用 |
| Terminal | connector terminal / stream terminal | terminal event + outbox effect | 持久化终态，清理 active turn，把后续业务副作用写入 durable outbox |

`Turn` 边界保持很薄：reservation、active、cancel、hook runtime handle、
processor/adapter supervision、terminal release。

## Source Adapter Contract

Source adapter 只做来源语义转换，不能预先组装最终运行事实。

| 来源 | `LaunchCommand` 应携带 |
|---|---|
| HTTP prompt | request DTO、auth identity、prompt payload、executor override |
| Task service | task id、phase/override/additional prompt source hint、task source identity |
| Workflow orchestrator | workflow/lifecycle source identity、step activation intent |
| Routine executor | routine source identity，系统身份来自 `AuthIdentity::system_routine(routine.id)` |
| Companion dispatch / parent resume | parent session id、dispatch/slice/target binding/source policy |
| Hook auto-resume | hook trigger identity、resume intent、follow-up hint |
| Local relay | workspace root、原始 MCP declaration、relay source identity |

`working_dir` 是 construction 解析结果，不属于用户 prompt input。Local relay 的
workspace root 是来源事实；resolved VFS、resolved MCP、capability state、
context bundle 和 connector input 都由 construction/launch 产出。

Task terminal effect 使用 durable binding 描述，由 construction/effects 解析。
command 边界不传内存 `post_turn_handler` 或其它 trait object。

## Construction Contract

`SessionConstructionProvider::build_construction` 直接输出 `SessionConstructionPlan`。
输出必须是 launch-ready final facts，不是 seed、partial plan 或等待 LaunchPlanner 补齐的
中间形态。

`SessionConstructionPlan` 至少覆盖：

- `ResolvedSessionOwner`，owner 解析顺序统一为 `Task -> Story -> Project`。
- workspace 与 typed working directory。`workspace.working_directory` 必须在进入
  `SessionLaunchPlanner` 前为 `Some`。
- VFS、MCP declaration resolution、capability state。`surface.vfs`、
  `projections.mcp_servers` 与 `projections.capability_state` 必须已经完成最终裁决。
- `SessionContextBundle` 与 continuation/context frames。
- identity、source contract、query/audit/inspector projections。
- resolution trace，用于审计为什么选择某个 owner/workspace/context。

Launch 前必须调用 `SessionConstructionPlan::validate_for_launch()` 或等价 gate：

- 缺少 `workspace.working_directory`、`execution_profile.executor_config`、
  `surface.vfs`、`projections.capability_state` 时拒绝 launch。
- `projections.capability_state.vfs.active` 必须等于 `surface.vfs`。
- `projections.capability_state.tool.mcp_servers` 必须等于
  `projections.mcp_servers`。
- pending runtime command 的 overlay 由 Construction 阶段形成 final
  `capability_state`，但 command store 的 `requested -> applied` 副作用仍只能在
  connector prompt accepted 后提交。

Construction 可以消费 runtime facts（session meta、live runtime 状态、requested
runtime commands、cached runtime capability snapshot），但这些 facts 一旦进入
`SessionConstructionPlan` 就必须体现在 `resolution` trace 中。LaunchPlanner 不允许再读取
cached profile、hub default VFS、local relay workspace root 或 source MCP declaration 来补齐
VFS/MCP/capability/executor facts。

Context endpoint、权限展示、audit 和 inspector 都投影同一份
`SessionConstructionPlan`。API route 的职责是 auth/permission、DTO 转换、
调用 use case、映射 response DTO。

Companion parent facts 由 construction/assembler 根据 parent session id 解析；
API/bootstrap 只传 parent 引用与 dispatch policy。

## Scenario: Capability Projection Normalization

### 1. Scope / Trigger

- Trigger: Session runtime surface、VFS、MCP、Skill baseline 与 `CapabilityState` 是同一份 construction projection 的不同维度，需要在 launch、context inspect 与 runtime transition 中保持一致。

### 2. Signatures

- Application entry: `derive_session_capability_projection(SessionCapabilityProjectionInput) -> SessionCapabilityProjection`
- Application entry: `normalize_capability_state_dimensions(&mut CapabilityState, Option<Vfs>, Vec<SessionMcpServer>, &SessionBaselineCapabilities)`
- Context query: `build_session_context_plan(...) -> SessionConstructionPlan`，并在 construction finalize 后生成 query-only `runtime_surface`

### 3. Contracts

- `CapabilityResolver` 继续只解析 tool / MCP / companion 维度。
- Effective VFS 由 construction finalize 合并 owner/session/runtime-command facts 后确定。
- Skill baseline 与 guidelines 从 effective VFS 派生；local extra skills 以 VFS skill name map 作为冲突基线。
- `CapabilityState.vfs.active` 必须等于 final `plan.surface.vfs`。
- `CapabilityState.tool.mcp_servers` 必须等于 final `plan.projections.mcp_servers`。
- `runtime_surface` 是 query DTO，只从 final `plan.surface.vfs` 生成。

### 4. Validation & Error Matrix

- final VFS 缺失且无可解析 workspace root -> `BadRequest`
- final VFS 缺少 default mount 或 default mount root 无效 -> `BadRequest`
- `CapabilityState.vfs.active != plan.surface.vfs` -> launch validation failure
- `CapabilityState.tool.mcp_servers != plan.projections.mcp_servers` -> launch validation failure

### 5. Good / Base / Bad Cases

- Good: pending VFS overlay 合并后，context response 的 `vfs` 与 `runtime_surface.mounts` 都包含 overlay mount。
- Base: 没有 pending runtime command 时，context response 从 construction base VFS 派生 surface。
- Bad: runtime transition 的 after-state 缺少 Skill baseline，导致下一轮 context frame 与工具可见说明不包含 active VFS 内嵌 skill。

### 6. Tests Required

- API/session context test：final `surface.vfs` 与 `runtime_surface` 使用同一 mount 集合。
- Application/session test：live 与 pending runtime transition 都从 active VFS 派生 Skill baseline。
- Canvas tool test：`present_canvas` 在 `canvas_presented` 事件前完成 session meta、active VFS 与 Skill baseline 同步。

### 7. Implementation Shape

```text
base construction facts
  -> effective VFS / MCP resolution
  -> derive_session_capability_projection
  -> normalize_capability_state_dimensions
  -> final SessionConstructionPlan
  -> query-only runtime_surface
```

## LaunchExecution Contract

`SessionLaunchPlanner::plan` 返回 `LaunchExecution`。planner 输入由
`SessionLaunchDeps`、`LaunchCommand`、`SessionConstructionPlan` 与 runtime facts
组成。

`LaunchExecution` 承载或引用：

- resolved prompt payload；
- `SessionConstructionPlan`；
- lifecycle / restore / hook / follow-up plan；
- pending runtime command apply plan；
- terminal effect plan；
- connector input projection；
- launch trace。

Connector input 的 working directory、executor config、MCP、VFS、identity、
capability state 和 context frame 都从 final `SessionConstructionPlan` 与
`LaunchExecution` 投影生成。`prompt_pipeline` 的职责是执行该计划，而不是重新解析
owner、context、VFS、MCP 或 capability。

`SessionLaunchPlanner` 只能处理 runtime-only planning：

- resolved prompt payload；
- lifecycle / restore / hook / follow-up；
- requested runtime command apply plan；
- terminal effect plan；
- connector input projection。

禁止在 LaunchPlanner 中出现 VFS/MCP/capability/executor fallback chain，尤其是：

- hub default VFS；
- cached session profile VFS/MCP/capability；
- local relay workspace root 到 VFS 的转换；
- source MCP declaration 合并；
- skill / guideline discovery；
- `SessionConstructionPlanner::plan_launch` 这类二次 construction。

## Terminal Effects

Terminal fact 先进入 event store，业务副作用进入 durable outbox。

当前 effect 类型：

- `hook_effects`
- `session_terminal_callback`
- `hook_auto_resume`

Outbox 状态为 `pending / running / succeeded / failed / dead-letter`。dispatcher
支持进程重启后的 replay，handler 以 idempotency key 保证幂等。

## Scenario: Freeform Session Lifecycle Ownership

### 1. Scope / Trigger

- Trigger: 普通自由会话也需要进入 LifecycleRun 过程归属模型，避免 session 与 workflow 过程形成两套事实源。

### 2. Signatures

- API create: `POST /sessions { title?: string, project_id: uuid } -> SessionMeta`
- Backend service: `FreeformLifecycleService::ensure_run_for_session(project_id, session_id) -> LifecycleRun`
- Builtin keys:
  - workflow: `builtin.freeform_agent`
  - lifecycle: `builtin.freeform_session`
  - activity: `main_conversation`
- DB: `lifecycle_runs.activity_state TEXT NULL`

### 3. Contracts

- `/sessions` 创建的是 project-scoped 业务会话，必须先校验调用者对 `project_id` 有 `Edit` 权限。
- 新 session 必须创建 `SessionBinding(owner_type=Project, owner_id=project_id, label=freeform)`。
- 没有显式 lifecycle 的普通会话必须调用 `ensure_run_for_session`，生成 `LifecycleRun.session_id = session.id`。
- 启动对账会扫描 project-bound 业务 root session；若没有任何 LifecycleRun，则补齐 freeform LifecycleRun。
- 对账跳过 `lifecycle_node:*`、`lifecycle_activity:*`、`companion:*` 这类派生 session label。
- freeform lifecycle 是单 Activity graph：`main_conversation` 使用 `Agent + ContinueRoot` 与 `ActivityCompletionPolicy::OpenEnded`。
- `OpenEnded` 表示普通 prompt terminal 不会自动完成 activity；归档、显式结束或后续产品动作再提交 completion/cancel event。

### 4. Validation & Error Matrix

- `/sessions` 缺少 `project_id` -> request DTO 反序列化失败。
- `project_id` 无编辑权限 -> `403`。
- builtin workflow/lifecycle definition 校验失败 -> `400 BadRequest`。
- 同一 session 已存在 activity run -> 返回既有 run，不创建重复 run。
- session 已绑定到其他 Project -> session binding repository 拒绝跨 Project 复用。
- 对账发现 session 已有任意 LifecycleRun -> 不补 freeform run。

### 5. Good/Base/Bad Cases

- Good: 用户在当前 Project 下创建普通会话，系统创建 session、Project binding、freeform LifecycleRun，并初始化 `main_conversation#1 ready`。
- Base: ProjectAgent 配置了显式 lifecycle 时，按显式 lifecycle 创建 run，不额外创建 freeform run。
- Base: 旧的 project-bound session 在启动对账中补齐 freeform run。
- Bad: 创建无 project scope 的裸 session；后续无法从项目过程视图反查其 LifecycleRun。

### 6. Tests Required

- `cargo test -p agentdash-application workflow::freeform`：断言 builtin definition、open-ended policy、run 初始化和幂等复用。
- `cargo check -p agentdash-api`：断言 API/request 接线编译通过。
- `cargo test -p agentdash-application reconcile::boot`：断言启动对账会跳过派生 session label。
- 前端 `pnpm --filter app-web typecheck`：断言 `createSession(title, projectId)` 调用契约一致。

### 7. Wrong vs Correct

#### Wrong

```rust
let meta = session_core.create_session(title).await?;
return Ok(meta);
```

#### Correct

```rust
let meta = session_core.create_session(title).await?;
session_binding_repo.create(&project_binding).await?;
freeform_service
    .ensure_run_for_session(project_id, &meta.id)
    .await?;
```

## Pending Runtime Commands

Runtime context / capability transition 的事实源是 runtime command event/store。
Projection 只服务查询、apply-once 与失败恢复。

状态流：

```text
requested -> applied
requested -> failed
```

connector.prompt accepted 后再标记 applied；connector.prompt 失败时保留
requested/failed 事实供下一轮恢复。

### Scenario: Runtime Context Patch Replay

#### 1. Scope / Trigger

- Trigger: pending runtime command 需要跨进程保存“下一轮应应用的 runtime context 变化”，同时保持 `CapabilityState`、VFS、Skill、MCP 与 runtime surface 由同一条 projection pipeline 生成。

#### 2. Signatures

- Persisted payload type: `PendingCapabilityStateTransition`
- Patch field: `RuntimeContextPatch { tool, companion, vfs_overlay, mount_directives }`
- Replay entry: `apply_runtime_context_patch(base_state, patch) -> CapabilityState`

#### 3. Contracts

- `PendingCapabilityStateTransition` 保存 phase metadata：`run_id`、`lifecycle_key`、`phase_node`、`capability_keys`、`source_turn_id`。
- `RuntimeContextPatch.tool` 承载 tool capability、tool policy 与 MCP server 列表；`companion` 承载 companion 维度；`vfs_overlay` 承载 runtime 追加的 VFS surface；`mount_directives` 承载 workflow/runtime 的 mount 指令。
- replay 先从 construction base capability state 开始，叠加 VFS overlay，再应用 mount directives，随后由 capability projection normalizer 写回 effective VFS、MCP、Skill baseline 与 guidelines。
- repository 继续使用 runtime command `payload_json` 容器；payload 语义是 intent，而不是 full `CapabilityState` projection。

#### 4. Validation & Error Matrix

- pending payload 反序列化失败 -> repository 返回数据错误，当前预研阶段不保留旧 payload 兼容分支。
- final VFS 缺少 default mount 或 root_ref -> construction `BadRequest`。
- connector prompt accepted -> command 标记 `applied`。
- connector setup / prompt 失败 -> command 保持 `requested` 或进入 `failed`，下一轮可按 store 状态恢复。

#### 5. Good / Base / Bad Cases

- Good: pending patch 含 VFS overlay，context query / next-turn launch replay 后 final VFS、runtime surface 与 Skill baseline 都包含 overlay mount。
- Base: pending patch 只改 tool/MCP，construction base VFS 原样进入 final projection。
- Bad: payload 保存闭包后的 Skill 列表，下一轮 replay 时会绕过 effective VFS 的 Skill baseline 派生。

#### 6. Tests Required

- Unit: patch replay 合并 VFS overlay、应用 mount directives，并断言 serialized payload 没有 `state` 字段。
- Repository: requested command supersede、applied、failed 状态仍能保存和读取 patch payload。
- Runtime: pending transition 的 event/context frame 使用 replay + normalizer 后的 final capability projection。

#### 7. Implementation Boundary

```rust
let mut state = apply_runtime_context_patch(&base_capability_state, &command.transition.patch);
normalize_capability_state_dimensions(&mut state, Some(effective_vfs), mcp_servers, &baseline);
```

`SessionLaunchPlanner` 不负责释放 turn claim 或清理 hook runtime。hook runtime
准备失败时，错误返回到 `SessionLaunchExecutor::execute_constructed_launch`，由 executor
统一调用 `TurnSupervisor::clear_turn_and_hook`，确保规划阶段不直接执行 turn 清理副作用。

## Scenario: Session Meta Tab Layout Persistence

### 1. Scope / Trigger

- Trigger: workspace panel 需要把 session tab layout 作为正式 session meta 持久化字段，而不是前端静默兼容路径。

### 2. Signatures

- API read: `GET /sessions/{id}/meta -> SessionMeta`
- API write: `PATCH /sessions/{id}/meta { title?: string, tab_layout?: JsonValue } -> SessionMeta`
- Rust meta: `SessionMeta { tab_layout: Option<serde_json::Value>, ... }`
- DB column: `sessions.tab_layout_json TEXT`

### 3. Contracts

- request `title`：存在时必须 trim 后非空，并写入 `title_source = user`。
- request `tab_layout`：存在时按 JSON 原样保存到 `SessionMeta.tab_layout`。
- response `SessionMeta` 使用既有 `camelCase` serde，因此前端读取字段为 `tabLayout`。
- 前端保存仍按 PATCH request 字段 `tab_layout` 发送，不能同时猜测 `tabLayout`/`tab_layout` 响应别名。

### 4. Validation & Error Matrix

- `{}` -> `400 BadRequest`，必须提供 `title` 或 `tab_layout`。
- `{ "title": "   " }` -> `400 BadRequest`。
- session 不存在 -> `404 NotFound`。
- `tab_layout` 不是前端 `SessionTabLayout` 形状 -> 前端 `loadSessionTabLayout` 返回 `null`，不静默吞掉网络/API 错误。

### 5. Good/Base/Bad Cases

- Good: PATCH `{ "tab_layout": { "tabs": [...], "active_tab_uri": "session://main" } }` 后，GET meta 返回 `tabLayout`。
- Base: 无已保存布局时，`tabLayout` 缺省或为 `null`，前端初始化默认 pinned tabs。
- Bad: 前端 catch 所有错误并假装后端不支持；这会掩盖 schema/API 漏接线。

### 6. Tests Required

- Repository stale-save 测试必须断言 `tab_layout_json` 不因 event projection merge 丢失。
- API/session check 必须覆盖 meta DTO 构造中的 `tab_layout` 字段。
- Frontend check/lint/test 必须覆盖 tab layout service 的类型边界。

### 7. Wrong vs Correct

#### Wrong

```typescript
try {
  await api.patch(`/sessions/${id}/meta`, { tab_layout: layout });
} catch {
  // pretend backend does not support it
}
```

#### Correct

```typescript
await api.patch(`/sessions/${id}/meta`, { tab_layout: layout });
```

调用方可以记录错误或展示失败状态，但 service 层不能把正式后端契约降级成静默兼容路径。

## Ready Gate

云端 `AppState::new_with_plugins` 返回前必须完成 session 主链路依赖绑定：

- runtime tool provider；
- MCP relay provider；
- terminal callback；
- session construction provider；
- context audit bus。

Ready gate 的职责是保证运行期看到完整依赖图。

## Verification

```powershell
rg -n "\.start_prompt\(" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "PreparedSessionInputs|finalize_request|LaunchCommand::.*_prepared|PromptSessionRequest|SessionLaunchIntent|PreparedLaunchPrompt|AugmentedLaunchInput|PromptAugmentInput|SessionConstructionFacts|SessionConstructionSeed" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::construction
```
