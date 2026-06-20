# WI-4 Runtime delivery 与 context usage 统计

## Status

completed

## Goal

让模型投递、front/debug 展示和 context usage 统计从同一 ContextFrame section 契约派生。

## Scope

- 审计所有能产生非空 `rendered_text` 的 frame。
- 明确 system prompt assembly 与 turn-start notice delivery 的边界。
- 补齐 `context_usage_items_from_context_frame` 对模型可见 section 的覆盖。
- 将 audit-only 片段设为 audit scope。
- 收束 `runtime_injection_fragments` 的语义，避免成为第二个 bundle/turn delta 投递面。

## Primary Files

- `crates/agentdash-application/src/session/context_frame.rs`
- `crates/agentdash-application/src/session/launch/preparation.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-application/src/session/hook_injection_sink.rs`
- `crates/agentdash-contracts/src/runtime/session.rs`
- `crates/agentdash-executor/src/connectors/context_frame_render.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`

## Acceptance

- [x] 每类模型可见 frame 都产生 usage item。
- [x] system prompt 与 turn-start notice 的 frame domain 边界一致。
- [x] audit-only 内容不再通过 runtime agent scope 暴露。
- [x] usage 统计不漏算 CAP、skills、tools、agents 等 section。

## Implementation Notes

- `context_usage_items_from_context_frame` 覆盖 CAP delta sections：capability keys、tool paths、MCP servers、VFS delta、tool schema delta、skill delta、companion roster delta。
- CAP delta 的结构化能力面归入 `capability_state` usage bucket；工具 schema、MCP tools、skills、agents 继续使用各自显式 usage kind。
- `SkillDelta` 的 added / removed / changed entries 均参与 usage item 生成，和 section 的模型可见文本保持一致。
- `CompactionSummary` section 生成 `compaction_summary` usage item，并与 projection summary tokens 合并到同一 category。

## Validation

- `cargo test -p agentdash-contracts projection_view_aggregates_context_frame_usage_items --lib`
- `cargo test -p agentdash-application live_runtime_context_transition_derives_skill_dimension_from_active_vfs --lib`
- `cargo fmt --check`

## Remaining Follow-up

- WI-5 需要继续确认前端 ContextFrame parser / renderer 与后端 section 列表完全同步。
