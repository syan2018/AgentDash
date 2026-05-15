# Final Convergence Execution Tracker

## 状态判定

当前重构未完成。此前 Batch 0-7 已完成的是迁移基础与局部收口，不是原始 PRD 的目标态。

本 tracker 是后续实现与 check 的唯一追踪清单。任何阶段停下来后，必须重新对照：

- `.trellis/tasks/05-14-session-launch-refactor-assessment/prd.md`
- `.trellis/tasks/05-14-session-launch-refactor-assessment/design.md`
- `.trellis/tasks/05-14-session-launch-refactor-assessment/docs/closure-checklist.md`
- 当前代码搜索结果

完成标准不是“旧测试通过”，而是生产主链路只剩：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> ExecutionContext connector projection
  -> SessionEvent / TerminalEffectOutbox
```

## 不允许继续保留的半收敛结构

以下对象不能作为最终架构边界，也不能在 check 时被解释为“已完成的统一层”：

- `PreparedSessionInputs`
- `finalize_request`
- `PreparedLaunchPrompt`
- `LaunchCommand::*_prepared`
- `LaunchCommand` 承载已组装 prompt / prompt material 的分支
- route-local `augment_prompt_request_for_owner` 业务分支
- route-local task/story/project context reconstruction
- `SessionHub` 业务 facade
- `start_prompt_with_follow_up` 巨型 planner
- 内存即时 terminal callback 作为唯一 effect 执行路径

迁移期间如果某个对象短暂存在，必须满足：

- 只在一个内部模块内被调用；
- 不从 `session::mod` re-export 成公共主链路类型；
- 不被任何 HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 入口直接构造或传递；
- 对应阶段必须有删除检查项和 `rg` 验证。

## 当前差池清单

### A. LaunchCommand 仍不是纯入口意图

当前状态：

- `LaunchCommand` 已不再持有 `PreparedLaunchPrompt`。
- `LaunchCommand::*_prepared` 已删除。
- `PreparedSessionInputs` 已删除。
- 生产入口已不再直接调用 `.start_prompt(...)`；当前 `rg "\.start_prompt\("` 只剩 hub 自测。
- `start_prompt` 已收紧为 `#[cfg(test)]`，生产代码不能再绕过 `LaunchCommand` 调用 prompt pipeline。
- `start_prompt_with_follow_up` 已收紧为 `pub(crate)`；crate 外部不能再绕过 `LaunchCommand` 直接进入 prompt pipeline。
- `PreparedLaunchPrompt` 已从 `session::` 顶层 re-export 移除；剩余命中集中在 `session::types`、assembler、API augmenter 与 hub pipeline。

仍不满足目标态的原因：

- `LaunchCommand` 仍通过 `PromptAugmentInput` 间接携带 task / companion 等 composition hint。
- `PromptAugmentInput` 到 `PreparedLaunchPrompt` 的输出仍是旧 prompt projection，而不是 `SessionConstructionPlan`。
- Local relay relaxed fallback 仍可把 `PromptAugmentInput` 投影成裸 prompt；这是本机 relay 的临时运行路径，不能扩大到 HTTP / Task / Workflow / Routine / Companion / Hook。

目标：

- `LaunchCommand` 只表达 source、session、user input、identity、source hints、overrides、follow-up hint。
- 禁止携带 VFS、MCP、capability、context bundle、hook trigger、post-turn handler 这类 construction / launch 产物。

退出检查：

```powershell
rg -n "LaunchCommand::.*_prepared|PreparedSessionInputs" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

生产主链路零命中。

### B. PreparedSessionInputs / finalize_request 仍是旧装配中心

当前状态：

- `PreparedSessionInputs` 已删除。
- `finalize_request` 已删除。
- `SessionAssemblyBuilder::build()` 不再产出平坦中间 DTO，只结束 builder 链。
- assembler 仍会投影为 `PreparedLaunchPrompt`，因此还不是最终 construction planner。

目标：

- 将 compose 逻辑迁入 `SessionConstructionPlanner`。
- `SessionAssemblyBuilder` 删除或退化为 construction planner 的私有 helper，且不产出旧半成品 request。

退出检查：

```powershell
rg -n "PreparedSessionInputs|finalize_request" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

生产代码零命中。

