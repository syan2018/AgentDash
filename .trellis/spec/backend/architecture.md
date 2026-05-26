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
| `agentdash-api` | HTTP 路由、DTO、中间件、AppState 装配 |
| `agentdash-application` | 用例编排、session/context/workflow/VFS/capability 服务 |
| `agentdash-domain` | 实体、值对象、Repository trait、领域错误 |
| `agentdash-infrastructure` | PostgreSQL / SQLite 持久化实现 |
| `agentdash-executor` | connector、LLM bridge、hook runtime 适配 |
| `agentdash-spi` | Connector / Hook / capability 等跨 crate port |
| `agentdash-agent` | Agent Loop 引擎 |
| `agentdash-agent-types` | Agent 领域通用类型 |
| `agentdash-agent-protocol` | Backbone Protocol 与协议适配 |
| `agentdash-relay` | Cloud/Local WebSocket relay 协议 |
| `agentdash-local` | 本机后端 |
| `agentdash-local-tauri` | Tauri 桌面托管壳 |

Agent runtime module baseline：

| Module | 当前职责 |
| --- | --- |
| `agentdash-agent/src/agent_loop.rs` | Agent loop 入口、turn/follow-up orchestration、runtime delegate stop/after-turn 调度 |
| `agentdash-agent/src/agent_loop/streaming.rs` | Assistant stream state machine、provider request delegate、message delta/event projection |
| `agentdash-agent/src/agent_loop/tool_call.rs` | Tool call prepare / approval / execute / finalize / result event projection |
| `agentdash-agent/src/agent_loop/tool_result.rs` | Tool execution error/approval result helper |

## AppState Bootstrap

`agentdash-api/src/bootstrap/` 承载 API 宿主的装配切片。每个 bootstrap 模块接收启动期输入，返回后续装配真实需要的 output struct，让 `AppState::new_with_plugins` 表达高层构造顺序。

Repository bootstrap 负责 PostgreSQL repository 实例化、`RepositorySet` 聚合、session persistence port、auth session service，以及启动期 Shared Library seed。这样 API composition root 依赖的是装配结果，而不是每个 repository 的具体初始化细节。

Relay bootstrap 负责创建 backend registry、backend runtime event channel、shell output registry 与 terminal cache。VFS bootstrap 基于 repository ports、session persistence、relay registry 和插件 mount providers 构建 mount provider registry、VFS service、mutation dispatcher、runtime tool provider 与 materializing MCP relay。这样 session runtime 装配只消费 VFS/relay 的明确输出。

Session bootstrap 负责组合 Pi / relay / plugin connectors，构建 `CompositeConnector`、execution hook provider、`SessionRuntimeBuilder` 及 session service handles，并完成 lifecycle terminal callback 与 runtime tool session handle 绑定。`SessionRuntimeBuilder` 作为显式输出保留给 AppState 完成 construction provider、hook effect registry 与 audit bus 这些 AppState-aware 延迟绑定。

Auth、runtime gateway 与 background worker bootstrap 分别负责认证模式校验、runtime action provider 组合、以及 AppState 构建完成后的 terminal effect replay、stall detector、routine scheduler 和 auth session cleanup。后台 worker 只在 AppState 已完成延迟绑定检查后启动。

`bootstrap` 只承载宿主装配，不承载业务/查询 helper。Session construction、project-agent context、workspace resolution 等 session 运行上下文逻辑归 `agentdash-application::session`；VFS surface summary 归 `agentdash-application::vfs`，API 侧只实现 backend online / mount edit capability 等 runtime projection adapter。仍依赖 `AppState` / `ApiError` / 鉴权的 session adapter 放在 `agentdash-api/src/session_use_cases/`，不回流到 bootstrap。

Project extension runtime projection 归 `agentdash-application::extension_runtime`，API 入口为 `agentdash-api/src/routes/extension_runtime.rs`。该 projection 从 Project enabled extension installations 派生 runtime actions、workspace tabs、permissions 与 bundle refs；Shared Library 只保留安装来源职责，Session construction 只读取 projection 作为启动/检查上下文。

Extension package artifact 归独立 `agentdash-domain::extension_package` / `agentdash-application::extension_package` 模块，API 入口为 `agentdash-api/src/routes/extension_package_artifacts.rs`。正式 packaged extension 安装以平台保存的 archive artifact 为事实源：后端校验 manifest、bundle digest 与 archive digest，保存 package metadata、manifest snapshot、storage ref 和 source version；Project extension installation 可引用 `package_artifact`，不要求再绑定 Shared Library source。

Project 授权规则由 `agentdash-domain::project::ProjectAuthorizationService` 表达；API、application 与 MCP 只把请求身份投影为 `ProjectAuthorizationContext` 后消费同一规则。Backend owner/scope/admin/personal 的跨聚合判定由 `agentdash-application::backend::BackendAuthorizationService` 表达，API route 只做 extractor、DTO 与错误映射。

## Local Decisions

- Repository trait 按 aggregate 边界定义，原因是持久化接口应反映领域一致性边界，而不是表结构。
- Session 事件、terminal effect outbox 与 runtime command store 的持久化 contract 放在 `agentdash-spi::session_persistence`，原因是这些 record 同时服务 application runtime 与 infrastructure adapter，不能把 infrastructure 绑定到 application 编排模块。
- `RepositorySet` 放在 application 层，原因是应用用例需要组合多个 port，API 层不应直接知道具体 repository 实现。
- PostgreSQL migration 与 SQLite 初始化策略分开维护，原因是云端业务库需要统一可审计 schema 历史，本机会话缓存则由本机 runtime 拥有 per-user 初始化生命周期。
- Project 授权放在 domain，原因是角色、主体 grant 与 template 可见性属于 Project 聚合语义，MCP 与 API 都需要在不反向依赖 application 的情况下复用同一判定。Backend 授权放在 application，原因是 backend scope 可能需要组合 Backend 与 Project repository，属于跨聚合用例编排。
- `agentdash-executor` 直接维护 Codex app-server bridge，原因是本机进程生命周期、connector 能力声明与 Backbone 事件投影属于 AgentDash runtime 边界，外部编排 crate 不应成为该边界的事实源。
- Extension package artifact 独立于 Shared Library，原因是正式插件包是平台可下载、可校验、可审计的运行产物；Shared Library 只表达 marketplace/source template 与 Project 资源安装来源。

## Contract Appendices

- [Directory Structure](./directory-structure.md)
- [Database Guidelines](./database-guidelines.md)
- [Repository Pattern](./repository-pattern.md)
- [Error Handling](./error-handling.md)
- [Domain Payload Typing](./domain-payload-typing.md)
- [Quality Guidelines](./quality-guidelines.md)
- [Logging Guidelines](./logging-guidelines.md)
- [Runtime Gateway](./runtime-gateway.md)
