# 模型上下文构成统计真实化执行计划

## Checklist

- [x] 读取相关 spec 与代码入口：contract DTO、ContextProjector、session projection route、ExecutionContext/context frame 构造、SessionProjectionView。
- [x] 读取 `references/claude-code` 的 `/context`、`analyzeContextUsage`、token estimation、microcompact 相关实现，提炼可迁移设计。
- [x] 找到 `/context/projection` 查询时可读取的最新 session/frame/runtime projection 事实源，确认可稳定读取持久化 context frames、tool schemas、skills。
- [x] 在后端新增 context usage builder，使它以 compact-aware `AgentContextEnvelope` + 持久化 context frames 生成真实分类。
- [x] 新增 `SessionProjectionViewResponse::from_envelope_and_context_items`，使 `context_usage_analysis` 合并非消息 usage items 与 message segments。
- [x] 新增 `SessionContextUsageItemResponse`，同步生成 TS contract 并让前端继续只消费 generated DTO。
- [x] 更新 token estimate 合计逻辑，确保 header token estimate 不只包含 messages。
- [x] 增加 Rust focused tests，覆盖 system/context frame/tool/MCP/skill/memory 分桶不再是 `not_loaded`。
- [x] 运行 contract generator / check，更新 `packages/app-web/src/generated/session-contracts.ts`。
- [x] 调整 `SessionProjectionView` 测试样例，覆盖真实 source 展示。
- [x] 运行最小验证命令。

## Validation Commands

```powershell
pnpm run contracts:check
pnpm run frontend:check
cargo test -p agentdash-contracts projection_tests
cargo check -p agentdash-application -p agentdash-api
pnpm --filter app-web test SessionProjectionView
pnpm --filter app-web test sessionStreamReducer
```

如修改 application/session projector，再补：

```powershell
cargo test -p agentdash-application session::context_projector
```

## Risky Files

- `crates/agentdash-agent-types/src/model/projection.rs`
- `crates/agentdash-contracts/src/runtime/session.rs`
- `crates/agentdash-application/src/session/context_projector.rs`
- `crates/agentdash-api/src/routes/sessions.rs`
- `packages/app-web/src/generated/session-contracts.ts`
- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx`
- `packages/app-web/src/features/session/ui/SessionProjectionView.test.tsx`

## Review Gate Before Start

- 确认实现不依赖前端重算 token。
- 确认不会为了旧 shape 添加兼容字段。
- 确认如果没有 live/non-message projection，UI 显示的是真实 0 或缺省，而不是 `not_loaded` 占位。
- 确认 deferred tool/category 被显示为“可用但未加载”，且不计入实际上下文占用。
