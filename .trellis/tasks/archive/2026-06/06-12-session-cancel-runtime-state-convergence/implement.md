# 取消后会话运行态模型收敛实施计划

## Batch 1: Current-State Proof And Regression Tests

- [ ] 补后端测试复现 Pi Agent cancel 后立即下一轮 prompt 的 busy 漏洞。
- [ ] 补 session runtime test 覆盖 cancel requested 后 workspace execution state 仍保持 running / cancelling。
- [ ] 补 AgentRun workspace route test 覆盖 cancelling 状态下 `send_next=false`、`steer=false`、`cancel` 状态原因。
- [ ] 补 run-scoped delivery anchor test，构造同一 agent 多 anchor，验证当前 run 只选自己的 runtime。

Validation:

```powershell
cargo test -p agentdash-agent wait_for_idle
cargo test -p agentdash-executor pi_agent cancel
cargo test -p agentdash-application runtime_control
cargo test -p agentdash-api lifecycle_agents
```

## Batch 2: Runtime State Contract

- [ ] 扩展 application runtime execution state，表达 claiming / running / cancelling / terminal / idle。
- [ ] 调整 `TurnSupervisor::request_cancel`，让 cancel requested 后状态进入 cancelling，并保留 active turn refs。
- [ ] 调整 `SessionCoreService::inspect_session_execution_state` 或新增 projection service，聚合 platform turn、connector live / closing 和 `SessionMeta`。
- [ ] 更新 Rust contracts 和 generated TypeScript。

Validation:

```powershell
cargo test -p agentdash-application session::runtime
pnpm run contracts:check
```

## Batch 3: Connector Cancel收口

- [ ] 修复 `Agent::wait_for_idle` 为不会丢通知的 loop wait。
- [ ] 为 Pi Agent connector 建立 cancel 后 idle confirmed 边界。
- [ ] 让 Pi Agent cancel 后 terminal / stream completion 与 platform cancelling 状态按同一顺序收口。
- [ ] 保持 relay connector 的 cancel terminal 语义与统一 projection 对齐。

Validation:

```powershell
cargo test -p agentdash-agent runtime_alignment
cargo test -p agentdash-executor pi_agent
cargo test -p agentdash-application relay_connector::tests::cancel
```

## Batch 4: AgentRun Workspace Projection

- [ ] 将 `build_agent_run_workspace_view` 的 action projection 改为消费统一运行态。
- [ ] `send_next` 仅在 ready 状态 enabled。
- [ ] `steer` 仅在 running 且 active turn ref 匹配时 enabled。
- [ ] cancelling / closing 状态提供结构化 reason。
- [ ] AgentRun command delivery runtime 解析改为 run-scoped anchor lookup。

Validation:

```powershell
cargo test -p agentdash-api lifecycle_agents
cargo test -p agentdash-application agent_message
```

## Batch 5: Frontend Control State

- [ ] 更新 generated DTO 消费点。
- [ ] 调整 `AgentRunWorkspacePage.chatControlState`，让 Ctrl+Enter 只执行 action projection 允许的 command。
- [ ] 调整 pending message / enqueue UI 对 cancelling 状态的处理。
- [ ] 添加 frontend tests 覆盖 ready、running、cancelling、terminal 的 primary / secondary action。

Validation:

```powershell
pnpm --filter app-web test -- AgentRunWorkspacePage
pnpm run frontend:check
```

## Batch 6: Final Verification And Cleanup

- [ ] grep 检查 AgentRun workspace command 不再使用 agent global latest delivery anchor。
- [ ] grep 检查 workspace action projection 不再只依赖 `has_active_turn` / platform `Running`。
- [ ] 手动或自动端到端验证：启动 Draft AgentRun，打断，立即发送第二句话，确认没有 native `Failed to fetch` 和 Pi Agent busy 泄漏。
- [ ] 手动或自动端到端验证：打断后 Ctrl+Enter 不触发 steer。
- [ ] 运行格式化、lint、migration guard。

Validation:

```powershell
cargo fmt --all --check
pnpm run backend:clippy
pnpm run migration:guard
git diff --check
```

## Commit Plan

- `test(session): 固化取消后运行态回归`
- `refactor(session): 收敛 runtime execution state`
- `fix(pi-agent): 取消后等待执行器 idle 收口`
- `fix(agentrun): 使用统一运行态投影工作台命令`
- `fix(app-web): 对齐 AgentRun 运行态控制行为`
- `chore(session): 清理旧状态推断路径`

## Review Gates

- [ ] 当前故障状态被釜底抽薪：取消后 platform idle 与 connector busy 不再能同时让 workspace 展示 ready。
- [ ] 目标架构完整迁移：AgentRun command、runtime-control projection、Pi connector cancel、frontend chat control 共享统一运行态 contract。
- [ ] 所有新增状态都有后端测试和前端 action 测试覆盖。
- [ ] 不依赖延迟、重试、窗口切换或客户端 refresh 修正状态。
