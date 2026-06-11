# FIX-001: workflow-orchestration 快速清理

## 模块

`workflow-orchestration`

## 来源

- `reviews/002-workflow-orchestration.md`
- worker: `019eb2a2-963e-7942-8a53-acdb1bd62b98`

## 更新

- 删除 `script_compiler.rs` 中 `human_gate_decision_port_mismatch` 不可达诊断。
- 删除 `WorkflowGraphCompileMode::LenientDiagnostics`，artifact edge 缺少 state exchange 统一为 blocking error。
- 修正 `complete_lifecycle_node` 注释和工具描述，不再宣称它是全局唯一推进路径，改为 Agent session 节点主动提交 terminal outcome。

## 涉及文件

- `crates/agentdash-application/src/workflow/orchestration/compiler.rs`
- `crates/agentdash-application/src/workflow/orchestration/script_compiler.rs`
- `crates/agentdash-application/src/workflow/tools/advance_node.rs`

## 验证

- `rg -n "LenientDiagnostics|human_gate_decision_port_mismatch|这是推进 lifecycle 的唯一方式|DAG 编排的唯一推进路径" crates/agentdash-application/src/workflow -g '*.rs'`：无结果。
- `cargo test -p agentdash-application workflow::orchestration`：31 passed，0 failed。
- 测试输出存在既有 `session::construction` dead_code warnings，与本次改动无关。

## Commit

`c079e519 refactor(workflow): 清理编排编译反常识分支`
