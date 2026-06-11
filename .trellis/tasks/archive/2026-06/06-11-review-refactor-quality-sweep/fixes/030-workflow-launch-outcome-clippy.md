# Fix 030: workflow launch outcome clippy gate

## 模块

- workflow-orchestration

## 问题

`pnpm run backend:clippy` 在收尾验收时发现 `AgentNodeLaunchOutcome` 与 `HumanGateOpenOutcome` 的成功分支携带完整 `OrchestrationRuntimeEvent`，触发 `clippy::large_enum_variant`。

## 更新

- 将 AgentCall 启动结果中的 `event` 改为 `Box<OrchestrationRuntimeEvent>`。
- 将 HumanGate 启动结果中的 `event` 改为 `Box<OrchestrationRuntimeEvent>`。
- 在 `OrchestrationExecutorLauncher` 消费 outcome 时解包事件后继续进入 reducer，保持运行语义不变。

## 涉及文件

- `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs`
- `crates/agentdash-application/src/workflow/orchestration/human_gate_launcher.rs`
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs`

## 验证

- `cargo fmt --package agentdash-application`
- `pnpm run backend:clippy`
- `git diff --check`

## Commit

- `9be19743`：`fix(workflow): 修复编排启动结果 clippy 阻塞`
