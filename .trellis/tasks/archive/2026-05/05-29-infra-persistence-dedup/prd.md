# infra 持久化层 sqlite/postgres 去重

> 病灶 5。事实源：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`。
> 依赖：`drop-step-lifecycle` 之后（step 删除 / kind 列已定，避免对将删表做去重）。

## Scope
`crates/agentdash-infrastructure/src/persistence/`。消除 sqlite/postgres session_repository 82% 逐行重复，并把混入 repo 的业务规则上移。

## 证据
- `sqlite/session_repository.rs`(3329) 与 `postgres/session_repository.rs`(2725) 约 82% 重复：`*_from_row`、辅助函数(`json_string`/`encode_u64_as_i64`/`validate_commit_session`/`source_range_pair`/`SessionProjection` 等约 600 行)逐字复制；差异仅占位符(`?` vs `$n`)、`RETURNING` 支持、`ANY` 批量、`initialize()` DDL。
- 全表扫后 Rust 侧过滤：`list_terminal_effects_by_status`/`list_runtime_commands_by_status`（sqlite:1209/1348, postgres:942/1077），有索引未用。
- `workflow_repository.rs` `install_workflow_template_bundle`(L283-544) 含业务规则（overwrite 语义/key 冲突/版本递增）。

## Approach
1. 抽 `persistence/session_core.rs`：所有 `*_from_row`（泛型 `sqlx::Row` bound）、辅助函数、`validate_commit_session` 等纯逻辑共享。两实现仅保留 pool/`initialize()`/SQL 字符串/方言差异。
   - 备选（更激进，待定）：若 sqlite 路径不完整（无对应 workflow/agent 等 sqlite 实现），评估直接删 sqlite 双实现。**默认走共享层方案**，删 sqlite 需先确认 sqlite 是否仍是开发/测试依赖。
2. status 查询下推 SQL（`status = ANY($1)` / `IN`），用上索引。
3. `install_workflow_template_bundle` 的 overwrite/key 校验上移 application service，repo 只留 CRUD。
4. sqlite `append_event` 用 `RETURNING` 消除二次 SELECT。

## Acceptance
- [ ] session_repository 重复显著下降（共享 `session_core` 或删 sqlite）
- [ ] status 查询走 SQL 过滤
- [ ] `cargo check --workspace` 通过；持久化相关测试通过

## Constraints
- 仅改 `crates/agentdash-infrastructure/`（+ 必要时 application service 接收上移的业务规则）。**不要 git commit**，orchestrator gate 后提交。
- 改表走 migration。
