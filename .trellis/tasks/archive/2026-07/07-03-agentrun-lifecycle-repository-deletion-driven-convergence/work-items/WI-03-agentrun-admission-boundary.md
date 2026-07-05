# WI-03 AgentRun Admission Boundary

## Objective

建立 `AgentRunAdmission` 原子用例边界，使 ProjectAgent start / AgentRun start / fork materialization 不再由 API 和多个 service 拼装半成品。

## Decisions

D-001, D-002, D-007, D-018

## Research Inputs

- `research/aggregate-ownership.md`
- `research/command-mailbox-delivery.md`
- `research/fork-lineage-baseline.md`
- `references/adversarial-first-principles-review.md`

## Scope

- 定义 admission 输入、输出和事务边界。
- 原子创建 LifecycleRun / LifecycleAgent 或 child AgentRun control records。
- 原子创建 initial AgentFrame revision。
- 原子创建 immutable runtime execution anchor 或 delivery trace ref。
- 原子创建 initial mailbox envelope。
- 原子写入 outer command receipt accepted refs。
- API 层只调用 admission，不调度首条消息。

## Out Of Scope

- 不实现完整 command lifecycle；交给 WI-04。
- 不实现 accepted turn commit；交给 WI-05。
- 不实现 fork baseline 细节；交给 WI-08。

## Dependencies

依赖 WI-00 的 start/fork 使用点清单。WI-04 需要对齐 mailbox envelope 形态。

## Implementation Notes

- admission 应是 application use case，不是 domain repository。
- admission 内部可以使用 unit of work 或 transaction port，但调用方不应知道底层仓储组合。
- 对 ProjectAgent start 和 AgentRun start 的输出 contracts 要以 AgentRun identity 为主。

## Acceptance

- start 失败不会留下互相不可解释的 run/agent/frame/session/receipt/mailbox 半成品。
- API 层不再执行 initial mailbox enqueue 或首条消息 scheduler 调度。
- admission output 足以让前端进入 AgentRun workspace，而不依赖 raw RuntimeSession identity。

## Validation

- start / project-agent launch 路径单元测试覆盖成功和中途失败回滚。
- migration 或 FK 约束能支持 admission 原子写入顺序。
- `rg` 确认 API 层没有残留首条消息调度逻辑。

## Acceptance Record - 2026-07-04 Batch B / Worker B1

- 新增 `AgentRunAdmissionService` application facade，API 面向 `admit_project_agent_start` / `admit_explicit_fork` / `admit_fork_submit` 进入 start/fork 用例；底层 ProjectAgent start 与 fork 服务继续持有现有 run/agent/frame/anchor/mailbox/receipt 写入顺序。
- ProjectAgent start API 只负责解析、鉴权、构造 admission 依赖与响应映射；initial mailbox envelope、outer receipt accepted refs、queued initial mailbox scheduler 仍由 application start admission 路径产出。
- AgentRun fork、fork-submit、composer auto-fork API 改为调用 fork admission；API 不再直接调用 fork service 的 materialization/submit 方法。
- Admission 测试入口已覆盖现有 ProjectAgent start 成功、重复命令、initial mailbox refs mismatch 清理、调度所有权，以及 fork materialization、fork-submit、重复命令、失败清理路径。
- WI-04 mailbox envelope 对齐点：ProjectAgent initial message 继续使用 `MailboxSourceIdentity::draft_start()`、`schedule_on_submit: false`、`{outer_client_command_id}:initial-message`；fork-submit child mailbox 继续使用 composer source、`schedule_on_submit: true`、`{outer_client_command_id}:fork-submit-message`。这些 envelope 现在由 admission 下方 application service 负责产出，便于 WI-04 后续迁移 owner/port。
- 验证命令：`rg` 确认 API routes 无 `initial_mailbox` / `schedule_initial` / `ProjectAgentRunInitialMailbox` / `draft_start` / `schedule_on_submit: false` 命中，且无 `.start_run(` / `.explicit_fork(` / `.fork_submit(` 直接调用；`cargo test -p agentdash-application-agentrun agent_run::project_agent_start::tests`；`cargo test -p agentdash-application-agentrun agent_run::fork::tests`；`cargo test -p agentdash-api routes::project_agents::tests`；`cargo test -p agentdash-api routes::lifecycle_agents::tests::agent_run_fork_response_preserves_redirect_and_lineage`；`cargo check -p agentdash-application-agentrun -p agentdash-application-lifecycle -p agentdash-api`；`cargo fmt --check`；`git diff --check`。
