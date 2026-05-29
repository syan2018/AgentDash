# 错误模型统一执行计划

## 前置核验

1. 记录基线计数：
   - `rg "InvalidConfig\(.*to_string" crates/agentdash-infrastructure | measure`
   - `rg "ApiError::Internal\(.*to_string" crates/agentdash-api | measure`
   - `rg "looks_like_unique_violation|looks_like_skill_asset_unique_violation" crates/agentdash-api`
   - `rg "Result<[^>]*, *String>" <8 个 application 模块>`
2. 确认 `cargo check --workspace` 当前基线状态。

## 实施步骤

1. **错误骨架**
   - 扩展 `DomainError`。
   - 新增 `agentdash_application::error::ApplicationError` 并从 `lib.rs` 导出。
   - 增加 `From<DomainError>`、`From<ConnectorError>`、必要的 `From<std::io::Error>`。

2. **Postgres 语义映射**
   - 改 `persistence/postgres/mod.rs::db_err` / `sql_err_for`。
   - 用 `sqlx::Error::RowNotFound` 与 `DatabaseError::is_unique_violation()` 生成结构化错误。
   - 跑 `cargo check -p agentdash-infrastructure`，修 repository 编译问题。

3. **API 入口映射**
   - 给 `ApiError` 增加 `From<ApplicationError>`。
   - 更新 `From<DomainError>`，让 `Conflict/Forbidden/Database` 有明确 HTTP 映射。
   - 删除 unique violation 字符串嗅探及其测试，改成结构化错误测试。

4. **Handler fan-out**
   - 分 route 文件替换 `.map_err(|e| ApiError::Internal(e.to_string()))` 为 `?` 或 `ApiError::from`。
   - 每批后跑 `cargo check -p agentdash-api`。
   - 无法替换的豁免写回 PRD 豁免清单，并标注具体原因。

5. **Application `Result<_, String>` fan-out**
   - 先迁移 `mcp_preset/definition.rs`、`project/management.rs`、`routine/executor.rs`。
   - 再迁移 hooks/context/companion 的纯函数或服务入口。
   - 每个模块保持外部调用方编译通过，不引入字符串二次解析。

6. **最终收口**
   - 重跑 acceptance grep。
   - `cargo check --workspace`。
   - 相关 API / application 测试。

## 回滚点

- Step 1 后如果局部 error 类型冲突过大，只保留骨架并通过 `From` 桥接，不强删局部枚举。
- Step 4 handler 批量替换按文件提交/检查，失败时只回退当前 route 文件。

## 验证命令

```powershell
cargo check --workspace
rg "InvalidConfig\(.*to_string" crates/agentdash-infrastructure
rg "ApiError::Internal\(.*to_string" crates/agentdash-api
rg "looks_like_unique_violation|looks_like_skill_asset_unique_violation" crates/agentdash-api
rg "Result<[^>]*, *String>" crates/agentdash-application/src/routine/executor.rs crates/agentdash-application/src/project/management.rs crates/agentdash-application/src/companion/tools.rs crates/agentdash-application/src/companion/skill_projection.rs crates/agentdash-application/src/context/workspace_sources.rs crates/agentdash-application/src/hooks/provider.rs crates/agentdash-application/src/hooks/script_engine.rs crates/agentdash-application/src/mcp_preset/definition.rs
```
