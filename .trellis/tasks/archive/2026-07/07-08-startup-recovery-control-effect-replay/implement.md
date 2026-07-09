# 实施计划：启动恢复 control-effect replay 栈溢出

## Phase 0 - Repro And Baseline

- [ ] 复制当前坏库或构造等价 fixture，记录 `runtime_sessions`、`agent_run_delivery_bindings`、`agent_run_control_effects`、`agent_run_mailbox_messages` 的关键状态。
- [ ] 写一个最小测试或 harness，复现 parent/child companion terminal fallback 在 stale delivery binding 下进入危险路径。
- [ ] 确认当前 `agentdash-server serve` 在坏库上会 stack overflow，作为原始红灯信号。

## Phase 1 - Outbox Replay Phasing

- [ ] 在 AgentRun control-effect service 增加 phased replay API。
- [ ] 调整 claim 查询，使 delivery convergence phase 只 claim `agent_run_delivery_convergence`。
- [ ] 调整 side-effect phase，使 wait producer / lifecycle / hook effects 在 delivery convergence 之后执行。
- [ ] 保持原有 `replay_control_effect_outbox(limit)` 兼容内部调用方式或替换调用点到 phased API。

## Phase 2 - Startup Recovery Ordering

- [ ] 调整 API post-AppState worker，启动期按 terminal recovery -> delivery convergence -> terminal side effects 的顺序执行。
- [ ] 确保启动期 replay 错误只进入诊断和 outbox 状态，不阻断服务 readiness。
- [ ] 为本机 runtime 如有同类启动恢复路径，确认是否需要同步采用相同顺序。

## Phase 3 - Mailbox Wake Guard

- [ ] 在 companion gate mailbox wake delivery 前校验 target AgentRun delivery binding 与 RuntimeSession terminal/running 状态一致。
- [ ] 对 parent stale running + terminal RuntimeSession 的情况返回可诊断错误或可重试状态，不进入 launch。
- [ ] 覆盖 completed / failed / interrupted terminal policy 下的 parent wake 行为。

## Phase 4 - Bounded Journal Projection

- [ ] 为 session item preview / JSON search / truncation metadata search 增加深度和大小边界。
- [ ] 增加深层 JSON、大 payload、动态工具 arguments 的单元测试。
- [ ] 确保 bounded preview 不破坏 lifecycle VFS item path 和 tool metadata 的稳定性。

## Phase 5 - Verification

- [ ] `cargo test -p agentdash-application-agentrun agent_run::control_effects`
- [ ] `cargo test -p agentdash-application-agentrun agent_run::delivery_state`
- [ ] `cargo test -p agentdash-application-runtime-session process_turn_terminal`
- [ ] `cargo test -p agentdash-application-workflow gate_wait_policy`
- [ ] `cargo test -p agentdash-application companion`
- [ ] 如修改 API/bootstrap：`cargo test -p agentdash-api`
- [ ] 如修改 migration：`pnpm run migration:guard`
- [ ] 用坏库或 fixture 启动 `agentdash-server serve`，确认无 stack overflow，outbox 最终收敛到 succeeded / failed / dead-letter。

## Review Gates

- [ ] PRD 验收项 AC1-AC7 全部有测试或人工验证证据。
- [ ] 所有新增诊断日志包含 session_id、turn_id、effect_id、effect_kind、delivery_runtime_session_id 等定位字段。
- [ ] 没有把 RuntimeSession meta 重新引入为 AgentRun workspace 运行态事实源。
- [ ] 没有通过清库、跳过 effect、吞错误的方式绕过恢复。

## Rollback Points

- Outbox phased replay API 可先以新方法并存，验证后再替换启动调用点。
- Mailbox wake guard 可先只阻断明确不一致状态，并记录诊断，再根据测试补齐 terminal policy。
- Bounded projection 可保持原有 public shape，仅改变 preview 文本生成方式。
