# Implementation Plan: Batch 7 Final Convergence

## Resume Protocol

每次上下文压缩、暂停或重新领取任务时，只读三份权威文件：

1. `.trellis/tasks/05-14-session-launch-refactor-assessment/prd.md`
2. `.trellis/tasks/05-14-session-launch-refactor-assessment/design.md`
3. 本文件

然后执行当前第一个未完成 commit slice。不要重新解释目标，不要追加旁路计划，不要把已经删除的旧结构作为迁移基础复活。

终态只认这一条生产主链路：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext projection
```

`LaunchCommand` 只表达 source intent。`SessionConstructionPlan` 是 owner / workspace / VFS / MCP / capability / context / identity / projection / trace 的事实源。`LaunchExecution` 是本次 prompt、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input 的执行计划。`ExecutionContext` 只在 connector 边界投影。

## Commit Slices

本 batch 固定为 6 次提交完成。除非编译错误迫使同一 slice 内补修，不再拆更小提交。

重新领取任务时的执行规则：

- 从本节找到第一个状态不是“已完成”的 commit slice。
- 只执行该 slice 的代码和文档，不创建 child task。
- 该 slice 验证通过后立即提交，再进入下一个 slice。
- 不新增过渡 payload、wrapper 或双主线；发现前一 slice 遗留错误时，在当前 slice 内直接删掉。

### Commit 1: 校准 source intent 与 construction provider 边界

状态：已完成。

提交信息：

```text
refactor(session): 校准 launch source 与 construction provider 边界
```

本次提交只做边界校准，不宣称终态完成：

- `PromptRequestAugmenter` / prompt augmenter 命名彻底替换为 `SessionConstructionProvider`。
- `SessionConstructionSeed` 类型名彻底删除；当时仍存在的 provider handoff 不得从 `session::mod` 顶层导出。
- task / companion source payload 命名脱离 `PromptAugment*`。
- API bootstrap 文件名脱离 `prompt_augmenter` / `session_launch_augmenter`。
- `SessionLaunchPlannerInput` 接收 `LaunchCommand` 原件；source contract、identity、follow-up、local relay workspace root、local relay MCP declarations 只能由 planner 从 command 投影，`prompt_pipeline` 不再重组这些 source facts。
- 文档只记录真实剩余阻塞，不把任何 provider handoff 写成终态边界。

退出检查：

```powershell
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
rg -n "SessionConstructionSeed|PromptRequestAugmenter|SharedPromptRequestAugmenter|PromptAugment|prompt_augmenter|session_launch_augmenter|decode_augmented|construction_seed|requires_augment|execute_launch_seed|SpyAugmenter|session::provider" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
git diff --check
```

完成定义：

- 旧 augmenter/seed 命名在生产代码归零。
- 当前未提交的 provider rename 和 planner source intent 收口可编译。
- tracker 不再把 `SessionConstructionSeed` 写成当前代码事实。

### Commit 2: 删除 `SessionConstructionFacts` production handoff

状态：已完成。

提交信息：

```text
refactor(session): 删除 construction facts 生产传递层
```

一次性完成，不再继续换名：

- `SessionConstructionProvider::build_construction` 返回 `SessionConstructionPlan` 所需的 construction 结果，不再返回 `(UserPromptInput, SessionConstructionFacts)`。
- assembler 不再把 VFS / MCP / capability / context / effect binding 写入 facts；这些字段直接进入 `SessionConstructionPlan` 或其 planner input。
- prompt payload 不再通过被 provider 改写的 `UserPromptInput` 传递。provider 产出的 context prompt blocks / executor profile 必须进入 construction projection，launch planner 再从 `LaunchCommand + SessionConstructionPlan` 生成 resolved prompt payload。
- `SessionLaunchPlannerInput` 删除 `construction_facts`。
- `SessionConstructionFacts` 类型删除。

实际完成事实：

- `SessionConstructionProvider::build_construction` 直接返回 `SessionConstructionPlan`。
- `SessionConstructionPlan.prompt` 承载 prompt blocks / env projection，executor profile 进入 `execution_profile`。
- API bootstrap、assembler、pipeline、planner 不再传递 facts tuple。
- companion dispatch 使用本次 child session construction plan，parent session 只作为 source policy 解析 parent facts。

退出检查：

```powershell
rg -n "SessionConstructionFacts" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "Result<\\(UserPromptInput, SessionConstruction" crates/agentdash-application/src crates/agentdash-api/src
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
```

完成定义：

- 生产主链路已经是 `LaunchCommand + SessionConstructionPlan + runtime facts -> LaunchExecution`。
- provider handoff 不存在。
- prompt/context/executor mutation 不再藏在 `UserPromptInput` 回传中。

### Commit 3: 让 context/query/audit/inspector 与 launch 同源

提交信息：

```text
refactor(session): 统一 context 查询与 construction plan 投影
```

本次提交关闭“两路但测试同步”的风险：

- context endpoint 只调用 construction query/use case，投影 `SessionConstructionPlan`。
- route/bootstrap 删除 task/story/project context response 主线重建分支。
- audit / inspector 所需字段进入 `ConstructionProjections`。
- owner 排序只来自 `SessionOwnerResolver`，launch、context query、权限展示不再各自解释 owner。

退出检查：

```powershell
rg -n "build_task_session_context|build_story_session_context_response|build_project_session_context_response|finalize_augmented_request" crates/agentdash-api/src/routes crates/agentdash-api/src/bootstrap
cargo test -p agentdash-application session::construction
cargo check -p agentdash-api
```

完成定义：

- launch/query/audit/inspector 不再各自组装 VFS / MCP / capability / context。
- context endpoint 和 launch 共享同一 construction plan projection。

### Commit 4: 收缩 `prompt_pipeline` 为执行器

提交信息：

```text
refactor(session): 将 prompt pipeline 收缩为 launch execution 执行器
```

本次提交只处理 launch 执行边界：

- `SessionLaunchPlanner` 输出完整 `LaunchExecution`。
- `prompt_pipeline` 只做 claim / activate、event append、connector.prompt、accepted 后 meta/pending/title 提交、processor supervision。
- connector.prompt 失败不得提交 bootstrap completed、pending applied、title generation 等成功副作用。
- hook session、runtime delegate、restore state、terminal effect handler 的解析归入 launch/effects 边界，不在 pipeline 里临时 fallback。

退出检查：

```powershell
rg -n "req\\.vfs|req\\.mcp_servers|req\\.capability_state|req\\.context_bundle|req\\.hook_snapshot_reload|req\\.post_turn_handler" crates/agentdash-application/src/session/prompt_pipeline.rs crates/agentdash-application/src/session/launch_planner.rs
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::hub
```

完成定义：

- pipeline 不再 planning。
- connector failure 不再写成功副作用。

### Commit 5: 拆掉有职责 `SessionHub`

提交信息：

```text
refactor(session): 拆分 session hub 业务职责
```

本次提交拆业务入口，不保留有职责 facade：

- 拆出 core / ownership / construction / launch / runtime / eventing / hooks / effects / pending / adapters 能力服务。
- `SessionHub` 若仍存在，只能作为短期依赖装配壳或测试 handle，不承载业务判断。
- 新调用点依赖具体能力服务，不再通过 hub 读写跨职责状态。

退出检查：

```powershell
rg -n "impl SessionHub|pub struct SessionHub" crates/agentdash-application/src/session
cargo check -p agentdash-application
cargo test -p agentdash-application session::hub
```

完成定义：

- 每个 `impl SessionHub` 命中要么删除，要么在 tracker 标记为无业务判断装配壳。
- 不能用 facade 包一层继续过关。

### Commit 6: Effects / pending / persistence 最终验证

提交信息：

```text
refactor(session): 完成 effects pending persistence 收口验证
```

本次提交只收尾运行语义和文档：

- terminal event 先落库，effect 进入 durable outbox；handler 有 durable identity 或 typed handler。
- pending runtime command 覆盖 requested / applied / failed，具备 apply-once 和失败恢复测试。
- 新增业务逻辑依赖 meta / event / outbox / runtime-command store 边界，不再扩张大 `SessionPersistence`。
- PostgreSQL / SQLite migration 通过。
- 父任务 tracker、closure checklist、session startup spec 与代码事实一致。

最终验证：

```powershell
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-infrastructure
cargo check -p agentdash-local
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::construction
cargo test -p agentdash-application session::hub
cargo test -p agentdash-application session::terminal_effects
cargo test -p agentdash-application session::runtime_commands
cargo test -p agentdash-application session::memory_persistence
cargo test -p agentdash-application session::path_policy
cargo test -p agentdash-infrastructure terminal_effect_outbox_persists_status_transitions
rg -n "PreparedSessionInputs|finalize_request|PreparedLaunchPrompt|SessionLaunchPlan|AugmentedLaunchInput|PromptSessionRequest|SessionLaunchIntent|LaunchCommand::.*_prepared|PromptAugmentInput|SessionConstructionFacts|SessionConstructionSeed" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "pending_capability_state_transitions_json" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-infrastructure/src
git diff --check
```

完成定义：

- 父任务 closure checklist 全部通过。
- final convergence tracker 中没有“过渡边界仍在生产主线”的未完成项。
- 可以标记父任务完成。

## Rules During Execution

- 每个 slice 完成后立即提交，不跨 slice 混合提交。
- 发现前一 slice 方向错误时，先修正当前 slice 文档和代码事实，再继续；不得在后续 slice 偷偷绕开。
- 不创建 subagent。
- 不做兼容旧内部 API 的双主线。
- 不新增只转发旧 payload 的 wrapper。
- 不把 resolved VFS / MCP / capability / context / hook / effect / working_dir 塞进 `LaunchCommand`。
