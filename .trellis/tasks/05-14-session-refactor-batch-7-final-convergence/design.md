# Design：Batch 7 Final Convergence

## Boundary

本 batch 不再处理已经完成的 working_dir、旧 pending 字段、store adapter 等基础迁移叙述。当前唯一关注点是最终架构是否真正收口。

目标边界：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> ExecutionContext
```

## Target Shape

### LaunchCommand

只表达来源意图：

- session/source/strictness；
- user input；
- identity；
- task / companion / local relay / continuation source hints；
- follow-up hint。

不持有 VFS、MCP、capability、context bundle、hook trigger、post-turn handler。

### SessionConstructionPlan

承载 session 构建事实：

- owner / source contract；
- workspace / typed working dir / VFS；
- executor profile；
- MCP / capability / session capabilities；
- context bundle / context frames / context projection；
- identity；
- audit / inspector / context endpoint projection；
- trace。

### LaunchExecution

承载本次启动执行计划：

- resolved prompt payload；
- construction；
- lifecycle；
- restore；
- hook；
- follow-up；
- runtime command；
- terminal effect；
- connector input；
- trace。

### SessionHub

不作为目标能力边界。若类型仍存在，不能承载业务判断，也不能作为最终完成遮羞布。

## Forbidden Outcomes

- `PromptAugmentInput` 改名后继续作为主链路。
- API bootstrap 返回增强 payload。
- planner 继续从 request payload 读 VFS/MCP/capability/context/hook。
- context endpoint 与 launch 各自构造 session surface。
- `SessionHub` 继续作为服务定位器并承载业务判断。

## Verification Matrix

最终收口后执行：

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
git diff --check
```
