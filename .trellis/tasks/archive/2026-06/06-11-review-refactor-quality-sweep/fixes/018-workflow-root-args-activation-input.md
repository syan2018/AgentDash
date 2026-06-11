# Fix 018: workflow root args activation input

## 范围

- `crates/agentdash-application/src/workflow/orchestration/runtime.rs`
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs`
- `crates/agentdash-application/src/workflow/orchestration/mod.rs`
- `crates/agentdash-application/src/workflow/script/preflight.rs`

## 变更

- 新增 application-private `OrchestrationActivationInput` 与 `RootInputBinding`，保留 `activate_orchestration(...)` 作为空输入路径，并新增 `activate_orchestration_with_input(...)` 供脚本运行时传入 root args。
- runtime activation 改为只从 typed activation input 物化 root node inputs，不再反读 plan metadata 中的 script args 或 root input bindings。
- `ScriptCompiler` 删除 concrete args compile input，`root_arg_keys` 只由 `document.args_schema` 表达 compile-time contract，未声明的 entry input 会产生 blocking diagnostic。
- `ScriptCompileOutput` 暴露 typed `root_input_bindings`，plan metadata 只保留静态 `args_schema`、limits、log markers、capability summary 与 provenance 信息。

## 验证

- `rustfmt --edition 2024 crates/agentdash-application/src/workflow/orchestration/runtime.rs crates/agentdash-application/src/workflow/orchestration/script_compiler.rs crates/agentdash-application/src/workflow/orchestration/mod.rs crates/agentdash-application/src/workflow/script/preflight.rs`：通过。
- `cargo test -p agentdash-application workflow::orchestration::runtime`：通过，9 passed；存在既有 dead_code warning。
- `cargo test -p agentdash-application workflow::orchestration::script_compiler`：通过，13 passed；存在既有 dead_code warning。
- `cargo test -p agentdash-application workflow::dispatch_service`：通过，7 passed；存在既有 dead_code warning。
- `cargo check -p agentdash-api`：通过。

## Commit

- 未提交，等待主控 agent 统一 review 与提交。
