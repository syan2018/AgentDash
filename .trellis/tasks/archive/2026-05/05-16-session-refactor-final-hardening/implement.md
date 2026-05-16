# Session 重构最终收尾清洁实施计划

## Checklist

1. 读取相关 spec 与代码边界。
   - `.trellis/spec/backend/session/session-startup-pipeline.md`
   - `.trellis/spec/backend/session/runtime-execution-state.md`
   - `.trellis/spec/backend/session/execution-context-frames.md`
   - `.trellis/spec/frontend/state-management.md`
   - `.trellis/spec/frontend/type-safety.md`

2. 修复 terminal persist failure cleanup。
   - 修改 `crates/agentdash-application/src/session/turn_processor.rs`。
   - 增加/调整测试，模拟 terminal persist 失败后 active turn 被释放。

3. 修复 runtime command apply-once 提交失败路径。
   - 修改 `crates/agentdash-application/src/session/prompt_pipeline.rs`。
   - 必要时扩展 `SessionRuntimeCommandStore` 测试用 fake/memory 行为。
   - 确保 applied 标记失败不会留下 requested command 静默重复执行。

4. 统一 context query 与 launch projection。
   - 梳理 `crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs` 与 `session_context_query.rs`。
   - 抽出可共享 finalization/projection 函数。
   - 给 context endpoint 或 construction planner 添加一致性测试。

5. 正式处理 tab layout。
   - 新增 `SessionMeta.tab_layout`，Postgres/SQLite migration，repository 读写，API patch/get 支持，前端移除静默 catch。
   - 若实现中发现 layout 结构已有更合适类型，使用结构化类型而不是裸字符串。

6. 薄化和命名清理。
   - 清理 `SessionRequestAssembler` 的过时注释。
   - 检索 `request` / `prepared` / `augment` 等旧语义残留，只处理确认误导维护者的命名或注释。
   - 不做无关重命名风暴。

7. 验证。
   - `cargo check -p agentdash-application`
   - `cargo check -p agentdash-api`
   - `cargo test -p agentdash-application session::`
   - `cargo test -p agentdash-api session`
   - 前端相关测试：优先运行 session service / workspace tab store / affected tests。

## Risky Files

- `crates/agentdash-application/src/session/prompt_pipeline.rs`
- `crates/agentdash-application/src/session/turn_processor.rs`
- `crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs`
- `crates/agentdash-api/src/bootstrap/session_context_query.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/session_repository.rs`
- `packages/app-web/src/services/session.ts`
- `packages/app-web/src/stores/workspaceTabStore.ts`

## Review Gates

- 任何新增 fallback 都必须出现在 `ConstructionResolutionPlan` 或 launch trace 中。
- API route 不允许新增 owner/VFS/capability 业务组装。
- 不允许静默吞掉后端不支持的功能。
- connector accepted 前后副作用边界必须清晰：accepted 前失败释放 turn；accepted 后失败要有 terminal/runtime command 可审计状态。

## Open Decision Before Implementation

无。用户已确认 `tab_layout` 按正式落库支持推进。
