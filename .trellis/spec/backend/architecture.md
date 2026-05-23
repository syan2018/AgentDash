# Backend Architecture

## Role

后端承载 AgentDash 的业务事实、运行编排、协议入口和本机能力中继。云端后端是业务数据与事件事实源；本机后端只管理本机进程、工具执行和物理文件访问。

## Invariants

- 云端代码不直接访问本机文件系统；本机代码不直接读写业务数据库。
- 业务数据归云端：Project、Story、Task、Workspace 元数据、Settings、StateChange、Session 事件。
- 物理执行归本机：第三方 Agent 进程、工作空间物理文件、本机 tool call。
- 后端依赖方向遵循整洁架构：Interface -> Application -> Domain，Infrastructure 实现 Domain/Application 所需端口。
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

## AppState Bootstrap

`agentdash-api/src/bootstrap/` 承载 API 宿主的装配切片。每个 bootstrap 模块接收启动期输入，返回后续装配真实需要的 output struct，让 `AppState::new_with_plugins` 表达高层构造顺序。

Repository bootstrap 负责 PostgreSQL repository 实例化、`RepositorySet` 聚合、session persistence port、auth session service，以及启动期 Shared Library seed。这样 API composition root 依赖的是装配结果，而不是每个 repository 的具体初始化细节。

Relay bootstrap 负责创建 backend registry、backend runtime event channel、shell output registry 与 terminal cache。VFS bootstrap 基于 repository ports、session persistence、relay registry 和插件 mount providers 构建 mount provider registry、VFS service、mutation dispatcher、runtime tool provider 与 materializing MCP relay。这样 session runtime 装配只消费 VFS/relay 的明确输出。

Session bootstrap 负责组合 Pi / relay / plugin connectors，构建 `CompositeConnector`、execution hook provider、`SessionRuntimeBuilder` 及 session service handles，并完成 lifecycle terminal callback 与 runtime tool session handle 绑定。`SessionRuntimeBuilder` 作为显式输出保留给 AppState 完成 construction provider、hook effect registry 与 audit bus 这些 AppState-aware 延迟绑定。

Auth、runtime gateway 与 background worker bootstrap 分别负责认证模式校验、runtime action provider 组合、以及 AppState 构建完成后的 terminal effect replay、stall detector、routine scheduler 和 auth session cleanup。后台 worker 只在 AppState 已完成延迟绑定检查后启动。

`bootstrap` 只承载宿主装配，不承载业务/查询 helper。Session construction、project-agent context、workspace resolution 等 session 运行上下文逻辑归 `agentdash-application::session`；VFS surface summary 归 `agentdash-application::vfs`，API 侧只实现 backend online / mount edit capability 等 runtime projection adapter。仍依赖 `AppState` / `ApiError` / 鉴权的 session adapter 放在 `agentdash-api/src/session_use_cases/`，不回流到 bootstrap。

## Local Decisions

- Repository trait 按 aggregate 边界定义，原因是持久化接口应反映领域一致性边界，而不是表结构。
- `RepositorySet` 放在 application 层，原因是应用用例需要组合多个 port，API 层不应直接知道具体 repository 实现。
- PostgreSQL migration 与 SQLite 初始化策略分开维护，原因是云端业务库和本机会话缓存承担不同生命周期。

## Contract Appendices

- [Directory Structure](./directory-structure.md)
- [Database Guidelines](./database-guidelines.md)
- [Repository Pattern](./repository-pattern.md)
- [Error Handling](./error-handling.md)
- [Domain Payload Typing](./domain-payload-typing.md)
- [Quality Guidelines](./quality-guidelines.md)
- [Logging Guidelines](./logging-guidelines.md)
- [Runtime Gateway](./runtime-gateway.md)
