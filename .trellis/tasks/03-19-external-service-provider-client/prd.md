# External Service Provider Client

## Goal

为虚拟上下文容器体系补齐一个通用的 `external_service` provider 接入层，使 AgentDash 可以通过统一 client 调用外部 provider service，并把企业 KM、规范库、文档中心、知识网关等非内置内容源接进统一 Address Space，而不需要把企业 API 定制逻辑写进主框架。

## Why This Exists

- 当前 `Project / Story` 级上下文容器已经可以通过 `inline_fs` 跑通第一版虚拟 mount 闭环。
- 下一步要支持真正可扩展的 provider 来源，就需要把“内容来源”从 AgentDash 主服务里抽出去。
- 企业级场景下，KM、文档库、对象存储、权限网关往往都不是 AgentDash 能直接理解的内部模型，因此更适合通过外部 provider service 做统一适配。
- 这个任务的重点不是立刻实现，而是先把接口、client 边界、错误语义和注册模型定清楚，避免后续实现时再返工统一抽象。

## Requirements

- 新增 `external_service` provider 类型的设计说明，并与已有 `inline_fs` 保持同一套 Address Space / mount 抽象。
- 定义 AgentDash 调用外部 provider service 的基础 client 契约，至少覆盖：
  - provider/service 标识
  - 能力声明
  - `list`
  - `read`
  - `stat`
  - `search`
- 定义 provider service 注册与连接配置模型，明确：
  - AgentDash 保存什么
  - provider service 保存什么
  - 凭证如何引用
  - 如何区分租户 / 项目 / 用户上下文
- 定义统一错误矩阵，明确超时、鉴权失败、资源不存在、能力不支持、provider 不可用等情况如何映射到 AgentDash 域错误。
- 明确首轮只读能力，不设计 `write / exec` 的落地实现。
- 设计结果必须能自然接入 `Project / Story` 容器派生链路，而不是新起一套旁路。

## Acceptance Criteria

- [ ] 明确 `external_service` provider 的领域模型与配置结构。
- [ ] 明确 provider client 与 service 之间的请求/响应契约。
- [ ] 明确 provider 注册、发现、凭证引用与租户隔离模型。
- [ ] 明确错误矩阵、超时策略、重试边界与缓存策略。
- [ ] 明确该能力如何接入现有 `AddressSpaceService` / mount 派生流程。
- [ ] 明确首轮不做什么，避免任务范围失控。

## MVP Scope

首轮只做设计，不立即实现代码，且约束在以下边界内：

- 只读虚拟内容访问
- 同步请求模型优先
- 单 provider service 返回标准文件形态资源
- 只覆盖 Task / Story Session 读取链路
- 不包含前端管理台设计
- 不包含企业权限体系深度映射
- 不包含 embedding / indexing / webhook 同步

## Proposed Design

### 1. Provider Registration

AgentDash 侧维护 `ProviderConnection` 元数据，至少包含：

- `provider_type = external_service`
- `service_base_url` 或 service registry key
- `credential_ref`
- `tenant_ref`
- 默认超时 / 重试 / 缓存策略
- 能力白名单

这里保存的是“如何访问 provider service”的连接信息，而不是 provider 内部资源内容。

### 2. Provider Resource Addressing

容器定义继续使用统一 mount 语义，不把外部服务接口暴露给 Agent。容器中只需要声明：

- `provider = external_service`
- `connection_ref`
- `resource_ref`
- 可选的子路径 / 查询范围
- 能力约束与权限收缩策略

Agent 仍然只看到 `mount + relative path`。

### 3. Provider Service Contract

建议 provider service 对 AgentDash 暴露一套稳定的资源接口：

- `GET/POST capability describe`
- `list`
- `read`
- `stat`
- `search`

返回应统一成文件形态资源视图，例如：

- 节点类型：`file` / `directory`
- 规范化 path
- size / etag / updated_at
- 可选 metadata
- 文本内容或二进制内容引用

### 4. Error Mapping

需要预先定义统一错误映射：

- `401/403` -> provider auth / permission denied
- `404` -> target not found
- `408/504` -> provider timeout
- `409` -> provider state conflict
- `422` -> invalid request / unsupported selector
- `501` 或 capability miss -> operation unsupported
- `5xx` -> provider unavailable

并明确哪些错误可重试，哪些错误要直接暴露给上层 session 创建或工具执行。

### 5. Cache Boundary

AgentDash 可以缓存：

- provider capability
- 短期目录 listing
- 小文本文件内容
- stat/search 的摘要结果

AgentDash 不负责持久保存 provider 原始业务数据，也不负责替代 provider 的权限系统。

## Out of Scope

- 立即实现 `external_service` provider client
- 设计完整可写文件系统协议
- 引入 provider push sync / webhook
- 为每个企业系统单独设计 adapter
- 在 AgentDash 主服务中持久化 provider 原始文件内容

## Delivery Notes

- 这个任务当前先保持 `planning`，作为后续实现的设计入口。
- 推荐等 `Project / Story` 编辑入口与统一 context composer 方向进一步清晰后，再进入实现切片。

## Related Files

- `.trellis/spec/backend/address-space-access.md`
- `.trellis/tasks/03-18-project-virtual-workspace-provider-service/prd.md`
- `crates/agentdash-api/src/address_space_access.rs`
- `crates/agentdash-executor/src/connector.rs`
- `crates/agentdash-executor/src/connectors/pi_agent.rs`
