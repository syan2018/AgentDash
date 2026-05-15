# Session Startup Pipeline

> **主题**：session 构建与 prompt launch 的唯一数据流。
>
> 当前重构目标由 `.trellis/tasks/05-14-session-launch-refactor-assessment`
> 约束。本 spec 不再把历史 prompt projection 节拍描述为目标态；它只记录
> 当前允许的迁移边界和必须继续删除的中间层。

## 目标主线

生产入口只能进入这条数据流：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> ExecutionContext connector projection
  -> SessionEvent / TerminalEffectOutbox
```

各阶段职责：

| 阶段 | 职责 | 禁止事项 |
|---|---|---|
| `LaunchCommand` | 来源意图：session、source、user input、identity、source hints、follow-up hint | 不携带已组装 VFS / MCP / capability / context bundle / hook trigger / prompt projection |
| `SessionConstructionPlan` | session 构建事实源：owner、workspace、VFS、MCP、capability、context、identity、query/audit projection、trace | 不处理 turn reservation、connector accepted、terminal effect 状态 |
| `LaunchExecution` | 单次 launch 执行计划：payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input | 不重建 owner / context / VFS / MCP / capability |
| `ExecutionContext` | connector SPI 投影 | 不反向成为 application 架构事实源 |

`Turn` 边界保持薄：reservation、active、cancel、hook runtime handle、processor/adapter supervision、terminal release。

## 当前迁移边界

`PreparedLaunchPrompt` 已删除，不能重新引入。

当前仍存在的迁移边界是 `SessionLaunchPlan`：

- 它只能承载跨 crate launch 输入，不是 session 构建事实源；
- 必须携带 owner/source 种子并在进入 `LaunchExecution` 前投影为 `SessionConstructionPlan`；
- 不允许被 `LaunchCommand` 持有；
- 不允许从 `session::mod` 顶层 re-export；
- 不允许被 HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 生产入口直接构造；
- 不允许作为新的长期 service / route / adapter 公开契约；
- 后续必须继续收缩到 `SessionConstructionPlanner / SessionLaunchPlanner / SessionLaunchExecutor` 内部，或删除。

`start_prompt` 是测试专用入口。生产代码必须走 `LaunchCommand`，不得重新添加直接调用 prompt pipeline 的旁路。

## Source Adapter 规则

所有来源只做来源语义转换：

- HTTP prompt：从请求 DTO 与 auth identity 构造 `LaunchCommand::http_prompt_input`。
- Task service：构造 `LaunchCommand::task_service_input`，task phase / override / additional prompt 只能作为 source hint 进入 planner。
- Workflow orchestrator：构造 `LaunchCommand::workflow_orchestrator_input`。
- Routine executor：构造 `LaunchCommand::routine_executor_input`，系统身份必须来自 `AuthIdentity::system_routine(routine.id)`。
- Companion dispatch / parent resume：构造对应 `LaunchCommand`，父 session slice 只能作为 construction planner 输入，不得先拼出 prompt projection。
- Hook auto-resume：必须 strict 复用主 construction/augment 路径；augmenter 缺失时失败，不做裸请求 fallback。
- Local relay：允许携带本机 relay 已解析出的 request MCP / VFS 输入，但仍只能经 `LaunchCommand::local_relay_prompt_input` 进入 hub。

新增入口不得：

- 直接构造 `SessionLaunchPlan`；
- 直接调用 `start_prompt_with_follow_up`；
- 直接修改 prompt projection 字段来表达 owner/context/capability；
- 在 route 层重建 Task / Story / Project 的 context 主线。

## Owner 与 Context 同源

owner 解析使用单一 `SessionOwnerResolver` / `ResolvedSessionOwner`。优先级必须统一为：

```text
Task -> Story -> Project
```

launch、context endpoint、权限展示、audit/inspector 不得各自排序或各自解释 owner。

`SessionConstructionPlan` 是 context endpoint 的最终事实源。API route 只能做：

- auth / permission；
- HTTP DTO 转换；
- 调用 use case / planner；
- 将 construction projection 映射为 response DTO。

route 层不得长期保留 `build_task_session_context`、`build_story_session_context_response`、
`build_project_session_context_response` 这类主线重建分支。

## LaunchExecution 规则

`LaunchExecution` 必须显式承载或引用：

- resolved prompt payload；
- `SessionConstructionPlan`；
- lifecycle plan；
- restore plan；
- hook launch plan；
- follow-up plan；
- runtime command apply plan；
- terminal effect plan；
- connector input projection；
- launch trace。

`prompt_pipeline` 的最终职责应收缩为执行计划：

- claim / activate turn；
- append start/user events；
- call connector；
- connector accepted 后提交 bootstrap / pending applied / title generation；
- spawn stream adapter / turn processor；
- terminal fact 进入 event store，effect 进入 outbox。

它不得继续按 request/meta/profile 临时 fallback 出 VFS、MCP、capability、context 或 owner。

## Terminal Effects

terminal event 必须先持久化。业务副作用必须进入 durable outbox，再由 dispatcher 执行。

effect 失败不能回滚 terminal fact，也不能破坏 active turn cleanup。

当前 effect 类型至少包括：

- `hook_effects`
- `session_terminal_callback`
- `hook_auto_resume`

目标语义：

- pending / running / succeeded / failed / dead-letter 可审计；
- dispatcher 支持进程重启后的 replay；
- handler 必须幂等。

## Pending Runtime Commands

pending runtime context / capability transition 不再存放在 `SessionMeta`。

事实源是 runtime command event/store，projection 只用于查询和 apply-once：

- requested；
- applied；
- failed。

connector.prompt 失败时不得标记 applied；下一轮必须仍可恢复。

## Ready Gate

云端 `AppState::new_with_plugins` 返回前必须完成 session 主链路依赖绑定，并通过 ready gate：

- runtime tool provider；
- MCP relay provider；
- terminal callback；
- prompt/construction augmenter；
- context audit bus。

不得把“稍后注入”的空值暴露为正式运行态。

## 必跑检查

```powershell
rg -n "\.start_prompt\(" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "PreparedSessionInputs|finalize_request|LaunchCommand::.*_prepared|PromptSessionRequest|SessionLaunchIntent" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "PreparedLaunchPrompt" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "SessionLaunchPlan" crates/agentdash-api/src/routes crates/agentdash-local/src crates/agentdash-application/src/task crates/agentdash-application/src/workflow crates/agentdash-application/src/routine
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::construction
```

`PreparedLaunchPrompt` 必须保持归零。`SessionLaunchPlan` 在最终态不能作为公共主链路边界；迁移期若仍有命中，必须仅位于本 spec 的“当前迁移边界”内，并在 task tracker 中列出删除点。
