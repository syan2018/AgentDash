# Fix 022: workflow executor launcher service split

## 范围

- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs`
- `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs`
- `crates/agentdash-application/src/workflow/orchestration/function_node_runner.rs`
- `crates/agentdash-application/src/workflow/orchestration/human_gate_launcher.rs`
- `crates/agentdash-application/src/workflow/orchestration/mod.rs`

## 变更

- 保留 `OrchestrationExecutorLauncher::drain_ready_nodes` 作为 scheduler-facing facade，调度循环、attempt policy blocking、`apply_event` reducer 写回仍集中在 facade。
- 新增 `AgentNodeLauncher`，承载 AgentCall executor spec 校验、procedure lookup、activity agent / frame / runtime session / anchor 创建，并把 `NodeStarted` event 返回给 facade。
- 新增 `FunctionNodeRunner`，承载 Function / BashExec 运行、function context 构造、API/Bash output mapping，以及 LocalEffect capability unsupported terminal error 构造。
- 新增 `HumanGateLauncher`，承载 HumanGate open payload / gate 创建，以及 human decision gate resolve 与 decision output helper。
- 行为保持不变：同步 Function/LocalEffect 仍按 `NodeStarted -> NodeCompleted|NodeFailed` 写入，HumanGate 仍 open 后进入 Running，AgentCall 仍创建 runtime session 后提交 `RuntimeSession` executor ref。

## 验证

- `cargo fmt --check`：通过。
- `cargo test -p agentdash-application workflow::orchestration::executor_launcher`：通过，6 passed；存在既有 `session::construction` dead_code warnings。
- `cargo test -p agentdash-application workflow::orchestration`：通过，34 passed；存在既有 `session::construction` dead_code warnings。
- `cargo check -p agentdash-api`：通过。

## Commit

- 未提交，等待主控 agent 统一 review 与提交。
