# API Handler Thinning 设计

## 目标边界

本任务让 `agentdash-api` 回到 transport adapter：路由负责 extractor、鉴权上下文投影、调用 application use case、HTTP/JSON 映射。业务编排、跨 repository 组装、输入校验和可复用 query/command 逻辑进入 `agentdash-application`。

本任务不改变 HTTP endpoint、wire response shape、权限语义或 session launch 行为。所有变更都应保持 `cargo check --workspace` 可通过，并按批次缩小风险。

## 分层决策

- `agentdash-domain` 不依赖 contract/protocol DTO。
- `agentdash-application` 不依赖 `agentdash-api`、`AppState`、`ApiError`、`axum::Json` 或 contract response DTO。
- `agentdash-contracts` 拥有 wire/codegen DTO；API route 将 application/domain result 映射为 contract response。
- API-only adapter DTO 放 `agentdash-api/src/dto`，仅用于入口 request/query 或 route-local adapter，不留 inline route struct。
- Application use case 返回 `ApplicationError` 或局部结构化 error，并提供到 `ApplicationError` 的转换；API 只做 `ApiError::from`。

原因：contract/API/protocol 是外层边界，应依赖 domain/application 并转换；domain/application 返回 contract DTO 会把 wire contract 反向压进核心层。

## 现状复核

当前命中面比 PRD 中的目标 6 个 route 更广：

- `state.repos.*` / `repos.*` 在 `routes` 与 `session_use_cases` 中仍有多模块命中，其中 `session_use_cases/construction.rs` 15 处、`backends.rs` 8 处、`projects.rs` 10 处、`stories.rs` 9 处、`workflows.rs` 15 处。
- `Json<Value>` 在 route 中仍有多处，`acp_sessions.rs` 6 处、`workflows.rs` 6 处、`projects.rs` 3 处。
- route inline DTO 分布在 auth、backend、canvas、workspace、workflow、routine、acp session 等模块。
- `session_use_cases` 当前依赖 `AppState`、`ApiError`、`crate::auth`、`ApiVfsSurfaceRuntimeProjection`，不能整文件平移到 application。

## 直调保留规则

允许暂时保留在 API 的直调仅限：

- composition root / bootstrap 装配需要访问 concrete repository。
- 权限 extractor 或 auth adapter 需要把 HTTP 身份转换为 application input。
- route-local projection 必须读取 runtime registry、websocket registry 或 transport-only online state，且无 application 复用价值。
- 迁移批次之间的临时桥接点，必须写入 PRD 保留清单并带退出条件。

目标 route 的业务 CRUD、跨 repo 编排、默认值推导、slug/sort 规则、级联删除/clone/promote、session construction query 都不属于保留范围。

## 批次设计

### 1. Session Use Cases 迁移

先在 `agentdash-application::session` 建 application-owned use case 模块，定义 input/deps/result：

- deps 只持有 application/domain/SPI port、session service、VFS application service、extension runtime application service。
- API 提供 adapter，把 `AppState`、auth context、runtime projection adapter 转成 use case input。
- `construction` / `context_query` 返回 `ApplicationError` 或 session 局部 error，不返回 `ApiError`。
- `ApiVfsSurfaceRuntimeProjection` 的 API 依赖需要倒置为 application-facing trait 或由 API 只补充 transport projection，不能直接搬进 application。

原因：session construction 是核心 launch/query 事实源，后续 session assembly converge 也会复用；先把 transport 依赖拆掉，才能安全移动。

### 2. 低风险 CRUD 下沉

优先处理 `canvases`、`projects`、`stories` 中明确的 create/update/delete/clone/aggregate delete：

- application service 负责 repository 调用、领域校验、级联关系和返回 read model。
- API route 只保留鉴权、path/query/body 解析和 contract mapping。
- 每个 route 完成后更新 repo 直调保留清单。

原因：这些模块大多是同步 repository 编排，行为风险低，能快速建立薄 handler 范式。

### 3. LLM Providers 下沉

`agentdash-contracts::llm_provider` 已存在，适合单独做 service：

- provider CRUD、sort order、credential 加密、verification state 更新进入 application。
- OAuth/probe 中真正依赖 HTTP/provider adapter 的部分可先留 API adapter，并在保留清单标退出条件。

### 4. Backends 最后处理

`backends.rs` 混合 backend registry、runtime health、本机 runtime 默认配置和 DB facts，放在最后：

- application 表达 backend list/detail/update 的业务 read model。
- API adapter 拼接 transport-only online/relay state。
- local runtime bootstrap 或 health registry 不进入 domain。

### 5. DTO 与 Router 收尾

前四批稳定后再清 inline DTO、`Json<Value>` 和 `routes.rs`：

- wire DTO 进入 `agentdash-contracts`。
- API-only request/query 进入 `agentdash-api/src/dto`。
- 各资源 route 导出 `pub fn router(...)`，根 `routes.rs` 只组合 secured/unsecured router。

原因：DTO 与 router 机械整理应在行为下沉后做，避免同一 diff 同时混入移动、业务迁移和命名调整。

## 与其它 Wave2 Child 的协调

- `session-assembly-converge` 只处理 assembler/builder/resolver 内部结构；本任务只移动 API-bound use case 边界，不抽大型 resolver。
- `domain-purification` 负责 domain 的 codegen/schema 约束移除；本任务不能为了 DTO 单源让 domain/application 返回 contract DTO。
- `frontend-server-state-refactor` 消费 contracts 与 API shape；本任务保持 wire shape，不触发前端状态迁移。
