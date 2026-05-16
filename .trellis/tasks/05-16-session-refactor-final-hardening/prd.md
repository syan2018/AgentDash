# Session 重构最终收尾清洁

## Goal

完成 `AgentDash_session_refactor_plan.md` 后续收尾，把 session 模块从“已基本收敛”推进到“主链路可依赖、分层足够薄、残留兼容路径清零”的状态。

本任务不追求大规模重写，而是清掉 review 中确认的剩余风险：终态清理、runtime command apply-once、context 查询与 launch construction 投影一致性、前端静默兼容路径，以及仍偏厚的 API/bootstrap/assembler/pipeline 层。

## User Value

- Agent prompt 失败、终态持久化失败或 runtime command 状态更新失败时，不会留下卡死的 active turn 或重复应用的能力变更。
- Session Inspector / Context 面板展示的上下文、VFS、capability 与实际启动使用的事实保持一致，降低调试成本。
- 前后端接口不再通过“静默失败”掩盖未实现字段，符合预研期“保持最正确状态”的项目约束。
- 后续维护者可以沿着单一 session 主链路阅读代码，而不是在 API route、bootstrap、assembler、prompt pipeline 之间追隐式补字段。

## Confirmed Facts

- 旧 `PromptSessionRequest` / `PreparedSessionInputs` / `SessionLaunchIntent` 在生产代码检索中已基本消失，当前主线是 `LaunchCommand -> SessionConstructionPlan -> LaunchExecution`。
- 所有生产启动来源已基本通过 `SessionLaunchService::launch_command` 进入，覆盖 HTTP、Task、Workflow、Routine、Companion、Local relay。
- `SessionOwnerResolver` 已统一 owner 优先级为 `Task -> Story -> Project`，启动与 context 查询都在使用该 resolver。
- `SessionConstructionPlan::validate_for_launch` 已要求 launch 前补齐 working directory、executor config、VFS 和 capability state。
- terminal effect outbox 与 runtime command store 已存在，数据库迁移包含 `session_terminal_effects`、`session_runtime_commands` 以及旧 pending capability 字段 drop。
- 当前 `/sessions/{id}/context` 查询仍走 context-query projection，launch 走 finalization 后 projection，二者可能在 runtime command overlay、cached capability、executor fallback、skill/capability projection 上漂移。
- `SessionTurnProcessor` 在 terminal event 持久化失败时会提前 return，可能跳过 active turn cleanup。
- connector accepted 后 `mark_runtime_commands_applied` 失败只记录 warning，可能导致 requested command 下一轮重复应用。
- 前端 `saveSessionTabLayout` / `loadSessionTabLayout` 对 `/sessions/{id}/meta` 的 `tab_layout` 做静默兼容，但后端 meta patch 当前只接受 `title`。
- 用户已确认 `tab_layout` 按正式落库支持推进，不删除该功能。

## Requirements

- 修复 terminal event 持久化失败时 active turn 不释放的问题，保证任何 terminal 处理路径都会清理 active turn 或显式进入可恢复状态。
- 修复 runtime command apply-once 语义：connector accepted 后不能因为 `applied` 标记失败而静默留下可重复应用的 requested command。
- 统一 session context 查询与 launch construction 的最终投影口径，至少让 VFS、MCP、capability、runtime command overlay 与 context endpoint 的可见结果不再隐式分叉。
- 清理前端 `tab_layout` 静默兼容路径：正式实现后端字段与迁移，移除前端“后端可能尚未支持”的静默回退。
- 继续薄化 session 分层，但只做低风险收尾：优先减少 API/bootstrap 层业务逻辑、拆小过厚模块的边界说明和显式 use case，不做大规模 crate 拆分。
- 补覆盖关键风险的单元测试或集成测试，至少覆盖 turn cleanup、runtime command apply failure、context projection consistency、前端 tab_layout 行为。

## Acceptance Criteria

- [ ] terminal event 持久化失败时，`TurnSupervisor` 中对应 session 不会残留 active turn；有回归测试覆盖。
- [ ] runtime command 在 connector accepted 后应用状态更新失败时，不会在下一轮被静默重复应用；有测试覆盖失败路径。
- [ ] `/sessions/{id}/context` 与 launch construction 共享同一最终 projection 或有明确、测试覆盖的投影模式差异；VFS/MCP/capability 不再漂移。
- [ ] 前端 session tab layout 不再依赖静默失败；后端、数据库与前端 service 明确支持该字段。
- [ ] session 启动主线仍保持 `LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> connector prompt -> terminal outbox`。
- [ ] API route 不新增 session construction 业务分支；能下沉的逻辑已下沉到 application use case 或 bootstrap provider 的更薄接口。
- [ ] `cargo check -p agentdash-application`、`cargo check -p agentdash-api` 通过。
- [ ] 相关 Rust 测试和前端测试通过；无法运行的测试需记录原因。

## Out of Scope

- 不拆新 crate。
- 不重写 connector 协议。
- 不重构整个前端 session UI。
- 不改变 Story/Task/Workflow 的产品语义。
- 不引入兼容旧 API 或旧数据库字段的回退方案。

## Open Questions

- 无。
