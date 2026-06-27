# AgentRun workspace 应用层重构

## Goal

将 AgentRun workspace 的业务投影、command policy 与 ProjectAgent start 首轮投递语义收束到 `agentdash-application`，让 `agentdash-api` 只保留鉴权、HTTP 参数解析、contract DTO mapping 与错误映射。

这次重构服务于后续 AgentRun / Lifecycle / RuntimeSession 大迁移：AgentRun workspace 应以 run / agent / frame / active AgentRunTurn / mailbox / command receipt 为事实源生成统一 read model，而不是在 API route 中散落状态判断和命令前置条件。

## Confirmed Facts

- `crates/agentdash-api/src/routes/project_agents.rs` 现在在 route 中装配 `ProjectAgentRunStartService` 与 `AgentRunMailboxService`，并把 `ProjectAgentRunStartDispatch` 手动映射为 `ProjectAgentRunStartResult`。
- `crates/agentdash-application/src/workflow/project_agent_run_start.rs` 的 `start_run` 已经 claim 外层 `project_agent_start` receipt，然后通过 `ProjectAgentRunInitialMessagePort` 调用 mailbox-first 首条消息。
- `ProjectAgentRunInitialMessagePort` 当前返回 `AgentRunMailboxCommandResult`，因此 ProjectAgent start 仍直接认识 mailbox command receipt / outcome / mailbox message 形态。
- `crates/agentdash-application/src/workflow/agent_run_mailbox.rs` 的 `accept_user_message` 负责 claim `agent_run_message` receipt、创建 mailbox envelope、调度并返回 scheduler outcome 与 accepted refs。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` 当前内联组装 `AgentRunWorkspaceView`，并在同一文件内分散维护 `control_plane`、`actions`、`conversation_state_code`、`workspace_delivery_status`、`execution_state_turn_id`、`runtime_command_state_dto`、command stale guard 和 command availability。
- `.trellis/spec/backend/architecture.md` 要求 API 层负责鉴权、DTO 和错误映射；业务编排进入 application 层。
- `.trellis/spec/backend/session/runtime-execution-state.md` 明确 AgentRun workspace public shell 和 conversation command surface 应由 AgentRun 控制面事实投影生成，并引用 RuntimeSession trace metadata 而不由其拥有 workspace 语义。
- `.trellis/spec/backend/session/agentrun-mailbox.md` 明确 composer submit / mailbox command 统一走 command receipt -> mailbox envelope -> scheduler outcome。

## Requirements

- 新增 application 层 AgentRun workspace read model 模块，集中生成 workspace shell、conversation state、control plane、actions、mailbox projection 所需的业务状态。
- 新增或迁移 application 层 command policy，集中处理 workspace command stale guard、command availability、replacement command、typed conflict detail。
- API route 只负责从 HTTP/path/body/current user 进入 application use case，并把 application read model / error 映射为 `agentdash-contracts` DTO / `ApiError`。
- ProjectAgent start 的外层 `project_agent_start` receipt 是 API 返回和 duplicate replay 的唯一 start command receipt。
- 首条 mailbox 消息仍走 mailbox-first envelope/scheduler，但 `ProjectAgentRunInitialMessagePort` 返回 start 专属的 typed launch refs，而不是泄漏 `AgentRunMailboxCommandResult`。
- Duplicate ProjectAgent start 只 replay 外层 start receipt 中持久化的 accepted refs。
- Rust contract DTO 与 generated TypeScript 继续作为前后端 wire source；前端不新增手写 AgentRun workspace DTO 别名。
- 若本次重构触及数据库 schema，必须补 migration 并运行 migration guard；当前规划预期以 application/API 边界迁移为主，不需要新增业务表。
- 实现阶段按可独立编译、可独立验收的 phase 分别提交，每个提交只包含一个收束点。

## Acceptance Criteria

- [ ] `lifecycle_agents.rs` 不再拥有 AgentRun workspace 状态投影规则；同一 `SessionExecutionState` 到 shell/control/actions/command state 的派生来自 application 层单一模型。
- [ ] AgentRun command precondition 和 stale guard 校验不再由 API route 私有实现；API 只映射 application conflict。
- [ ] `ProjectAgentRunInitialMessagePort` 不再返回 `AgentRunMailboxCommandResult`，ProjectAgent start 不再消费 mailbox command receipt 作为外层 start 语义。
- [ ] ProjectAgent start 首次提交、duplicate replay、initial mailbox launch refs 缺失/不匹配等路径有 application 测试覆盖。
- [ ] AgentRun workspace projection 覆盖 idle、starting claimed、running active、cancelling、completed、failed、interrupted、missing delivery runtime、missing frame、terminal agent 等状态组合。
- [ ] AgentRun mailbox command route 仍然返回现有 contract DTO，并通过 contracts/typecheck 验证前端消费无 drift。
- [ ] `cargo test -p agentdash-application` 中相关 workflow/session 测试通过。
- [ ] `cargo check -p agentdash-api` 通过。
- [ ] 若 contract 生成结果变化，`pnpm run contracts:check` 与 `pnpm --dir packages/app-web run typecheck` 通过。
- [ ] 每个独立可用 phase 都有单独 commit，commit message 使用项目约定的中文格式并在 body 说明该 phase 的具体收束内容。

## Scope Boundaries

- 本任务聚焦 AgentRun workspace application read model、command policy 和 ProjectAgent start receipt 语义。
- `agentdash-application-ports` 与 `agentdash-relay` 的依赖边界作为独立研究任务处理。
- 不重新设计 mailbox envelope、scheduler barrier/drain mode、RuntimeSession event store 或 connector turn lifecycle。
- 前端只做 contract 消费必要调整，不做页面视觉或交互重构。

## Execution Shape

- 本任务作为单一实现任务完成，不创建 Trellis 子任务。
- 主线程负责整体边界、跨模块集成、最终验证与提交。
- 如需使用 subagent，仅在同一任务内派发互不重叠的实现切片或研究切片，并由主线程统一合并。

## Parallelization Notes

- ProjectAgent start receipt 收束与 AgentRun workspace projection 纯模型可以并行推进，原因是前者主要触及 `project_agent_run_start.rs` / `project_agents.rs`，后者主要新增 application workspace projection 模块和测试。
- Workspace query service 与 command policy 最终都会改动 `lifecycle_agents.rs` route integration，适合在 projection 模型稳定后由主线程或单一实现 agent 收口。
- Subagent 派发必须声明互不重叠的 write ownership，并在 prompt 中说明当前任务路径、身份、禁止等待其它 subagent、不得回滚他人变更。

## Open Question

- 是否按当前单任务方案进入 implementation 阶段？
