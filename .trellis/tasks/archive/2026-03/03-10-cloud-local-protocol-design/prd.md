# 云端/本机双后端架构：通信协议与数据归属设计

## Goal

设计 AgentDashboard 云端后端与本机后端之间的完整通信协议和数据归属模型，为后续 Task 2（拆分本机后端）和 Task 3（构建云端后端）提供可执行的技术规范。

**核心原则：云端是数据主人、能力暴露者和云端原生 Agent（PiAgent）引擎；本机是第三方 Agent 执行工人和 PiAgent 的远程工具环境。**

## What I already know

- 当前只有一个单体后端 `agentdash-server`，承担了所有角色
- Domain 层已有 `BackendType::Local | Remote` 概念，但无运行时行为差异
- `RemoteAcpConnector` 为空壳，所有方法返回未实现
- SSE/NDJSON 流已实现单向推送和 Resume 语义
- ACP 协议 SDK 已集成（`agent-client-protocol` crate）
- `AgentConnector` trait 是良好的执行能力抽象
- Clean Architecture 分层（Domain/Infrastructure/Application/API）可复用
- Axum 已启用 `ws` feature 但未使用
- 项目为预研阶段，无兼容性包袱

## Requirements

### R1: 数据归属定义

明确定义云端与本机各自拥有的数据实体：

**云端拥有（Cloud Owns）：**
- `Project` — 项目定义和配置
- `Workspace` — 工作空间元数据（含 `backend_id` 标记物理文件所在本机，`container_ref` 为绝对路径）
- `Story` — 用户价值单元
- `Task` — 执行单元定义（通过 `workspace_id → Workspace.backend_id` 确定执行本机）
- `Backend` — 已注册本机列表、鉴权凭证、在线状态
- `View` / `UserPreferences` — 跨后端视图和偏好
- `Settings` — 系统级和用户级设置
- `MCP` — 对外暴露的 Model Context Protocol 服务
- `StateChange` — 变更日志（不可变事件流）

**云端执行（Cloud Runs）：**
- `PiAgent AgentLoop` — 云端原生 Agent 执行引擎，直接访问云端 DB
- `CloudContextProvider` — 为 PiAgent 加载完整上下文

**本机拥有（Local Owns）：**
- `ExecutorHub` — 第三方 Agent 会话运行时（内存态 + JSONL 持久化）
- `AgentConnector` 实例 — 本地第三方 Agent 进程管理（Claude Code、Codex 等）
- `ToolExecutor` — PiAgent tool call 的本地执行环境
- Workspace 物理文件 — 实际的 Git worktree / 文件系统
- Agent 执行中间状态 — ACP SessionNotification 流（仅第三方 Agent）
- 本机能力声明 — 可用第三方执行器列表、变体、模型

### R2: WebSocket 通信协议

设计本机主动连接云端的 WebSocket 协议：

**连接建立：**
- 本机启动时通过 CLI 参数指定云端地址：`--cloud-url wss://cloud.example.com/ws/backend`
- 本机携带 `--token <auth-token>` 进行身份认证
- 云端验证 token 后注册该本机为在线 Backend

**消息格式：**
- 基于 JSON 的请求/响应/事件三类消息
- 每条消息包含 `type`、`id`（用于请求-响应配对）、`payload`
- 支持双向流式传输（Agent 执行输出通过 WS 实时回传）

**消息类型（云端→本机）：**
- `command.prompt` — 执行第三方 Agent prompt
- `command.cancel` — 取消执行
- `command.discover` — 查询本机第三方能力/执行器列表
- `command.workspace_files` — 读取工作空间文件
- `command.tool.*` — PiAgent tool call 路由（file_read / file_write / shell_exec / file_list）
- `command.ping` — 心跳检测

**消息类型（本机→云端）：**
- `event.registered` — 注册成功，上报能力
- `event.session_notification` — ACP 会话通知流（实时转发）
- `event.capabilities_changed` — 能力变更（安装/卸载执行器）
- `event.pong` — 心跳响应
- `response.*` — 对应 command 的响应

