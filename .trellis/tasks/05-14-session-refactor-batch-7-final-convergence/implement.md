# Implementation Plan：Batch 7 Final Convergence

## Ordered Steps

- [x] 清理旧字段/旧类型 grep 与文档漂移。
- [x] 更新 `.trellis/spec/backend/session/*`。
- [x] 收紧 `working_dir` 路径策略与测试。
- [x] 清理旧 pending meta 持久化映射。
- [x] 拆出 session persistence store 能力边界。
- [x] 收窄 AppState / SessionHub ready 初始化边界。
- [x] 更新 parent task notes，记录当前状态与剩余真实风险。
- [ ] 继续删除 `SessionLaunchPlan` 跨 crate handoff 与 bootstrap 输出。
- [x] 将 `bootstrap/session_context_query.rs` 与 launch construction planner 合流。
  - Task / Story / Project 的 VFS、capability、context snapshot projection 已迁入 `SessionConstructionPlanner`。
  - API 侧仅保留权限校验、session meta 读取、DTO 投影与 `runtime_surface` 展示态补全。
- [ ] 将 `SessionHub` 业务方法拆到 construction / launch / runtime / effects / pending 服务，删除有职责 facade。
- [ ] 运行最终验证矩阵。
- [ ] 标记 Batch 7 与 parent task 完成。
- [ ] 整理提交历史、force-push 并更新 PR。

## Final Validation

- `cargo fmt --check`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-infrastructure`
- `cargo check -p agentdash-local`
- `cargo test -p agentdash-application session::hub`
- `cargo test -p agentdash-application session::terminal_effects`
- `cargo test -p agentdash-application session::runtime_commands`
- `cargo test -p agentdash-application session::memory_persistence`
- `cargo test -p agentdash-infrastructure terminal_effect_outbox_persists_status_transitions`
- `cargo test -p agentdash-application session::path_policy`
- `rg -n "PromptSessionRequest|SessionLaunchIntent|has_live_runtime" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src`
- `rg -n "pending_capability_state_transitions" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src`
- `rg -n "pending_capability_state_transitions_json" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-infrastructure/src`
- `rg -n "execute_effects|on_session_terminal|request_hook_auto_resume" crates/agentdash-application/src/session/turn_processor.rs`
- `git diff --check`

Known warning: `crates/agentdash-application/src/canvas/management.rs` still has pre-existing unused import `CANVAS_SYSTEM_RUNTIME_BRIDGE_REFERENCE_PATH`.

## Exit Criteria

- 分支可 review。
- 本轮已执行 batch 的事实、验证和剩余风险都在 Trellis task 中可追溯。
- 只要 `SessionLaunchPlan`、`SessionHub` 或 context query 仍是生产主线差池，本 Batch 只能保持 `in_progress`。
