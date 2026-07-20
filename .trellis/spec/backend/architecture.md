# Backend Architecture

## Role

后端承载 AgentDash 的业务事实、运行编排、协议入口和本机能力中继。云端后端是业务数据与事件事实源；本机后端只管理本机进程、工具执行和物理文件访问。

## Invariants

- 云端代码不直接访问本机文件系统；本机代码不直接读写业务数据库。
- 业务数据归云端：Project、Story、Task、Workspace 元数据、Settings、StateChange、Session 事件。
- 物理执行归本机：第三方 Agent 进程、工作空间物理文件、本机 tool call。
- 后端依赖方向遵循整洁架构：Interface -> Application -> Domain/SPI，Infrastructure 实现 Domain/SPI 中的持久化端口，不依赖 application 编排层。
- API 层负责鉴权、请求/响应 DTO 和错误映射；业务编排进入 application 层。
- Domain 层定义实体、值对象、Repository trait 与领域错误，不依赖具体持久化或接口框架。
- 跨聚合一致性使用显式 command port / unit of work，不伪装成单一 aggregate repository。

## Current Baseline

主要 crate：

| Crate | 当前职责 |
| --- | --- |
| `agentdash-api` | HTTP 路由模块、API-only DTO、中间件、AppState 装配 |
| `agentdash-application` | 产品用例编排与 AgentFrame/product fact source adapters |
| `agentdash-application-workflow` | Workflow catalog、builtin templates、graph/script compiler、orchestration reducer 与 executor launcher |
| `agentdash-application-hooks` | Product Hook preset、policy、script 与 typed Product/Complete Agent evaluation |
| `agentdash-application-shared-library` | Shared Library seed、external marketplace import/refresh、Project install/publish/source-status use cases |
| `agentdash-domain` | 实体、值对象、Repository trait、领域错误 |
| `agentdash-infrastructure` | PostgreSQL / SQLite 持久化实现 |
| `agentdash-agent-runtime-contract` | Application ↔ Managed Runtime command、snapshot、change 与 availability contract |
| `agentdash-agent-runtime` | Managed Runtime operation、admission、normalized projection、change/outbox 与 Tool Broker |
| `agentdash-agent-runtime-host` | Complete Agent service instance、offer、binding、generation、effect 与 recovery host |
| `agentdash-agent-runtime-wire` | Cloud/Local 与 Remote Complete Agent 的 typed bidirectional transport |
| `agentdash-integration-native-agent` | Dash Agent 的 Complete Agent service adapter |
| `agentdash-integration-codex` | Codex App Server 到 Complete Agent service 的 anti-corruption adapter |
| `agentdash-integration-remote-runtime` | 远端 Complete Agent service placement proxy，不拥有 Agent 业务语义 |
| `agentdash-llm-provider` | Native Agent 使用的 provider bridge 与 credential-scoped resolver |
| `agentdash-platform-spi` | 平台 feature、Hook policy、VFS/MCP 与 tool source ports |
| `agentdash-agent-core` | 无隐藏持久状态的 provider/tool loop |
| `agentdash-agent` | Dash Agent ordered history、fork、compaction 与 lifecycle |
| `agentdash-agent-service-api` | Host ↔ Complete Agent 的 dependency-light typed seam |
| `agentdash-agent-protocol` | AgentDash-owned canonical conversation standard families 与 typed extensions |
| `agentdash-relay` | Cloud/Local WebSocket relay 协议 |
| `agentdash-local` | 本机后端 |
| `agentdash-local-tauri` | Tauri 桌面托管壳，管理本机 runtime 与 external/sidecar Dashboard API 进程边界 |

Agent runtime module baseline：

| Module | 当前职责 |
| --- | --- |
| `agentdash-application-agentrun::runtime_facade` | 产品 coordinate/command 到 canonical Runtime 的唯一 facade |
| `agentdash-infrastructure::agent_runtime_composition` | PostgreSQL Managed Runtime、Business Surface、Integration Host、driver callbacks 与 durable workers 的生产装配 |
| `agentdash-agent-runtime-host` | definition -> instance -> verified offer -> durable binding -> Complete Agent effect/inspect/reconcile |
| `agentdash-agent-runtime` | canonical operation、snapshot/change cursor、projection、outbox 与 broker call exactly-once |
| `agentdash-agent` | Dash Agent history-maintained AgentSession 与 lifecycle ledger |
| `agentdash-agent-core` | Dash Agent 内部使用的纯 Agent loop，不拥有 Runtime/Product/Agent persistence |

