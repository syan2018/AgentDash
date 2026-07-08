# 修复启动恢复 control-effect replay 栈溢出

## Goal

解决 AgentDash 在进程中断后再次启动时，因为 RuntimeSession 恢复与 AgentRun control-effect replay 交错而触发 `thread 'main' has overflowed its stack` 的启动崩溃。修复后，数据库中存在 stale `running` RuntimeSession、stale AgentRun delivery binding、未完成 mailbox/gate/outbox 状态时，服务启动必须完成可诊断恢复，而不是直接退出。

## Background

2026-07-08 启动日志显示：

- `agentdash_application_runtime_session::session::runtime_control` 在 `recover_interrupted_sessions` 中发现 session `91533d57-999a-443a-8e4b-f68bedf9e652` 上次未正常结束，尝试标记为 `interrupted`。
- 随后 `agentdash-server` 进程以 `thread 'main' has overflowed its stack` 退出。
- `pnpm dev` 因后端退出停止桌面端开发进程，并强制终止 embedded PostgreSQL 进程树。

本轮诊断已确认：

- 源码工作区未被修改，`git status --short` 为空。
- 单独启动 embedded PostgreSQL 做只读检查时，PostgreSQL 自身 crash recovery 可以完成。
- 目标 RuntimeSession 已经成功写入 `turn_terminal(turn_interrupted)` 和 `session_rewound` 事件，`runtime_sessions.last_delivery_status` 已是 `interrupted`。
- 崩溃后仍存在 `agent_run_control_effects.status = running` 的 outbox 记录，覆盖 `agent_run_delivery_convergence`、`wait_producer_terminal_convergence`、`lifecycle_terminal_convergence`。
- 直接用同一数据库启动 `agentdash-server serve` 可以稳定复现主线程 stack overflow。
- 复现后其中一个 child RuntimeSession 的 `agent_run_delivery_convergence` 推进为 `succeeded`，剩余 effect 停在 `wait_producer_terminal_convergence` / `lifecycle_terminal_convergence`，说明启动崩溃边界在 terminal event 持久化之后、control-effect replay 完成之前。

## Requirements

- R1. 启动恢复必须把 RuntimeSession terminal fact 与 AgentRun control-effect 副作用分阶段处理，避免在扫描 stale RuntimeSession 的同时立即触发 mailbox wake、lifecycle drain 或 hook auto-resume。
- R2. control-effect replay 必须支持确定性的相位顺序：先全量收敛 `agent_run_delivery_convergence`，再执行 lifecycle / wait producer / hook 类副作用，避免 parent/child 同时中断时 child fallback 唤醒尚未 terminal 收敛的 parent delivery。
- R3. wait producer terminal fallback 在投递 parent mailbox wake 前必须校验 target delivery binding 与 RuntimeSession terminal fact 的一致性；不一致时应返回可重试或可诊断状态，而不是进入 session launch。
- R4. 启动期 replay 不能在主启动路径中执行无界递归或无界投影；单条 effect 失败必须进入 outbox 状态机和诊断日志，不能让服务进程崩溃。
- R5. session item / journal fallback 投影中用于 preview、JSON 查找或序列化的逻辑必须有大小和深度边界，避免坏 event payload 触发栈溢出。
- R6. 修复必须保留既有事实源语义：RuntimeSession 仍先持久化 `turn_terminal`，AgentRun delivery binding 仍是用户可见运行态事实源，control-effect outbox 仍是 terminal 副作用可重试事实源。
- R7. 本项目处于预研期，不需要兼容旧错误行为；数据库 schema 如需调整应通过 migration 完成。

## Acceptance Criteria

- [ ] AC1. 使用当前坏库或等价 fixture 启动 `agentdash-server serve` 时，服务不再因 `thread 'main' has overflowed its stack` 退出。
- [ ] AC2. 当 parent/child companion RuntimeSession 同时从 running 恢复为 interrupted 时，所有相关 AgentRun delivery binding 先收敛到 terminal，再允许 wait producer fallback 投递 parent wake。
- [ ] AC3. `agent_run_control_effects` 中过期 `running` lease 能被启动 replay 正确 reclaim，并最终进入 `succeeded`、`failed` 或 `dead-letter`，不永久停在不可观察的 `running`。
- [ ] AC4. 如果 parent target delivery 与 RuntimeSession terminal fact 不一致，mailbox wake 不触发 launch，并记录可诊断错误或可重试状态。
- [ ] AC5. 新增回归测试覆盖启动 replay 的相位顺序、stale running outbox reclaim、companion child terminal fallback 唤醒 parent 的冲突防护。
- [ ] AC6. 新增或更新测试覆盖 session item / journal fallback 对深层或大型 JSON payload 的 bounded preview 行为。
- [ ] AC7. 运行后端相关验证命令，至少包含 affected crates 的 Rust tests；如涉及 migration，额外运行 `pnpm run migration:guard`。

## Out Of Scope

- 不做兼容旧错误状态的长期 fallback 逻辑；修复应让控制面事实在启动时收敛到正确状态。
- 不把 RuntimeSession meta 重新作为 AgentRun 用户可见运行态事实源。
- 不通过清空 dev 数据库作为修复手段；可以在验证中复制坏库或构造 fixture。
- 不改变 companion 产品语义，只修复 terminal fallback 与 mailbox wake 的恢复一致性。

## Open Question

- 是否把“当前开发库中残留 outbox 状态的一次性修复脚本/doctor 命令”纳入本任务？推荐答案：纳入诊断/doctor 能力，但不纳入自动数据修补；自动路径应依赖修复后的 replay 收敛。
