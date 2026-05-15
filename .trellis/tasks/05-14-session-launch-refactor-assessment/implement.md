# Implementation Plan

## Current Conclusion

父任务已经进入最终收口阶段。此前 batch 只完成迁移基础，不能把旧 payload 改名、移动代码位置、抽出 wrapper 解释为完成。

最终目标只允许：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext projection
```

权威执行 tracker 是 `docs/final-convergence-execution-tracker.md`。当前 Batch 7 负责一次性收口，不再继续创建碎片化 child task。具体执行以 `../05-14-session-refactor-batch-7-final-convergence/implement.md` 的 6 个 commit slice 为唯一计划。

## Execution Order

### 1. Correct Entry Intent Boundary

- [x] `LaunchCommand` 不再持有 `PromptAugmentInput`。
- [x] `SessionConstructionProvider` 不再接收 `PromptAugmentInput` 作为输入。
- [x] `SessionLaunchPlannerInput` 不再包含 `request: PromptAugmentInput`。
- [x] `PromptAugmentInput` 不再从 `session::mod` re-export。
- [x] local relay 不再把已组装 `Vfs` 塞进 `LaunchCommand` 或 seed；只传 workspace root source fact，由 planner/construction 解析。
- [x] `UserPromptInput.working_dir` 移出 prompt input。
- [x] `LaunchCommand` 只保留 source、actor、target ids、prompt、executor override、follow-up hint、特殊来源策略 payload。
- [x] task `post_turn_handler` trait object 迁出 command；API bootstrap 不再创建内存 handler，只写入 durable `TerminalHookEffectBinding`，后续继续把 binding 生成迁入 construction provider。
- [x] companion parent VFS/MCP/context snapshot 迁出 command；当前 parent capability 临时投影仍在 bootstrap，后续由 construction 从 parent session facts 解析。
- [x] local relay MCP 只作为 request declaration 保留，不作为 resolved MCP surface 使用。

### 2. Complete `SessionConstructionPlan`

- [x] `ContextPlan` 承载完整 `SessionContextBundle` 与 continuation context frame。
- [x] construction provider 直接返回 `SessionConstructionPlan`，不再返回 `UserPromptInput + SessionConstructionFacts`。
- [x] assembler 将 VFS、MCP、capability、context、executor profile、prompt projection、task effect binding 写入 `SessionConstructionPlan`。
- [ ] construction 持有完整 working dir plan、MCP declaration resolution、identity、workspace、owner、source trace。
- [ ] construction 持有完整 companion slice / context bundle / audit projection / inspector projection。
- [x] local relay workspace root 作为 source fact 进入 construction 解析，并记录 VFS 来源。
- [x] construction 持有 task effect durable binding，并通过 effects registry 解析即时 handler / replay handler。
- [ ] construction 持有 context frame plan、audit projection、inspector projection。
- [x] Task / Story / Project session detail 与 session context endpoint 均投影同一 construction query plan。
- [ ] audit、inspector 都投影同一 construction。

### 3. Collapse `LaunchExecution`

- [x] `SessionLaunchPlanner` 消费 `LaunchCommand + SessionConstructionPlan + runtime facts`。
- [ ] `LaunchExecution` 在 connector.prompt 前完整包含 prompt、construction、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input、trace。
- [ ] connector input 由 `LaunchExecution` 投影为 `ExecutionContext`。
- [x] `prompt_pipeline` 只执行计划，不再读取 request/meta/profile 做策略 fallback。
- [x] connector.prompt 失败路径不提交 bootstrap/pending/title 等成功副作用。

### 4. Delete `PromptAugmentInput` Production Handoff

- [x] `SessionConstructionProvider` 不再返回增强后的 `PromptAugmentInput`。
- [x] API bootstrap 输出 `SessionConstructionPlan`，不再返回 `UserPromptInput + SessionConstructionFacts` 过渡 tuple。
- [x] 删除 `LaunchCommand::to_augment_input()`。
- [x] `prompt_pipeline` 不再接收 `PromptAugmentInput`。
- [x] `PromptAugmentInput` 最终代码中不能作为 production helper、跨 crate handoff、planner input 或 augmented output 保留。
- [x] 删除当前 `SessionLaunchRequest` 过渡 envelope。
- [x] 删除当前 `SessionConstructionFacts` provider handoff。

### 5. Remove Business `SessionHub`

- [ ] construction / launch / runtime / eventing / hooks / effects / pending / adapters 有独立服务边界。
- [ ] `SessionHub` 不再承载业务判断。
- [ ] 若类型在中间提交仍存在，该提交不得标记最终完成。

### 6. Effects / Pending / Persistence Finalization

- [ ] 所有 outbox effect handler 都具备 durable identity 或 typed handler。
- [ ] pending runtime command 覆盖 apply-once、connector failure、failed/retry/recovery。
- [ ] 新增业务逻辑按 meta/event/outbox/runtime-command store 能力依赖。
- [ ] PostgreSQL / SQLite migration 验证通过。

## Validation

完成大块收口后统一执行：

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
rg -n "PromptAugmentInput" crates/agentdash-api/src/bootstrap crates/agentdash-application/src/session/launch.rs crates/agentdash-application/src/session/launch_planner.rs crates/agentdash-application/src/session/prompt_pipeline.rs
rg -n "working_dir" crates/agentdash-application/src/session/types.rs crates/agentdash-application/src/session/launch_planner.rs crates/agentdash-application/src/session/assembler.rs crates/agentdash-local/src/handlers/prompt.rs
rg -n "post_turn_handler|parent_vfs|parent_mcp_servers|parent_context_bundle" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "PreparedSessionInputs|finalize_request|PreparedLaunchPrompt|SessionLaunchPlan|AugmentedLaunchInput|PromptSessionRequest|SessionLaunchIntent|LaunchCommand::.*_prepared" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "pending_capability_state_transitions_json" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-infrastructure/src
git diff --check
```

## Commit Plan

按 `.trellis/tasks/05-14-session-refactor-batch-7-final-convergence/implement.md` 的 6 个固定 commit slice 执行。提交信息遵循项目要求：`type(scope): 中文提交信息`，body 分点说明具体更新。
