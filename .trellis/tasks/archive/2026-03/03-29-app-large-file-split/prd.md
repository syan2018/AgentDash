# Application 层巨型文件拆分

## Goal

将 `agentdash-application` 中两个超大文件 `hooks/mod.rs`（1719 行）和 `session/hub.rs`（1486 行）拆分为职责明确的子模块，改善日常开发体验、降低合并冲突概率，不改变任何外部接口。

## 背景

上一轮 `app-contract-layering-refactor` 已完成分层架构修正，但部分文件因迁入合并导致体量过大：
- `hooks/mod.rs` 混合了 Provider 实现、Workflow contribution 构建、Completion 决策、工具函数和 ~800 行测试
- `session/hub.rs` 已从 2148 行降至 1486 行（拆出 hub_support/session_store/types），但 prompt 执行管线和 hook trigger 逻辑仍在单一文件中

## Requirements

### R1: hooks/mod.rs 拆分（1719 行 -> mod.rs ~100 行 + 子文件）

将 `crates/agentdash-application/src/hooks/mod.rs` 拆分为：

| 目标文件 | 内容 |
|---------|------|
| `hooks/provider.rs` | `AppExecutionHookProvider` struct 定义 + `ExecutionHookProvider` trait impl（load_session_snapshot / refresh / evaluate_hook / advance / append_execution_log） |
| `hooks/workflow_contribution.rs` | `build_workflow_step_fragments`、`build_workflow_policies`、`build_step_summary_markdown`、`build_instruction_injection_markdown` |
| `hooks/completion.rs` | `apply_completion_decision` 方法及 `ActiveWorkflowLocator`、`ActiveWorkflowChecklistEvidenceSummary`、`build_completion_record_artifacts_from_snapshot` |
| `hooks/helpers.rs` | `shell_exec_rewritten_args`、`build_subagent_result_context`、`SubagentResult`、`tool_call_failed`、`is_update_task_status_tool`、`is_report_workflow_artifact_tool`、`absolutize_cwd_to_workspace_relative`、`normalize_path_display_for_hook`、`extract_tool_arg`、`extract_payload_str`、`extract_payload_string_list` |
| `hooks/mod.rs` | 仅保留 `pub mod` 声明、`pub use` re-exports、以及少量共享 helper（`global_builtin_sources`、`dedupe_tags`、`merge_hook_contribution`、`source_summary_from_refs` 等） |

- 测试代码随逻辑移入各自的子模块（`#[cfg(test)] mod tests`）
- 已有的 `hooks/rules.rs`、`hooks/owner_resolver.rs`、`hooks/snapshot_helpers.rs`、`hooks/workflow_snapshot.rs` 保持不动

### R2: session/hub.rs 拆分（1486 行 -> hub.rs ~800 行 + 子文件）

将 `crates/agentdash-application/src/session/hub.rs` 中以下逻辑拆出：

| 目标文件 | 内容 |
|---------|------|
| `session/prompt_pipeline.rs` | `start_prompt` 的内部流处理循环（消费 `ExecutionStream`、发射 notification、记录 session meta） |
| `session/event_bridge.rs` | `emit_session_hook_trigger`、`inject_notification`、hook trigger 相关的辅助逻辑 |

- `hub.rs` 保留 `SessionHub` struct 定义、公共 API 方法签名、session 创建/恢复/列表等非流处理逻辑
- 已有的 `hub_support.rs`、`session_store.rs`、`types.rs`、`hook_runtime.rs`、`hook_delegate.rs`、`hook_events.rs` 保持不动

## Acceptance Criteria

- [ ] `cargo check --workspace` 通过
- [ ] `cargo clippy --workspace -- -D warnings` 通过（排除预存 lint）
- [ ] `cargo test --workspace` 全部通过，测试数量不减少
- [ ] `hooks/mod.rs` 行数 <= 200（不含空行和注释）
- [ ] `session/hub.rs` 行数 <= 900
- [ ] 无外部接口变更（所有 pub 类型和函数的路径通过 re-export 保持兼容）
- [ ] 无逻辑变更（纯文件组织重构）

## Technical Notes

- 使用 `pub(super)` 或 `pub(crate)` 控制新模块的可见性，仅通过 `mod.rs` re-export 需要暴露的符号
- 测试中使用的 helper（如 `snapshot_with_workflow`）可以放在独立的 `hooks/test_helpers.rs` 或直接在对应模块的 test block 中
- hub.rs 拆分需要注意 `sessions: Arc<Mutex<HashMap<...>>>` 的跨文件共享，可通过传递 `&self` 方法或 `pub(super)` 字段解决

## Risk

- 中等：需要仔细处理模块可见性（pub/pub(crate)/pub(super)），确保内部依赖不被意外暴露
- 无行为变更，全部为文件组织重构
