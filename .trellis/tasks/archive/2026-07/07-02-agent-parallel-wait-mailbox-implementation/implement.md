# Agent 并行等待与 mailbox 回传实施计划

## Checklist

1. 固化 wait contract
   - 确认复用 `LifecycleGate` 还是新增 lifecycle wait record。
   - 写清 wait owner、wake envelope、projection DTO。
   - 如需 schema，新增 migration、domain model、repository trait。

2. 补 companion 基线测试
   - sub wait=false dispatch child mailbox。
   - child `companion_respond` 回写 parent mailbox。
   - parent request/response、human response、dedup。
   - 现有 wait=true timeout/cancel 行为。

3. 实现 waiting projection
   - AgentRun workspace 查询返回 open waits。
   - generated contract 更新。
   - frontend workspace/status bar 展示等待条目。

4. 实现通用 wake adapter
   - event resolved -> source identity -> mailbox envelope -> schedule/notify。
   - 支持 companion result/human response/subagent result。
   - 预留 exec result source namespace。

5. 实现 wait activity watcher
   - 先查询已有 pending/resolved activity。
   - 再等待 mailbox/gate notification 或 timeout。
   - 返回 summary、timed_out、refs，不返回大结果。

6. 收束 companion wait=true
   - 从长期 tool 内轮询转向 durable wait + mailbox resume。
   - 至少保证 timeout、cancel、restart 后 projection 可解释。

7. 接入 exec completion
   - 定义 exec wait source identity。
   - exec completion/failure/cancel 写 wait resolution 和 mailbox wake envelope。
   - 不改变终端展示修复任务的职责边界。

8. 前端刷新与展示
   - workspace control plane 消费 wait projection。
   - `mailbox_state_changed`、companion result、exec wait event 触发 refresh。
   - mailbox row/source label 覆盖新增 source kinds。

9. 守卫检查
   - 搜索确认没有新增旧 Session 形态端点。
   - 搜索确认没有引入 Codex runtime dependency。

## Validation Commands

- `cargo test -p agentdash-domain agent_run_mailbox`
- `cargo test -p agentdash-infrastructure agent_run_mailbox`
- `cargo test -p agentdash-application-agentrun mailbox`
- `cargo test -p agentdash-application companion`
- `cargo run -p agentdash-contracts --bin generate_workflow_contracts`
- `pnpm --filter @agentdash/app-web test -- agent-run-workspace`
- `pnpm --filter @agentdash/app-web test -- agentRunMailbox`

## Risk Points

- wait owner 与 mailbox envelope 职责混淆会导致 receipt、gate、pending action 三套状态互相打架。
- wait=true 轮询替换为 durable suspend/resume 时，当前 active turn 的停止/继续语义需要严密测试。
- result dedup key 不稳定会重复唤醒 parent Agent。
- delivery accepted 后崩溃必须遵守 existing `delivery_result_unknown` recovery 语义。
- 前端等待条目和 mailbox result 不能重复显示为两条不可解释的消息。

## Sub-Agent Dispatch

Phase 2 建议按 disjoint write scope 拆派：

- 后端 domain/repository worker：wait owner、migration、repository/projection。
- 后端 application worker：wake adapter、scheduler/wait watcher、companion integration。
- 前端 worker：workspace waiting projection、mailbox result/refresh UI。
- check agent：contract generation、route guard、mailbox/recovery tests。
