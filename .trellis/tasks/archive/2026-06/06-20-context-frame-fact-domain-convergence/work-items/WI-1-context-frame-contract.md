# WI-1 ContextFrame 协议与事实域冻结

## Status

completed

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

- [x] `design.md` 中有最终 frame taxonomy。
- [x] 每个有效 section 都有明确事实域。
- [x] 无生产者 section 的删除/重定义决策已记录。
- [x] 后续 WI 可以按本工作项结果实施，不再重新定义协议。

## Decisions

- CAP frame kind 直接拆为 `capability_state_snapshot` 与 `capability_state_delta`。snapshot 用于 bootstrap / initial 全量能力闭包，delta 用于 live、pending 和 next-turn apply 的运行期变化。
- `ContextFrameSection::ToolSchema` full section 已删除；工具 schema 只通过 `tool_schema_delta` 随 CAP frame 投递，避免形成第二套完整工具事实面。
- `ContextFrameSection::HookInjection` 已删除；hook 产出的任务语义统一经 `HookInjection -> ContextFragment -> assignment_context` 投递，pending action 仍可携带 `RuntimeHookInjectionEntry` 作为 action 指令的一部分。
- `RUNTIME_AGENT_CONTEXT_SLOTS` alias 已删除；assignment slot 白名单只保留任务语义入口。
- 前端 ContextFrame parser 对未知 section 保留 `unknown_section` 诊断视图，保证协议漂移可见。

## Validation

- `pnpm --filter app-web test -- contextFrame ContextFrameCard`
- `pnpm --filter app-web run check`
- `cargo test -p agentdash-application runtime_context_transition --lib`
- `cargo test -p agentdash-application assignment_context_frame --lib`
- `cargo test -p agentdash-contracts projection_view_aggregates_context_frame_usage_items --lib`
- `cargo test -p agentdash-spi injections_included_in_data --lib`
