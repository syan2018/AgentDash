# API Handler Thinning 执行计划

## 阶段 1：规划基线

1. 记录基线：
   - `rg --count "\bstate\.repos\.|\brepos\." crates/agentdash-api/src/routes crates/agentdash-api/src/session_use_cases`
   - `rg --count "Json<\s*serde_json::Value\s*>|Json<\s*Value\s*>" crates/agentdash-api/src/routes`
   - `rg --count "^\s*(pub\s+)?struct\s+\w+(Response|Request|Dto|Payload|Body|Query)" crates/agentdash-api/src/routes`
   - `rg -n "ApiError|AppState|crate::" crates/agentdash-api/src/session_use_cases`
   - `rg -n "agentdash_contracts|crate::rpc::ApiError|crate::app_state::AppState|axum::Json" crates/agentdash-application/src crates/agentdash-domain/src crates/agentdash-spi/src`
2. 更新 PRD 的 repo 直调保留清单格式，执行中每批填写具体保留项与退出条件。
3. 先运行 `cargo check -p agentdash-application -p agentdash-api` 获取迁移前基线。

## 阶段 2：Session Use Cases 迁移

1. 在 `agentdash-application/src/session/` 下建立 use case 模块，先迁无 HTTP extractor 依赖的 construction helper。
2. 抽出 `SessionConstructionUseCaseDeps` / `SessionContextQueryUseCaseDeps`，deps 只包含 application/domain/SPI 端口。
3. 将 `ApiError` 分支改为 `ApplicationError` 或 session 局部 error。
4. API 侧保留薄 adapter：从 `AppState` 和 auth extractor 构造 use case input/deps，并把错误映射为 `ApiError`。
5. 移除 `crates/agentdash-api/src/session_use_cases/`，并更新 imports。
6. 验证：
   - `Test-Path crates/agentdash-api/src/session_use_cases` = `False`
   - `rg "ApiError|AppState|crate::" crates/agentdash-application/src/session -n` 不命中新迁模块
   - `cargo check -p agentdash-application -p agentdash-api`

## 阶段 3：低风险 CRUD 下沉

1. `canvases`：promote/clone/delete/inline file append 等业务进入 application canvas service。
2. `projects`：create/update/delete/clone、grant/visibility 规则进入 application project service。
3. `stories`：story/task CRUD、级联删除、state change 组装进入 application story service。
4. 每个 route 迁完后更新 PRD repo 直调保留清单。
5. 验证：
   - `rg "\bstate\.repos\.|\brepos\." crates/agentdash-api/src/routes/{canvases,projects,stories}.rs -n`
   - `cargo check -p agentdash-application -p agentdash-api`

## 阶段 4：LLM Providers 下沉

1. 新建 application llm provider service，承接 provider CRUD、sort order、credential encryption/update、verification status。
2. API route 使用 existing `agentdash-contracts::llm_provider` response，保留 OAuth/probe adapter 边界并写清保留原因。
3. 验证：
   - `rg "\bstate\.repos\.|\brepos\." crates/agentdash-api/src/routes/llm_providers.rs -n`
   - `pnpm run contracts:check`
   - `cargo check -p agentdash-application -p agentdash-api`

## 阶段 5：Backends 下沉

1. 拆分 backend DB fact、runtime registry fact、health projection 的 ownership。
2. application 返回 backend read model；API adapter 只拼 transport-only online/local runtime projection。
3. 默认 backend config、scope/owner 规则下沉 application。
4. 验证：
   - `rg "\bstate\.repos\.|\brepos\." crates/agentdash-api/src/routes/backends.rs -n`
   - backend route 相关测试通过
   - `cargo check -p agentdash-application -p agentdash-api`

## 阶段 6：DTO 与 Router 收尾

1. 清 route inline DTO：
   - contract response/request 迁 `agentdash-contracts`
   - API-only query/body 迁 `agentdash-api/src/dto`
2. `Json<Value>` 改具名 struct，并统一 camelCase。
3. 每个 route module 导出 `pub fn router(...)`，根 `routes.rs` 只组合 secured/unsecured router。
4. 验证：
   - `rg "Json<\s*serde_json::Value\s*>|Json<\s*Value\s*>" crates/agentdash-api/src/routes -n` = 0
   - `rg "^\s*(pub\s+)?struct\s+\w+(Response|Request|Dto|Payload|Body|Query)" crates/agentdash-api/src/routes -n` = 0
   - `rg -c "pub fn router" crates/agentdash-api/src/routes`
   - `cargo check --workspace`
   - `pnpm run contracts:check`

## 回滚点

- Session use case 迁移独立提交；若 application deps 抽象过大，只保留 helper 下沉并延后删除 API adapter。
- CRUD route 每个资源独立提交；失败时回退单 route。
- DTO/router 收尾独立提交，避免与行为迁移混在一起。
