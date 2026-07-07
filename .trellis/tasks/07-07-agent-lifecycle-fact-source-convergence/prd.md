# Agent 生命周期事实源收束

## Goal

让 RuntimeSession terminal、AgentRun control effects、LifecycleGate wait policy、mailbox wake、hook effect 和 relay terminal 语义收束到稳定事实源。修复后，terminal 后续副作用必须可幂等落库、可 claim、可 replay；companion wait 必须作为 LifecycleGate 的原子附属策略创建；relay 的 runtime terminal 与 interactive terminal 必须在事件命名和投影上保持清晰边界。

## Background

当前分支已经把旧的 session terminal callback / terminal effects 入口收成 `RuntimeTerminalBoundary -> AgentRunControlEffectPort`，并把旧散 key 的 wait payload 改为 `GateWaitPolicyEnvelope { schema_version, wait_policy, display }`。这些方向正确，但 review 发现仍存在事实源闭环缺口：

- `agent_run_control_effects` 当前边插入边同步执行，且 `WaitProducerTerminalConvergence` 在 delivery convergence 执行成功后才派生；如果后续插入或执行失败，terminal fact 已成立但 wait/lifecycle/hook 副作用可能没有 durable replay 机会。
- control effect row 缺少稳定幂等键，重复 terminal evidence 可以生成多组 effect。
- replay 没有 claim/lease，多个 API 实例或并发 replay 可能执行同一 effect。
- companion wait gate 仍先 open gate 再补写 wait policy；补写失败会留下无法被 producer terminal fallback 识别的 open gate。
- hook effect 仍依赖内存 handler，durable replay 不能保证恢复同一 side effect。
- relay 侧 runtime session terminal 与 interactive terminal / PTY terminal 的命名边界仍不够清晰。

## Requirements

1. `agent_run_control_effects` 必须成为 terminal 后续副作用的 durable outbox，而不是普通审计表。
2. 同一 RuntimeSession terminal evidence 重复进入时，必须得到同一组 control effect rows，不能生成重复副作用事实。
3. outbox consumer 必须通过原子 claim/lease 获取执行权；pending、failed、running-expired rows 可重试，超过尝试次数进入 dead-letter。
4. `observe_runtime_terminal` 必须先 materialize 完整 effect set，再执行；任一 effect 执行失败不能阻止其他 effect 落库。
5. `WaitProducerTerminalConvergence` 不能依赖 delivery convergence replay 再次返回 updated=true；wait producer fallback 的 durable row 必须在 terminal intake 阶段可恢复。
6. `GateWaitPolicyEnvelope` 保持为 LifecycleGate payload 的 typed wait policy；companion wait policy 必须随 gate open 原子写入。
7. 代码、日志、phase 和 public names 应从 `wait_obligation` 收束为 gate wait policy / gate producer terminal fallback / gate mailbox wake 语义。
8. `GateMailboxWakeIntent` 的执行边界必须固定；推荐纳入 control effect outbox，不能保留 replay Noop 的虚假 effect kind。
9. durable hook effect 必须可 replay：handler 执行结果返回 `Result`，并有可恢复 handler identity；不可 durable 的 hook effect 不写 durable outbox。
10. RuntimeSession terminal 主入口保持 `RuntimeTerminalBoundary -> AgentRunControlEffectPort`。
11. relay runtime session terminal 与 interactive terminal / PTY terminal 的类型、命名、payload 和前端投影必须可区分。
12. 数据库 migration 可直接收正模型；当前项目未上线，不需要兼容旧 `wait_obligation` payload 或旧 outbox shape。

## Acceptance Criteria

- [ ] 同一 `delivery_runtime_session_id + turn_id + terminal_event_seq + effect kind` 重复 intake 只生成同一 durable control effect。
- [ ] delivery convergence 失败、wait fallback 失败、lifecycle convergence 失败、hook effect 失败互不阻止彼此的 effect rows 落库。
- [ ] 两个 replay worker 并发执行时，不能 claim 同一 effect row。
- [ ] failed / running-expired effect 可重试；attempt 超限后进入 dead-letter，且不阻塞其他 effect。
- [ ] child agent terminal without `companion_respond` 会通过 gate producer terminal fallback resolve gate，并产生一次 parent mailbox wake。
- [ ] terminal/result race 不覆盖已有 companion result，且 mailbox wake 幂等。
- [ ] companion wait gate 创建后立即包含 valid `GateWaitPolicyEnvelope`，不存在 open gate 后再补写失败导致缺少 wait policy 的窗口。
- [ ] hook handler 返回错误时 effect 标记 failed 并可 replay；未注册 durable handler 不会被标记 succeeded。
- [ ] 不可 durable hook effect 不写入 durable outbox。
- [ ] relay `event.session_state_changed` 只驱动 RuntimeSession / AgentRun terminal；interactive terminal lost 只更新 terminal resource state。
- [ ] backend disconnect 同时影响 runtime session 与 interactive terminal 时，前端不会把两类 lost 合并为同一 terminal 状态。
- [ ] 相关 Rust tests、migration guard、TypeScript contract/codegen 或前端 tests 按实际改动范围通过。

## Out of Scope

- 不引入独立 `wait_obligations` aggregate 或 table。
- 不保留兼容旧 wait payload 的长期 fallback。
- 不把 RuntimeSession trace metadata 升格为 AgentRun workspace running/terminal 事实源。