### C. SessionConstructionPlan 还不是真正事实源

现状问题：

- 当前 `SessionConstructionPlan` 字段仍不完整。
- context endpoint route 已只调用 `bootstrap/session_context_query.rs` 并投影 `SessionConstructionPlan`。
- `bootstrap/session_context_query.rs` 仍按 Task / Story / Project 分支重建 VFS / context / capability，且复用 route context builder；还没有与 launch construction 合流。
- launch augment 与 context query 不是同一个 construction 结果的投影。
- `LaunchExecution.construction` 当前大多仍为 `None`，说明执行链路没有真正消费 construction fact。
- `augment_prompt_request_for_owner` 已从 API route 移到 `bootstrap/session_launch_augmenter.rs`，route 文件不再承载 prompt launch composition 主分支。
- `bootstrap/session_launch_augmenter.rs` 仍返回 `PreparedLaunchPrompt`，还不是最终 `SessionConstructionPlanner`。

目标：

`SessionConstructionPlan` 必须承载：

- owner / binding；
- source contract；
- workspace；
- typed working dir；
- executor profile；
- VFS；
- MCP；
- capability；
- context bundle / context frames / context endpoint projection；
- identity；
- query / audit / inspector projections；
- resolution trace。

退出检查：

```powershell
rg -n "build_task_session_context|build_story_session_context_response|build_project_session_context_response|finalize_augmented_request" crates/agentdash-api/src/routes
```

session context 主线不再在 route 层调用这些 builder；route 只做 auth、DTO、use case。

### D. LaunchExecution 还不是完整 per-launch plan

现状问题：

- `prompt_pipeline` 仍临时解析 payload、VFS fallback、executor fallback、MCP fallback、capability fallback、hook reload、restore、follow-up、pending command、terminal effect。
- `LaunchExecution` 仍偏 summary/context projection。
- `start_prompt_with_follow_up` 仍以 `PreparedLaunchPrompt` 作为输入；这不是最终执行边界。

目标：

`LaunchExecution` 必须显式包含：

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

退出检查：

```powershell
rg -n "CachedSessionProfile|HubDefault|SessionMeta\\)|req\\.mcp_servers|req\\.capability_state|req\\.context_bundle|req\\.vfs" crates/agentdash-application/src/session/prompt_pipeline.rs
```

执行函数不再读取 request/meta/profile 做策略 fallback；策略来源必须是 `LaunchExecution`。

### E. connector accepted 前仍可能提交副作用

现状问题：

- 需要保证 bootstrap state、pending command applied、title generation 等都不早于 `connector.prompt` accepted。

目标：

- `connector.prompt` 返回 `Ok(stream)` 后才能提交 bootstrap / pending applied。
- `connector.prompt` 失败只能留下 failed terminal event，不得推进成功投影。

退出检查：

- `connector_setup_failure_does_not_commit_bootstrap_or_pending_commands` 测试通过。
- pending command failure 后仍为 pending 或 failed，可恢复，不丢失。

### F. SessionHub 仍是业务 facade

现状问题：

- `SessionHub` 仍持有 connector、hook provider、runtime registry、turn supervisor、stores、persistence、prompt augmenter、terminal callback 等。
- 多个业务行为仍实现为 `impl SessionHub`。
- `launch_command` 仍在 hub facade 内完成 augment 与旧 prompt pipeline 分发。

目标：

拆成：

- `SessionConstructionPlanner`
- `SessionLaunchPlanner`
- `SessionLaunchExecutor`
- `SessionRuntimeRegistry`
- `TurnSupervisor`
- `SessionEventWriter`
- `TerminalEffectOutbox`
- `PendingRuntimeCommandStore / Projector`
- source adapters

`SessionHub` 删除；若短期存在，只能是无业务逻辑 wrapper。

退出检查：

