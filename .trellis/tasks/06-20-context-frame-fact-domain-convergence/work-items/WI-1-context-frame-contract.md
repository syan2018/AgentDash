# WI-1 ContextFrame 协议与事实域冻结

## Status

planned

## Goal

冻结 ContextFrame 的事实域分类、section 生命周期和模型投递规则，为后续实现提供唯一协议依据。

## Scope

- 审阅 `ContextFrame` / `ContextFrameSection` 当前枚举。
- 为每个有效 section 标注事实源、生产者、模型可见性、前端展示语义和 usage 归类。
- 决定 `capability_state_update` 是否拆为 snapshot/delta，或保留名称并加入明确 mode。
- 决定 `ContextFrameSection::HookInjection` 与 `ToolSchema` full section 的去留。
- 决定 `RUNTIME_AGENT_CONTEXT_SLOTS`、`bootstrap_context` 命名和 `bootstrap_fragments` 的目标命名。

## Primary Files

- `crates/agentdash-spi/src/hooks/mod.rs`
- `crates/agentdash-application/src/session/context_frame.rs`
- `crates/agentdash-application/src/session/launch/preparation.rs`
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
- `crates/agentdash-contracts/src/runtime/session.rs`

## Acceptance

- [ ] `design.md` 中有最终 frame taxonomy。
- [ ] 每个有效 section 都有明确事实域。
- [ ] 无生产者 section 的删除/重定义决策已记录。
- [ ] 后续 WI 可以按本工作项结果实施，不再重新定义协议。