## AppState Bootstrap

`agentdash-api/src/bootstrap/` 承载 API 宿主的装配切片。每个 bootstrap 模块接收启动期输入，返回后续装配真实需要的 output struct，让 `AppState::new_with_plugins` 表达高层构造顺序。

Repository bootstrap 负责 PostgreSQL repository 实例化、composition-root `RepositorySet` 聚合、Product/Runtime/Host/Agent owner-specific ports、auth session service，以及启动期 Shared Library seed。这样 API 宿主依赖的是启动期装配结果；进入 route helper 或 application service 后，必须拆成具名 use-case deps，而不是继续传递全量 set。

Relay bootstrap 负责创建 backend registry、backend runtime event channel、shell output registry 与 terminal cache。VFS bootstrap 基于 repository ports、session persistence、relay registry 和插件 mount providers 构建 mount provider registry、VFS service、mutation dispatcher、runtime tool provider 与 materializing MCP relay。这样 session runtime 装配只消费 VFS/relay 的明确输出。

Agent Runtime bootstrap 负责收集受信 Integration contributions 与 trust manifests，构造 PostgreSQL Host/Managed Runtime/Business Surface/Tool Broker/Hook callback，并启动 outbox、context、hook effect 与 recovery workers。Native、Codex 和企业远端服务都经过 definition、instance、offer、binding 生命周期；Relay 只提供 remote placement transport。

Auth、runtime gateway 与 background worker bootstrap 分别负责认证模式校验、runtime action provider 组合、以及 AppState 构建完成后的 terminal effect replay、stall detector、routine scheduler 和 auth session cleanup。后台 worker 只在 AppState 已完成延迟绑定检查后启动。

`bootstrap` 只承载宿主装配，不承载业务/查询 helper。Frame construction、project-agent context、workspace resolution 等 session 运行上下文逻辑归 `agentdash-application::session`；VFS surface summary 归 `agentdash-application::vfs`，API 侧只实现 backend online / mount edit capability 等 runtime projection adapter。需要 `AppState` 与鉴权身份的 session adapter 留在 API 的 session frame resolver 边界，负责把 HTTP/runtime state 投影为 application use case 输入，不持有业务编排事实。

`agentdash-api/src/routes.rs` 是 composition root。每个 `agentdash-api/src/routes/*.rs` 资源模块导出 `pub fn router() -> Router<Arc<AppState>>`，并在模块内声明自己的 endpoint table；根 router 只合并 secured/public routers、MCP router、WebSocket route 与全局 CORS/trace layer。API-only request/query/response DTO 放在 `agentdash-api/src/dto/`，跨端共享 DTO 放在 `agentdash-contracts`，route 模块不内联 HTTP DTO。

Project extension runtime projection 归 `agentdash-application::extension_runtime`，API 入口为 `agentdash-api/src/routes/extension_runtime.rs`。该 projection 从 Project enabled extension installations 派生 runtime actions、workspace tabs、permissions 与 bundle refs；Shared Library 只保留安装来源职责，Frame construction 只读取 projection 作为启动/检查上下文。

Project extension management 归 `agentdash-application::extension_management`，API 入口为 `agentdash-api/src/routes/project_extensions.rs`。Management list 从 `ProjectExtensionInstallation` 读取安装事实，并补充 source status、package mode、artifact summary 与 capability summary；runtime projection 继续只表达运行视图。

Extension package artifact 归独立 `agentdash-domain::extension_package` / `agentdash-application::extension_package` 模块，API 入口为 `agentdash-api/src/routes/extension_package_artifacts.rs` 与 project-facing import route。正式 packaged extension 安装以平台保存的 archive artifact 为事实源：后端校验 manifest、bundle digest 与 archive digest，保存 package metadata、manifest snapshot、storage ref 和 source version。Artifact 归属使用 `owner_kind = project | library_asset` 与 `owner_id`：Project-owned artifact 服务本地导入和 Canvas promote，LibraryAsset-owned artifact 服务 Marketplace packaged template。Project extension installation 可引用 `package_artifact`；archive download access 通过当前 Project installation 判定。Archive object 读写通过 `agentdash-platform-spi::extension_package::ExtensionPackageArtifactStorage` 端口进入 application use case，由 `agentdash-infrastructure::storage` 提供 filesystem adapter，原因是 API route 只表达入口语义，不能拥有 object storage path 与 filesystem normalization。

