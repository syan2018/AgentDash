# 实施计划：Runtime 稳定性与执行准入小修

## 顺序提交计划

1. `refactor(relay): 细化本机后端命令错误分类`
   - 阅读 `crates/agentdash-api/src/relay/registry.rs` 与调用方。
   - 引入稳定错误类型或错误码。
   - 覆盖 offline、send failed、timeout、response dropped 测试。

2. `feat(routine): 后端不可用时记录跳过执行`
   - 阅读 `crates/agentdash-application/src/routine/executor.rs`、`workspace/resolution.rs`、Routine DTO/前端展示。
   - 明确 failed vs skipped 边界。
   - 补测试：backend/workspace 不在线时 execution 为 skipped，配置错误仍 failed。

3. `test(executor): 补充 provider adapter 行为矩阵`
   - 阅读 `crates/agentdash-executor/src/connectors`。
   - 先补 fixture/mock 可覆盖项。
   - 对暂不适合自动化的 poisoned/resume 场景沉淀 checklist。

## 验证命令

- `pnpm run backend:check`
- `cargo test -p agentdash-api relay::registry`
- `cargo test -p agentdash-application routine`
- `cargo test -p agentdash-executor`

如全量命令耗时过长，可先运行受影响 crate 的定向测试，再在收尾阶段补 `pnpm run backend:check`。

## 回滚点

- relay 错误类型改动若牵动过大，可先退回为内部 enum + `anyhow` 包装。
- Routine skipped 若影响既有 UI，可先保持 API 字段不变，仅改变 status/reason。
