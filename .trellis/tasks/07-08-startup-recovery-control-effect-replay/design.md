# 设计：启动恢复 control-effect replay 分相收敛

## Problem Statement

启动恢复目前把 RuntimeSession terminal fact 写入、AgentRun control-effect materialize、control-effect claim/execute 放在同一条同步路径里。parent/child AgentRun 同时中断时，child 的 wait producer fallback 可能在 parent delivery binding 仍是 stale `running` 时唤醒 parent mailbox，从而重新进入 session launch / mailbox 调度链路，最终在主线程启动路径栈溢出。

## Current Evidence

- `SessionRuntimeService::recover_interrupted_sessions` 扫描 `runtime_sessions.last_delivery_status = running`，对每条 session 调用 terminal processor。
- `process_turn_terminal` 会先持久化 `turn_terminal`，再写 `session_rewound` marker，然后提交 terminal boundary evidence。
- `RuntimeTerminalBoundaryService` 将 evidence 交给 `AgentRunControlEffectService::observe_runtime_terminal`。
- `observe_runtime_terminal` materialize 多种 effect 后立即 claim 并执行，包括 delivery convergence、wait producer convergence、lifecycle convergence、hook effects、hook auto resume。
- API 启动 worker 同步 `await replay_control_effect_outbox(100)`，replay 中的 stack overflow 会打死 `agentdash-server` 主线程。
- 坏库中 target RuntimeSession 已 terminal，但 AgentRun delivery binding 仍可能 stale running，control-effect outbox 存在过期 running lease。

## Target Model

RuntimeSession terminal recovery 和 AgentRun terminal side effects 拆成三个相位：

1. Terminal fact phase
   - 扫描 stale RuntimeSession。
   - 对缺失 terminal fact 的 session 写入 `turn_terminal` 与 rewind marker。
   - 只 materialize outbox，或者 materialize 后不执行会唤醒新工作的副作用。

2. Delivery convergence phase
   - Reclaim stale running control-effect leases。
   - 只执行 `agent_run_delivery_convergence`。
   - 目标是让所有 AgentRun current delivery binding 先从 stale running 收敛到 terminal / stale no-op。

3. Side-effect phase
   - 执行 lifecycle terminal convergence、wait producer terminal convergence、hook effects、hook auto resume。
   - 这些 effect 可以产生 mailbox wake、lifecycle drain、hook follow-up，但它们观察到的 delivery binding 已完成第一相收敛。

## Boundary Contracts

- RuntimeSession terminal event 仍是 terminal trace fact 的第一落点。
- AgentRun delivery binding 仍是 workspace/list/command availability 的事实源。
- `agent_run_control_effects` 仍是 terminal side effect outbox，负责重试、dead-letter 和审计。
- Wait producer fallback 不能越过 delivery binding 与 RuntimeSession terminal fact 的一致性检查。
- 启动 worker 可以调度 replay，但不应该在 AppState ready 前执行会创建新 turn 的复杂递归链路。

## Proposed Changes

### Recovery API

为 control-effect replay 增加 phased entry point，例如：

```rust
pub enum AgentRunControlEffectReplayPhase {
    DeliveryConvergence,
    TerminalSideEffects,
}

pub async fn replay_control_effect_outbox_phase(
    &self,
    phase: AgentRunControlEffectReplayPhase,
    limit: u32,
) -> Result<usize, String>;
```

`DeliveryConvergence` 只 claim `agent_run_delivery_convergence`。`TerminalSideEffects` claim 其他 effect kind，并保留每条 effect 的 retry/dead-letter 语义。

### Startup Worker

启动期 worker 按固定顺序执行：

```text
recover_interrupted_sessions terminal facts
replay DeliveryConvergence until bounded batch exhausted
replay TerminalSideEffects with bounded batch
spawn periodic/background replay loop
```

如果某个 phase 出错，服务仍应启动，并留下 outbox 状态与诊断；不能让启动进程因为 replay 错误退出。

### Mailbox Wake Guard

`deliver_companion_mailbox_message` / gate wake delivery 前置校验：

- 当前 delivery binding 必须匹配 intent 的 target runtime session。
- 如果 target binding 是 `running`，RuntimeSession 也必须是 active/running；若 RuntimeSession 已 terminal，则返回 conflict/retryable error。
- 如果 target binding 是 `terminal`，根据 companion gate/wake 语义判断是否允许投递 follow-up；不允许时 effect 进入 failed/dead-letter 或 no-op 成功，不能触发 launch。

### Bounded Projection

Session item / journal fallback 中以下逻辑需要有深度与大小边界：

- JSON preview
- nested preview/truncation metadata search
- large thread item target rendering

边界应返回稳定摘要，如 `json_payload_too_deep` / bounded text preview，而不是递归遍历完整 payload。

## Test Strategy

- Unit test `AgentRunControlEffectService` phased replay：delivery phase 不 claim wait/lifecycle/hook；side-effect phase 不抢 delivery。
- Integration-style memory repository test：parent/child delivery 都 stale running，child terminal fallback 在 parent delivery 未收敛时不 launch，delivery phase 后 side-effect phase 才允许继续。
- PostgreSQL repository test：过期 `running` lease 被 phase replay reclaim；未过期 running 不被重复 claim。
- API/bootstrap test 或 service-level test：startup replay error 不阻断 AppState ready。
- Session item projection unit test：深层 JSON 与大 payload 不 stack overflow，preview 有明确边界。

## Operational Notes

- 当前坏库可作为人工验证材料，但实现测试应构造 deterministic fixture，避免依赖个人 dev DB。
- 若改动 outbox schema 或 claim 查询，需要新增 migration 并运行 migration guard。
- 当前任务不应通过手动清理 outbox 作为修复；手动 SQL 只可用于诊断或恢复开发环境。

## Risks

- Phased replay 改变 effect 执行顺序，必须保证已有 terminal side effects 的幂等 dedup key 仍然成立。
- Mailbox wake guard 如果过严，可能阻断合法的 terminal follow-up；测试要覆盖 completed/failed/interrupted 三种 terminal policy。
- Background replay 从同步启动路径移出后，前端可能短时间看到 stale projection；需要靠 projection invalidation 和 command availability 的事实源一致性保证最终收敛。
