# Fix 015: workflow script preflight convergence

## 范围

- `crates/agentdash-application/src/workflow/script/capability_summary.rs`
- `crates/agentdash-application/src/workflow/script/mod.rs`
- `crates/agentdash-application/src/workflow/script/preflight.rs`
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs`

## 变更

- 将 workflow script capability summary 抽取为 `workflow::script::capability_summary` 的单一 typed interpreter，输出 `WorkflowScriptCapabilitySummary`。
- `ScriptCompiler` 删除独立 JSON summary builder，compile output 与 plan metadata 都复用 typed summary 并通过 `serde_json::to_value` 写入 metadata。
- `capability_effect` 在 compile 阶段产生 `local_effect_capability_not_supported` blocking diagnostic，不再生成 `ExecutorSpec::LocalEffect`。
- preflight 在存在 blocking diagnostic 时不返回 `plan_snapshot` / `plan_preview`，但仍返回 typed capability summary 供 UI 展示。

## 验证

- `cargo test -p agentdash-application workflow::script::preflight`：通过，3 passed；存在既有 dead_code warning。
- `cargo test -p agentdash-application workflow::orchestration::script_compiler`：通过，12 passed；存在既有 dead_code warning。
- `cargo check -p agentdash-api`：通过。

## Commit

- 未提交，等待主控 agent 统一 review 与提交。
