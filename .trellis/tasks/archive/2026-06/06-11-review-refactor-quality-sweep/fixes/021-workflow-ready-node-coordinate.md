# Fix 021: workflow ReadyNode coordinate/view cleanup

## 范围

- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs`
- `crates/agentdash-application/src/workflow/orchestration/ready_node.rs`
- `crates/agentdash-application/src/workflow/orchestration/mod.rs`

## 变更

- 新增 module-local `RuntimeNodeCoordinate`，把 runtime executor 坐标收敛为 `run_id + orchestration_id + node_path + attempt`，并用 helper 输出 `orchestration_node_coordinate.v1` detail。
- 拆除 `ReadyNodeTarget` 长程 DTO，改为 `ReadyNodeView` / `RunningNodeView` 在需要时从当前 `LifecycleRun` 构建短生命周期 view；executor async 路径只传 typed coordinate。
- `function_context` 改为从 fresh running view 和当前 `StateExchangeSnapshot` 构建，避免依赖旧 target 上缓存的 runtime/state snapshot。
- executor block / function mismatch / terminal materialization / LocalEffect unsupported 等错误 detail 改用 typed coordinate helper；LocalEffect 回归测试覆盖 `node_id != node_path` 场景，防止坐标漂移。

## 验证

- `rustfmt --edition 2024 crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs crates/agentdash-application/src/workflow/orchestration/ready_node.rs crates/agentdash-application/src/workflow/orchestration/mod.rs`：通过。
- `cargo test -p agentdash-application workflow::orchestration::executor_launcher`：通过，6 passed；存在既有 `session::construction` dead_code warning。
- `cargo test -p agentdash-application workflow::orchestrator`：通过，0 matched / 715 filtered；存在既有 `session::construction` dead_code warning。
- `cargo check -p agentdash-api`：通过。

## Commit

- 未提交，等待主控 agent 统一 review 与提交。