```powershell
rg -n "impl SessionHub|pub struct SessionHub|SessionHub::launch|start_prompt_with_follow_up" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

无业务主线依赖。

### G. Terminal effects 还不是完整 durable outbox

现状问题：

- outbox record 已存在。
- dispatcher 已支持按 `pending/running/failed` 状态从 durable outbox 重放，并在 `AppState` ready 后触发一次启动恢复。
- effect 失败会有限重试，达到上限后进入 `dead_letter`。
- `session_terminal_callback` replay 依赖当前进程已注入 callback；未注入时会显式失败/死信。
- `hook_effects` replay 仍不能真正执行，因为它依赖原 turn 的 post-turn handler，当前没有 durable handler registry。

目标：

- terminal event 先落库；
- effects 进入 durable outbox；
- worker 可从 pending/running stale 状态恢复；
- effect handler 幂等；
- failed/retry/dead-letter 可审计。
- `hook_effects` 必须改为可由 durable handler registry 或明确的 typed effect handler 重放，不能继续依赖原 turn 的内存 handler。

退出检查：

- effect enqueue 成功但进程中断后可重放；
- effect failure 不破坏 terminal event；
- `SessionTurnProcessor` 不直接执行业务副作用。
- `TerminalEffectType::HookEffects` 不再 replay 为 unavailable。

### H. Persistence store 边界仍未真正拆干净

现状问题：

- `SessionMetaStore / SessionEventStore / SessionTerminalEffectStore / SessionRuntimeCommandStore` 只是 adapter 投影。
- 大 `SessionPersistence` 仍是实际底层主接口。

目标：

- meta / event / projection / outbox / runtime-command projection store 有明确长期边界。
- 普通 meta save 不回退 projection 字段。

退出检查：

- 生产 service 不依赖大 `SessionPersistence` 做跨职责调用。
- repository 主线按 store 边界组织。

## 快刀执行策略

这不是兼容性重构。项目未上线，不保留旧 API / 旧 DB 字段 / 旧内部壳作为兼容层。

优先选择：

1. 新建目标旁路 `SessionConstructionPlanner + SessionLaunchPlanner + SessionLaunchExecutor`。
2. 将所有入口一次性切到新旁路。
3. 删除旧 `PreparedSessionInputs / finalize_request / *_prepared / PreparedLaunchPrompt` 主线。
4. 修编译与测试。
5. 再拆 `SessionHub` 剩余 facade。

不选择：

- 新旧双主线长期并行；
- 用测试同步两套 context construction；
- 给旧 request 壳换名后继续传递；
- “先保留 wrapper，以后再说”的收尾方式。

## 分阶段执行清单

### Phase 0：修正当前工作区与语义止血

- [x] 修复当前 `cargo check -p agentdash-application` 编译错误。
- [x] connector accepted 后再 commit bootstrap state。
- [x] connector accepted 后再 mark pending command applied。
- [x] connector failure 测试覆盖 bootstrap/pending 不被推进。

### Phase 1：建立目标旁路

- [ ] 新增 `SessionConstructionPlanner`。
- [ ] 新增 `SessionLaunchPlanner`。
- [ ] 新增 `SessionLaunchExecutor`。
- [ ] `LaunchCommand` 改为纯入口意图。
  - [x] 删除 `LaunchCommand::*_prepared`，避免 `LaunchCommand` 继续接收 `PreparedSessionInputs`。
  - [x] 将 HTTP Story/Project、Task service、Workflow orchestrator、Routine executor、Companion dispatch 调用点从直接传递 `PreparedSessionInputs` 推到 prompt 边界。
  - [x] 停止从 `session::mod` re-export `PreparedSessionInputs`。
  - [x] 停止从 `session::mod` re-export `finalize_request`。
  - [x] 删除 `PreparedSessionInputs` 与 `finalize_request` 代码实体。
  - [x] 删除中途引入的 `SessionAssemblyDraft`，避免把旧 DTO 改名保留。
- [x] 删除 `LaunchCommand` 内部的 `PreparedLaunchPrompt` 字段。
- [x] 删除 `LaunchCommand` 内部承载已组装 prompt material 的分支；未迁移入口显式停在 `start_prompt` 调用点，不再伪装成统一 command。
- [x] 将 `start_prompt` 收紧为测试专用，防止生产代码新增旧 prompt 旁路。
- [x] 将 `start_prompt_with_follow_up` 从 public API 收紧为 crate 内部执行函数。
- [x] 停止从 `session::mod` re-export `PreparedLaunchPrompt`，避免新增入口从主 namespace 继续依赖旧投影。
- [ ] 将 VFS/MCP/capability/context/hook/post-turn 从 command 主体移入 construction/launch planner。
- [ ] `LaunchExecution` 持有完整 construction / launch plan。

### Phase 2：入口一次性切换

- [ ] HTTP prompt 只构造 `LaunchCommand`。
- [x] Task service 只构造 `LaunchCommand`。
- [x] Workflow orchestrator 只构造 `LaunchCommand`。
- [x] Routine executor 只构造 `LaunchCommand`。
- [x] Companion dispatch / parent resume 只构造 `LaunchCommand`。
- [x] Hook auto-resume 只构造 `LaunchCommand`。
- [x] Local relay prompt 只构造 `LaunchCommand`。

当前显性入口迁移检查：

```powershell
rg -n "\.start_prompt\(" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

