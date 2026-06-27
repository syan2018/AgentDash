# Research: backend-layer-topology

- Query: 盘查后端 crate/分层骨架的主链路拓扑与耦合点，覆盖 Cargo workspace、agentdash-api / application / domain / infrastructure / spi / contracts / executor / agent / agent-types / agent-protocol 的依赖方向、模块边界、composition root、repository/port/DTO/contract 关系。
- Scope: internal
- Date: 2026-06-21

## Findings

### 文件发现

| Path | Description |
| --- | --- |
| `Cargo.toml` | workspace 成员与内部 crate path dependency 总入口。 |
| `crates/agentdash-api/src/app_state.rs` | API composition root，高层装配 repository、relay、VFS、session、runtime gateway 与 background workers。 |
| `crates/agentdash-api/src/bootstrap/*.rs` | AppState 装配切片，包含 repositories / relay / VFS / session / runtime gateway / workers。 |
| `crates/agentdash-api/src/routes.rs` | HTTP router composition root，合并各资源 route。 |
| `crates/agentdash-api/src/routes/*.rs` | HTTP endpoint handlers，承担鉴权、DTO 映射，并广泛调用 application use case / RepositorySet。 |
| `crates/agentdash-application/src/lib.rs` | application 模块索引，承载 use case、runtime 编排、服务和 read model。 |
| `crates/agentdash-application/src/repository_set.rs` | 聚合 domain repository ports 的 `RepositorySet`。 |
| `crates/agentdash-application-ports/src/*` | application 消费、API/local 实现的 transport/runtime port。 |
| `crates/agentdash-domain/src/lib.rs` | domain aggregate、值对象、repository trait 与领域错误索引。 |
| `crates/agentdash-infrastructure/src/lib.rs` | PostgreSQL/SQLite adapter、script/storage/secret/function runner 实现导出。 |
| `crates/agentdash-spi/src/lib.rs` | connector、hooks、session persistence、runtime capability、platform tool 等 SPI port。 |
| `crates/agentdash-contracts/src/lib.rs` | browser-facing wire DTO 与 TypeScript generation 的 contract crate。 |
| `crates/agentdash-executor/src/lib.rs` | connector、Codex/Pi Agent bridge、MCP discovery/relay adapter。 |
| `crates/agentdash-agent/src/lib.rs` | Agent loop engine、bridge trait、tool registry 和 event stream。 |
| `crates/agentdash-agent-types/src/lib.rs` | Agent shared model/runtime/protocol value types。 |
| `crates/agentdash-agent-protocol/src/lib.rs` | Backbone envelope/event、Codex user input adapter 和 ACP block re-export。 |

### 相关规范

