# 补齐通用 waitable activity 与回传闭环

## Goal

实现 AgentDashboard-native 的通用 waitable activity / wait module，让 exec、companion/subagent、human response、mailbox wake 等并行活动通过同一等待模型返回 AgentRun，而不是各工具各自私有轮询或只靠前端 projection。

这是父任务 `07-03-waitable-activity-exec-closure` 的第二阶段子任务。

## Requirements

1. 定义 waitable activity owner 和状态模型，owner 必须锚定 AgentRun control-plane，可引用 RuntimeSession delivery/trace，但不由 RuntimeSession 拥有。
2. 提供 wait service，支持 register、update、wait、timeout、cancel/terminate observation、result summary/ref projection。
3. Agent tool catalog 必须暴露通用 `wait` 工具，能等待指定 activity refs 或当前 AgentRun 范围内的 activity kinds。
4. `wait` 返回 bounded summary、status、source refs、result refs、cursor 和下一步读取方式，不搬运大结果正文。
5. exec running terminal 必须以 canonical `terminal_id` 注册为 `kind=exec` activity，completion/failure/cancel/lost 能更新 activity 并唤醒 wait。
6. companion/subagent/human gate 必须通过 LifecycleGate adapter 接入 wait service，替代工具内部私有 polling 作为最终形态。
7. human/user response 等待是 waitable source 的一种，不作为 companion tool 的独立等待协议。
8. mailbox wake adapter 必须使用稳定 source identity/dedup key 写入 AgentRun mailbox envelope，并由 scheduler 负责 delivery。
9. wait module 必须能观察已有 pending activity，也能等待未来 activity/notification。
10. workspace snapshot / waiting item projection 必须能表达 exec/human/subagent/companion activity 的状态和 refs。
11. 不新增 `/sessions/*` 控制面，不接入 Codex runtime，不复制 Codex Thread/AgentPath identity。

## Acceptance Criteria

- [ ] 新增 wait module 的 domain/application boundary 和 tests。
- [ ] Agent tool catalog 中出现通用 `wait` 工具。
- [ ] `wait` 可等待 exec activity：running -> output/ready -> completed/failed/cancelled。
- [ ] `wait` timeout 不终止后台 activity；terminate/cancel 必须由明确工具或动作触发。
- [ ] companion/subagent dispatch 和 human request 通过 wait module 生成/等待 activity。
- [ ] gate resolution 后 activity 更新，必要时写入 mailbox wake envelope，重复 result 不重复入队。
- [ ] mailbox pending/completed wake 可唤醒 `wait`，scheduler 仍是 launch/steer/resume authority。
- [ ] frontend waiting item projection 能展示 exec/human/subagent activity 的 kind/status/preview/ref，并保持 terminal output 由 terminal projection 承载。
- [ ] 后端和前端测试覆盖 wait timeout、completed、failed/cancelled、mailbox dedup、generated contract 消费。
- [ ] 搜索确认未新增 `/sessions/*` 控制 endpoint，未把 RuntimeSession 重新提升为 workspace command owner。

## Evidence

- Mailbox spec 已定义 waiting item 和 `kind="exec"` 投影方向：`.trellis/spec/backend/session/agentrun-mailbox.md`。
- companion/human 当前 private polling 在 `crates/agentdash-application/src/companion/tools.rs:243`。
- mailbox scheduler 是 delivery authority：`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:333`。
- frontend waiting item contract 在 `crates/agentdash-contracts/src/runtime/workflow.rs:1151`。
- Codex reference 证明 wait 应返回 small status/ref，而非大正文：`.trellis/tasks/07-03-waitable-activity-exec-closure/research/subagent-codex-reference.md`。
