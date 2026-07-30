# Verification

## 已完成

- AgentRun Product 持有 durable Mailbox；普通 producer 经统一 intake，公开 Runtime command 不再暴露普通 Submit/Steer。
- Queue 先持久接收，再尽力读取/投递 Complete Agent；Runtime 暂不可用不会丢失已接收输入。
- 显式 Steer 携带 expected turn，和 SubmitInput 保持一义一命令。
- dispatcher owner lease、消息 claim lease、顺序、pause/resume、move/delete/promote 与 unknown-result inspect 恢复已接入。
- recovery scan 同时覆盖 idle intake 与等待上一回合结束的消息；unknown settlement 保留 claim lease，过期后以同一 effect identity 继续 inspect。
- Complete Agent 暂未绑定、binding repository 暂时失败、unavailable 或 retryable Agent error 会释放 claim 并回到 queued，不会把 durable intake 错标为 terminal failure。
- `AgentLoopTurnBoundary + DrainMode::All` 在同一 dispatcher lease 中批量 claim 并逐条 settle，不再固定只消费一条。
- Channel、Companion、Routine、Workflow、Canvas/interaction 等 producer 已收敛到 Product input delivery；该 delivery 是 Mailbox 的薄入口。
- Dash 同 source 的 command/callback/surface mutation 共用 repository writer，避免合法的 owner 内并发 CAS 被判为 `repository state changed`。
- Complete Agent Hook outcome 可表达组合 decision、continue 与 refresh；Native 在 AfterTurn/BeforeStop safe boundary 接入，continuation 先形成 durable Mailbox/Hook ledger。
- Composer 普通提交只走 `/composer-submit`；运行中空输入显示 Stop，有内容显示 Queue，页面明确展示 `Enter 排队 · Ctrl/Cmd+Enter Steer`。
- Waiting / Steer / Pending Mailbox 投影与 pause/resume、promote、recall、move、delete 控件已恢复。

## 已通过验证

- `cargo test -p agentdash-application-agentrun`：120 unit + 2 doc tests。
- Mailbox PostgreSQL repository：7 tests，包括 boundary target scan、unknown lease recovery、顺序与 pause/resume。
- Native `steer_and_interrupt_orchestrate_the_active_turn`：通过。
- Native core callback suite：6 tests，通过。
- 前端 Mailbox/Composer/command-state 定向测试：30 tests，通过；完整相关集合此前为 69 tests，通过。
- `pnpm --filter app-web typecheck`：通过。
- 任务相关 ESLint：通过。
- `pnpm run contracts:check`：通过。
- `pnpm run agent-runtime:guard`：通过。
- `git diff --check`：通过。
- 最终 `pnpm dev`：Rust 重编译成功、migration schema 6 成功、API health 200、local runtime 注册、Vite 5380 就绪。
- 浏览器打开原日志对应 AgentRun，确认 Stop / Queue 与显式 Steer 提示；未发送测试消息，避免污染用户历史。

## 仍需完成后再关闭任务

- 增加同一测试内强制重叠 Dash Core callback commit 与 Steer commit 的确定性并发回归，而不只验证 active-turn steer orchestration。
- 补 producer 级数据库/E2E 矩阵，逐一证明 Companion parent/result/human、Routine、Workflow 与 Canvas 的 source identity、dedup 和 active-turn Queue 行为。
- 收敛 Product Hook ledger 中尚未由独立恢复 worker 消费的通用 `EmitEffect` / refresh effect 语义，并删除不需要的 lease/schema，或实现完整 claim/settlement。
- 补 transcript provenance 的 Native/Codex/Remote 持久化回放验收。
- 用隔离测试 AgentRun 实际提交 Queue、Steer、pause/resume、promote/reorder/delete；不得污染现有用户 AgentRun。
