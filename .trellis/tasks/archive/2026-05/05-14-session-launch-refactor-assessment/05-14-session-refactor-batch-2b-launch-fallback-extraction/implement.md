# Implementation Plan：Batch 2b Fallback Extraction

## Steps

1. 阅读 `session/launch.rs` 与 `prompt_pipeline.rs`，确认当前 summary 缺口。
2. 扩展 `LaunchSummary` 与 `LaunchExecutionInput`，加入 VFS/MCP/capability/follow-up/restore/working-dir source。
3. 在 `prompt_pipeline.rs` 计算现有 fallback 时同步生成来源摘要。
4. 将 follow-up session id 解析提前到 `LaunchExecution` 输入之前，summary 记录来源。
5. 补 `session::launch` 单测覆盖来源摘要矩阵。
6. 回归 pending apply-once、connector setup failure、prompt_pipeline focused tests。

## Candidate Commands

```powershell
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application pending_capability_state_transition_applies_on_next_prompt_and_clears_meta
cargo test -p agentdash-application start_prompt_records_failed_terminal_when_connector_setup_fails
cargo test -p agentdash-application session::prompt_pipeline
cargo fmt --check
```

## Commit Plan

```text
refactor(session): 补全 LaunchExecution fallback 摘要

- 扩展 LaunchSummary 的 lifecycle、restore、follow-up、VFS/MCP/capability 来源字段。
- 让 prompt pipeline 在 connector prompt 前形成可审计 fallback summary。
- 保持入口 request 与运行时副作用边界不变。
```

## Exit Criteria

- Batch 3 gate 的主要阻塞解除：fallback 来源已能由 `LaunchExecution` summary 解释。
- `PromptSessionRequest` 仍可存在，但不再是 fallback 来源的唯一解释载体。
- 回归测试通过。

## Verification

```powershell
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application pending_capability_state_transition_applies_on_next_prompt_and_clears_meta
cargo test -p agentdash-application start_prompt_records_failed_terminal_when_connector_setup_fails
cargo test -p agentdash-application session::prompt_pipeline
cargo fmt --check
```

以上命令均通过。
