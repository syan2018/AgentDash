# 优化上下文计算策略实施计划

## Checklist

- [x] 阅读相关 Trellis spec：agent protocol、executor、application session、agent compaction、frontend session。
- [x] 梳理当前 usage event 的 Rust 与 TypeScript 类型生成链路。
- [x] Milestone 1：设计并实现规范化 usage 数据结构，明确 provider context、pending estimate、current context、cumulative usage。
- [x] Milestone 1：更新 Codex bridge 的 token usage 映射，保留 total/last 语义。
- [x] Milestone 1：更新 session stream extraction 与前端 `TokenUsageInfo`。
- [x] Milestone 1：更新 `ContextUsageRing` 与 usage card 的字段来源和展示文案。
- [x] Milestone 2：集中后端 token estimate helper，替换 streaming、compaction、projection 中的重复估算。
- [x] Milestone 2：更新 compaction evaluation，使用 current context pressure 与 effective context window。
- [x] Milestone 3：在后端或 session model 中提供上下文构成 segments 与 details，避免前端重复计算 token。
- [x] Milestone 3：为上下文查看窗口补齐 Claude Code 对齐粒度：主分类、二级详情、message breakdown、top tools、top attachments。
- [x] Milestone 3：设计并实现上下文查看窗口的前端入口、窗口结构和状态展示。
- [x] 补充后端单元测试。
- [x] 补充前端模型与 UI 计算测试。
- [x] 验证没有新增 `/context` slash command 入口。
- [x] 根据实际改动更新 Trellis spec。
- [x] 运行 lint、type-check、测试与必要的前端验证。

## Key Files

- `crates/agentdash-agent-protocol/src/backbone/event.rs`
- `crates/agentdash-agent-protocol/src/compat/mod.rs`
- `crates/agentdash-executor/src/connectors/codex_bridge.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-agent/src/agent_loop/streaming.rs`
- `crates/agentdash-agent/src/compaction/mod.rs`
- `crates/agentdash-application/src/session/context_projector.rs`
- `packages/app-web/src/features/session/model/types.ts`
- `packages/app-web/src/features/session/model/useSessionStream.ts`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/session/ui/SessionUsageCard.tsx`
- `packages/app-web/src/features/session/ui/`

## Reference Evidence

- `references/codex/codex-rs/protocol/src/protocol.rs`
- `references/codex/codex-rs/tui/src/token_usage.rs`
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs`
- `references/claude-code/src/utils/context.ts`
- `references/claude-code/src/utils/tokens.ts`
- `references/claude-code/src/utils/analyzeContext.ts`
- `references/claude-code/src/services/compact/autoCompact.ts`
- `references/claude-code/src/commands/context/context-noninteractive.ts`

## Context Inspector Granularity

- Header metrics: current context, effective window, remaining, reserve, cumulative usage, pending estimate.
- Category table: system/developer, system tools, MCP tools, agents, memory, skills, messages, compaction summary, reserve, free space.
- Detail sections: system sections, tools, MCP tools, agents, memory files, skills.
- Message breakdown: user messages, assistant messages, tool calls, tool results, attachments.
- Top contributors: top tools with call/result split, top attachments by type.
- Deferred entries: visible with loaded/deferred state, excluded from context pressure when not loaded.
- Not in first implementation: per-message token audit.

## Risk Controls

- Usage total authority belongs to provider data when available; local category estimates explain composition but do not override provider totals.
- `currentContextTokens` must be the final pressure value used by UI and compaction; do not add `pendingEstimateTokens` again at call sites.
- Context inspector first version should keep detail sections optional when a source is unavailable, but the payload shape and source labels must be stable.
- If a category is estimated from schema or transcript rather than provider usage, mark it as `local_estimate`.
- Keep implementation milestones independently verifiable even if committed together.

## Validation Commands

- `cargo test`
- `pnpm test`
- `pnpm typecheck`
- `pnpm lint`

具体命令以仓库脚本为准，实施前需要确认 package scripts 与 Rust workspace 测试入口。

## Planning Question

首轮实施聚焦基础语义闭环和前端上下文查看窗口。窗口第一版对齐 Claude Code 的主分类与二级详情粒度，不做逐条消息 token 审计。