生产代码零命中；仅允许 hub 自测命中。下一阶段不能再新增任何生产 `start_prompt` 调用点。

### Phase 3：删除旧主线

- [x] 删除 `PreparedSessionInputs`。
- [x] 删除 `finalize_request`。
- [x] 删除 `LaunchCommand::*_prepared`。
- [x] 删除 `LaunchCommand` 已组装 prompt 分支。
- [x] 从 route 文件删除 `augment_prompt_request_for_owner` 业务分支；当前集中到 `bootstrap/session_launch_augmenter.rs`。
- [ ] 将 `bootstrap/session_launch_augmenter.rs` 输出从 `PreparedLaunchPrompt` 改为 construction/launch plan。
- [ ] 删除 `PreparedLaunchPrompt` 或退化成 launch executor 私有 connector projection。
- [ ] 删除 `start_prompt_with_follow_up` planner 职责。

### Phase 4：Context 同源

- [ ] context endpoint 只投影 `SessionConstructionPlan`。
- [x] `GET /sessions/{id}/context` route 只调用 context query use case 并投影 `SessionConstructionPlan`。
- [ ] audit / inspector 只投影 `SessionConstructionPlan`。
- [ ] route 层不再重建 task/story/project VFS/capability/context。
- [x] `acp_sessions.rs` route 层不再直接重建 task/story/project VFS/capability/context。
- [ ] `bootstrap/session_context_query.rs` 与 launch construction planner 合流，删除独立重建主线。
- [ ] launch 与 context endpoint 一致性测试覆盖 Task / Story / Project。

### Phase 5：Effects / Pending / Persistence 收尾

- [ ] terminal effect worker 支持 durable replay / retry / dead-letter。
  - [x] `pending/running/failed` outbox 启动后可重放。
  - [x] failure 达到上限后进入 `dead_letter`。
  - [x] outbox 恢复接在 `AppState` ready gate 之后。
  - [ ] `hook_effects` 具备 durable handler registry / typed handler，不再依赖原 turn 内存 handler。
- [ ] pending command apply-once 与 failure recovery 测试覆盖。
- [ ] store 边界从 adapter split 变为真实长期接口。
- [ ] `SessionHub` 删除或只剩无业务 wrapper。

## 每次停顿前必须执行的 check

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
rg -n "PreparedSessionInputs|finalize_request|LaunchCommand::.*_prepared|PromptSessionRequest|SessionLaunchIntent" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "finalize_augmented_request|build_task_owner_prompt_request|build_story_owner_prompt_request|build_project_owner_prompt_request" crates/agentdash-api/src/routes/acp_sessions.rs
git diff --check
```

允许暂时失败的 check 必须在当前停顿说明里写明原因和下一步删除点；不能标记任务完成。

## 完成定义

只有全部满足以下条件，才能把父任务重新标记为 done：

- [ ] 生产主链路没有 `PreparedSessionInputs`。
- [ ] 生产主链路没有 `finalize_request`.
- [ ] `LaunchCommand` 不携带 construction / execution 产物。
- [ ] `SessionConstructionPlan` 是 launch/query/audit/inspector 的唯一事实源。
- [ ] `LaunchExecution` 是唯一 launch 策略计划。
- [ ] `prompt_pipeline` 不再承担 planner 职责。
- [ ] `SessionHub` 不再是业务能力入口。
- [ ] terminal effects 具备 durable replay / retry / dead-letter。
- [ ] pending runtime command connector failure 不丢失、apply once 可审计。
- [ ] 最终验证矩阵通过。
