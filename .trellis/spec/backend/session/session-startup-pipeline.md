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

`SessionConstructionPlan` 至少覆盖：

- `ResolvedSessionOwner`，owner 解析顺序统一为 `Task -> Story -> Project`。
- workspace 与 typed working directory。
- VFS、MCP declaration resolution、capability state。
- `SessionContextBundle` 与 continuation/context frames。
- identity、source contract、query/audit/inspector projections。
- resolution trace，用于审计为什么选择某个 owner/workspace/context。

Context endpoint、权限展示、audit 和 inspector 都投影同一份
`SessionConstructionPlan`。API route 的职责是 auth/permission、DTO 转换、
调用 use case、映射 response DTO。

Companion parent facts 由 construction/assembler 根据 parent session id 解析；
API/bootstrap 只传 parent 引用与 dispatch policy。

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
capability state 和 context frame 都从 `SessionConstructionPlan` 与
`LaunchExecution` 投影生成。`prompt_pipeline` 的职责是执行该计划，而不是重新解析
owner、context、VFS、MCP 或 capability。

## Terminal Effects

Terminal fact 先进入 event store，业务副作用进入 durable outbox。

当前 effect 类型：

- `hook_effects`
- `session_terminal_callback`
- `hook_auto_resume`

Outbox 状态为 `pending / running / succeeded / failed / dead-letter`。dispatcher
支持进程重启后的 replay，handler 以 idempotency key 保证幂等。

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

`SessionLaunchPlanner` 不负责释放 turn claim 或清理 hook runtime。hook runtime
准备失败时，错误返回到 `SessionLaunchExecutor::execute_constructed_launch`，由 executor
统一调用 `TurnSupervisor::clear_turn_and_hook`，确保规划阶段不直接执行 turn 清理副作用。

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