- `.trellis/spec/backend/architecture.md`: 后端依赖方向是 `Interface -> Application -> Domain/SPI`，Infrastructure 实现 Domain/SPI port，不依赖 application 编排层；API composition root 在 `agentdash-api`。
- `.trellis/spec/backend/directory-structure.md`: crate 分层基线与添加新模块步骤；`agentdash-application-ports` 是 API/local 实现、application 消费的纯 port。
- `.trellis/spec/backend/repository-pattern.md`: domain 定义 aggregate repository port，infrastructure 提供 `Postgres*Repository`，application 通过 `RepositorySet` 编排 port。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: `agentdash-contracts` 是业务 HTTP DTO / stream envelope / generated TS wire source；route-local DTO 只用于极小 transport wrapper。
- `.trellis/tasks/06-21-module-topology-coupling-review/design.md`: 本 slice 输出 `research/01-backend-layer-topology.md`，后续用耦合矩阵和 backlog 汇总。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md`: 已覆盖 Lifecycle、Session/AgentRun、VFS/Local/Extension、Frontend/Contracts/Permission 的局部过厚与重复事实源问题。

### 1. 模块/子模块清单与一句话职责

| Module | Responsibility | Main files |
| --- | --- | --- |
| `agentdash-api` | Interface layer 与 API composition root：HTTP route、DTO/error mapping、AppState 装配、auth/runtime adapter。 | `crates/agentdash-api/src/lib.rs:2`, `crates/agentdash-api/src/app_state.rs:125`, `crates/agentdash-api/src/routes.rs:52` |
| `agentdash-api::bootstrap` | API 宿主装配切片：Postgres repository set、relay runtime、VFS kernel、session runtime、runtime gateway、background workers。 | `crates/agentdash-api/src/bootstrap/repositories.rs:38`, `crates/agentdash-api/src/bootstrap/repositories.rs:131`, `crates/agentdash-api/src/app_state.rs:149` |
| `agentdash-api::routes` | 资源级 endpoint table；每个 route module 输出 `router()`，根 router 只合并 secured/public routers。 | `crates/agentdash-api/src/routes.rs:2`, `crates/agentdash-api/src/routes.rs:71`, `crates/agentdash-api/src/routes.rs:118` |
| `agentdash-application` | 用例编排与 read model：project/story/task/session/workflow/VFS/capability/runtime 等应用服务。 | `crates/agentdash-application/src/lib.rs:1`, `crates/agentdash-application/src/lib.rs:30`, `crates/agentdash-application/src/lib.rs:39` |
| `agentdash-application::repository_set` | application-level repository port bundle，持有所有 domain repository trait object 与 runtime creator。 | `crates/agentdash-application/src/repository_set.rs:44`, `crates/agentdash-application/src/repository_set.rs:45`, `crates/agentdash-application/src/repository_set.rs:83` |
| `agentdash-application-ports` | application 依赖的 transport/runtime port，供 API/local/executor composition root 实现或适配。 | `crates/agentdash-application-ports/Cargo.toml:8`, `crates/agentdash-application-ports/Cargo.toml:12` |
| `agentdash-domain` | 领域实体、值对象、repository trait、领域错误、授权规则等核心业务事实。 | `crates/agentdash-domain/src/lib.rs:1`, `crates/agentdash-domain/src/lib.rs:24`, `crates/agentdash-domain/src/lib.rs:29` |
| `agentdash-infrastructure` | Postgres/SQLite persistence adapter、migration readiness、script runtime、secret、storage、function runner。 | `crates/agentdash-infrastructure/src/lib.rs:1`, `crates/agentdash-infrastructure/src/lib.rs:16`, `crates/agentdash-infrastructure/src/lib.rs:55` |
| `agentdash-spi` | 跨 crate SPI：connector/hook/session persistence/platform/tool capability/runtime context port。 | `crates/agentdash-spi/src/lib.rs:1`, `crates/agentdash-spi/src/lib.rs:6`, `crates/agentdash-spi/src/lib.rs:94` |
| `agentdash-contracts` | Rust wire DTO 与 TS generation source，覆盖业务 HTTP DTO、stream envelope、shared enums。 | `crates/agentdash-contracts/src/lib.rs:2`, `crates/agentdash-contracts/src/lib.rs:30`, `crates/agentdash-contracts/src/lib.rs:43` |
| `agentdash-executor` | Connector/LLM bridge/MCP adapter 实现层，连接 Codex/Pi Agent 与 SPI/application ports。 | `crates/agentdash-executor/src/lib.rs:1`, `crates/agentdash-executor/src/lib.rs:5`, `crates/agentdash-executor/Cargo.toml:34` |
| `agentdash-agent` | 纯 Agent loop engine，提供 loop、bridge trait、tool registry、event stream。 | `crates/agentdash-agent/src/lib.rs:1`, `crates/agentdash-agent/src/lib.rs:9`, `crates/agentdash-agent/Cargo.toml:9` |
| `agentdash-agent-types` | Agent model/protocol/runtime shared types，不承担编排和持久化。 | `crates/agentdash-agent-types/src/lib.rs:1`, `crates/agentdash-agent-types/src/lib.rs:30` |
| `agentdash-agent-protocol` | Backbone protocol 与外部协议 adapter，重导出 Codex protocol 与 agent-types block。 | `crates/agentdash-agent-protocol/src/lib.rs:1`, `crates/agentdash-agent-protocol/src/lib.rs:24`, `crates/agentdash-agent-protocol/src/lib.rs:26` |

### 2. 主链路拓扑：入口 -> application use case -> domain/SPI -> infrastructure/contract

#### 2.1 Cloud HTTP command/query 主链路

1. `agentdash-api::routes` 组织 endpoint：根 router 在 `create_router()` 中合并资源 router，证据见 `crates/agentdash-api/src/routes.rs:52` 与 `crates/agentdash-api/src/routes.rs:71-118`。
2. route handler 从 extractor / `AppState` 取得身份、DTO、`RepositorySet`、runtime service handle 或 transport adapter。典型证据：`projects.rs` 调 `load_project_detail_facts(&state.repos, project_id)`，见 `crates/agentdash-api/src/routes/projects.rs:116`；`mcp_presets.rs` 直接构造 `McpPresetService::new(state.repos.mcp_preset_repo.as_ref())`，见 `crates/agentdash-api/src/routes/mcp_presets.rs:82`。
3. application use case 消费 `&RepositorySet` 或具体 repository trait object，编排 domain aggregate、domain service、SPI runtime surface。`RepositorySet` 持有 `Arc<dyn ProjectRepository>` 等 domain port，见 `crates/agentdash-application/src/repository_set.rs:45-83`。
4. domain 层提供 repository trait、entity/value object 与 domain error；infrastructure 提供具体 Postgres adapter 并在 API bootstrap 中实例化，见 `crates/agentdash-api/src/bootstrap/repositories.rs:13-27` 与 `crates/agentdash-api/src/bootstrap/repositories.rs:46-129`。
5. API route 将 application/domain/internal model 映射成 `agentdash-contracts` 或 API-local DTO；cross-feature/front-end consumed DTO 预期来自 `agentdash-contracts`，见 `.trellis/spec/cross-layer/frontend-backend-contracts.md`。

#### 2.2 Repository / persistence 装配链路

1. `AppState::new_with_plugins` 调用 repository bootstrap，见 `crates/agentdash-api/src/app_state.rs:149`。
2. `build_repositories()` 先做 schema readiness check，见 `crates/agentdash-api/src/bootstrap/repositories.rs:38` 与 `crates/agentdash-api/src/bootstrap/repositories.rs:42`。
3. 同一 bootstrap 模块 new 出所有 `Postgres*Repository` 与 storage adapter，见 `crates/agentdash-api/src/bootstrap/repositories.rs:46-129`、`crates/agentdash-api/src/bootstrap/repositories.rs:192`。
4. bootstrap 将具体实现聚合进 `RepositorySet`，见 `crates/agentdash-api/src/bootstrap/repositories.rs:131`；`AppState` 持有 `pub repos: RepositorySet`，见 `crates/agentdash-api/src/app_state.rs:126`。
5. application services 接收 `RepositorySet` 或 trait object；因此主业务持久化依赖方向是 API composition root -> infrastructure concrete -> application `RepositorySet` -> domain repository trait。

#### 2.3 Runtime / SPI / executor 装配链路

1. API bootstrap 构建 relay runtime、VFS kernel、session runtime、runtime gateway；高层装配顺序集中在 `AppState::new_with_plugins`，见 `crates/agentdash-api/src/app_state.rs:167`, `crates/agentdash-api/src/app_state.rs:178`, `crates/agentdash-api/src/app_state.rs:194`, `crates/agentdash-api/src/app_state.rs:231`。
2. `agentdash-spi` 定义 connector/hook/session persistence/platform capability 等 port，见 `crates/agentdash-spi/src/lib.rs:1-7`、`crates/agentdash-spi/src/lib.rs:94-127`。
3. `agentdash-executor` 依赖 `agentdash-spi`、`agentdash-application-ports`、`agentdash-domain`、`agentdash-agent-types`、`agentdash-agent-protocol`，并以可选 feature 引入 `agentdash-agent`，见 `crates/agentdash-executor/Cargo.toml:8-12`, `crates/agentdash-executor/Cargo.toml:26`, `crates/agentdash-executor/Cargo.toml:34`。
4. `agentdash-agent` 依赖 `agentdash-agent-types` 与少量 domain enum，见 `crates/agentdash-agent/Cargo.toml:9-11`；`agentdash-agent-protocol` 依赖 `agentdash-agent-types` 与 Codex/ACP external protocol，见 `crates/agentdash-agent-protocol/Cargo.toml:9-12`。
5. 实际 cloud runtime 组合点仍在 API/AppState，而 local/desktop 组合点属于其他 slice；本报告只标注 executor/agent crate 依赖方向，不深挖 Session/AgentRun runtime 细节。

#### 2.4 Contract / frontend generation 链路

1. contract crate 按 domain 拆 DTO module，见 `crates/agentdash-contracts/src/lib.rs:2-43`。
2. contract generation binary 位于 `crates/agentdash-contracts/src/generate_ts.rs`，crate manifest 声明 `generate_contracts_ts`，见 `crates/agentdash-contracts/Cargo.toml:9-11`。
3. contract crate 当前依赖 `agentdash-domain`、`agentdash-spi`、`agentdash-agent-protocol`、`agentdash-agent-types`，见 `crates/agentdash-contracts/Cargo.toml:15-18`。
4. API route 与 API dto module 使用 contract DTO，例：`agentdash-api/src/dto/mod.rs` re-export project/story/task/workspace contract DTO，见 `crates/agentdash-api/src/dto/mod.rs:28-34`；route 模块也直接 import contract DTO，例如 `crates/agentdash-api/src/routes/workspace_module.rs:22`。

### 3. 与其它模块的耦合点：文件/目录级证据

| Coupling | From | To | Relationship | Evidence | Risk |
| --- | --- | --- | --- | --- | --- |
| API composition root 同时依赖内外层 concrete | `agentdash-api` | application/domain/SPI/contracts/infrastructure/executor/agent/relay/mcp | 这是有意的宿主装配耦合；API crate manifest 依赖几乎所有后端 crate。 | `crates/agentdash-api/Cargo.toml:16-25`, `crates/agentdash-api/Cargo.toml:54-56`; `crates/agentdash-api/src/app_state.rs:149-231` | 合理但高风险：如果 route handler 也承担业务编排，composition root 边界会向 route 扩散。 |
| AppState bootstrap 直接实例化大量 infrastructure adapter | `agentdash-api::bootstrap::repositories` | `agentdash-infrastructure` | 具体 Postgres adapter 集中在 API 装配模块 new 出并注入 `RepositorySet`。 | `crates/agentdash-api/src/bootstrap/repositories.rs:13-27`, `crates/agentdash-api/src/bootstrap/repositories.rs:46-129`, `crates/agentdash-api/src/bootstrap/repositories.rs:131` | 符合 composition root；下一轮应看 bootstrap 是否只装配、不承载业务/query helper。 |
| API routes 广泛直接触达 `state.repos` | `agentdash-api::routes` | `agentdash-application::RepositorySet` / domain repository traits | route 不只调用 use case，也直接调用 repo 或用 repo 构造 service。 | `crates/agentdash-api/src/routes/backend_access.rs:115`, `crates/agentdash-api/src/routes/backends.rs:85`, `crates/agentdash-api/src/routes/routines.rs:129`, `crates/agentdash-api/src/routes/project_vfs_mounts.rs:121`, `crates/agentdash-api/src/routes/mcp_presets.rs:82` | P1 候选：需要区分薄 CRUD/DTO route 与跨聚合编排 route；跨聚合编排留在 route 会削弱 application 边界。 |
| `RepositorySet` 规模过大且被 API 与 application 广泛共享 | `agentdash-application::repository_set` | domain repository ports | 单个 struct 汇聚 project/story/workflow/session/runtime/mailbox/permission 等全部 repository trait object。 | `crates/agentdash-application/src/repository_set.rs:44-83` | P1 候选：作为 application-wide port bundle 符合 spec，但会让 use case 轻易拿到全仓库能力；需审查是否已有按模块窄依赖包。 |
| application 主依赖 `agentdash-contracts` | `agentdash-application` | `agentdash-contracts` | application manifest 直接依赖 contract crate；多处 application module 使用 contract DTO。 | `crates/agentdash-application/Cargo.toml:8-12`; `crates/agentdash-application/src/agent_run/conversation_snapshot.rs:3`, `crates/agentdash-application/src/agent_run/workspace/query.rs:1`, `crates/agentdash-application/src/capability/tool_catalog.rs:6`, `crates/agentdash-application/src/session/eventing.rs:8` | P0/P1 候选：规范把 API route 映射为 contract DTO，但 application 内部直接返回/消费 wire DTO 会让 use case 与 browser contract 同步演进。 |
| contract crate 依赖 domain/SPI/protocol 并内置转换 | `agentdash-contracts` | `agentdash-domain`, `agentdash-spi`, `agentdash-agent-protocol` | DTO crate 不是纯 wire shape；它 import domain/SPI type 并实现 `From<domain>`。 | `crates/agentdash-contracts/Cargo.toml:15-18`; `crates/agentdash-contracts/src/workspace/contract.rs:15`, `crates/agentdash-contracts/src/context/contract.rs:15`, `crates/agentdash-contracts/src/runtime/session.rs:11` | P1 候选：这能减少 API mapper 重复，但也使 contract crate 成为 domain/SPI 的外层 adapter；需确认是否符合“Domain 不依赖 contracts”的单向目标。 |
| application ports 依赖 relay/SPI/protocol/domain | `agentdash-application-ports` | relay/SPI/agent-protocol/domain | transport port crate 位于 application 边界，但依赖多种 runtime/wire type。 | `crates/agentdash-application-ports/Cargo.toml:8-12` | P1 候选：这是 spec 允许的边界 crate，但需检查 port 是否保持纯 trait/轻 DTO，还是承载 relay wire 事实。 |
| executor 依赖 application-ports 与 SPI，同时可选依赖 agent engine | `agentdash-executor` | application-ports/SPI/domain/agent/agent-protocol | executor 作为 runtime adapter 同时面向 application transport port 与 agent loop。 | `crates/agentdash-executor/Cargo.toml:8-12`, `crates/agentdash-executor/Cargo.toml:26`, `crates/agentdash-executor/Cargo.toml:34`, `crates/agentdash-executor/src/lib.rs:1-11` | P2 候选：方向大体正确；下一轮只需确认 executor 没有成为 application 编排事实源。 |
| SPI 依赖 domain 和 agent-protocol | `agentdash-spi` | `agentdash-domain`, `agentdash-agent-protocol`, `agentdash-agent-types` | SPI port re-export domain common types、agent types、session persistence protocol records。 | `crates/agentdash-spi/Cargo.toml:9-10`, `crates/agentdash-spi/Cargo.toml:25`; `crates/agentdash-spi/src/lib.rs:11`, `crates/agentdash-spi/src/lib.rs:29`, `crates/agentdash-spi/src/lib.rs:127` | P2 候选：符合 runtime port 共享，但 SPI 变大后可能吸收领域 DTO 和 wire DTO，需要按 port cluster 保持窄。 |
| `agentdash-agent` 依赖 `agentdash-domain` | `agentdash-agent` | `agentdash-domain` | Agent loop engine 为 `ThinkingLevel` 等 domain enum 依赖 domain。 | `crates/agentdash-agent/Cargo.toml:9-11`; `crates/agentdash-agent/src/types.rs:18` | P2 候选：规范已注明 Agent 子系统独立于主分层；这个 domain enum 依赖是小耦合，但可后续确认是否应下沉到 agent-types/SPI。 |
| infrastructure 依赖 agent-protocol 与 SPI | `agentdash-infrastructure` | `agentdash-spi`, `agentdash-agent-protocol` | persistence adapter 需要 session event / runtime command / hook records；script/storage adapter 实现 SPI。 | `crates/agentdash-infrastructure/Cargo.toml:8-10`; `crates/agentdash-infrastructure/src/persistence/session_core.rs:6`, `crates/agentdash-infrastructure/src/function_runner.rs:9`, `crates/agentdash-infrastructure/src/storage/extension_package_artifact_fs.rs:3` | P2 候选：符合 repository-pattern 中 session persistence 不经 `RepositorySet` 的例外；不在本轮深挖 Session runtime。 |

### 代码模式

- 正向模式：API composition root 装配 concrete adapter，application 持有 trait object bundle。证据：`AppState` 持有 `RepositorySet` (`crates/agentdash-api/src/app_state.rs:126`)，repository bootstrap 实例化 Postgres adapter (`crates/agentdash-api/src/bootstrap/repositories.rs:46-129`) 并填充 `RepositorySet` (`crates/agentdash-api/src/bootstrap/repositories.rs:131`)。
- 正向模式：domain 不依赖 contracts/application/infrastructure；domain crate manifest 没有内部 workspace business dependency，domain lib 只导出业务模块和 `DomainError`，见 `crates/agentdash-domain/src/lib.rs:1-29`。
- 风险模式：route handler 直接拿 `state.repos.*` 做跨聚合或 repository 操作，而不是始终经 application use case。证据：`crates/agentdash-api/src/routes/backend_access.rs:115`, `crates/agentdash-api/src/routes/project_vfs_mounts.rs:121`, `crates/agentdash-api/src/routes/routines.rs:129`。
- 风险模式：application 直接 import `agentdash_contracts::*`，使 application read model / command policy / session eventing 接近 wire DTO 层。证据：`crates/agentdash-application/src/agent_run/workspace/query.rs:1`, `crates/agentdash-application/src/session/eventing.rs:8`。
- 风险模式：contract crate 直接从 domain/SPI 构造 DTO，减少 route mapper 但扩大 contract crate 对内部模型变化的敏感度。证据：`crates/agentdash-contracts/src/workspace/contract.rs:101`, `crates/agentdash-contracts/src/runtime/session.rs:11-16`。

### 4. 值得下一轮深挖的 review 问题

| Priority | Question | Why now | Suggested reviewer scope |
| --- | --- | --- | --- |
| P0 | application 是否应直接依赖 `agentdash-contracts`，还是 contract mapping 应回到 API/application adapter 边界？ | 规范强调 API route 使用 contract DTO、route-local DTO 仅小包装；当前 application 多处 import contract DTO，说明 wire shape 已进入 use case/read model 层。这个问题会影响后续所有 generated contract 与 application projection 的分工。 | `crates/agentdash-application/src/agent_run/*`, `crates/agentdash-application/src/session/eventing.rs`, `crates/agentdash-application/src/workspace_module/*`, `crates/agentdash-application/src/capability/tool_catalog.rs`, `crates/agentdash-contracts/src/*` |
| P1 | API routes 直接使用 `state.repos` 的边界如何划分：哪些是薄 CRUD，哪些应下沉到 application use case？ | route 中直接 repo 操作范围很广，既有简单 CRUD，也有 backend/workspace/project/permission/lifecycle 等跨聚合入口；如果不分类，后续 route 会自然继续吸收编排。 | `crates/agentdash-api/src/routes/*.rs`, `crates/agentdash-application/src/{project,workspace,backend,permission,llm_provider,mcp_preset,skill_asset,shared_library}/*` |
| P1 | `RepositorySet` 是否需要按 use case/module 拆成窄依赖包，避免所有 application service 拿到全仓库能力？ | `RepositorySet` 当前包含 39 个左右 port 字段，跨 Project/Story/Workflow/Session/Permission/Runtime/Mailbox；作为单一 bundle 容易隐藏跨聚合读写。 | `crates/agentdash-application/src/repository_set.rs`, application services 中 `repos: &RepositorySet` / `repos: RepositorySet` 的调用点 |
| P1 | `agentdash-contracts` 依赖 domain/SPI 的转换职责是否应保持，还是改为纯 wire DTO + API mapper？ | contract crate 依赖 domain/SPI/protocol 能复用类型，但也把 wire contract 与内部模型绑定；尤其 runtime/session contract import `agentdash_spi::session_persistence`，需要确认 contract source 的权威边界。 | `crates/agentdash-contracts/Cargo.toml`, `crates/agentdash-contracts/src/{workspace,context,story,task,runtime,workflow}/*`, API dto/route mappers |
| P1 | `agentdash-application-ports` 是否仍保持“纯 port”，还是已承载 relay/protocol wire 事实？ | spec 明确该 crate 是 transport trait、轻量 DTO/error；它依赖 relay、agent-protocol、SPI、domain，下一轮应确认没有变成第二个 contracts/SPI 混合层。 | `crates/agentdash-application-ports/src/*`, `crates/agentdash-application/src/relay_connector.rs`, `crates/agentdash-api/src/bootstrap/{relay,runtime_gateway,session}.rs`, local relay implementation |
| P2 | executor/agent/SPI 的当前依赖方向是否需要把少量 domain enum 下沉到 agent-types/SPI？ | `agentdash-agent` 为 `ThinkingLevel` 依赖 domain，SPI 也 re-export domain common types；当前看是低风险，但会影响 Agent 子系统独立性。 | `crates/agentdash-agent/src/types.rs`, `crates/agentdash-domain/src/common/*`, `crates/agentdash-spi/src/lib.rs`, `crates/agentdash-agent-types/src/*` |
| P2 | AppState bootstrap 是否只做装配，还是已有业务/query helper 下沉不足？ | spec 明确 bootstrap 只承载宿主装配，不承载业务/query helper；当前 bootstrap 规模较大但大体是装配，需要下一轮按 module 过一遍。 | `crates/agentdash-api/src/bootstrap/*.rs`, `crates/agentdash-api/src/app_state.rs` |

### 5. 不应重复 review 的内容

- 不重复深挖 Workflow/Lifecycle/Task 的 reducer、Task projection、lifecycle start/drain、status aggregation、`LifecycleDispatchService` 过厚等问题；这些已由 `.trellis/tasks/06-14-module-overdesign-review/research/01-lifecycle-workflow-task.md` 与 `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` 覆盖。
- 不重复深挖 Session/AgentRun/RuntimeSession 的 workspace projection、runtime-control、SessionRuntimeInner、AgentRuntimeDelegate、MailboxService、direct steer 路径；这些已由 `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` 覆盖。
- 不重复深挖 VFS tool provider、local CommandHandler、extension host contract/schema、VFS mount metadata、Tauri shell、frontend mount selection；这些已由 `.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md` 覆盖。
- 不重复深挖 PermissionGrant/companion grant、capability catalog、permission nested DTO、executor discovery、session system event UI；这些已由 `.trellis/tasks/06-14-module-overdesign-review/research/04-frontend-contracts-permission.md` 覆盖。
- 本 slice 只把上述问题作为“分层拓扑上的已知耦合背景”引用，不进入局部代码修复建议。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 在当前 shell 返回 `Current task: (none)`；本文件按用户显式指定的 task path `.trellis/tasks/06-21-module-topology-coupling-review` 与输出路径写入，不做其它路径猜测。
- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` 仍是占位内容；实际研究范围以用户本轮调度说明、`design.md`/`implement.md` 和后端/cross-layer specs 为准。
- 本轮未运行测试或 cargo check；这是只读架构研究，未修改业务代码。
- 本轮刻意不深入 Workflow/Lifecycle 与 Session/AgentRun 细节；只在 crate/分层关系上标出与这些模块相交的边界。
- 未使用外部资料。
