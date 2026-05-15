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
| `LaunchCommand` | 来源意图：session、source、actor、prompt、executor override、source policy、follow-up hint | 不携带 working_dir、已组装 VFS / MCP / capability / context bundle / hook trigger / effect handler / prompt projection |
| `SessionConstructionPlan` | session 构建事实源：owner、workspace、working dir、VFS、MCP、capability、context、identity、query/audit projection、trace | 不处理 turn reservation、connector accepted、terminal effect 状态 |
| `LaunchExecution` | 单次 launch 执行计划：payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input | 不重建 owner / context / VFS / MCP / capability |
| `ExecutionContext` | connector SPI 投影 | 不反向成为 application 架构事实源 |

`Turn` 边界保持薄：reservation、active、cancel、hook runtime handle、processor/adapter supervision、terminal release。

## 当前迁移边界

`PreparedLaunchPrompt` 已删除，不能重新引入。

`AugmentedLaunchInput` 已删除，不能重新引入。

`PromptAugmentInput` 已删除，不能重新引入。

`SessionLaunchRequest` 已删除，不能重新引入。

当前仍存在的迁移边界是 `SessionConstructionSeed`：

- 它只用于承接 API/bootstrap 到 construction planner 的暂存；
- 它不是最终架构边界，不是 session 构建事实源，也不是 launch plan；
- 它不得从 `session::mod` 顶层 re-export；跨模块引用必须显式写作 construction 模块引用，避免被当成正式 session API；
- 不允许被 HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 生产入口直接构造，入口必须构造 `LaunchCommand`；
- 不允许继续扩张成新的长期 service / route / adapter 公开契约；
- `working_dir_input` 已归零，launch summary/input 不得重新携带 working dir hint；当前只允许 construction seed 暂存 `working_dir_hint`，并最终迁入 construction provider / `SessionConstructionPlan.workspace`。
- 后续必须把 working dir hint / VFS / MCP / capability / context / identity 字段直接拆入 `SessionConstructionPlanner` / `SessionConstructionPlan`。hook reload 已由 launch lifecycle 推导；task effect 已改为 durable binding，后续继续把 binding 生成迁入 construction provider，然后删除这个过渡 seed。

`start_prompt` 是测试专用入口。生产代码必须走 `LaunchCommand`，不得重新添加直接调用 prompt pipeline 的旁路。

## Source Adapter 规则

所有来源只做来源语义转换：

- HTTP prompt：从请求 DTO 与 auth identity 构造 `LaunchCommand::http_prompt_input`；HTTP 不提供 resolved working dir / VFS / MCP / context。
- Task service：构造 `LaunchCommand::task_service_input`，task phase / override / additional prompt 只能作为 source hint 进入 planner；task effect binding 不得作为 trait object 穿过 command，必须以 durable binding 描述进入 construction/effects。
- Workflow orchestrator：构造 `LaunchCommand::workflow_orchestrator_input`。
- Routine executor：构造 `LaunchCommand::routine_executor_input`，系统身份必须来自 `AuthIdentity::system_routine(routine.id)`。
- Companion dispatch / parent resume：构造对应 `LaunchCommand`，只携带 parent session / dispatch / slice / target binding 等策略 payload；父 session VFS / MCP / context snapshot 必须由 construction 从 parent facts 解析，不得先拼出 prompt projection。
- Hook auto-resume：必须 strict 复用主 construction/augment 路径；augmenter 缺失时失败，不做裸请求 fallback。
- Local relay：允许携带本机 relay 请求中的 workspace root 与 MCP declaration；VFS、resolved MCP、capability、working dir 与 connector input 仍必须由 construction/launch 投影生成，不能作为已组装事实塞进 `LaunchCommand`。

新增入口不得：

- 直接构造 `PromptAugmentInput` 或任何已增强 prompt payload；`PromptAugmentInput` 已删除，命中即为回归；
- 直接调用 `start_prompt_with_follow_up`；
- 直接修改 prompt projection 字段来表达 owner/context/capability；
- 把 `working_dir` 当作用户 prompt input 继续向 launch 传递；
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

当前 `SessionConstructionPlan.context` 已保留完整 `SessionContextBundle`，不能再退回只存
`bundle_id` / fragment count 的摘要形态。context frame、audit、inspector projection
仍需继续落入 construction。

Companion parent session facts 的解析归 application construction/assembler 侧。API/bootstrap
不得读取 parent capability state 后自行拆出 parent VFS / MCP / context snapshot；它只能传入
parent session id 与 dispatch/slice/source policy。

Task terminal hook effect binding 的 durable handler 描述归 application construction/assembler
侧生成。API/bootstrap 不得创建内存 post-turn handler，也不得直接组装 task effect binding payload。

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

`LaunchExecution` 的 `construction` 不允许是可选字段。无法产出 `SessionConstructionPlan`
时应在 planner 阶段失败，不得把缺失 construction 的 launch 继续交给 connector。
resolved prompt payload 必须由 `LaunchExecution` 承载，不能作为 parallel planner result
绕过 launch execution 交给 pipeline。
pending runtime commands / pending capability transitions 必须由 `LaunchExecution.runtime_commands`
承载，pipeline 只能执行该 plan，不得从 planner result 旁路读取待应用命令。
terminal post-turn handler 必须由 `LaunchExecution.terminal_effects` 承载，follow-up id 必须由
`LaunchExecution` 的 follow-up plan / summary 投影，不能继续作为 parallel planner result。
hook session、effective capability state、base capability state 也必须由 `LaunchExecution`
的 context / runtime command plan 投影，不能继续挂在 parallel planner result。
working dir hint 只能属于 construction workspace plan；`LaunchExecution` / launch summary
只保留解析后的 `working_directory`，不得重新携带 `working_dir_input` 或等价字段。
connector input 的 working directory / executor config / MCP / VFS / identity 必须由
`SessionConstructionPlan` 投影，不得作为 `LaunchExecutionInput` 的并行事实再次传入。
`SessionLaunchPlanner::plan` 应直接返回 `LaunchExecution`，不得重新引入只用于并行携带
launch 字段的 planner result 壳。

`SessionLaunchPlanner` 负责把临时 launch 输入解析为 `LaunchExecution`。当前它仍借用
`SessionHub` 依赖，后续需要继续与 `SessionConstructionPlanner` 合流，不能成为新的
长期 facade。

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
rg -n "AugmentedLaunchInput|into_augmented_launch_input" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "PromptAugmentInput" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::construction
```

`PreparedLaunchPrompt`、`AugmentedLaunchInput`、`PromptAugmentInput`、`SessionLaunchRequest`
必须保持归零。`SessionConstructionSeed` 若仍存在，必须只作为
删除中的过渡 seed，并在 task tracker 中列出下一步拆入 construction / launch / effects
的删除点。
