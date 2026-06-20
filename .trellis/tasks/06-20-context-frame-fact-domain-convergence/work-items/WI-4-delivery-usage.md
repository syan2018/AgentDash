# WI-4 Runtime delivery 与 context usage 统计

## Status

planned

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

- [ ] 每类模型可见 frame 都产生 usage item。
- [ ] system prompt 与 turn-start notice 的 frame domain 边界一致。
- [ ] audit-only 内容不再通过 runtime agent scope 暴露。
- [ ] usage 统计不漏算 CAP、skills、tools、agents 等 section。