Canvas 发布为插件的用例归 `agentdash-application::canvas::promotion`，API 入口为 `POST /api/canvases/{id}/promote-extension`。该用例从 Canvas 聚合生成 `.agentdash-extension.tgz`，写入 Project scoped extension package artifact，再安装为 Project extension installation。

Project 授权规则由 `agentdash-domain::project::ProjectAuthorizationService` 表达；API、application 与 MCP 只把请求身份投影为 `ProjectAuthorizationContext` 后消费同一规则。Backend owner/scope/admin/personal 的跨聚合判定由 `agentdash-application::backend::BackendAuthorizationService` 表达，API route 只做 extractor、DTO 与错误映射。

## Local Decisions

- Repository trait 按 aggregate 边界定义，原因是持久化接口应反映领域一致性边界，而不是表结构。
- Product command/mailbox、Managed Runtime operation/change/outbox、Host effect/binding 与 Dash Agent history 分别由其 typed owner contract 定义，原因是这些事实具有不同事务和恢复权威，不能再聚合进平台 `SessionPersistence`。
- `RepositorySet` 只作为 application/bootstrap composition result 保留，原因是启动期需要统一持有 repository ports；业务用例使用具名 deps struct，原因是 constructor 签名必须暴露真实 aggregate 依赖，避免 service locator 进入 application 逻辑。
- PostgreSQL migration 与 SQLite 初始化策略分开维护，原因是云端业务库需要统一可审计 schema 历史，本机会话缓存则由本机 runtime 拥有 per-user 初始化生命周期。
- Project 授权放在 domain，原因是角色、主体 grant 与 template 可见性属于 Project 聚合语义，MCP 与 API 都需要在不反向依赖 application 的情况下复用同一判定。`ProjectAuthorizationContext` 保留认证身份的 `user_id` 与 `subject` 别名，原因是企业目录解析、登录态 claim 与授权持久化可能使用不同但等价的用户标识，Project 角色判定需要在同一领域入口完成身份收束。Backend 授权放在 application，原因是 backend scope 可能需要组合 Backend 与 Project repository，属于跨聚合用例编排。
- Canvas access projection 放在 domain，原因是 Canvas 管理 API、Workspace Module descriptor、runtime VFS mount 暴露和 Canvas 文件操作都需要消费同一份 view/edit/runtime-write 语义；各 application adapter 只负责提供当前身份与 Project access 上下文。
- Codex app-server bridge 归 `agentdash-integration-codex`，原因是 vendor protocol、进程生命周期与 native hook materialization 都是 Driver adapter 语义；AgentDash canonical state 与 capability truth 由 owned Runtime contract/Host 持有。
- `agentdash-process` 承载 AgentDash 自有后台子进程启动 substrate，原因是本机 relay、MCP stdio、tool shell、workspace probe、function runner、desktop sidecar 和 Codex bridge 都可能从桌面 GUI 宿主触发 console 子进程；统一的 `ProcessVisibility` / `ProcessDomain` 边界让 Windows 后台启动静默、诊断可按 domain/program/cwd/visibility 回溯，且不记录 args/env 等 credential-bearing 值。
- Extension package artifact 独立于 LibraryAsset payload，原因是正式插件包是平台可下载、可校验、可审计的运行产物；owner 模型让 Project 本地导入与 LibraryAsset Marketplace 模板共享同一套 digest、storage 与访问校验。
- Extension package archive object storage 端口放在 `agentdash-platform-spi`，原因是 application 需要消费该端口表达用例意图，而 infrastructure 需要实现该端口且不应反向依赖 application 编排层。
- Route module 自持 router 表，原因是 endpoint ownership 应与 handler/module ownership 对齐；根 router 只表达 secured/public 装配，避免跨资源长链路表成为协议事实源。
- `agentdash-local-tauri` 通过 external origin 或 `agentdash-server` sidecar 连接 Dashboard API，原因是 AppState、migration、HTTP route 与 API diagnostics 的 composition ownership 属于 `agentdash-api`/`agentdash-server`，桌面壳只负责本机能力、进程生命周期和 readiness projection。

## Contract Appendices

- [Directory Structure](./directory-structure.md)
- [Database Guidelines](./database-guidelines.md)
- [Repository Pattern](./repository-pattern.md)
- [Error Handling](./error-handling.md)
- [Domain Payload Typing](./domain-payload-typing.md)
- [Quality Guidelines](./quality-guidelines.md)
- [Logging Guidelines](./logging-guidelines.md)
- [Runtime Gateway](./runtime-gateway.md)