### R3: 中继模型（Relay Model）

定义云端如何将前端请求中继到本机：

```
第三方 Agent：前端 → Cloud REST API → Task.workspace_id → Workspace.backend_id → WS command.prompt → 本机执行
             ← Cloud SSE/NDJSON ← 缓存+转发 ← 本机 WS event.session_notification

PiAgent：    前端 → Cloud REST API → 云端 AgentLoop 执行 → tool call → Workspace.backend_id → WS command.tool.* → 本机执行
             ← Cloud SSE/NDJSON ← 云端直接产出 SessionNotification
```

- 第三方 Agent：云端不执行 Agent 任务，只做路由和中继
- PiAgent：云端直接运行 AgentLoop，tool call 路由到本机，SessionNotification 云端直产
- 云端缓存执行状态（用于断线恢复和前端查询）
- 本机断线时，云端标记 Backend 为 offline，相关第三方 Agent Task 状态为 interrupted

### R4: 鉴权与注册

- 云端提供 Backend 注册 API：`POST /api/backends/register` 返回 token
- 本机使用该 token 建立 WebSocket 连接
- 云端维护 token → backend_id 的映射
- 本机断线后自动重连（指数退避）

### R5: 产出物

本 Task 的交付物不是代码，而是设计文档：

1. **`docs/modules/09-relay.md`** — 云端/本机中继模块设计文档
2. **`docs/relay-protocol.md`** — WebSocket 消息协议详细定义
3. **更新 `docs/core-design.md`** — 补充云端/本机分离的架构描述
4. **更新 `.trellis/spec/project-overview.md`** — 修正中控层描述
5. **更新 `.trellis/spec/backend/index.md`** — 补充双后端数据归属说明

## Acceptance Criteria

- [x] 数据归属清单明确列出每个实体归属方（云端/本机/共享），无模糊地带
- [x] WebSocket 消息协议定义完整的消息类型、格式、错误码
- [x] 中继模型覆盖 Task 执行全生命周期（prompt → streaming → complete/fail/cancel）
- [x] 鉴权流程包含注册、连接、心跳、断线重连四个阶段
- [x] 设计文档可直接指导 Task 2 和 Task 3 的实现
- [x] 与现有 ACP 协议兼容（SessionNotification 可直接通过 WS 转发）

## Definition of Done

- [x] 设计文档已写入对应路径
- [x] 现有架构文档已同步更新
- [x] 团队确认设计方案可行

## Out of Scope

- 不实现任何代码（本 Task 纯设计）
- 不考虑多云端部署（单一云端实例）
- 不考虑本机之间的 P2P 通信
- 不考虑数据加密（留给后续安全增强）
- 不考虑云端数据库选型（PostgreSQL vs SQLite 等，由 Task 3 决定）

## Technical Notes

### 现有可复用基础

| 资产 | 路径 | 复用方式 |
|------|------|---------|
| `AgentConnector` trait | `crates/agentdash-executor/src/connector.rs` | 本机执行能力抽象 |
| ACP `SessionNotification` | `agent-client-protocol` crate | WS 消息载体 |
| SSE Resume 语义 | `crates/agentdash-api/src/stream.rs` | 事件流 ID 机制 |
| `BackendConfig` / `BackendType` | `crates/agentdash-domain/src/backend/` | 数据模型基础 |
| `_meta.agentdash` 元信息 | `crates/agentdash-acp-meta/` | 消息溯源 |

### 关键设计约束

1. WebSocket 必须由本机主动发起（NAT 友好）
2. Agent 执行输出必须实时流式转发（不能等执行完再批量回传）
3. 云端必须缓存足够状态以支持前端断线恢复（Resume）
4. 协议必须支持多个本机同时在线，互不干扰

### 参考 crate 依赖

- `tokio-tungstenite` — 已在项目 patch 中（OpenAI fork with proxy feature）
- `axum` ws feature — 已启用

## Dependencies

- 无前置依赖，可立即开始
- 本 Task 完成后 unblock Task 2 和 Task 3
