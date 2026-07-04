# WI-05 Accepted Turn Frame Lifecycle

## Objective

把 RuntimeSession accepted、AgentRun frame commit、mailbox accepted refs、command receipt outcome、Lifecycle node started 收束到同一业务提交边界。

## Decisions

D-008, D-009, D-011, D-012

## Research Inputs

- `research/command-mailbox-delivery.md`
- `research/agentframe-context-surface.md`
- `research/aggregate-ownership.md`
- `references/adversarial-first-principles-review.md`

## Scope

- 定义 `AgentRunTurnAccepted` 或等价 accepted fact。
- accepted boundary 同步提交 frame commit / applied frame binding。
- accepted boundary 同步提交 mailbox accepted refs 和 command receipt outcome。
- accepted boundary 同步提交 delivery attempt terminal state。
- Lifecycle `NodeStarted` 只由 accepted fact 推进。
- terminal state 只由真实 terminal fact 推进。

## Out Of Scope

- 不决定 AgentFrame 内部 canonical surface；交给 WI-07。
- 不处理 current delivery physical shape；交给 WI-06。
- 不重建 fork baseline；交给 WI-08。

## Dependencies

依赖 WI-03 admission、WI-04 command/mailbox、WI-06 delivery binding、WI-07 AgentFrame/ContextDelivery 的边界定义。

## Implementation Notes

- RuntimeSession event append 可以先发生在内部 trace 层，但对外 accepted success 必须等 AgentRun boundary 提交成功。
- 生产路径不能使用 noop accepted launch commit。
- 若发生失败，恢复流程要能判断 accepted boundary 是否已经完成，而不是只看 runtime event。

## Acceptance

- 不存在 RuntimeSession accepted success 但 AgentRun frame/current surface 丢失的路径。
- `NodeStarted` 不再由 materialization / allocation 推进。
- accepted commit 失败时，command outcome 和 mailbox item 不会表现为成功。
- ContextFrame emission 能追溯到同一个 accepted input fact。

## Validation

- accepted success / frame commit failure / lifecycle update failure 的事务测试。
- Lifecycle node state projection rebuild 测试。
- end-to-end start -> accepted turn -> workspace visible frame 流程验证。

## Implementation Record

- 2026-07-05：删除 RuntimeSession launch 的 noop accepted commit 替代路径；缺少 `AcceptedLaunchCommitPort` 时 launch 直接失败。
- 2026-07-05：`AcceptedLaunchCommitPort::commit_accepted_launch` 改为返回 `Result`，AgentRun frame revision、current delivery binding、Lifecycle accepted-start 任一失败都会让 RuntimeSession launch 返回失败。
- 2026-07-05：RuntimeSession commit 阶段严格提交 user input / turn started / `ContextDeliveryRecord` / `context_frame` / session meta / runtime command applied；失败会清理 turn/hook 并写入 failed terminal。
- 2026-07-05：新增 `AcceptedTurnLifecycleAdvancePort`，Lifecycle 根据 RuntimeSession anchor 在 accepted commit 中提交 `NodeStarted`；graph dispatch 和 workflow agent-node allocation 改为 `NodeClaimed`，仅移出 ready queue，不写 started trace。
- 2026-07-05：新增 frame commit failure、lifecycle update failure、mailbox launch failure、accepted lifecycle start、NodeClaimed reducer 测试；本切片无数据库迁移。
- 2026-07-05：Remaining risk：当前 accepted boundary 不是跨 RuntimeSession event/meta/runtime-command store、AgentFrame、delivery binding、LifecycleRun、mailbox/receipt 的物理单 DB 事务。代码门槛是所有 accepted commit 步骤成功后才向 mailbox/receipt 返回 accepted refs；任一步骤失败会让 RuntimeSession launch 返回错误并使 mailbox/receipt 标记失败。若 frame/binding 等前置事实已写入而后续步骤失败，后续恢复必须以 mailbox/receipt 未 accepted、Lifecycle 未 started 和 failed terminal 诊断为准做 reconcile。
